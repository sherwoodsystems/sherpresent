import Testing

@testable import sherpresent_output

@MainActor
@Suite struct NotesPagingTests {
    private let page = #"{"type":"scroll","payload":{"direction":"page","pixels":150}}"#

    private func status(_ slide: Int) -> String {
        #"{"type":"status","payload":{"current_slide":\#(slide),"total_slides":3,"is_presenting":true}}"#
    }

    /// Slide 1 has 30 full-width lines (two pages); slide 2 one line.
    private func content() -> NotesContent {
        let line =
            "Line: welcome everyone, this is a long talking point that keeps going so the notes overflow."
        let long = Array(repeating: line, count: 30).joined(separator: "\\n")
        let c = NotesContent()
        _ = c.handle(status(1))
        _ = c.handle(#"{"type":"notes","payload":{"1":"\#(long)","2":"Short"}}"#)
        return c
    }

    @Test func pagesThenWrapsToTheTop() {
        let c = content()
        #expect(c.handle(page))
        #expect(c.firstLine > 0)
        // Second page reaches the end; the next press goes back to the top.
        #expect(c.handle(page))
        #expect(c.firstLine == 0)
    }

    @Test func slideChangeStartsAtTheTop() {
        let c = content()
        _ = c.handle(page)
        _ = c.handle(status(2))
        #expect(c.firstLine == 0)
        _ = c.handle(status(1))
        #expect(c.firstLine == 0)
    }

    @Test func notesThatFitDoNotMove() {
        let c = content()
        _ = c.handle(status(2))
        #expect(!c.handle(page))
        #expect(c.firstLine == 0)
    }

    @Test func upGoesBackAPageAndDownStopsAtTheEnd() {
        let c = content()
        _ = c.handle(#"{"type":"scroll","payload":{"direction":"down"}}"#)
        let second = c.firstLine
        #expect(second > 0)
        #expect(!c.handle(#"{"type":"scroll","payload":{"direction":"down"}}"#))
        #expect(c.firstLine == second)
        _ = c.handle(#"{"type":"scroll","payload":{"direction":"up"}}"#)
        #expect(c.firstLine == 0)
    }
}
