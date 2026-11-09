//! The browser transport (`src/net_web.rs`, #106) re-implements
//! `bevy_replicon_iroh`'s wire format so browsers and desktops can play
//! together. If a dependency bump changes these, update `net_web.rs` too.

#[test]
fn desktop_transport_constants_match_the_browser_copy() {
    assert_eq!(bevy_replicon_iroh::ALPN, b"bevy-replicon-iroh/0");
    assert_eq!(bevy_replicon_iroh::frame::STREAM_MARKER, 0xB7);
    assert_eq!(bevy_replicon_iroh::REPLICON_STREAM_PRIORITY, 100);
}
