import Foundation

/// Which language(s) a captions output shows. Same values as the web
/// overlay's `?text=`, plus `translated`, its default.
enum CaptionText: String, Sendable {
    case translated
    case source
    /// The original in a smaller face above the translation.
    case both
}

/// One caption line as drawn: its main text, and in `both` mode the original
/// shown above it.
struct VisibleLine: Equatable {
    var source: String?
    var text: String
}

/// Which caption lines are on screen, and what text each shows.
///
/// A direct port of the web overlay's state handling (`assets/captions.html`)
/// so both outputs agree line for line. Pure, so it can be exercised without
/// a GPU.
struct CaptionState {
    private(set) var finals: [Segment] = []
    private(set) var live: Segment?
    var translateEnabled = false
    var textMode = CaptionText.translated
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

    /// Marks a translation still catching up with the speaker.
    static let pending = "…"

    mutating func text(of seg: Segment) -> String {
        if textMode == .source { return seg.source }
        if !translateEnabled { return seg.translated.isEmpty ? seg.source : seg.translated }
        var text = ""
        if !seg.translated.isEmpty {
            lastGoodTranslated = (seg.id, seg.translated)
            text = seg.translated
        } else if let held = lastGoodTranslated, held.id == seg.id {
            text = held.text
        }
        // A line still being spoken ends in "…" (or is just that, before its
        // first translated words), so the audience can see more is coming.
        if !seg.final && !seg.source.trimmingCharacters(in: .whitespaces).isEmpty {
            let trimmed = text.trimmingCharacters(in: .whitespaces)
            text = trimmed.isEmpty ? Self.pending : trimmed + " " + Self.pending
        }
        return text
    }

    /// Lines to lay out, oldest first. Blank lines are skipped.
    mutating func visibleLines() -> [VisibleLine] {
        var shown = finals
        if let live { shown.append(live) }
        return shown.compactMap { seg in
            let main = text(of: seg).trimmingCharacters(in: .whitespacesAndNewlines)
            // The original only earns its own row when there's a translation
            // to sit above; otherwise `main` already is the original.
            let source =
                textMode == .both && translateEnabled
                ? seg.source.trimmingCharacters(in: .whitespacesAndNewlines) : ""
            if main.isEmpty && source.isEmpty { return nil }
            return VisibleLine(source: source.isEmpty ? nil : source, text: main)
        }
    }
}
