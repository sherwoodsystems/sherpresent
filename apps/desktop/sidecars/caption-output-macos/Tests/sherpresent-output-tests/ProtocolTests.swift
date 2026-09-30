import Testing

@testable import sherpresent_output

@Suite struct ProtocolTests {
    @Test func settingsDecodeIncludingBoxColour() {
        let line =
            ##"{"type":"settings","settings":{"fontSize":72,"maxLines":3,"background":true,"boxColor":"#223344","chromaColor":"#00B140"}}"##
        guard case .settings(let s) = InMessage.parse(line) else {
            Issue.record("not parsed as settings")
            return
        }
        #expect(s.fontSize == 72)
        #expect(s.maxLines == 3)
        #expect(s.background)
        #expect(s.boxColor == 0x223344)
    }

    @Test func missingSettingsFieldsFallBackToDefaults() {
        // An older app that doesn't send a field must not break decoding.
        guard case .settings(let s) = InMessage.parse(#"{"type":"settings","settings":{}}"#) else {
            Issue.record("not parsed as settings")
            return
        }
        #expect(s == OverlaySettings())
    }

    @Test func segmentAndReplay() {
        let line =
            #"{"type":"segment","segment":{"id":3,"source":"hi","translated":"salut","final":true,"timestamp":0}}"#
        guard case .segment(let seg) = InMessage.parse(line) else {
            Issue.record("not parsed as segment")
            return
        }
        #expect(seg == Segment(id: 3, source: "hi", translated: "salut", final: true))

        guard case .replay(let segs) = InMessage.parse(#"{"type":"replay","segments":[]}"#) else {
            Issue.record("not parsed as replay")
            return
        }
        #expect(segs.isEmpty)
    }

    @Test func unknownAndMalformedLinesAreIgnored() {
        for line in ["", "garbage", #"{"type":"future"}"#, #"{"type":"segment"}"#] {
            guard case .unknown = InMessage.parse(line) else {
                Issue.record("\(line) should be ignored")
                continue
            }
        }
    }

    @Test func notesKeysBecomeSlideNumbers() {
        guard
            case .notes(let notes) = NotesMessage.parse(
                #"{"type":"notes","payload":{"1":"Intro","x":"skip"}}"#)
        else {
            Issue.record("not parsed as notes")
            return
        }
        #expect(notes == [1: "Intro"])
    }

    @Test func nullTimerMeansOntimeIsOff() {
        guard case .timer(let t) = NotesMessage.parse(#"{"type":"timer","payload":null}"#) else {
            Issue.record("not parsed as timer")
            return
        }
        #expect(t == nil)
    }
}
