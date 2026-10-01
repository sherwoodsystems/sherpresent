import Testing

@testable import sherpresent_output

private func seg(_ id: UInt64, _ source: String, _ translated: String = "", final: Bool = false) -> Segment {
    Segment(id: id, source: source, translated: translated, final: final)
}

/// Main texts as laid out, blanks dropped.
private func texts(_ s: inout CaptionState) -> [String] {
    s.visibleLines().map(\.text).filter { !$0.isEmpty }
}

/// Mirrors the web overlay's state handling (`assets/captions.html`); the two
/// must agree line for line.
@Suite struct CaptionStateTests {
    @Test func straightCaptionsShowTheSource() {
        var s = CaptionState()
        s.translateEnabled = false
        s.apply(seg(1, "hello", final: true))
        #expect(texts(&s) == ["hello"])
    }

    @Test func translatedLineWaitsForItsTranslation() {
        // No source-language flash: "…" until the translation arrives.
        var s = CaptionState()
        s.translateEnabled = true
        s.apply(seg(1, "hello"))
        #expect(texts(&s) == ["…"])
        s.apply(seg(1, "hello world", "bonjour"))
        #expect(texts(&s) == ["bonjour …"])
        s.apply(seg(1, "hello world", "bonjour le monde", final: true))
        #expect(texts(&s) == ["bonjour le monde"])
    }

    @Test func translationIsHeldWhileTheSourceRuns() {
        var s = CaptionState()
        s.translateEnabled = true
        s.apply(seg(1, "hello", "bonjour"))
        _ = texts(&s)
        s.apply(seg(1, "hello there", ""))
        #expect(texts(&s) == ["bonjour …"])
    }

    @Test func finalReplacesTheLiveLineInsteadOfDuplicatingIt() {
        var s = CaptionState()
        s.apply(seg(1, "hel"))
        s.apply(seg(1, "hello", final: true))
        #expect(texts(&s) == ["hello"])
        #expect(s.live == nil)
    }

    @Test func keepsOnlyRecentSegments() {
        var s = CaptionState()
        s.maxLines = 1
        for i in 1...10 { s.apply(seg(UInt64(i), "line \(i)", final: true)) }
        #expect(s.finals.count == s.keepSegments)
        #expect(texts(&s).last == "line 10")
    }

    @Test func clearBlanksEverything() {
        var s = CaptionState()
        s.apply(seg(1, "a", final: true))
        s.apply(seg(2, "b"))
        s.clear()
        #expect(s.isEmpty)
        #expect(texts(&s).isEmpty)
    }

    @Test func replayDropsTheLiveLine() {
        var s = CaptionState()
        s.apply(seg(9, "live"))
        s.replay([seg(1, "a", final: true), seg(2, "b", final: true)])
        #expect(texts(&s) == ["a", "b"])
    }

    @Test func sourceModeShowsTheOriginalImmediately() {
        var s = CaptionState()
        s.translateEnabled = true
        s.textMode = .source
        s.apply(seg(1, "hello"))
        #expect(texts(&s) == ["hello"])
    }

    @Test func bothModeShowsTheOriginalBeforeItsTranslation() {
        var s = CaptionState()
        s.translateEnabled = true
        s.textMode = .both
        s.apply(seg(1, "hello"))
        #expect(s.visibleLines() == [VisibleLine(source: "hello", text: "…")])
        s.apply(seg(1, "hello world", "bonjour"))
        #expect(s.visibleLines() == [VisibleLine(source: "hello world", text: "bonjour …")])
    }

    @Test func bothModeWithoutTranslationDoesNotRepeatTheLine() {
        var s = CaptionState()
        s.translateEnabled = false
        s.textMode = .both
        s.apply(seg(1, "hello", final: true))
        #expect(s.visibleLines() == [VisibleLine(source: nil, text: "hello")])
    }
}
