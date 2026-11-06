import Testing

@testable import sherpresent_speech

@Suite struct ArgsTests {
    private let base = ["--protocol", "\(Protocol.version)", "--source", "en-US"]

    @Test func translatingNeedsATarget() {
        #expect(throws: Args.ParseError.self) { try Args.parse(base) }
        let args = try? Args.parse(base + ["--target", "fr"])
        #expect(args?.translate == true)
        #expect(args?.target == "fr")
    }

    @Test func straightCaptionsNeedNoTarget() throws {
        let args = try Args.parse(base + ["--no-translate"])
        #expect(!args.translate)
    }

    @Test func probeAcceptsABlankTarget() throws {
        // How Rust asks about straight captions: no translation pack check.
        let args = try Args.parse(["--probe"] + base + ["--target", ""])
        #expect(args.probe)
        #expect(args.target.isEmpty)
    }

    @Test func rejectsStaleProtocolAndUnknownFlags() {
        #expect(throws: Args.ParseError.self) {
            try Args.parse(["--protocol", "0", "--source", "en"])
        }
        #expect(throws: Args.ParseError.self) { try Args.parse(base + ["--bogus"]) }
    }

    @Test func sourceIsRequired() {
        #expect(throws: Args.ParseError.self) {
            try Args.parse(["--protocol", "\(Protocol.version)", "--no-translate"])
        }
    }
}
