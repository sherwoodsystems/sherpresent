import Testing

@testable import sherpresent_output

@Suite struct ColorTests {
    @Test func parsesLongAndShortHex() {
        #expect(hexColor("#00b140") == 0x00B140)
        #expect(hexColor("fff") == 0xFFFFFF)
        #expect(hexColor("#123") == 0x112233)
    }

    @Test func rejectsAnythingElse() {
        #expect(hexColor("transparent") == nil)
        #expect(hexColor("#12345") == nil)
        #expect(hexColor("#zzzzzz") == nil)
    }
}
