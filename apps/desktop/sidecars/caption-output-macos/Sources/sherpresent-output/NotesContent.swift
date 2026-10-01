import CoreGraphics
import CoreText
import Foundation

/// The current slide's notes on an opaque frame, styled after the stage view
/// (`build_page_html` in `webserver.rs`): slide counter and LIVE badge on
/// top, the notes auto-sized to fill the middle, and an Ontime timer strip
/// along the bottom when Ontime is configured.
///
/// Notes too long for the screen even at the smallest size are paged like a
/// teleprompter by `scroll` messages: whole lines at a time, `page` wrapping
/// back to the top after the last screenful, and every slide change starting
/// at the top again.
@MainActor
final class NotesContent: FrameContent {
    private var status = SlideStatus()
    private var notes: [Int: String] = [:]
    private var timer: TimerPayload?
    /// First line shown of the current slide's notes
    private(set) var firstLine = 0
    private var shown = Picture.empty

    func handle(_ line: String) -> Bool {
        switch NotesMessage.parse(line) {
        case .status(let s):
            if s.current != status.current { firstLine = 0 }
            status = s
        case .notes(let n):
            if n[status.current] != notes[status.current] { firstLine = 0 }
            notes = n
        case .timer(let t): timer = t
        case .scroll(let direction): scroll(direction)
        case .unknown: return false
        }
        // Status repeats with changes the feed doesn't show (zoom, builds) and
        // the timer ticks in milliseconds; only redraw what's visible.
        let now = picture()
        guard now != shown else { return false }
        shown = now
        return true
    }

    // MARK: - What's on screen

    /// Everything the frame depends on.
    private struct Picture: Equatable {
        var counter: String
        var presenting: Bool
        var body: Body
        /// First line of the notes on screen
        var firstLine: Int
        var timer: TimerPicture?

        static let empty = Picture(
            counter: "--/--", presenting: false, body: .waiting, firstLine: 0, timer: nil)
    }

    private enum Body: Equatable {
        /// No notes scanned yet
        case waiting
        case noNotes
        case text(String)
    }

    private struct TimerPicture: Equatable {
        var title: String
        var value: String
        var valueColor: UInt32
        var badge: String
        var badgeFill: UInt32
        var badgeText: UInt32
    }

    private func picture() -> Picture {
        let body: Body
        if notes.isEmpty {
            body = .waiting
        } else if let text = notes[status.current]?.trimmingCharacters(in: .whitespacesAndNewlines),
            !text.isEmpty
        {
            body = .text(text)
        } else {
            body = .noNotes
        }
        return Picture(
            counter: status.total > 0 ? "\(status.current) / \(status.total)" : "--/--",
            presenting: status.presenting,
            body: body,
            firstLine: firstLine,
            timer: timer.map(Self.timerPicture))
    }

    /// Move `firstLine` by a screenful. Notes that fit don't move.
    private func scroll(_ direction: ScrollDirection) {
        guard case .text(let text) = picture().body else { return }
        let area = Self.notesArea(
            width: CGFloat(Args.width), height: CGFloat(Args.height), hasTimer: timer != nil)
        let layout = notesLayout(text, in: area)
        guard layout.lineCount > 0 else { return }
        let first = min(firstLine, layout.lineCount - 1)
        let last = layout.lastVisible(from: first, height: area.height)
        let atEnd = last >= layout.lineCount - 1
        switch direction {
        case .page: firstLine = atEnd ? 0 : last + 1
        case .down: if !atEnd { firstLine = last + 1 }
        case .up: firstLine = layout.previousPage(before: first, height: area.height)
        }
    }

    /// Same colours and labels as the stage view's `updateTimerDisplay`.
    private static func timerPicture(_ t: TimerPayload) -> TimerPicture {
        guard t.connected else {
            return TimerPicture(
                title: "Ontime disconnected", value: "--:--", valueColor: 0x666666,
                badge: "OFFLINE", badgeFill: 0x555555, badgeText: 0xAAAAAA)
        }
        let playback = t.playback
        var valueColor: UInt32 = 0x22C55E
        if let c = t.current, c < 0 {
            valueColor = 0xEF4444
        } else if let c = t.current, c > 0, c < 60_000, playback == "play" {
            valueColor = 0xEAB308
        } else if playback == "pause" {
            valueColor = 0xAAAAAA
        } else if playback == "stop" || playback == "armed" {
            valueColor = 0x666666
        }

        let badge: String
        switch playback {
        case nil: badge = "STOPPED"
        case "play": badge = "RUNNING"
        case let p?: badge = p.uppercased()
        }
        let (fill, text): (UInt32, UInt32) =
            switch playback {
            case "play": (0x22C55E, 0x000000)
            case "pause": (0xEAB308, 0x000000)
            case "roll": (0x3B82F6, 0xFFFFFF)
            case "armed": (0xF97316, 0x000000)
            default: (0x555555, 0xAAAAAA)
            }
        return TimerPicture(
            title: t.title, value: formatTime(t.current), valueColor: valueColor,
            badge: badge, badgeFill: fill, badgeText: text)
    }

    /// `h:mm:ss` or `mm:ss`, negative for overtime. Mirrors the stage view.
    static func formatTime(_ ms: Double?) -> String {
        guard let ms else { return "--:--:--" }
        let total = Int((abs(ms) / 1000).rounded(.down))
        let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60)
        let sign = ms < 0 ? "-" : ""
        let mmss = String(format: "%02d:%02d", m, s)
        return h > 0 ? "\(sign)\(h):\(mmss)" : "\(sign)\(mmss)"
    }

    // MARK: - Drawing

    private static let barHeight: CGFloat = 112
    private static let stripHeight: CGFloat = 300
    private static let border: CGFloat = 3
    private static let margin: CGFloat = 80
    private static let notesPadding: CGFloat = 56
    /// Notes shrink from the largest size that fits down to the smallest;
    /// anything still overflowing at the smallest is paged (`scroll`).
    private static let notesSizes = Array(stride(from: CGFloat(88), through: 36, by: -4))

    func draw(in ctx: CGContext, width w: CGFloat, height h: CGFloat) {
        let p = shown
        ctx.setFillColor(rgb(0x1A1A1A))
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))

        // Core Graphics is bottom-up: the bar is at the top of the range.
        let barBottom = h - Self.barHeight
        ctx.setFillColor(rgb(0x222222))
        ctx.fill(CGRect(x: 0, y: barBottom, width: w, height: Self.barHeight))
        ctx.setFillColor(rgb(0x333333))
        ctx.fill(CGRect(x: 0, y: barBottom - Self.border, width: w, height: Self.border))

        let barMid = barBottom + Self.barHeight / 2
        let counter = label(p.counter, font(60, bold: true, monoDigits: true), rgb(0xEEEEEE))
        draw(counter, x: 56, midY: barMid, in: ctx)
        pill(
            p.presenting ? "LIVE" : "IDLE", size: 30,
            fill: rgb(p.presenting ? 0x22C55E : 0x555555), text: rgb(p.presenting ? 0x000000 : 0xAAAAAA),
            right: w - 56, midY: barMid, in: ctx)

        if let t = p.timer { drawTimer(t, width: w, in: ctx) }

        let area = Self.notesArea(width: w, height: h, hasTimer: p.timer != nil)
        switch p.body {
        case .waiting: placeholder("Waiting for presentation data…", in: area, ctx)
        case .noNotes: placeholder("No notes", in: area, ctx)
        case .text(let text): drawNotes(text, from: p.firstLine, in: area, ctx)
        }
    }

    /// Where the notes go: between the top bar and the timer strip, if any.
    private static func notesArea(width w: CGFloat, height h: CGFloat, hasTimer: Bool) -> CGRect {
        let notesBottom = hasTimer ? stripHeight + border : 0
        let barBottom = h - barHeight
        return CGRect(
            x: margin, y: notesBottom + notesPadding,
            width: w - 2 * margin,
            height: barBottom - border - notesBottom - 2 * notesPadding)
    }

    private func drawTimer(_ t: TimerPicture, width w: CGFloat, in ctx: CGContext) {
        let h = Self.stripHeight
        ctx.setFillColor(rgb(0x333333))
        ctx.fill(CGRect(x: 0, y: h, width: w, height: Self.border))

        if !t.title.isEmpty {
            let title = label(t.title, font(38), rgb(0xAAAAAA), maxWidth: w - 2 * Self.margin)
            draw(title, x: (w - title.width) / 2, midY: h - 50, in: ctx)
        }
        let value = label(t.value, font(150, bold: true, monoDigits: true), rgb(t.valueColor))
        draw(value, x: (w - value.width) / 2, midY: h / 2, in: ctx)
        pill(
            t.badge, size: 24, fill: rgb(t.badgeFill), text: rgb(t.badgeText),
            centerX: w / 2, midY: 48, in: ctx)
    }

    /// The notes laid out in one tall frame, with every line's extent, so a
    /// page is just a window onto it.
    private struct NotesLayout {
        static let frameHeight: CGFloat = 100_000

        let frame: CTFrame
        /// Each line's top and bottom, measured down from the top of the frame
        let tops: [CGFloat]
        let bottoms: [CGFloat]

        var lineCount: Int { tops.count }

        /// How far down the frame a page starting at `first` begins. Every
        /// page keeps the first line's top margin, so pages line up.
        func offset(_ first: Int) -> CGFloat { tops[first] - tops[0] }

        /// The last whole line that fits under `first`; always at least `first`.
        func lastVisible(from first: Int, height: CGFloat) -> Int {
            var last = first
            while last + 1 < lineCount, bottoms[last + 1] - offset(first) <= height { last += 1 }
            return last
        }

        /// Where the page ending just above `first` starts.
        func previousPage(before first: Int, height: CGFloat) -> Int {
            guard first > 0 else { return 0 }
            var start = first - 1
            while start > 0, bottoms[first - 1] - offset(start - 1) <= height { start -= 1 }
            return start
        }
    }

    private func notesLayout(_ text: String, in area: CGRect) -> NotesLayout {
        var setter = CTFramesetterCreateWithAttributedString(notesString(text, size: Self.notesSizes[0]))
        for size in Self.notesSizes {
            setter = CTFramesetterCreateWithAttributedString(notesString(text, size: size))
            let fit = CTFramesetterSuggestFrameSizeWithConstraints(
                setter, CFRange(), nil, CGSize(width: area.width, height: .greatestFiniteMagnitude), nil)
            if fit.height <= area.height { break }
        }

        let height = NotesLayout.frameHeight
        let path = CGPath(rect: CGRect(x: 0, y: 0, width: area.width, height: height), transform: nil)
        let frame = CTFramesetterCreateFrame(setter, CFRange(), path, nil)
        let lines = CTFrameGetLines(frame) as? [CTLine] ?? []
        var origins = [CGPoint](repeating: .zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(), &origins)
        var tops: [CGFloat] = []
        var bottoms: [CGFloat] = []
        for (line, origin) in zip(lines, origins) {
            var ascent: CGFloat = 0
            var descent: CGFloat = 0
            CTLineGetTypographicBounds(line, &ascent, &descent, nil)
            tops.append(height - origin.y - ascent)
            bottoms.append(height - origin.y + descent)
        }
        return NotesLayout(frame: frame, tops: tops, bottoms: bottoms)
    }

    private func drawNotes(_ text: String, from firstLine: Int, in area: CGRect, _ ctx: CGContext) {
        let layout = notesLayout(text, in: area)
        guard layout.lineCount > 0 else { return }
        // The timer strip can come and go between pages, changing the area.
        let first = min(firstLine, layout.lineCount - 1)
        let last = layout.lastVisible(from: first, height: area.height)
        let top = layout.offset(first)
        let shown = layout.bottoms[last] - top

        // Show whole lines only: clip just under the last one that fits.
        ctx.saveGState()
        ctx.clip(to: CGRect(x: 0, y: area.maxY - shown, width: area.maxX + Self.margin, height: shown))
        // A frame fills its path from the top down: top-left, like the stage
        // view. Shift it up so the page's first line sits at the top.
        ctx.translateBy(x: area.minX, y: area.maxY - NotesLayout.frameHeight + top)
        CTFrameDraw(layout.frame, ctx)
        ctx.restoreGState()

        if last < layout.lineCount - 1 {
            // More below: a cue for whoever's reading that the clicker has
            // another page.
            let more = label("▼", font(36, bold: true), rgb(0x666666))
            draw(more, x: area.maxX - more.width, midY: area.minY - Self.notesPadding / 2, in: ctx)
        }
    }

    private func notesString(_ text: String, size: CGFloat) -> NSAttributedString {
        let multiple: CGFloat = 1.3
        let style = withUnsafePointer(to: multiple) { ptr in
            var setting = CTParagraphStyleSetting(
                spec: .lineHeightMultiple, valueSize: MemoryLayout<CGFloat>.size, value: ptr)
            return CTParagraphStyleCreate(&setting, 1)
        }
        return NSAttributedString(
            string: text,
            attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): font(size),
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): rgb(0xEEEEEE),
                NSAttributedString.Key(kCTParagraphStyleAttributeName as String): style,
            ])
    }

    private func placeholder(_ text: String, in area: CGRect, _ ctx: CGContext) {
        let l = label(text, font(48, italic: true), rgb(0x555555), maxWidth: area.width)
        draw(l, x: area.midX - l.width / 2, midY: area.midY, in: ctx)
    }

    /// A capsule label, like the stage view's badges.
    private func pill(
        _ text: String, size: CGFloat, fill: CGColor, text color: CGColor,
        right: CGFloat? = nil, centerX: CGFloat? = nil, midY: CGFloat, in ctx: CGContext
    ) {
        let l = label(text, font(size, bold: true), color)
        let boxW = l.width + 1.8 * size
        let boxH = 1.8 * size
        let x = right.map { $0 - boxW } ?? (centerX ?? 0) - boxW / 2
        ctx.setFillColor(fill)
        ctx.addPath(
            CGPath(
                roundedRect: CGRect(x: x, y: midY - boxH / 2, width: boxW, height: boxH),
                cornerWidth: boxH / 2, cornerHeight: boxH / 2, transform: nil))
        ctx.fillPath()
        draw(l, x: x + 0.9 * size, midY: midY, in: ctx)
    }

    // MARK: - Text helpers

    /// One line of text, measured.
    private struct Label {
        let line: CTLine
        let width: CGFloat
        let capHeight: CGFloat
    }

    /// Cut to `maxWidth` with an ellipsis if it doesn't fit.
    private func label(_ text: String, _ font: CTFont, _ color: CGColor, maxWidth: CGFloat = .infinity)
        -> Label
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
        return Label(
            line: line, width: CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)),
            capHeight: CTFontGetCapHeight(font))
    }

    /// Draw with the cap height centred on `midY`, so labels of any case sit
    /// visually centred in their bar or badge.
    private func draw(_ l: Label, x: CGFloat, midY: CGFloat, in ctx: CGContext) {
        ctx.textPosition = CGPoint(x: x.rounded(), y: (midY - l.capHeight / 2).rounded())
        CTLineDraw(l.line, ctx)
    }

    private func font(_ size: CGFloat, bold: Bool = false, italic: Bool = false, monoDigits: Bool = false)
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
