import Testing

@testable import sherpresent_speech

@Suite struct TurnBuilderTests {
    @Test func sentenceEndClosesTheLine() {
        var t = TurnBuilder(maxChars: 180)
        let closed1 = t.finalize("hello")
        #expect(!closed1)
        let closed2 = t.finalize("world.")
        #expect(closed2)
        #expect(t.committed == "hello world.")
    }

    @Test func longLineClosesWithoutPunctuation() {
        var t = TurnBuilder(maxChars: 10)
        let closed3 = t.finalize("0123456789")
        #expect(closed3)
    }

    @Test func blankFragmentsAreIgnored() {
        var t = TurnBuilder(maxChars: 180)
        let closed4 = t.finalize("   ")
        #expect(!closed4)
        #expect(t.isEmpty)
    }

    @Test func lineJoinsCommittedAndVolatileText() {
        var t = TurnBuilder(maxChars: 180)
        #expect(t.line(volatile: "live") == "live")
        _ = t.finalize("done")
        #expect(t.line(volatile: "") == "done")
        #expect(t.line(volatile: "live") == "done live")
    }

    @Test func closeStartsANewTurn() {
        var t = TurnBuilder(maxChars: 180)
        _ = t.finalize("a.")
        let id = t.turnID
        t.close()
        #expect(t.isEmpty)
        #expect(t.turnID == id + 1)
    }
}
