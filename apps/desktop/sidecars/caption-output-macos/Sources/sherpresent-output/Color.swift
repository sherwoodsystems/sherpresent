import CoreGraphics

/// Opaque sRGB colour from `0xRRGGBB`.
func rgb(_ hex: UInt32) -> CGColor {
    CGColor(
        srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

/// `0xRRGGBB` from a CSS `#rgb` / `#rrggbb` string (Rust has already
/// sanitized it to one of those), or nil.
func hexColor(_ css: String) -> UInt32? {
    var hex = css.hasPrefix("#") ? String(css.dropFirst()) : css
    if hex.count == 3 { hex = hex.map { "\($0)\($0)" }.joined() }
    guard hex.count == 6 else { return nil }
    return UInt32(hex, radix: 16)
}
