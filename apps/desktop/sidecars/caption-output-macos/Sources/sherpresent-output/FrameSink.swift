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
    func stop()
}

@MainActor
func makeSinks(_ kinds: [String], args: Args, device: MTLDevice) throws -> [FrameSink] {
    try kinds.map { kind in
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

    func publish(_ frame: Frame) {
        guard let buffer = queue?.makeCommandBuffer() else { return }
        // Syphon surfaces are bottom-up (the OpenGL convention its receivers
        // read them in), while our texture is top-down like any Metal texture.
        // `flipped: false` copies it through as-is, and OBS showed it upside
        // down; `true` makes the server write it bottom-up.
        server.publishFrameTexture(
            frame.texture, on: buffer,
            imageRegion: NSRect(x: 0, y: 0, width: frame.width, height: frame.height),
            flipped: true)
        buffer.commit()
    }

    func stop() {
        server.stop()
    }
}
