//! `bevy_replicon_iroh`'s wire format, restated for the browser transport
//! (`net_web.rs`, #106) which can't use that crate (it needs tokio threads).
//! Desktop and browser peers must agree byte for byte, so the native tests
//! below check these constants against the crate itself.
//!
//! Per connection: each side opens one uni stream, writes [`STREAM_MARKER`],
//! accepts the peer's uni stream and reads its marker. Reliable channels then
//! go as `[channel_id: u8][len: u32 le][payload]` frames on that stream;
//! unreliable channels as `[channel_id: u8][payload]` QUIC datagrams.

/// ALPN of `bevy_replicon_iroh` 0.20 (`bevy_replicon_iroh::ALPN`).
pub const ALPN: &[u8] = b"bevy-replicon-iroh/0";
/// First byte of every replicon stream (`bevy_replicon_iroh::frame::STREAM_MARKER`).
pub const STREAM_MARKER: u8 = 0xB7;
/// Stream priority for replicon traffic (`bevy_replicon_iroh::REPLICON_STREAM_PRIORITY`).
pub const STREAM_PRIORITY: i32 = 100;

/// The 5-byte header in front of a stream frame's payload.
pub fn frame_header(channel_id: u8, len: usize) -> [u8; 5] {
    let mut header = [0u8; 5];
    header[0] = channel_id;
    header[1..5].copy_from_slice(&(len as u32).to_le_bytes());
    header
}

/// Channel id and payload length from a stream frame header.
pub fn parse_header(header: [u8; 5]) -> (u8, usize) {
    let len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
    (header[0], len as usize)
}

/// A datagram: `[channel_id][payload]`.
pub fn encode_datagram(channel_id: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(channel_id);
    buf.extend_from_slice(payload);
    buf
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn constants_match_bevy_replicon_iroh() {
        assert_eq!(ALPN, bevy_replicon_iroh::ALPN);
        assert_eq!(STREAM_MARKER, bevy_replicon_iroh::frame::STREAM_MARKER);
        assert_eq!(
            STREAM_PRIORITY,
            bevy_replicon_iroh::REPLICON_STREAM_PRIORITY
        );
    }

    #[test]
    fn header_roundtrips() {
        let h = frame_header(7, 70_000);
        assert_eq!(h, [7, 0x70, 0x11, 0x01, 0x00]);
        assert_eq!(parse_header(h), (7, 70_000));
        assert_eq!(encode_datagram(3, b"hi"), vec![3, b'h', b'i']);
    }
}
