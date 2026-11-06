import AppKit
@preconcurrency import CoreMedia
import Foundation
import Metal
@preconcurrency import ScreenCaptureKit

/// `--content slideshow`: captures PowerPoint's slide show window with
/// ScreenCaptureKit and publishes it to the sinks, but only while a show is
/// running. Between shows nothing is published, so receivers hold the last
/// slide.
///
/// Unlike `FrameContent` this is video, not a picture drawn on change, so it
/// bypasses `FrameRenderer`: each captured IOSurface goes straight to the
/// sinks as a texture.
///
/// Needs Screen Recording permission, which macOS attributes to the app that
/// launched us (SherPresent, or the terminal in development).
@MainActor
final class SlideshowCapture: NSObject {
    enum State: String {
        /// No slide show window (or PowerPoint isn't running).
        case waiting
        case capturing
        /// Screen Recording permission hasn't been granted.
        case denied
    }

    private static let pollInterval: Duration = .seconds(1)

    private let device: MTLDevice
    private let sinks: [FrameSink]
    private var stream: SCStream?
    private var windowID: CGWindowID?
    /// The captured window's size; a change means a new crop.
    private var windowSize: CGSize?
    private var state: State?
    private var lastLoggedWindows: [String] = []

    init(device: MTLDevice, sinks: [FrameSink]) {
        self.device = device
        self.sinks = sinks
    }

    /// Whether a window is PowerPoint's audience-facing slide show, as
    /// opposed to an editing window or Presenter View. The one place to teach
    /// it another app (Keynote) later.
    nonisolated static func isSlideshowWindow(bundleID: String?, title: String?) -> Bool {
        guard bundleID?.lowercased() == "com.microsoft.powerpoint", let title else { return false }
        return title.localizedCaseInsensitiveContains("Slide Show")
            && !title.localizedCaseInsensitiveContains("Presenter")
    }

    /// The 16:9 area of a window of `size`, in its own coordinates: where
    /// PowerPoint draws a 16:9 slide. It centres the slide in the part of the
    /// display below `topInset` (the notch on a MacBook). A 4:3 deck in a
    /// taller window loses its top and bottom edges; 16:9 is the default and
    /// what the frame is anyway.
    nonisolated static func slideRect(in size: CGSize, topInset: CGFloat = 0) -> CGRect {
        let aspect = CGFloat(Args.width) / CGFloat(Args.height)
        let usable = CGSize(width: size.width, height: size.height - topInset)
        guard usable.width > 0, usable.height > 0 else { return .zero }
        if usable.width / usable.height > aspect {
            let w = usable.height * aspect
            return CGRect(x: (usable.width - w) / 2, y: topInset, width: w, height: usable.height)
        }
        let h = usable.width / aspect
        return CGRect(x: 0, y: topInset + (usable.height - h) / 2, width: usable.width, height: h)
    }

    /// The notch inset of the display a full-screen window fills, or 0.
    private static func topInset(for window: SCWindow) -> CGFloat {
        let screen = NSScreen.screens.first { $0.frame.size == window.frame.size }
        return screen?.safeAreaInsets.top ?? 0
    }

    func start() {
        Task { @MainActor in
            while !Task.isCancelled {
                await poll()
                try? await Task.sleep(for: Self.pollInterval)
            }
        }
    }

    func stop() {
        stopStream()
    }

    private func poll() async {
        let content: SCShareableContent
        do {
            content = try await SCShareableContent.excludingDesktopWindows(
                false, onScreenWindowsOnly: false)
        } catch {
            // Without the permission this is the only call that fails, and it
            // keeps failing until it's granted; keep polling so granting it
            // mid-show just works.
            stopStream()
            setState(.denied, detail: error.localizedDescription)
            return
        }

        logPowerPointWindows(content.windows)
        let window = content.windows.first {
            Self.isSlideshowWindow(bundleID: $0.owningApplication?.bundleIdentifier, title: $0.title)
        }
        guard let window else {
            stopStream()
            setState(.waiting)
            return
        }
        // Moving the show to another display can re-create or resize the
        // window.
        if window.windowID != windowID || window.frame.size != windowSize || stream == nil {
            stopStream()
            await startStream(window)
        }
    }

    private func startStream(_ window: SCWindow) async {
        let config = SCStreamConfiguration()
        config.width = Args.width
        config.height = Args.height
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
        config.showsCursor = false
        config.scalesToFit = true
        // PowerPoint letterboxes the slide inside a window shaped like the
        // display (16:10 on a MacBook); take just the slide so it fills the
        // frame instead of keeping PowerPoint's bars.
        config.sourceRect = Self.slideRect(in: window.frame.size, topInset: Self.topInset(for: window))
        config.queueDepth = 3

        let filter = SCContentFilter(desktopIndependentWindow: window)
        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        do {
            // Main queue: frames hop straight to the sinks, which live on the
            // main actor. A 1080p blit per frame is well under a millisecond.
            try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: .main)
            try await stream.startCapture()
        } catch {
            logErr("slideshow capture failed to start: \(error.localizedDescription)", level: "warn")
            setState(.waiting)
            return
        }
        self.stream = stream
        self.windowID = window.windowID
        self.windowSize = window.frame.size
        logErr("capturing \"\(window.title ?? "")\" (window \(window.windowID))")
        setState(.capturing)
    }

    private func stopStream() {
        guard let stream else { return }
        self.stream = nil
        windowID = nil
        windowSize = nil
        // The window is usually already gone, so a failed stop is expected.
        stream.stopCapture { _ in }
    }

    fileprivate func publish(_ sampleBuffer: CMSampleBuffer) {
        guard sampleBuffer.isValid, Self.isComplete(sampleBuffer),
            let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer),
            let surface = CVPixelBufferGetIOSurface(pixelBuffer)?.takeUnretainedValue()
        else { return }

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
        desc.usage = [.shaderRead]
        desc.storageMode = .shared
        guard let texture = device.makeTexture(descriptor: desc, iosurface: surface, plane: 0) else {
            return
        }
        // Sinks finish their copy before returning, so the surface can go back
        // to ScreenCaptureKit's pool as soon as we do.
        for sink in sinks { sink.publish(texture: texture, width: width, height: height) }
    }

    /// Only `.complete` frames carry new pixels; `.idle` means unchanged.
    private nonisolated static func isComplete(_ sampleBuffer: CMSampleBuffer) -> Bool {
        guard
            let attachments = CMSampleBufferGetSampleAttachmentsArray(
                sampleBuffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
            let raw = attachments.first?[.status] as? Int,
            let status = SCFrameStatus(rawValue: raw)
        else { return false }
        return status == .complete
    }

    private func setState(_ new: State, detail: String? = nil) {
        guard new != state else { return }
        state = new
        if let detail { logErr("slideshow: \(new.rawValue): \(detail)", level: "warn") }
        emit(.capture(state: new.rawValue))
    }

    /// When the set of PowerPoint windows changes, list them on stderr: the
    /// title match is the fragile part, so this is what to look at when a
    /// show isn't picked up.
    private func logPowerPointWindows(_ windows: [SCWindow]) {
        let titles =
            windows
            .filter { $0.owningApplication?.bundleIdentifier.lowercased() == "com.microsoft.powerpoint" }
            .map { "\"\($0.title ?? "")\" \(Int($0.frame.width))x\(Int($0.frame.height))" }
            .sorted()
        guard titles != lastLoggedWindows else { return }
        lastLoggedWindows = titles
        logErr("PowerPoint windows: \(titles.isEmpty ? "none" : titles.joined(separator: ", "))")
    }
}

extension SlideshowCapture: SCStreamOutput, SCStreamDelegate {
    nonisolated func stream(
        _ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType
    ) {
        guard type == .screen else { return }
        // Delivered on the main queue (see `startStream`) and used
        // synchronously, so the buffer never actually crosses threads.
        nonisolated(unsafe) let buffer = sampleBuffer
        MainActor.assumeIsolated { publish(buffer) }
    }

    nonisolated func stream(_ stream: SCStream, didStopWithError error: Error) {
        let id = ObjectIdentifier(stream)
        logErr("slideshow capture stopped: \(error.localizedDescription)", level: "warn")
        Task { @MainActor in
            // The next poll restarts it if the window is still there.
            if let current = self.stream, ObjectIdentifier(current) == id {
                self.stream = nil
                self.windowID = nil
            }
        }
    }
}
