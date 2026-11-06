import CoreGraphics
import CoreText
import Foundation

/// The QLab count-out on an opaque frame. Same picture as the `/qlab` web
/// page (`assets/qlab.html`), laid out with the same numbers:
///
/// - one linear cue running: its kind, number and name, the time left very
///   large in the kind's colour, and a bar that drains as it counts down;
/// - a video and an audio cue: the two stacked, video on top;
/// - nothing: "Nothing playing", with the cue at the playhead along the
///   bottom;
/// - not connected: why.
///
/// Rust has already dropped looping cues and picked the one cue per kind
/// that ends last.
@MainActor
final class QLabContent: FrameContent, TextDrawing {
    private var shown = Picture.offline("Connecting…")

    func handle(_ line: String) -> Bool {
        guard let payload = QLabMessage.parse(line) else { return false }
        // State arrives about ten times a second; redraw only when what's on
        // screen (a second ticking, a pixel of the bar) changes.
        let now = Self.picture(payload)
        guard now != shown else { return false }
        shown = now
        return true
    }

    // MARK: - What's on screen

    enum Kind: Equatable {
        case video, audio

        var title: String { self == .video ? "VIDEO" : "AUDIO" }
        var color: UInt32 { self == .video ? 0x38BDF8 : 0xFBBF24 }
    }

    struct Count: Equatable {
        var kind: Kind
        var label: String
        var time: String
        var total: String
        /// Time left, in thousandths of the cue: finer than a pixel of the bar
        var left: Int
        var paused: Bool
        var others: Int
    }

    enum Picture: Equatable {
        case offline(String)
        case idle(next: QLabCue?)
        case one(Count)
        case two(video: Count, audio: Count)
    }

    static func picture(_ p: QLabPayload) -> Picture {
        guard p.connected else {
            return .offline(p.configured ? (p.message ?? "Connecting…") : "Set a QLab host in SherPresent")
        }
        let video = p.video.map { count(.video, $0) }
        let audio = p.audio.map { count(.audio, $0) }
        switch (video, audio) {
        case (let v?, let a?): return .two(video: v, audio: a)
        case (let c?, nil), (nil, let c?): return .one(c)
        case (nil, nil): return .idle(next: p.next)
        }
    }

    static func count(_ kind: Kind, _ c: QLabCountdown) -> Count {
        let left = c.duration > 0 ? min(1, max(0, c.remaining / c.duration)) : 0
        return Count(
            kind: kind, label: [c.number, c.name].filter { !$0.isEmpty }.joined(separator: "  ·  "),
            time: formatTime(c.remaining), total: "of \(formatTime(c.duration))",
            left: Int((left * 1000).rounded()), paused: c.paused, others: c.others)
    }

    /// `m:ss` or `h:mm:ss`, rounded up so `0:00` shows only as the cue ends.
    /// Mirrors the web page's `formatTime`.
    static func formatTime(_ seconds: Double) -> String {
        let total = max(0, Int((seconds - 0.001).rounded(.up)))
        let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60)
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
    }

    // MARK: - Drawing

    /// Sizes and positions (top-down, within the band) for the single and
    /// stacked layouts. Same as `countdown` in `qlab.html`.
    private struct Layout {
        var header: CGFloat, pill: CGFloat, label: CGFloat
        var time: CGFloat, timeSize: CGFloat
        var bar: CGFloat, barH: CGFloat
        var total: CGFloat, totalSize: CGFloat

        static let big = Layout(
            header: 250, pill: 30, label: 56, time: 540, timeSize: 380, bar: 820, barH: 32, total: 885,
            totalSize: 40)
        static let stacked = Layout(
            header: 90, pill: 26, label: 46, time: 285, timeSize: 250, bar: 455, barH: 24, total: 505,
            totalSize: 34)
    }

    private static let barX: CGFloat = 160
    private static let stripHeight: CGFloat = 220

    func draw(in ctx: CGContext, width w: CGFloat, height h: CGFloat) {
        ctx.setFillColor(rgb(0x0B0B0C))
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        // Core Graphics is bottom-up; the layout is top-down like the page.
        let y = { (top: CGFloat) in h - top }

        switch shown {
        case .offline(let message):
            centered("QLab", font(40, bold: true), 0x666666, midY: y(440), width: w, in: ctx)
            centered(message, font(52), 0x888888, midY: y(530), width: w, in: ctx)
        case .idle(let next):
            centered("Nothing playing", font(64, italic: true), 0x555555, midY: y(430), width: w, in: ctx)
            drawNext(next, width: w, in: ctx)
        case .one(let c):
            drawCount(c, top: 0, layout: .big, width: w, height: h, in: ctx)
        case .two(let video, let audio):
            drawCount(video, top: 0, layout: .stacked, width: w, height: h, in: ctx)
            ctx.setFillColor(rgb(0x222222))
            ctx.fill(CGRect(x: 0, y: h / 2 - 1.5, width: w, height: 3))
            drawCount(audio, top: h / 2, layout: .stacked, width: w, height: h, in: ctx)
        }
    }

    private func drawCount(
        _ c: Count, top: CGFloat, layout: Layout, width w: CGFloat, height h: CGFloat, in ctx: CGContext
    ) {
        let y = { (offset: CGFloat) in h - top - offset }
        let color = c.paused ? 0x777777 : c.kind.color

        // Header: kind badge, cue, then PAUSED and "+N more" when they apply.
        let gap = layout.pill
        let kindW = pillWidth(c.kind.title, size: layout.pill)
        let pausedW = c.paused ? gap + pillWidth("PAUSED", size: layout.pill) : 0
        let more = c.others > 0 ? label("+\(c.others) more", font(0.75 * layout.label), rgb(0x777777)) : nil
        let moreW = more.map { gap + $0.width } ?? 0
        let name = label(
            c.label, font(layout.label), rgb(0xD4D4D4), maxWidth: w - 240 - kindW - gap - pausedW - moreW)
        var x = (w - (kindW + gap + name.width + pausedW + moreW)) / 2
        let headerY = y(layout.header)
        pill(
            c.kind.title, size: layout.pill, fill: rgb(c.kind.color), text: rgb(0x000000), left: x,
            midY: headerY,
            in: ctx)
        x += kindW + gap
        draw(name, x: x, midY: headerY, in: ctx)
        x += name.width
        if c.paused {
            pill(
                "PAUSED", size: layout.pill, fill: rgb(0x555555), text: rgb(0xDDDDDD), left: x + gap,
                midY: headerY, in: ctx)
            x += pausedW
        }
        if let more { draw(more, x: x + gap, midY: headerY, in: ctx) }

        centered(
            c.time, font(layout.timeSize, bold: true, monoDigits: true), color, midY: y(layout.time),
            width: w, in: ctx)

        // Count-down bar: the time left filled, draining to the left.
        let barW = w - 2 * Self.barX
        let track = CGRect(x: Self.barX, y: y(layout.bar) - layout.barH / 2, width: barW, height: layout.barH)
        let shape = CGPath(
            roundedRect: track, cornerWidth: layout.barH / 2, cornerHeight: layout.barH / 2, transform: nil)
        ctx.saveGState()
        ctx.addPath(shape)
        ctx.clip()
        ctx.setFillColor(rgb(0x262626))
        ctx.fill(track)
        ctx.setFillColor(rgb(color))
        ctx.fill(
            CGRect(
                x: track.minX, y: track.minY, width: barW * CGFloat(c.left) / 1000, height: layout.barH))
        ctx.restoreGState()

        let total = label(c.total, font(layout.totalSize, monoDigits: true), rgb(0x777777))
        draw(total, x: w - Self.barX - total.width, midY: y(layout.total), in: ctx)
    }

    /// The bottom strip: the cue at the playhead.
    private func drawNext(_ next: QLabCue?, width w: CGFloat, in ctx: CGContext) {
        let strip = Self.stripHeight
        ctx.setFillColor(rgb(0x151515))
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: strip))
        ctx.setFillColor(rgb(0x2A2A2A))
        ctx.fill(CGRect(x: 0, y: strip - 3, width: w, height: 3))

        let midY = strip / 2
        let margin: CGFloat = 80
        guard let next else {
            draw(
                label("No cue at the playhead", font(48, italic: true), rgb(0x555555)), x: margin, midY: midY,
                in: ctx)
            return
        }
        pill("NEXT", size: 30, fill: rgb(0x2A2A2A), text: rgb(0xAAAAAA), left: margin, midY: midY, in: ctx)
        var x = margin + pillWidth("NEXT", size: 30) + 40
        if !next.number.isEmpty {
            let number = label(next.number, font(72, bold: true), rgb(0xFFFFFF), maxWidth: w - margin - x)
            draw(number, x: x, midY: midY, in: ctx)
            x += number.width + 32
        }
        draw(
            label(next.name, font(72), rgb(0xDDDDDD), maxWidth: max(0, w - margin - x)), x: x, midY: midY,
            in: ctx)
    }

    private func centered(
        _ text: String, _ font: CTFont, _ color: UInt32, midY: CGFloat, width w: CGFloat, in ctx: CGContext
    ) {
        let l = label(text, font, rgb(color), maxWidth: w - 160)
        draw(l, x: (w - l.width) / 2, midY: midY, in: ctx)
    }
}
