import CoreText
import Foundation

/// One wrapped row of caption text, ready to draw.
struct Row {
    let line: CTLine
    let width: CGFloat
}

/// Turns caption texts into the rows that fit on screen, matching the web
/// overlay's CSS: `--size` px at 1920 wide, `line-height: 1.22`, a block
/// `--width`% of the frame, `text-wrap: balance`, and a window exactly
/// `maxLines` rows tall that keeps the newest rows.
struct CaptionLayout {
    static let leading: CGFloat = 1.22

    let frameWidth = CGFloat(Args.width)
    let frameHeight = CGFloat(Args.height)
    let settings: OverlaySettings
    let font: CTFont

    init(settings: OverlaySettings) {
        self.settings = settings
        self.font = Self.makeFont(size: CGFloat(settings.fontSize) * CGFloat(Args.width) / 1920)
    }

    var fontSize: CGFloat { CGFloat(settings.fontSize) * frameWidth / 1920 }
    var rowHeight: CGFloat { fontSize * Self.leading }
    var blockWidth: CGFloat { frameWidth * CGFloat(settings.width) / 100 }
    var bottomInset: CGFloat { frameHeight * CGFloat(settings.safeArea) / 100 }

    /// The overlay's font stack is Inter, then Helvetica Neue, at weight 650.
    /// Bold is the nearest static face.
    private static func makeFont(size: CGFloat) -> CTFont {
        for name in ["Inter-Bold", "HelveticaNeue-Bold"] {
            let f = CTFontCreateWithName(name as CFString, size, nil)
            if (CTFontCopyPostScriptName(f) as String) == name { return f }
        }
        return CTFontCreateUIFontForLanguage(.emphasizedSystem, size, nil)
            ?? CTFontCreateWithName("Helvetica-Bold" as CFString, size, nil)
    }

    func attributed(_ text: String, font: CTFont) -> NSAttributedString {
        NSAttributedString(
            string: text,
            attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorFromContextAttributeName as String): true,
            ])
    }

    /// Newest `maxLines` rows across all texts, oldest first. Wraps from the
    /// newest text back and stops once the window is full, so older lines
    /// that would only be clipped off the top are never typeset.
    func rows(for texts: [String]) -> [Row] {
        let limit = max(1, settings.maxLines)
        var rows: [Row] = []
        for text in texts.reversed() where rows.count < limit {
            rows.insert(contentsOf: wrap(attributed(text, font: font)), at: 0)
        }
        return Array(rows.suffix(limit))
    }

    /// Greedy wrap, then balance: shrink the measure as far as it goes without
    /// adding a row, so a two-row caption splits evenly instead of leaving one
    /// orphaned word. Same idea as CSS `text-wrap: balance`.
    func wrap(_ text: NSAttributedString) -> [Row] {
        let typesetter = CTTypesetterCreateWithAttributedString(text)
        let greedy = breakLines(typesetter, length: text.length, width: blockWidth)
        guard greedy.count > 1 else { return greedy }

        var lo: CGFloat = blockWidth / CGFloat(greedy.count)
        var hi = blockWidth
        var best = greedy
        for _ in 0..<12 {
            let mid = (lo + hi) / 2
            let attempt = breakLines(typesetter, length: text.length, width: mid)
            if attempt.count <= greedy.count {
                best = attempt
                hi = mid
            } else {
                lo = mid
            }
        }
        return best
    }

    private func breakLines(_ ts: CTTypesetter, length: Int, width: CGFloat) -> [Row] {
        var rows: [Row] = []
        var start = 0
        while start < length {
            let count = CTTypesetterSuggestLineBreak(ts, start, Double(width))
            guard count > 0 else { break }
            let line = CTTypesetterCreateLine(ts, CFRange(location: start, length: count))
            // Trailing spaces don't count toward centring.
            let w = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
                - CGFloat(CTLineGetTrailingWhitespaceWidth(line))
            rows.append(Row(line: line, width: w))
            start += count
        }
        return rows
    }
}
