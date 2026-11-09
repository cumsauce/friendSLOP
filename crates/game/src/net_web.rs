//! Browser stand-in for `net.rs` (#106). Same public API, same wire protocol.
//!
//! `bevy_replicon_iroh` drives iroh through a tokio runtime and `block_on`s
//! every tick, which a browser's single main thread can't do. This module
//! talks to iroh directly instead, with `wasm_bindgen_futures::spawn_local`
//! tasks feeding channels that the Bevy systems drain each frame. It speaks
//! `bevy_replicon_iroh`'s exact format (ALPN, stream marker, frames and
//! datagrams), so a browser can join a desktop host and vice versa.
//!
//! Browsers can't send UDP, so every browser connection goes through iroh's
//! free public relays (the `N0` preset): a bit more latency, nothing to run.
//!
//! Hosting starts the session straight away (this peer is the server, like a
//! desktop host before friends join); the endpoint comes online a moment
//! later and the join code appears then. If it can't, you just play solo.

use std::str::FromStr;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::channels::Channel;
use bevy_replicon::shared::backend::connected_client::{NetworkId, NetworkIdMap};
use bytes::Bytes;
use iroh::endpoint::{Connection, Endpoint, RecvStream, SendStream, presets};
use iroh::{EndpointAddr, EndpointId, SecretKey};

use crate::AppState;

/// `bevy_replicon_iroh::ALPN`: must match the desktop build byte for byte.
const ALPN: &[u8] = b"bevy-replicon-iroh/0";
/// `bevy_replicon_iroh::frame::STREAM_MARKER`.
const STREAM_MARKER: u8 = 0xB7;
/// `bevy_replicon_iroh::REPLICON_STREAM_PRIORITY`.
const STREAM_PRIORITY: i32 = 100;
/// How long a join may take before we give up (relay lookup + handshake).
const JOIN_TIMEOUT_SECS: f32 = 25.0;

/// Ask the network layer to host or join.
#[derive(Message, Clone, Debug)]
pub enum NetCommand {
    Host,
    Join(String),
}

/// What the UI shows about the connection.
#[derive(Resource, Default, Debug)]
pub struct NetStatus {
    pub join_code: Option<String>,
    pub message: String,
}

/// Copying uses the browser's async clipboard (works on https/localhost).
/// Reading it needs a permission prompt per paste, so paste does nothing:
/// phones join by opening the host's link (`?join=<code>`) instead.
#[derive(Resource, Default)]
pub struct Clipboard;

impl Clipboard {
    pub fn set(&self, text: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        // The returned promise is ignored: a refusal just means no copy.
        let _ = window.navigator().clipboard().write_text(text);
    }
    pub fn get(&self) -> Option<String> {
        None
    }
}

/// A live connection to a host (inserted once the handshake is done).
#[derive(Resource)]
pub struct IrohClient {
    endpoint: Endpoint,
    connection: Connection,
    out: async_channel::Sender<(u8, Bytes)>,
    incoming: async_channel::Receiver<(u8, Bytes)>,
    closed: async_channel::Receiver<()>,
}

impl IrohClient {
    fn close(self) {
        self.connection.close(0u32.into(), b"disconnecting");
        let endpoint = self.endpoint;
        wasm_bindgen_futures::spawn_local(async move { endpoint.close().await });
    }
}

/// This browser is hosting and its endpoint is online.
#[derive(Resource)]
struct WebServer {
    endpoint: Endpoint,
    accepted: async_channel::Receiver<Peer>,
    disconnected: async_channel::Receiver<NetworkId>,
}

/// Results of the async work, delivered to the Bevy side.
enum Outcome {
    Hosted(Endpoint),
    HostFailed(String),
    Joined(IrohClient),
    JoinFailed(String),
}

/// Async work in flight. `generation` lets a teardown (Esc, a new host/join,
/// a timeout) disown whatever is still connecting.
#[derive(Resource)]
struct Pending {
    tx: async_channel::Sender<(u32, Outcome)>,
    rx: async_channel::Receiver<(u32, Outcome)>,
    generation: u32,
    joining_for: Option<f32>,
}

impl Default for Pending {
    fn default() -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self {
            tx,
            rx,
            generation: 0,
            joining_for: None,
        }
    }
}

/// A client that finished the handshake with this host.
struct Peer {
    network_id: NetworkId,
    connection: Connection,
    out: async_channel::Sender<(u8, Bytes)>,
    incoming: async_channel::Receiver<(u8, Bytes)>,
}

#[derive(Component)]
struct PeerLink {
    connection: Connection,
    out: async_channel::Sender<(u8, Bytes)>,
    incoming: async_channel::Receiver<(u8, Bytes)>,
}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetStatus>()
            .init_resource::<Clipboard>()
            .init_resource::<Pending>()
            .add_message::<NetCommand>()
            .add_observer(close_departed_peer)
            .add_systems(Startup, join_from_url)
            .add_systems(Update, (handle_net_commands, finish_pending).chain())
            .add_systems(
                PreUpdate,
                (
                    client_connected.run_if(resource_added::<IrohClient>),
                    client_disconnected.run_if(resource_removed::<IrohClient>),
                    (client_check_closed, client_receive)
                        .chain()
                        .run_if(resource_exists::<IrohClient>),
                )
                    .in_set(ClientSystems::ReceivePackets),
            )
            .add_systems(
                PreUpdate,
                (server_accept, server_check_disconnects, server_receive)
                    .chain()
                    .run_if(resource_exists::<WebServer>)
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
                        .run_if(resource_exists::<WebServer>),
                    disconnect_by_request,
                ),
            )
            .add_systems(OnEnter(ClientState::Connected), enter_game)
            .add_systems(OnExit(ClientState::Connected), back_to_menu);
    }
}

/// Stop hosting or leave the host, and disown anything still connecting.
/// No-op when nothing is running.
pub fn teardown_transport(world: &mut World) {
    if let Some(mut pending) = world.get_resource_mut::<Pending>() {
        pending.generation = pending.generation.wrapping_add(1);
        pending.joining_for = None;
    }
    if let Some(client) = world.remove_resource::<IrohClient>() {
        client.close();
    }
    if let Some(server) = world.remove_resource::<WebServer>() {
        wasm_bindgen_futures::spawn_local(async move { server.endpoint.close().await });
    }
    let peers: Vec<Entity> = world
        .query_filtered::<Entity, With<PeerLink>>()
        .iter(world)
        .collect();
    for peer in peers {
        world.despawn(peer);
    }
    if let Some(mut next) = world.get_resource_mut::<NextState<ServerState>>() {
        next.set(ServerState::Stopped);
    }
}

/// `index.html?join=<code>` joins straight away: the link a browser host shares.
fn join_from_url(mut commands: MessageWriter<NetCommand>) {
    let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) else {
        return;
    };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return;
    };
    if let Some(code) = params.get("join").filter(|c| !c.trim().is_empty()) {
        commands.write(NetCommand::Join(code));
    } else if params.has("host") {
        commands.write(NetCommand::Host);
    }
}

/// The page's own address with `?join=<code>`, for friends to open.
fn share_link(code: &str) -> String {
    let base = web_sys::window()
        .and_then(|w| {
            let loc = w.location();
            Some(format!("{}{}", loc.origin().ok()?, loc.pathname().ok()?))
        })
        .unwrap_or_default();
    format!("{base}?join={code}")
}

fn handle_net_commands(mut commands: Commands, mut requests: MessageReader<NetCommand>) {
    for request in requests.read() {
        // Same as the desktop: a new host/join replaces whatever was running.
        commands.queue(teardown_transport);
        let request = request.clone();
        commands.queue(move |world: &mut World| begin(world, request));
    }
}

/// Start hosting or joining (after the teardown, so its generation is current).
fn begin(world: &mut World, request: NetCommand) {
    let Some(pending) = world.get_resource::<Pending>() else {
        return;
    };
    let generation = pending.generation;
    let tx = pending.tx.clone();
    match request {
        NetCommand::Host => {
            if let Some(mut next) = world.get_resource_mut::<NextState<ServerState>>() {
                next.set(ServerState::Running);
            }
            if let Some(mut next) = world.get_resource_mut::<NextState<AppState>>() {
                next.set(AppState::InGame);
            }
            if let Some(mut status) = world.get_resource_mut::<NetStatus>() {
                status.join_code = None;
                status.message = "Hosting! Getting a join code from the relay...".into();
            }
            wasm_bindgen_futures::spawn_local(async move {
                let outcome = match bind().await {
                    Ok(endpoint) => {
                        endpoint.online().await;
                        Outcome::Hosted(endpoint)
                    }
                    Err(e) => Outcome::HostFailed(e),
                };
                let _ = tx.send((generation, outcome)).await;
            });
        }
        NetCommand::Join(code) => {
            let id = match EndpointId::from_str(code.trim()) {
                Ok(id) => id,
                Err(e) => {
                    say(world, format!("That join code doesn't look right: {e}"));
                    return;
                }
            };
            say(world, "Connecting through the relay...".into());
            if let Some(mut pending) = world.get_resource_mut::<Pending>() {
                pending.joining_for = Some(0.0);
            }
            wasm_bindgen_futures::spawn_local(async move {
                let outcome = match connect(id).await {
                    Ok(client) => Outcome::Joined(client),
                    Err(e) => Outcome::JoinFailed(e),
                };
                let _ = tx.send((generation, outcome)).await;
            });
        }
    }
}

fn say(world: &mut World, message: String) {
    if let Some(mut status) = world.get_resource_mut::<NetStatus>() {
        status.message = message;
    }
}

/// Pick up finished async work, and give up on a join that takes too long.
fn finish_pending(
    mut commands: Commands,
    time: Res<Time>,
    server_state: Res<State<ServerState>>,
    clipboard: Res<Clipboard>,
    mut pending: ResMut<Pending>,
    mut status: ResMut<NetStatus>,
) {
    while let Ok((generation, outcome)) = pending.rx.try_recv() {
        if generation != pending.generation {
            // Disowned: close whatever it produced.
            match outcome {
                Outcome::Hosted(endpoint) => {
                    wasm_bindgen_futures::spawn_local(async move { endpoint.close().await });
                }
                Outcome::Joined(client) => client.close(),
                _ => {}
            }
            continue;
        }
        match outcome {
            Outcome::Hosted(endpoint) if *server_state.get() == ServerState::Running => {
                let code = endpoint.id().to_string();
                let link = share_link(&code);
                info!("hosting, join code: {code}, link: {link}");
                clipboard.set(&link);
                status.message = format!("Hosting! Friends can open {link}");
                status.join_code = Some(code);
                commands.insert_resource(start_accepting(endpoint));
            }
            Outcome::Hosted(endpoint) => {
                wasm_bindgen_futures::spawn_local(async move { endpoint.close().await });
            }
            Outcome::HostFailed(e) => {
                status.message = format!("Could not reach the relay ({e}). Playing solo.");
            }
            Outcome::Joined(client) => {
                pending.joining_for = None;
                status.message = "Connected!".into();
                commands.insert_resource(client);
            }
            Outcome::JoinFailed(e) => {
                pending.joining_for = None;
                status.message = format!("Could not join: {e}");
            }
        }
    }
    if let Some(waited) = pending.joining_for.as_mut() {
        *waited += time.delta_secs();
        if *waited >= JOIN_TIMEOUT_SECS {
            pending.generation = pending.generation.wrapping_add(1);
            pending.joining_for = None;
            status.message =
                "Could not reach that host (timed out). Check the code and try again.".into();
        }
    }
}

async fn bind() -> Result<Endpoint, String> {
    Endpoint::builder(presets::N0)
        .secret_key(SecretKey::generate())
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| e.to_string())
}

async fn connect(id: EndpointId) -> Result<IrohClient, String> {
    let endpoint = bind().await?;
    let result = handshake_with_host(&endpoint, id).await;
    match result {
        Ok((connection, out, incoming, closed)) => Ok(IrohClient {
            endpoint,
            connection,
            out,
            incoming,
            closed,
        }),
        Err(e) => {
            endpoint.close().await;
            Err(e)
        }
    }
}

type Links = (
    Connection,
    async_channel::Sender<(u8, Bytes)>,
    async_channel::Receiver<(u8, Bytes)>,
    async_channel::Receiver<()>,
);

/// Client side of `bevy_replicon_iroh`'s handshake: we open our stream first.
async fn handshake_with_host(endpoint: &Endpoint, id: EndpointId) -> Result<Links, String> {
    let connection = endpoint
        .connect(EndpointAddr::new(id), ALPN)
        .await
        .map_err(|e| e.to_string())?;
    let send = open_marked(&connection).await?;
    let recv = accept_marked(&connection).await?;
    let (out, incoming) = spawn_pumps(&connection, send, recv);
    let (closed_tx, closed) = async_channel::bounded(1);
    let watch = connection.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let reason = watch.closed().await;
        debug!("host connection closed: {reason}");
        let _ = closed_tx.send(()).await;
    });
    Ok((connection, out, incoming, closed))
}

async fn open_marked(connection: &Connection) -> Result<SendStream, String> {
    let mut send = connection.open_uni().await.map_err(|e| e.to_string())?;
    let _ = send.set_priority(STREAM_PRIORITY);
    send.write_all(&[STREAM_MARKER])
        .await
        .map_err(|e| e.to_string())?;
    Ok(send)
}

async fn accept_marked(connection: &Connection) -> Result<RecvStream, String> {
    let mut recv = connection.accept_uni().await.map_err(|e| e.to_string())?;
    let mut marker = [0u8; 1];
    recv.read_exact(&mut marker)
        .await
        .map_err(|e| e.to_string())?;
    if marker[0] != STREAM_MARKER {
        return Err("the other side runs an incompatible version".into());
    }
    Ok(recv)
}

/// Background tasks for one connection: a writer draining `out` onto the
/// stream, a reader for the stream and one for datagrams, both into `incoming`.
fn spawn_pumps(
    connection: &Connection,
    mut send: SendStream,
    mut recv: RecvStream,
) -> (
    async_channel::Sender<(u8, Bytes)>,
    async_channel::Receiver<(u8, Bytes)>,
) {
    let (out, out_rx) = async_channel::unbounded::<(u8, Bytes)>();
    let (in_tx, incoming) = async_channel::unbounded();
    wasm_bindgen_futures::spawn_local(async move {
        while let Ok((channel, payload)) = out_rx.recv().await {
            if send
                .write_all(&encode_frame(channel, &payload))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let tx = in_tx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(frame) = read_frame(&mut recv).await {
            if tx.send(frame).await.is_err() {
                break;
            }
        }
    });
    let datagrams = connection.clone();
    wasm_bindgen_futures::spawn_local(async move {
        while let Ok(datagram) = datagrams.read_datagram().await {
            if let Some(frame) = decode_datagram(&datagram)
                && in_tx.send(frame).await.is_err()
            {
                break;
            }
        }
    });
    (out, incoming)
}

/// `[channel_id: u8][len: u32 le][payload]`, as `bevy_replicon_iroh::frame`.
fn encode_frame(channel: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.push(channel);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

async fn read_frame(recv: &mut RecvStream) -> Option<(u8, Bytes)> {
    let mut header = [0u8; 5];
    recv.read_exact(&mut header).await.ok()?;
    let len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let mut payload = vec![0u8; len];
    if len > 0 {
        recv.read_exact(&mut payload).await.ok()?;
    }
    Some((header[0], Bytes::from(payload)))
}

/// `[channel_id: u8][payload]`.
fn encode_datagram(channel: u8, payload: &[u8]) -> Bytes {
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(channel);
    buf.extend_from_slice(payload);
    Bytes::from(buf)
}

fn decode_datagram(datagram: &Bytes) -> Option<(u8, Bytes)> {
    let channel = *datagram.first()?;
    Some((channel, datagram.slice(1..)))
}

/// Accept friends on a hosting endpoint (server side of the handshake: the
/// host opens its stream first, then accepts the client's).
fn start_accepting(endpoint: Endpoint) -> WebServer {
    let (accepted_tx, accepted) = async_channel::unbounded();
    let (gone_tx, disconnected) = async_channel::unbounded();
    let listener = endpoint.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let mut next_id = 0u64;
        while let Some(incoming) = listener.accept().await {
            let network_id = NetworkId::new(next_id);
            next_id += 1;
            let accepted_tx = accepted_tx.clone();
            let gone_tx = gone_tx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                if connection.alpn() != ALPN {
                    connection.close(0u32.into(), b"unknown protocol");
                    return;
                }
                let handshake = async {
                    let send = open_marked(&connection).await?;
                    let recv = accept_marked(&connection).await?;
                    Ok::<_, String>((send, recv))
                };
                let (send, recv) = match handshake.await {
                    Ok(streams) => streams,
                    Err(e) => {
                        warn!("a friend's handshake failed: {e}");
                        return;
                    }
                };
                let (out, incoming) = spawn_pumps(&connection, send, recv);
                let watch = connection.clone();
                let peer = Peer {
                    network_id,
                    connection,
                    out,
                    incoming,
                };
                if accepted_tx.send(peer).await.is_err() {
                    return;
                }
                watch.closed().await;
                let _ = gone_tx.send(network_id).await;
            });
        }
    });
    WebServer {
        endpoint,
        accepted,
        disconnected,
    }
}

fn client_connected(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Connected);
}

fn client_disconnected(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Disconnected);
}

fn client_check_closed(mut commands: Commands, client: Res<IrohClient>) {
    if client.closed.try_recv().is_ok() {
        commands.queue(teardown_transport);
    }
}

fn client_receive(client: Res<IrohClient>, mut messages: ResMut<ClientMessages>) {
    while let Ok((channel, bytes)) = client.incoming.try_recv() {
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
        send_one(
            &client.connection,
            &client.out,
            kinds.get(channel),
            channel,
            &bytes,
        );
    }
}

fn send_one(
    connection: &Connection,
    out: &async_channel::Sender<(u8, Bytes)>,
    kind: Option<&Channel>,
    channel: usize,
    bytes: &Bytes,
) {
    if matches!(kind, Some(Channel::Unreliable)) {
        let _ = connection.send_datagram(encode_datagram(channel as u8, bytes));
    } else {
        let _ = out.try_send((channel as u8, bytes.clone()));
    }
}

fn server_accept(mut commands: Commands, server: Res<WebServer>) {
    while let Ok(peer) = server.accepted.try_recv() {
        let max_size = peer.connection.max_datagram_size().unwrap_or(1200);
        commands.spawn((
            ConnectedClient { max_size },
            peer.network_id,
            PeerLink {
                connection: peer.connection,
                out: peer.out,
                incoming: peer.incoming,
            },
        ));
    }
}

fn server_check_disconnects(
    mut commands: Commands,
    server: Res<WebServer>,
    network_map: Res<NetworkIdMap>,
) {
    while let Ok(network_id) = server.disconnected.try_recv() {
        if let Some(&client) = network_map.get(&network_id) {
            commands.entity(client).try_despawn();
        }
    }
}

fn server_receive(mut messages: ResMut<ServerMessages>, peers: Query<(Entity, &PeerLink)>) {
    for (client, link) in &peers {
        while let Ok((channel, bytes)) = link.incoming.try_recv() {
            messages.insert_received(client, channel as usize, bytes);
        }
    }
}

fn server_send(
    channels: Res<RepliconChannels>,
    mut messages: ResMut<ServerMessages>,
    peers: Query<&PeerLink>,
) {
    let kinds = channels.server_channels();
    for (client, channel, bytes) in messages.drain_sent() {
        let Ok(link) = peers.get(client) else {
            continue;
        };
        send_one(
            &link.connection,
            &link.out,
            kinds.get(channel),
            channel,
            &bytes,
        );
    }
}

fn disconnect_by_request(mut commands: Commands, mut requests: MessageReader<DisconnectRequest>) {
    for request in requests.read() {
        commands.entity(request.client).try_despawn();
    }
}

fn close_departed_peer(remove: On<Remove<ConnectedClient>>, peers: Query<&PeerLink>) {
    if let Ok(link) = peers.get(remove.entity) {
        link.connection.close(0u32.into(), b"disconnected");
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
    // Passive disconnects (host quit, connection dropped) land here too.
    commands.queue(teardown_transport);
    commands.remove_resource::<crate::player::LocalPlayer>();
    for e in &replicated {
        commands.entity(e).despawn();
    }
    status.message = "Disconnected.".into();
    next.set(AppState::Menu);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_the_desktop_format() {
        assert_eq!(encode_frame(3, b"hi"), vec![3, 2, 0, 0, 0, b'h', b'i']);
        let d = encode_datagram(7, b"yo");
        assert_eq!(decode_datagram(&d), Some((7, Bytes::from_static(b"yo"))));
        assert_eq!(decode_datagram(&Bytes::new()), None);
    }
}
