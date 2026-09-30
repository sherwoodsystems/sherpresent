import Foundation

/// Which caption lines are on screen, and what text each shows.
///
/// A direct port of the web overlay's state handling (`assets/captions.html`)
/// so both outputs agree line for line. Pure, so it can be exercised without
/// a GPU.
struct CaptionState {
    private(set) var finals: [Segment] = []
    private(set) var live: Segment?
    var translateEnabled = false
    var maxLines = OverlaySettings().maxLines
    /// Segments kept around. The row window in `CaptionLayout` does the real
    /// limiting; this only bounds work, generously, since a segment is rarely
    /// shorter than one row. Same formula as the web overlay.
    var keepSegments: Int { maxLines * 2 + 2 }

    /// The last non-empty translation, held so a new partial that hasn't been
    /// translated yet doesn't blank the line (or flash the source language).
    private var lastGoodTranslated: (id: UInt64, text: String)?

    mutating func apply(_ seg: Segment) {
        if seg.final {
            // Live and final share an id; replace, never append.
            if live?.id == seg.id { live = nil }
            if let last = finals.last, last.id == seg.id {
                finals[finals.count - 1] = seg
            } else {
                finals.append(seg)
            }
            if finals.count > keepSegments { finals.removeFirst(finals.count - keepSegments) }
        } else {
            live = seg
        }
    }

    /// Blank the screen (the silence timeout). The next segment starts fresh.
    mutating func clear() {
        finals = []
        live = nil
    }

    var isEmpty: Bool { finals.isEmpty && live == nil }

    mutating func replay(_ segs: [Segment]) {
        finals = Array(segs.suffix(keepSegments))
        live = nil
    }

    mutating func text(of seg: Segment) -> String {
        if !translateEnabled { return seg.translated.isEmpty ? seg.source : seg.translated }
        if !seg.translated.isEmpty {
            lastGoodTranslated = (seg.id, seg.translated)
            return seg.translated
        }
        if let held = lastGoodTranslated, held.id == seg.id { return held.text }
        return ""
    }

    /// Texts to lay out, oldest first. Empty strings are skipped.
    mutating func visibleTexts() -> [String] {
        var shown = finals
        if let live { shown.append(live) }
        return shown.map { text(of: $0) }
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }
}
