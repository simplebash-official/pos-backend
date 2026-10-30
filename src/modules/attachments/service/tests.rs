use super::*;

#[test]
fn photos_are_recognised_by_their_bytes() {
    assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]), Some("image/jpeg"));
    assert_eq!(
        sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
        Some("image/png")
    );
    assert_eq!(sniff(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("image/webp"));
    assert_eq!(sniff(b"%PDF-1.4"), None);
    assert_eq!(sniff(b"<svg></svg>"), None);
}
