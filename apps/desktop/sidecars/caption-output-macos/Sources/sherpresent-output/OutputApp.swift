import Foundation
import Metal

/// Owns the content, the renderer and the sinks. Everything runs on the main
/// actor: messages arrive a few times a second at most and a 1080p text
/// render takes about a millisecond, so there is nothing to parallelize.
@MainActor
final class OutputApp {
    private let args: Args
    private let renderer: FrameRenderer
    private let sinks: [FrameSink]
    private let content: FrameContent
    private var lastSinkStatus: [SinkStatus] = []

    init(args: Args) throws {
        guard let device = MTLCreateSystemDefaultDevice() else {
            throw OutputError("No Metal device available")
        }
        guard let renderer = FrameRenderer(device: device, width: Args.width, height: Args.height) else {
            throw OutputError("Could not allocate a \(Args.width)x\(Args.height) frame")
        }
        self.args = args
        self.renderer = renderer
        self.content = try makeContent(args.content, text: args.text)
        // --render-png is an offline render: no sinks, nothing published.
        self.sinks = args.renderPNG == nil ? try makeSinks(args: args, device: device) : []
    }

    func start() {
        // Publish one frame up front so a receiver can be wired up before
        // anything happens.
        redraw()
        lastSinkStatus = sinks.map(\.status)
        emit(.ready(width: Args.width, height: Args.height, sinks: lastSinkStatus))

        // Receivers come and go; report it so Settings can show "connected".
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            MainActor.assumeIsolated { self.reportSinkStatus() }
        }
    }

    func handle(_ line: String) {
        if content.handle(line) { redraw() }
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
        for sink in sinks { sink.stop() }
        exit(0)
    }

    private func redraw() {
        renderer.render { ctx, w, h in content.draw(in: ctx, width: w, height: h) }
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
