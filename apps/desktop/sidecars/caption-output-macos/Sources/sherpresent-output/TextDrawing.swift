import CoreGraphics
import CoreText
import Foundation

/// One line of text, measured.
struct TextLabel {
    let line: CTLine
    let width: CGFloat
    let capHeight: CGFloat
}

/// Single-line text and badges for the opaque contents (notes, QLab), which
/// share the stage view's look.
protocol TextDrawing {}

extension TextDrawing {
    /// Cut to `maxWidth` with an ellipsis if it doesn't fit.
    func label(_ text: String, _ font: CTFont, _ color: CGColor, maxWidth: CGFloat = .infinity)
        -> TextLabel
    {
        let attrs: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
        ]
        var line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attrs))
        if CTLineGetTypographicBounds(line, nil, nil, nil) > maxWidth {
            let ellipsis = CTLineCreateWithAttributedString(
                NSAttributedString(string: "…", attributes: attrs))
            line = CTLineCreateTruncatedLine(line, Double(maxWidth), .end, ellipsis) ?? line
        }
        return TextLabel(
            line: line, width: CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)),
            capHeight: CTFontGetCapHeight(font))
    }

    /// Draw with the cap height centred on `midY`, so labels of any case sit
    /// visually centred in their bar or badge.
    func draw(_ l: TextLabel, x: CGFloat, midY: CGFloat, in ctx: CGContext) {
        ctx.textPosition = CGPoint(x: x.rounded(), y: (midY - l.capHeight / 2).rounded())
        CTLineDraw(l.line, ctx)
    }

    /// How wide `pill` draws `text` at `size`.
    func pillWidth(_ text: String, size: CGFloat) -> CGFloat {
        label(text, font(size, bold: true), rgb(0)).width + 1.8 * size
    }

    /// A capsule label, like the stage view's badges. Placed by its left or
    /// right edge or its centre.
    func pill(
        _ text: String, size: CGFloat, fill: CGColor, text color: CGColor,
        left: CGFloat? = nil, right: CGFloat? = nil, centerX: CGFloat? = nil, midY: CGFloat,
        in ctx: CGContext
    ) {
        let l = label(text, font(size, bold: true), color)
        let boxW = l.width + 1.8 * size
        let boxH = 1.8 * size
        let x = left ?? right.map { $0 - boxW } ?? (centerX ?? 0) - boxW / 2
        ctx.setFillColor(fill)
        ctx.addPath(
            CGPath(
                roundedRect: CGRect(x: x, y: midY - boxH / 2, width: boxW, height: boxH),
                cornerWidth: boxH / 2, cornerHeight: boxH / 2, transform: nil))
        ctx.fillPath()
        draw(l, x: x + 0.9 * size, midY: midY, in: ctx)
    }

    func font(_ size: CGFloat, bold: Bool = false, italic: Bool = false, monoDigits: Bool = false)
        -> CTFont
    {
        var f =
            CTFontCreateUIFontForLanguage(bold ? .emphasizedSystem : .system, size, nil)
            ?? CTFontCreateWithName("Helvetica" as CFString, size, nil)
        if italic, let i = CTFontCreateCopyWithSymbolicTraits(f, size, nil, .traitItalic, .traitItalic) {
            f = i
        }
        if monoDigits {
            // Tabular figures, so a ticking timer or slide counter doesn't
            // jitter sideways as the digits change.
            let feature: [String: Any] = [
                kCTFontFeatureTypeIdentifierKey as String: kNumberSpacingType,
                kCTFontFeatureSelectorIdentifierKey as String: kMonospacedNumbersSelector,
            ]
            let desc = CTFontDescriptorCreateWithAttributes(
                [kCTFontFeatureSettingsAttribute as String: [feature]] as CFDictionary)
            f = CTFontCreateCopyWithAttributes(f, size, nil, desc)
        }
        return f
    }
}
