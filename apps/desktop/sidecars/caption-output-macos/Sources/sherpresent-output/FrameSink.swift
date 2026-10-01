import Foundation
import Metal
import Syphon

/// Somewhere rendered frames go.
///
/// The seam for adding outputs: an NDI sink would implement this, read
/// `frame.surface` (CPU BGRA, premultiplied) and hand it to `NDIlib_send`.
/// Register it in `makeSinks`.
@MainActor
protocol FrameSink: AnyObject {
    var status: SinkStatus { get }
    func publish(_ frame: Frame)
    /// A frame that isn't ours, such as a captured window. Must finish with
    /// `texture` before returning: its memory goes back to its owner.
    func publish(texture: MTLTexture, width: Int, height: Int)
    func stop()
}

extension FrameSink {
    func publish(_ frame: Frame) {
        publish(texture: frame.texture, width: frame.width, height: frame.height)
    }
}

@MainActor
func makeSinks(args: Args, device: MTLDevice) throws -> [FrameSink] {
    try args.sinks.map { kind in
        switch kind {
        case "syphon": return SyphonSink(name: args.serverName, device: device)
        default: throw Args.ParseError.badValue("--sink", kind)
        }
    }
}

/// Publishes to Syphon, for receivers on this Mac: OBS, Resolume, QLab,
/// Millumin, MadMapper...
@MainActor
final class SyphonSink: FrameSink {
    private let server: SyphonMetalServer
    private let queue: MTLCommandQueue?

    init(name: String, device: MTLDevice) {
        server = SyphonMetalServer(name: name, device: device, options: nil)
        queue = device.makeCommandQueue()
    }

    var status: SinkStatus {
        SinkStatus(kind: "syphon", name: server.name ?? "", hasClients: server.hasClients)
    }

    func publish(texture: MTLTexture, width: Int, height: Int) {
        guard let buffer = queue?.makeCommandBuffer() else { return }
        // Syphon surfaces are bottom-up (the OpenGL convention its receivers
        // read them in), while our texture is top-down like any Metal texture.
        // `flipped: false` copies it through as-is, and OBS showed it upside
        // down; `true` makes the server write it bottom-up.
        server.publishFrameTexture(
            texture, on: buffer,
            imageRegion: NSRect(x: 0, y: 0, width: width, height: height),
            flipped: true)
        buffer.commit()
        // The copy reads the same IOSurface the renderer draws the next frame
        // into on the CPU (or one ScreenCaptureKit reuses); finish it first so
        // a quick follow-up can't tear. A 1080p blit is well under a
        // millisecond.
        buffer.waitUntilCompleted()
    }

    func stop() {
        server.stop()
    }
}
