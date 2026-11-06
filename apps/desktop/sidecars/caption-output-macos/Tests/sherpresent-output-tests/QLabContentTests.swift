import Testing

@testable import sherpresent_output

@MainActor
@Suite struct QLabContentTests {
    private func line(video: String = "null", audio: String = "null", next: String = "null") -> String {
        #"{"type":"qlab","payload":{"configured":true,"connected":true,"message":null,"workspace":"Show","workspaces":["Show"],"video":\#(video),"audio":\#(audio),"next":\#(next)}}"#
    }

    private func cue(_ number: String, remaining: Double, duration: Double, paused: Bool = false) -> String {
        #"{"number":"\#(number)","name":"Film","remaining":\#(remaining),"duration":\#(duration),"paused":\#(paused),"others":0}"#
    }

    @Test func timeRoundsUpSoZeroOnlyShowsAtTheEnd() {
        #expect(QLabContent.formatTime(9.2) == "0:10")
        #expect(QLabContent.formatTime(10) == "0:10")
        #expect(QLabContent.formatTime(0.05) == "0:01")
        #expect(QLabContent.formatTime(0) == "0:00")
        #expect(QLabContent.formatTime(-3) == "0:00")
        #expect(QLabContent.formatTime(75) == "1:15")
        #expect(QLabContent.formatTime(3725) == "1:02:05")
    }

    @Test func layoutFollowsWhatIsRunning() throws {
        let one = try #require(QLabMessage.parse(line(video: cue("5", remaining: 30, duration: 120))))
        guard case .one(let c) = QLabContent.picture(one) else {
            Issue.record("one cue should use the single layout")
            return
        }
        #expect(c.kind == .video)
        #expect(c.time == "0:30")
        #expect(c.left == 250)
        #expect(c.label == "5  ·  Film")

        let both = try #require(
            QLabMessage.parse(
                line(
                    video: cue("5", remaining: 30, duration: 120), audio: cue("6", remaining: 8, duration: 10)
                )))
        guard case .two(let v, let a) = QLabContent.picture(both) else {
            Issue.record("video and audio should stack")
            return
        }
        #expect(v.kind == .video)
        #expect(a.kind == .audio)

        let idle = try #require(QLabMessage.parse(line(next: #"{"number":"7","name":"Interval"}"#)))
        #expect(QLabContent.picture(idle) == .idle(next: QLabCue(number: "7", name: "Interval")))
    }

    @Test func offlineShowsTheReason() throws {
        let p = try #require(
            QLabMessage.parse(
                #"{"type":"qlab","payload":{"configured":true,"connected":false,"message":"Wrong OSC passcode","video":null,"audio":null,"next":null}}"#
            ))
        #expect(QLabContent.picture(p) == .offline("Wrong OSC passcode"))
    }

    @Test func redrawsOnlyWhenTheSecondOrBarChanges() {
        let c = QLabContent()
        #expect(c.handle(line(video: cue("5", remaining: 29.95, duration: 3000))))
        // Same second, same thousandth of the bar: nothing to redraw.
        #expect(!c.handle(line(video: cue("5", remaining: 29.9, duration: 3000))))
        #expect(c.handle(line(video: cue("5", remaining: 28.9, duration: 3000))))
        #expect(!c.handle(#"{"type":"timer","payload":null}"#))
    }
}
