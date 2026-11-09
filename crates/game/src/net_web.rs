//! Browser version of `net.rs` (#106): the same public API over iroh's
//! browser build, so browser and desktop players share games and join codes.
//!
//! `bevy_replicon_iroh` drives iroh through a tokio runtime and `block_on`s
//! every tick, which a browser's single thread can't do. Here iroh runs in
//! `spawn_local` tasks instead and talks to Bevy through channels that the
//! systems below drain without ever blocking. The bytes on the wire are
//! exactly `bevy_replicon_iroh`'s (see `net_wire.rs`), so a desktop host can't
//! tell a browser client apart, and the other way round.
//!
//! In the browser iroh can't send UDP: every connection goes through iroh's
//! public relays (the `N0` preset), and the join code is found via iroh's
//! pkarr/DNS lookup, just like on desktop. That works both ways, so a browser
//! can host too.

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender, TryRecvError, unbounded};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::channels::Channel;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use bytes::Bytes;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use n0_future::time::{Duration, timeout};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::AppState;
use crate::menu::JoinCode;
use crate::net_wire::{
    ALPN, STREAM_MARKER, STREAM_PRIORITY, encode_datagram, frame_header, parse_header,
};

/// Ask the network layer to host or join.
#[derive(Message, Clone, Debug)]
pub enum NetCommand {
    Host,
    Join(String),
}

/// What the UI shows about the connection.
#[derive(Resource, Default, Debug)]
pub struct NetStatus {
    /// Set while hosting: friends paste this to join.
    pub join_code: Option<String>,
    /// Last human-readable status or error.
    pub message: String,
}

/// The browser clipboard is async and needs a user gesture: `get` starts a
/// read and the text lands in the join box a frame or two later. Ctrl/Cmd+V
/// also arrives as a page `paste` event, which needs no permission at all.
#[derive(Resource, Default)]
pub struct Clipboard {
    pasted: Arc<Mutex<Option<String>>>,
}

impl Clipboard {
    pub fn set(&self, text: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let promise = window.navigator().clipboard().write_text(text);
        spawn_local(async move {
            if JsFuture::from(promise).await.is_err() {
                warn!("the browser didn't let us copy the join code");
            }
        });
    }

    /// Always `None` right away; the text is delivered into the join box.
    pub fn get(&self) -> Option<String> {
        let window = web_sys::window()?;
        let promise = window.navigator().clipboard().read_text();
        let slot = self.pasted.clone();
        spawn_local(async move {
            match JsFuture::from(promise).await {
                Ok(text) => {
                    if let (Some(text), Ok(mut slot)) = (text.as_string(), slot.lock()) {
                        *slot = Some(text);
                    }
                }
                Err(_) => warn!("the browser didn't let us read the clipboard: use Ctrl/Cmd+V"),
            }
        });
        None
    }
}

/// One established replicon connection, seen from Bevy: what arrived, a queue
/// to send on, and a signal when it closes. Dropping it closes the connection.
struct Link {
    incoming: Receiver<(u8, Bytes)>,
    outgoing: Sender<(u8, Bytes, bool)>,
    closed: Receiver<String>,
    max_datagram: usize,
}

/// A client connection to a host (connecting or connected). Same name as the
/// desktop's transport resource, so the menu can tell a join is under way.
#[derive(Resource)]
pub struct IrohClient {
    pending: Receiver<Result<Link, String>>,
    link: Option<Link>,
    /// Dropped with the resource: tells a still-running connect to give up.
    _cancel: Sender<()>,
}

/// Hosting from the browser: new client links arrive here.
#[derive(Resource)]
pub struct IrohServer {
    accepted: Receiver<Link>,
    failed: Receiver<String>,
    next_id: u64,
    /// Dropped with the resource: stops the accept loop and the endpoint.
    _stop: Sender<()>,
}

/// A browser host's connection to one client (on the `ConnectedClient` entity).
#[derive(Component)]
struct WebLink(Link);

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetStatus>()
            .init_resource::<Clipboard>()
            .add_message::<NetCommand>()
            .add_systems(Startup, listen_for_paste)
            .add_systems(Update, (handle_net_commands, deliver_paste))
            .add_systems(OnEnter(ClientState::Connected), enter_game)
            .add_systems(OnExit(ClientState::Connected), back_to_menu)
            .add_systems(
                PreUpdate,
                (
                    client_started.run_if(resource_added::<IrohClient>),
                    client_stopped.run_if(resource_removed::<IrohClient>),
                    client_receive.run_if(resource_exists::<IrohClient>),
                )
                    .chain()
                    .in_set(ClientSystems::ReceivePackets),
            )
            .add_systems(
                PreUpdate,
                (
                    server_started.run_if(resource_added::<IrohServer>),
                    server_stopped.run_if(resource_removed::<IrohServer>),
                    server_receive.run_if(resource_exists::<IrohServer>),
                )
                    .chain()
                    .in_set(ServerSystems::ReceivePackets),
            )
            .add_systems(
                PostUpdate,
                (
                    client_send
                        .in_set(ClientSystems::SendPackets)
                        .run_if(resource_exists::<IrohClient>),
                    server_send
                        .in_set(ServerSystems::SendPackets)
                        .run_if(resource_exists::<IrohServer>),
                    disconnect_by_request,
                ),
            );
    }
}

/// Close the transports: dropping them ends their tasks, which close the
/// connection and endpoint cleanly. No-op without either.
pub fn teardown_transport(world: &mut World) {
    world.remove_resource::<IrohClient>();
    world.remove_resource::<IrohServer>();
}

fn handle_net_commands(
    mut commands: Commands,
    mut requests: MessageReader<NetCommand>,
    clipboard: Res<Clipboard>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
) {
    for request in requests.read() {
        match request {
            NetCommand::Host => {
                commands.queue(teardown_transport);
                let key = SecretKey::generate();
                let code = key.public().to_string();
                info!("hosting from the browser, join code: {code}");
                clipboard.set(&code);
                status.message =
                    "Hosting! Join code copied (friends join through iroh's relays).".into();
                status.join_code = Some(code);
                commands.insert_resource(host(key));
                next.set(AppState::InGame);
            }
            NetCommand::Join(code) => {
                let id = match EndpointId::from_str(code.trim()) {
                    Ok(id) => id,
                    Err(e) => {
                        status.message = format!("That join code doesn't look right: {e}");
                        continue;
                    }
                };
                status.message = "Connecting through iroh's relay...".into();
                commands.queue(teardown_transport);
                commands.insert_resource(join(id));
            }
        }
    }
}

// ---------------------------------------------------------------- async side

fn join(host: EndpointId) -> IrohClient {
    let (result_tx, pending) = unbounded();
    let (cancel, cancelled) = unbounded::<()>();
    spawn_local(async move {
        let attempt = async {
            let endpoint = Endpoint::builder(presets::N0)
                .alpns(vec![ALPN.to_vec()])
                .bind()
                .await
                .map_err(|e| format!("could not start networking: {e}"))?;
            let connecting = async {
                let connection = endpoint
                    .connect(EndpointAddr::new(host), ALPN)
                    .await
                    .map_err(|e| format!("{e}"))?;
                handshake(connection).await
            };
            match timeout(Duration::from_secs(25), connecting).await {
                Ok(Ok(link)) => Ok((endpoint, link)),
                Ok(Err(e)) => {
                    endpoint.close().await;
                    Err(e)
                }
                Err(_) => {
                    endpoint.close().await;
                    Err("timed out".to_string())
                }
            }
        };
        // The menu may give up (Esc, timeout) before we finish.
        let outcome = n0_future::future::or(async { Some(attempt.await) }, async {
            let _ = cancelled.recv().await;
            None
        })
        .await;
        match outcome {
            Some(Ok((endpoint, (link, done)))) => {
                if result_tx.send(Ok(link)).await.is_err() {
                    // Cancelled right as we connected: the link is dropped,
                    // which closes the connection.
                }
                done.await;
                endpoint.close().await;
            }
            Some(Err(e)) => {
                let _ = result_tx.send(Err(e)).await;
            }
            None => {}
        }
    });
    IrohClient {
        pending,
        link: None,
        _cancel: cancel,
    }
}

fn host(key: SecretKey) -> IrohServer {
    let (accepted_tx, accepted) = unbounded();
    let (failed_tx, failed) = unbounded();
    let (stop, stopped) = unbounded::<()>();
    spawn_local(async move {
        let endpoint = match Endpoint::builder(presets::N0)
            .secret_key(key)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
        {
            Ok(endpoint) => endpoint,
            Err(e) => {
                let _ = failed_tx.send(format!("{e}")).await;
                return;
            }
        };
        loop {
            let incoming = n0_future::future::or(endpoint.accept(), async {
                let _ = stopped.recv().await;
                None
            })
            .await;
            let Some(incoming) = incoming else { break };
            let accepted_tx = accepted_tx.clone();
            spawn_local(async move {
                let connection = match incoming.await {
                    Ok(c) => c,
                    Err(e) => return debug!("incoming connection failed: {e}"),
                };
                if connection.alpn() != ALPN {
                    connection.close(0u32.into(), b"unexpected ALPN");
                    return;
                }
                match handshake(connection).await {
                    Ok((link, done)) => {
                        if accepted_tx.send(link).await.is_ok() {
                            done.await;
                        }
                    }
                    Err(e) => debug!("client handshake failed: {e}"),
                }
            });
        }
        endpoint.close().await;
    });
    IrohServer {
        accepted,
        failed,
        next_id: 0,
        _stop: stop,
    }
}

/// `bevy_replicon_iroh`'s handshake (the same on both sides): open a uni
/// stream and mark it, then accept the peer's and check its marker. Returns
/// the link plus a future that ends once the connection has closed.
async fn handshake(
    connection: Connection,
) -> Result<(Link, impl Future<Output = ()> + 'static), String> {
    let err = |e: &dyn std::fmt::Display| format!("{e}");
    let mut send = connection.open_uni().await.map_err(|e| err(&e))?;
    send.set_priority(STREAM_PRIORITY).map_err(|e| err(&e))?;
    send.write_all(&[STREAM_MARKER])
        .await
        .map_err(|e| err(&e))?;
    let mut recv = connection.accept_uni().await.map_err(|e| err(&e))?;
    let mut marker = [0u8; 1];
    recv.read_exact(&mut marker).await.map_err(|e| err(&e))?;
    if marker[0] != STREAM_MARKER {
        connection.close(0u32.into(), b"bad stream marker");
        return Err("the other side runs an incompatible version".into());
    }

    let (in_tx, incoming) = unbounded();
    let (outgoing, out_rx) = unbounded::<(u8, Bytes, bool)>();
    let (closed_tx, closed) = unbounded();

    spawn_local(read_frames(recv, in_tx.clone()));
    {
        let connection = connection.clone();
        spawn_local(async move {
            while let Ok(datagram) = connection.read_datagram().await {
                if let Some((&channel, payload)) = datagram.split_first()
                    && in_tx
                        .send((channel, Bytes::copy_from_slice(payload)))
                        .await
                        .is_err()
                {
                    break;
                }
            }
        });
    }
    spawn_local(write_frames(connection.clone(), send, out_rx));

    let max_datagram = connection.max_datagram_size().unwrap_or(1200);
    let done = async move {
        let reason = connection.closed().await;
        let _ = closed_tx.send(format!("{reason}")).await;
    };
    Ok((
        Link {
            incoming,
            outgoing,
            closed,
            max_datagram,
        },
        done,
    ))
}

async fn read_frames(mut recv: RecvStream, tx: Sender<(u8, Bytes)>) {
    loop {
        let mut header = [0u8; 5];
        if recv.read_exact(&mut header).await.is_err() {
            return;
        }
        let (channel, len) = parse_header(header);
        let mut payload = vec![0u8; len];
        if len > 0 && recv.read_exact(&mut payload).await.is_err() {
            return;
        }
        if tx.send((channel, Bytes::from(payload))).await.is_err() {
            return;
        }
    }
}

/// Sends everything Bevy queues. When Bevy drops the link (leaving, or the
/// host kicking this client) the queue closes and so does the connection.
async fn write_frames(
    connection: Connection,
    mut send: SendStream,
    rx: Receiver<(u8, Bytes, bool)>,
) {
    while let Ok((channel, bytes, unreliable)) = rx.recv().await {
        if unreliable {
            let datagram = Bytes::from(encode_datagram(channel, &bytes));
            if let Err(e) = connection.send_datagram(datagram) {
                debug!("failed to send datagram: {e}");
            }
        } else {
            let header = frame_header(channel, bytes.len());
            if send.write_all(&header).await.is_err()
                || (!bytes.is_empty() && send.write_all(&bytes).await.is_err())
            {
                break;
            }
        }
    }
    connection.close(0u32.into(), b"disconnecting");
}

// --------------------------------------------------------------- Bevy side

fn client_started(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Connecting);
}

fn client_stopped(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Disconnected);
}

fn client_receive(
    mut commands: Commands,
    mut client: ResMut<IrohClient>,
    mut messages: ResMut<ClientMessages>,
    mut state: ResMut<NextState<ClientState>>,
    mut status: ResMut<NetStatus>,
) {
    if client.link.is_none() {
        match client.pending.try_recv() {
            Ok(Ok(link)) => {
                info!("connected to the host through iroh");
                client.link = Some(link);
                state.set(ClientState::Connected);
                status.message = "Connected!".into();
            }
            Ok(Err(e)) => {
                status.message = format!("Could not join: {e}");
                commands.remove_resource::<IrohClient>();
                return;
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Closed) => {
                commands.remove_resource::<IrohClient>();
                return;
            }
        }
    }
    let Some(link) = &client.link else { return };
    if let Ok(reason) = link.closed.try_recv() {
        debug!("host connection closed: {reason}");
        commands.remove_resource::<IrohClient>();
    }
    while let Ok((channel, bytes)) = link.incoming.try_recv() {
        messages.insert_received(channel as usize, bytes);
    }
}

fn client_send(
    channels: Res<RepliconChannels>,
    client: Res<IrohClient>,
    mut messages: ResMut<ClientMessages>,
) {
    let kinds = channels.client_channels();
    for (channel, bytes) in messages.drain_sent() {
        let Some(link) = &client.link else { continue };
        let unreliable = matches!(kinds.get(channel), Some(Channel::Unreliable));
        let _ = link.outgoing.try_send((channel as u8, bytes, unreliable));
    }
}

fn server_started(mut state: ResMut<NextState<ServerState>>) {
    state.set(ServerState::Running);
}

fn server_stopped(mut state: ResMut<NextState<ServerState>>) {
    state.set(ServerState::Stopped);
}

fn server_receive(
    mut commands: Commands,
    mut server: ResMut<IrohServer>,
    mut messages: ResMut<ServerMessages>,
    mut status: ResMut<NetStatus>,
    clients: Query<(Entity, &WebLink)>,
) {
    if let Ok(e) = server.failed.try_recv() {
        // The world keeps running (solo), friends just can't reach it.
        status.message = format!("Could not host: {e}. Playing solo.");
        status.join_code = None;
    }
    while let Ok(link) = server.accepted.try_recv() {
        server.next_id += 1;
        let client = commands
            .spawn((
                ConnectedClient {
                    max_size: link.max_datagram,
                },
                NetworkId::new(server.next_id),
                WebLink(link),
            ))
            .id();
        info!("a friend connected (client {client})");
    }
    for (entity, WebLink(link)) in &clients {
        if link.closed.try_recv().is_ok() {
            commands.entity(entity).despawn();
            continue;
        }
        while let Ok((channel, bytes)) = link.incoming.try_recv() {
            messages.insert_received(entity, channel as usize, bytes);
        }
    }
}

fn server_send(
    channels: Res<RepliconChannels>,
    mut messages: ResMut<ServerMessages>,
    clients: Query<&WebLink>,
) {
    let kinds = channels.server_channels();
    for (entity, channel, bytes) in messages.drain_sent() {
        let Ok(WebLink(link)) = clients.get(entity) else {
            continue;
        };
        let unreliable = matches!(kinds.get(channel), Some(Channel::Unreliable));
        let _ = link.outgoing.try_send((channel as u8, bytes, unreliable));
    }
}

fn disconnect_by_request(mut commands: Commands, mut requests: MessageReader<DisconnectRequest>) {
    for request in requests.read() {
        if let Ok(mut e) = commands.get_entity(request.client) {
            e.despawn();
        }
    }
}

fn enter_game(mut next: ResMut<NextState<AppState>>) {
    next.set(AppState::InGame);
}

fn back_to_menu(
    mut commands: Commands,
    replicated: Query<Entity, With<Replicated>>,
    mut next: ResMut<NextState<AppState>>,
    mut status: ResMut<NetStatus>,
) {
    // Passive disconnects (host quit, connection dropped) land here.
    commands.queue(teardown_transport);
    commands.remove_resource::<crate::player::LocalPlayer>();
    for e in &replicated {
        commands.entity(e).despawn();
    }
    status.message = "Disconnected.".into();
    next.set(AppState::Menu);
}

// ---------------------------------------------------------------- clipboard

/// Ctrl/Cmd+V on the page: the browser hands us the text in a `paste` event.
fn listen_for_paste(clipboard: Res<Clipboard>) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let slot = clipboard.pasted.clone();
    let on_paste = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(
        move |event: web_sys::ClipboardEvent| {
            let text = event.clipboard_data().and_then(|d| d.get_data("text").ok());
            if let (Some(text), Ok(mut slot)) = (text, slot.lock()) {
                *slot = Some(text);
            }
        },
    );
    if document
        .add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref())
        .is_err()
    {
        warn!("could not listen for paste events");
    }
    // Lives as long as the page.
    on_paste.forget();
}

fn deliver_paste(clipboard: Res<Clipboard>, code: Option<ResMut<JoinCode>>) {
    let Some(mut code) = code else { return };
    let Some(text) = clipboard.pasted.lock().ok().and_then(|mut s| s.take()) else {
        return;
    };
    code.paste(&text);
}
