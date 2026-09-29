import Foundation
import Metal

/// Owns caption state, the renderer and the sinks. Everything runs on the main
/// actor: messages arrive a few times a second at most and a 1080p text
/// render takes about a millisecond, so there is nothing to parallelize.
@MainActor
final class OutputApp {
    private let args: Args
    private let renderer: FrameRenderer
    private let sinks: [FrameSink]
    private var state = CaptionState()
    private var settings = OverlaySettings()
    private var lastTexts: [String] = []
    private var lastSinkStatus: [SinkStatus] = []

    init(args: Args) throws {
        guard let device = MTLCreateSystemDefaultDevice() else {
            throw OutputError("No Metal device available")
        }
        guard let renderer = FrameRenderer(device: device, width: args.width, height: args.height) else {
            throw OutputError("Could not allocate a \(args.width)x\(args.height) frame")
        }
        self.args = args
        self.renderer = renderer
        // --render-png is an offline render: no sinks, nothing published.
        self.sinks = args.renderPNG == nil ? try makeSinks(args.sinks, args: args, device: device) : []
        state.keepSegments = settings.maxLines * 2 + 2
    }

    func start() {
        // Publish one empty frame up front so a receiver can be wired up and
        // show transparency before anyone speaks.
        redraw(force: true)
        lastSinkStatus = sinks.map(\.status)
        emit(.ready(width: args.width, height: args.height, sinks: lastSinkStatus))

        // Receivers come and go; report it so Settings can show "connected".
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            MainActor.assumeIsolated { self.reportSinkStatus() }
        }
    }

    func handle(_ line: String) {
        switch InMessage.parse(line) {
        case .segment(let seg): state.apply(seg)
        case .replay(let segs): state.replay(segs)
        case .status(let s): state.translateEnabled = s.translateEnabled ?? false
        case .settings(let s):
            let changed = s != settings
            settings = s
            state.keepSegments = s.maxLines * 2 + 2
            if changed {
                redraw(force: true)
                return
            }
        case .unknown: return
        }
        redraw(force: false)
    }

    /// stdin closed: the app is stopping this output.
    func finish() -> Never {
        if let path = args.renderPNG {
            do {
                try renderer.writePNG(to: path)
            } catch {
                logErr("render-png failed: \(error.localizedDescription)", level: "error")
                exit(1)
            }
        }
        sinks.forEach { $0.stop() }
        exit(0)
    }

    private func redraw(force: Bool) {
        let layout = CaptionLayout(
            frameWidth: CGFloat(args.width), frameHeight: CGFloat(args.height), settings: settings)
        let texts = state.visibleTexts()

        // Partials repeat often with no visible change; skip identical frames
        // (Syphon asks publishers to only send frames that differ). Rows are a
        // pure function of texts + settings, and a settings change forces a
        // redraw, so comparing the texts is enough.
        if !force && texts == lastTexts { return }
        lastTexts = texts

        let rows = layout.rows(for: texts)

        renderer.render(rows: rows, layout: layout)
        for sink in sinks { sink.publish(renderer.frame) }
    }

    private func reportSinkStatus() {
        let now = sinks.map(\.status)
        let changed = zip(now, lastSinkStatus).contains { $0.hasClients != $1.hasClients }
        if changed {
            lastSinkStatus = now
            emit(.sinks(now))
        }
    }
}

struct OutputError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = d }
}

