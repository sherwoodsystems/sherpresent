import Foundation
import Testing

@testable import sherpresent_output

@Suite struct SlideshowCaptureTests {
    private let powerPoint = "com.microsoft.Powerpoint"

    @Test func matchesOnlyTheAudienceSlideShowWindow() {
        #expect(
            SlideshowCapture.isSlideshowWindow(
                bundleID: powerPoint, title: "PowerPoint Slide Show - [Deck.pptx]"))
        #expect(
            SlideshowCapture.isSlideshowWindow(
                bundleID: "com.microsoft.powerpoint", title: "PowerPoint Slide Show - Deck"))

        #expect(
            !SlideshowCapture.isSlideshowWindow(
                bundleID: powerPoint, title: "PowerPoint Presenter View - [Deck.pptx]"))
        #expect(!SlideshowCapture.isSlideshowWindow(bundleID: powerPoint, title: "Deck.pptx"))
        #expect(!SlideshowCapture.isSlideshowWindow(bundleID: powerPoint, title: nil))
        #expect(!SlideshowCapture.isSlideshowWindow(bundleID: "com.apple.iWork.Keynote", title: "Slide Show"))
        #expect(!SlideshowCapture.isSlideshowWindow(bundleID: nil, title: "PowerPoint Slide Show"))
    }

    @Test func cropsToTheCentredSixteenByNineSlide() {
        // MacBook-shaped window: bars top and bottom.
        #expect(
            SlideshowCapture.slideRect(in: CGSize(width: 1800, height: 1169))
                == CGRect(x: 0, y: 78.25, width: 1800, height: 1012.5))
        // Notched MacBook: centred in the area below the notch.
        #expect(
            SlideshowCapture.slideRect(in: CGSize(width: 1800, height: 1169), topInset: 38)
                == CGRect(x: 0, y: 97.25, width: 1800, height: 1012.5))
        // Ultrawide: bars left and right.
        #expect(
            SlideshowCapture.slideRect(in: CGSize(width: 3440, height: 1440))
                == CGRect(x: 440, y: 0, width: 2560, height: 1440))
        // Already 16:9: the whole window.
        #expect(
            SlideshowCapture.slideRect(in: CGSize(width: 1920, height: 1080))
                == CGRect(x: 0, y: 0, width: 1920, height: 1080))
    }

    @Test func slideshowContentParses() throws {
        let args = try Args.parse(["--protocol", "4", "--content", "slideshow", "--name", "Slides"])
        #expect(args.content == "slideshow")
        #expect(args.serverName == "Slides")
    }

    @Test func captureStateLine() throws {
        let line = OutMessage.capture(state: "capturing").line
        let wire = try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: String]
        #expect(wire == ["type": "capture", "state": "capturing"])
    }
}
