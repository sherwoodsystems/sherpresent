import CoreGraphics
import CoreText
import Foundation

/// The current slide's notes on an opaque frame, styled after the stage view
/// (`build_page_html` in `webserver.rs`): slide counter and LIVE badge on
/// top, the notes auto-sized to fill the middle, and an Ontime timer strip
/// along the bottom when Ontime is configured.
@MainActor
final class NotesContent: FrameContent {
    private var status = SlideStatus()
    private var notes: [Int: String] = [:]
    private var timer: TimerPayload?
    private var shown = Picture.empty

    func handle(_ line: String) -> Bool {
        switch NotesMessage.parse(line) {
        case .status(let s): status = s
        case .notes(let n): notes = n
        case .timer(let t): timer = t
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
        var timer: TimerPicture?

        static let empty = Picture(counter: "--/--", presenting: false, body: .waiting, timer: nil)
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
            timer: timer.map(Self.timerPicture))
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
    /// anything still overflowing at the smallest is cut off at the bottom.
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

        var notesBottom: CGFloat = 0
        if let t = p.timer {
            drawTimer(t, width: w, in: ctx)
            notesBottom = Self.stripHeight + Self.border
        }

        let area = CGRect(
            x: Self.margin, y: notesBottom + Self.notesPadding,
            width: w - 2 * Self.margin,
            height: barBottom - Self.border - notesBottom - 2 * Self.notesPadding)
        switch p.body {
        case .waiting: placeholder("Waiting for presentation data…", in: area, ctx)
        case .noNotes: placeholder("No notes", in: area, ctx)
        case .text(let text): drawNotes(text, in: area, ctx)
        }
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

    private func drawNotes(_ text: String, in area: CGRect, _ ctx: CGContext) {
        let path = CGPath(rect: area, transform: nil)
        for size in Self.notesSizes {
            let setter = CTFramesetterCreateWithAttributedString(notesString(text, size: size))
            let fit = CTFramesetterSuggestFrameSizeWithConstraints(
                setter, CFRange(), nil, CGSize(width: area.width, height: .greatestFiniteMagnitude), nil)
            if fit.height <= area.height || size == Self.notesSizes.last {
                // A frame fills its path from the top down: top-left, like
                // the stage view.
                CTFrameDraw(CTFramesetterCreateFrame(setter, CFRange(), path, nil), ctx)
                return
            }
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
    private func label(_ text: String, _ font: CTFont, _ color: CGColor, maxWidth: CGFloat = .infinity) -> Label {
        let attrs: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
        ]
        var line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attrs))
        if CTLineGetTypographicBounds(line, nil, nil, nil) > maxWidth {
            let ellipsis = CTLineCreateWithAttributedString(NSAttributedString(string: "…", attributes: attrs))
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
