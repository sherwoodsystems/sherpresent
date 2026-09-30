import CoreGraphics
import CoreText

/// Live captions on a transparent frame, matching the web overlay.
@MainActor
final class CaptionContent: FrameContent {
    private var state = CaptionState()
    /// Rebuilt only when settings change, so the font is created once per
    /// change rather than on every partial.
    private var layout = CaptionLayout(settings: OverlaySettings())
    private var texts: [String] = []

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
            texts = state.visibleTexts()
            return true
        case .unknown: return false
        }

        // Partials repeat often with no visible change; skip identical frames
        // (Syphon asks publishers to only send frames that differ). Rows are a
        // pure function of texts + layout, and a layout change always redraws,
        // so comparing the texts is enough.
        let now = state.visibleTexts()
        guard now != texts else { return false }
        texts = now
        return true
    }

    func draw(in ctx: CGContext, width w: CGFloat, height: CGFloat) {
        let rows = layout.rows(for: texts)
        let font = layout.font
        let ascent = CTFontGetAscent(font)
        let descent = CTFontGetDescent(font)
        let rowHeight = layout.rowHeight
        // CSS half-leading: the glyph box sits centred in its row.
        let baselineInRow = (rowHeight - (ascent + descent)) / 2 + descent

        if layout.settings.background {
            // Closed-caption boxes, one per row, sized like the web overlay's
            // inline background: the glyph box plus 0.3em each side. Opaque,
            // matching the web overlay, and drawn before the shadow is set so
            // the boxes themselves cast none.
            let pad = (0.3 * layout.fontSize).rounded()
            let boxHeight = ascent + descent
            ctx.setFillColor(rgb(layout.settings.boxColor))
            for (i, row) in rows.reversed().enumerated() where row.width > 0 {
                let rowBottom = layout.bottomInset + CGFloat(i) * rowHeight
                ctx.fill(
                    CGRect(
                        x: ((w - row.width) / 2 - pad).rounded(),
                        y: (rowBottom + (rowHeight - boxHeight) / 2).rounded(),
                        width: (row.width + 2 * pad).rounded(),
                        height: boxHeight.rounded()))
            }
        }

        ctx.saveGState()
        ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        if layout.settings.shadow {
            // One pass of the web overlay's three-layer text-shadow; close
            // enough on a key, and cheap.
            ctx.setShadow(
                offset: CGSize(width: 0, height: -0.035 * layout.fontSize),
                blur: 0.18 * layout.fontSize,
                color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.9))
        }

        // Core Graphics is bottom-up: row 0 from the bottom is the newest.
        for (i, row) in rows.reversed().enumerated() {
            let y = layout.bottomInset + CGFloat(i) * rowHeight + baselineInRow
            let x = (w - row.width) / 2
            ctx.textPosition = CGPoint(x: x.rounded(), y: y.rounded())
            CTLineDraw(row.line, ctx)
        }
        ctx.restoreGState()
    }
}
