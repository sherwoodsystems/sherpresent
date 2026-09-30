import CoreGraphics
import Foundation
import IOSurface
import ImageIO
import Metal

/// One rendered frame: a BGRA IOSurface, and a Metal texture over the same
/// memory. Syphon wants the texture; a CPU sink such as NDI would read the
/// surface directly. Premultiplied alpha, which is what Core Graphics draws
/// and what Metal and Syphon receivers expect.
struct Frame {
    let surface: IOSurface
    let texture: MTLTexture
    let width: Int
    let height: Int
}

/// The frame a `FrameContent` draws onto.
///
/// Only draws when asked; the caller renders on change, never on a timer, so
/// an idle show costs nothing.
@MainActor
final class FrameRenderer {
    let frame: Frame
    private let context: CGContext

    init?(device: MTLDevice, width: Int, height: Int) {
        let props: [IOSurfacePropertyKey: any Sendable] = [
            .width: width,
            .height: height,
            .bytesPerElement: 4,
            .pixelFormat: 0x4247_5241,  // 'BGRA'
        ]
        guard let surface = IOSurface(properties: props) else { return nil }

        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
        desc.usage = [.shaderRead]
        desc.storageMode = .shared
        guard let texture = device.makeTexture(descriptor: desc, iosurface: surface, plane: 0) else {
            return nil
        }

        surface.lock(options: [], seed: nil)
        defer { surface.unlock(options: [], seed: nil) }
        guard
            let ctx = CGContext(
                data: surface.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: surface.bytesPerRow,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
                    | CGBitmapInfo.byteOrder32Little.rawValue)
        else { return nil }

        self.context = ctx
        self.frame = Frame(surface: surface, texture: texture, width: width, height: height)
    }

    /// Clear the frame to transparent, let `draw` paint it, and flush.
    func render(_ draw: (CGContext, CGFloat, CGFloat) -> Void) {
        let surface = frame.surface
        surface.lock(options: [], seed: nil)
        defer { surface.unlock(options: [], seed: nil) }

        let w = CGFloat(frame.width)
        let h = CGFloat(frame.height)
        context.clear(CGRect(x: 0, y: 0, width: w, height: h))
        draw(context, w, h)
        context.flush()
    }

    /// PNG of the current frame, for `--render-png` and tests.
    func writePNG(to path: String) throws {
        let surface = frame.surface
        surface.lock(options: .readOnly, seed: nil)
        defer { surface.unlock(options: .readOnly, seed: nil) }
        guard let image = context.makeImage() else {
            throw NSError(domain: "render", code: 1, userInfo: [NSLocalizedDescriptionKey: "makeImage failed"])
        }
        let url = URL(fileURLWithPath: path) as CFURL
        guard let dest = CGImageDestinationCreateWithURL(url, "public.png" as CFString, 1, nil) else {
            throw NSError(domain: "render", code: 2, userInfo: [NSLocalizedDescriptionKey: "cannot write \(path)"])
        }
        CGImageDestinationAddImage(dest, image, nil)
        guard CGImageDestinationFinalize(dest) else {
            throw NSError(domain: "render", code: 3, userInfo: [NSLocalizedDescriptionKey: "PNG encode failed"])
        }
    }
}
