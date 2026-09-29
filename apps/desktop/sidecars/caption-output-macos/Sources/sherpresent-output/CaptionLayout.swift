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

    let frameWidth: CGFloat
    let frameHeight: CGFloat
    let settings: OverlaySettings

    var fontSize: CGFloat { CGFloat(settings.fontSize) * frameWidth / 1920 }
    var rowHeight: CGFloat { fontSize * Self.leading }
    var blockWidth: CGFloat { frameWidth * CGFloat(settings.width) / 100 }
    var bottomInset: CGFloat { frameHeight * CGFloat(settings.safeArea) / 100 }

    /// The overlay's font stack is Inter, then Helvetica Neue, at weight 650.
    /// Bold is the nearest static face.
    func font() -> CTFont {
        for name in ["Inter-Bold", "HelveticaNeue-Bold"] {
            let f = CTFontCreateWithName(name as CFString, fontSize, nil)
            if (CTFontCopyPostScriptName(f) as String) == name { return f }
        }
        return CTFontCreateUIFontForLanguage(.emphasizedSystem, fontSize, nil)
            ?? CTFontCreateWithName("Helvetica-Bold" as CFString, fontSize, nil)
    }

    func attributed(_ text: String, font: CTFont) -> NSAttributedString {
        NSAttributedString(
            string: text,
            attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorFromContextAttributeName as String): true,
            ])
    }

    /// Newest `maxLines` rows across all texts, oldest first.
    func rows(for texts: [String]) -> [Row] {
        let font = font()
        var all: [Row] = []
        for text in texts {
            all.append(contentsOf: wrap(attributed(text, font: font)))
        }
        return Array(all.suffix(max(1, settings.maxLines)))
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
