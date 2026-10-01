import CoreGraphics
import CoreText

/// Live captions on a transparent frame, matching the web overlay.
@MainActor
final class CaptionContent: FrameContent {
    private var state = CaptionState()
    /// Rebuilt only when settings change, so the font is created once per
    /// change rather than on every partial.
    private var layout = CaptionLayout(settings: OverlaySettings())
    private var lines: [VisibleLine] = []

    init(text: CaptionText = .translated) {
        state.textMode = text
    }

    func handle(_ line: String) -> Bool {
        switch InMessage.parse(line) {
        case .segment(let seg): state.apply(seg)
        case .replay(let segs): state.replay(segs)
        // The app's silence timeout fired; see `run_silence_clear` in Rust.
        case .clear: state.clear()
        case .status(let s): state.translateEnabled = s.translateEnabled ?? false
        case .settings(let s):
            // Only fields that affect the picture are decoded, so any
            // difference here means the frame really changes.
            guard s != layout.settings else { return false }
            layout = CaptionLayout(settings: s)
            state.maxLines = s.maxLines
            lines = state.visibleLines()
            return true
        case .unknown: return false
        }

        // Partials repeat often with no visible change; skip identical frames
        // (Syphon asks publishers to only send frames that differ). Rows are a
        // pure function of lines + layout, and a layout change always redraws,
        // so comparing the lines is enough.
        let now = state.visibleLines()
        guard now != lines else { return false }
        lines = now
        return true
    }

    func draw(in ctx: CGContext, width w: CGFloat, height: CGFloat) {
        let rows = layout.rows(for: lines)

        // Core Graphics is bottom-up: walk from the newest row at the bottom.
        // Rows differ in height once the smaller original is shown.
        var placed: [(row: Row, bottom: CGFloat)] = []
        var bottom = layout.bottomInset
        for row in rows.reversed() {
            placed.append((row, bottom))
            bottom += row.height
        }

        if layout.settings.background {
            // Closed-caption boxes, one per row, sized like the web overlay's
            // inline background: the glyph box plus 0.3em each side. Opaque,
            // matching the web overlay, and drawn before the shadow is set so
            // the boxes themselves cast none. Like the overlay, only the main
            // text is boxed, not the original above it.
            let font = layout.font
            let pad = (0.3 * layout.fontSize).rounded()
            let boxHeight = CTFontGetAscent(font) + CTFontGetDescent(font)
            ctx.setFillColor(rgb(layout.settings.boxColor))
            for (row, rowBottom) in placed where row.width > 0 && !row.isSource {
                ctx.fill(
                    CGRect(
                        x: ((w - row.width) / 2 - pad).rounded(),
                        y: (rowBottom + (row.height - boxHeight) / 2).rounded(),
                        width: (row.width + 2 * pad).rounded(),
                        height: boxHeight.rounded()))
            }
        }

        ctx.saveGState()
        if layout.settings.shadow {
            // One pass of the web overlay's three-layer text-shadow; close
            // enough on a key, and cheap.
            ctx.setShadow(
                offset: CGSize(width: 0, height: -0.035 * layout.fontSize),
                blur: 0.18 * layout.fontSize,
                color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.9))
        }

        for (row, rowBottom) in placed {
            let font = row.isSource ? layout.sourceFont : layout.font
            let ascent = CTFontGetAscent(font)
            let descent = CTFontGetDescent(font)
            // CSS half-leading: the glyph box sits centred in its row.
            let baselineInRow = (row.height - (ascent + descent)) / 2 + descent
            // The overlay's `.source` is at 0.8 opacity.
            ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: row.isSource ? 0.8 : 1))
            ctx.textPosition = CGPoint(
                x: ((w - row.width) / 2).rounded(), y: (rowBottom + baselineInRow).rounded())
            CTLineDraw(row.line, ctx)
        }
        ctx.restoreGState()
    }
}
