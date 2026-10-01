import CoreText
import Foundation

/// One wrapped row of caption text, ready to draw.
struct Row {
    let line: CTLine
    let width: CGFloat
    let height: CGFloat
    /// The smaller original-language text shown above a translation.
    var isSource = false
}

/// Turns caption texts into the rows that fit on screen, matching the web
/// overlay's CSS: `--size` px at 1920 wide, `line-height: 1.22`, a block
/// `--width`% of the frame, greedy wrapping, and a window exactly
/// `maxLines` main rows tall that keeps the newest rows. In `both` mode the
/// original sits above each line at `.source`'s 0.62em, weight 500.
struct CaptionLayout {
    static let leading: CGFloat = 1.22
    static let sourceScale: CGFloat = 0.62

    let frameWidth = CGFloat(Args.width)
    let frameHeight = CGFloat(Args.height)
    let settings: OverlaySettings
    let font: CTFont
    let sourceFont: CTFont

    init(settings: OverlaySettings) {
        self.settings = settings
        let size = CGFloat(settings.fontSize) * CGFloat(Args.width) / 1920
        self.font = Self.makeFont(size: size, faces: ["Inter-Bold", "HelveticaNeue-Bold"])
        self.sourceFont = Self.makeFont(
            size: size * Self.sourceScale, faces: ["Inter-Medium", "HelveticaNeue-Medium"])
    }

    var fontSize: CGFloat { CGFloat(settings.fontSize) * frameWidth / 1920 }
    var rowHeight: CGFloat { fontSize * Self.leading }
    var blockWidth: CGFloat { frameWidth * CGFloat(settings.width) / 100 }
    var bottomInset: CGFloat { frameHeight * CGFloat(settings.safeArea) / 100 }

    /// The overlay's font stack is Inter, then Helvetica Neue, at weight 650
    /// (bold is the nearest static face) or 500 for the original.
    private static func makeFont(size: CGFloat, faces: [String]) -> CTFont {
        for name in faces {
            let f = CTFontCreateWithName(name as CFString, size, nil)
            if (CTFontCopyPostScriptName(f) as String) == name { return f }
        }
        return CTFontCreateUIFontForLanguage(.emphasizedSystem, size, nil)
            ?? CTFontCreateWithName("Helvetica-Bold" as CFString, size, nil)
    }

    func attributed(_ text: String, font: CTFont) -> NSAttributedString {
        NSAttributedString(
            // The overlay's `text-transform: uppercase`, so wrapping matches.
            string: settings.uppercase ? text.uppercased() : text,
            attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorFromContextAttributeName as String): true,
            ])
    }

    /// The newest rows that fit the window, oldest first. Wraps from the
    /// newest line back and stops once the window is full, so older lines
    /// that would only be clipped off the top are never typeset.
    func rows(for lines: [VisibleLine]) -> [Row] {
        let window = CGFloat(max(1, settings.maxLines)) * rowHeight + 0.5
        var rows: [Row] = []
        var used: CGFloat = 0
        for line in lines.reversed() where used < window {
            var block = wrap(line.text, source: false)
            if let source = line.source {
                block.insert(contentsOf: wrap(source, source: true), at: 0)
            }
            rows.insert(contentsOf: block, at: 0)
            used += block.reduce(0) { $0 + $1.height }
        }
        // Whole rows off the top, like the overlay's clip.
        while used > window, !rows.isEmpty {
            used -= rows.removeFirst().height
        }
        return rows
    }

    /// Greedy wrap, like the overlay. Not balanced: balancing re-splits every
    /// row on each new word, so text already read jumps between rows.
    /// `source` sets the original's smaller face above a translation.
    func wrap(_ text: String, source: Bool) -> [Row] {
        let font = source ? sourceFont : font
        let height = source ? rowHeight * Self.sourceScale : rowHeight
        let string = attributed(text, font: font)
        let ts = CTTypesetterCreateWithAttributedString(string)
        var rows: [Row] = []
        var start = 0
        while start < string.length {
            let count = CTTypesetterSuggestLineBreak(ts, start, Double(blockWidth))
            guard count > 0 else { break }
            let line = CTTypesetterCreateLine(ts, CFRange(location: start, length: count))
            // Trailing spaces don't count toward centring.
            let w =
                CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
                - CGFloat(CTLineGetTrailingWhitespaceWidth(line))
            rows.append(Row(line: line, width: w, height: height, isSource: source))
            start += count
        }
        return rows
    }
}
