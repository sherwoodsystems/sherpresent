import Testing

@testable import sherpresent_output

private func seg(_ id: UInt64, _ source: String, _ translated: String = "", final: Bool = false) -> Segment {
    Segment(id: id, source: source, translated: translated, final: final)
}

/// Mirrors the web overlay's state handling (`assets/captions.html`); the two
/// must agree line for line.
@Suite struct CaptionStateTests {
    @Test func straightCaptionsShowTheSource() {
        var s = CaptionState()
        s.translateEnabled = false
        s.apply(seg(1, "hello", final: true))
        #expect(s.visibleTexts() == ["hello"])
    }

    @Test func translatedLineWaitsForItsTranslation() {
        // No source-language flash before the translation arrives.
        var s = CaptionState()
        s.translateEnabled = true
        s.apply(seg(1, "hello"))
        #expect(s.visibleTexts().isEmpty)
        s.apply(seg(1, "hello world", "bonjour"))
        #expect(s.visibleTexts() == ["bonjour"])
    }

    @Test func translationIsHeldWhileTheSourceRuns() {
        var s = CaptionState()
        s.translateEnabled = true
        s.apply(seg(1, "hello", "bonjour"))
        _ = s.visibleTexts()
        s.apply(seg(1, "hello there", ""))
        #expect(s.visibleTexts() == ["bonjour"])
    }

    @Test func finalReplacesTheLiveLineInsteadOfDuplicatingIt() {
        var s = CaptionState()
        s.apply(seg(1, "hel"))
        s.apply(seg(1, "hello", final: true))
        #expect(s.visibleTexts() == ["hello"])
        #expect(s.live == nil)
    }

    @Test func keepsOnlyRecentSegments() {
        var s = CaptionState()
        s.maxLines = 1
        for i in 1...10 { s.apply(seg(UInt64(i), "line \(i)", final: true)) }
        #expect(s.finals.count == s.keepSegments)
        #expect(s.visibleTexts().last == "line 10")
    }

    @Test func clearBlanksEverything() {
        var s = CaptionState()
        s.apply(seg(1, "a", final: true))
        s.apply(seg(2, "b"))
        s.clear()
        #expect(s.isEmpty)
        #expect(s.visibleTexts().isEmpty)
    }

    @Test func replayDropsTheLiveLine() {
        var s = CaptionState()
        s.apply(seg(9, "live"))
        s.replay([seg(1, "a", final: true), seg(2, "b", final: true)])
        #expect(s.visibleTexts() == ["a", "b"])
    }
}
