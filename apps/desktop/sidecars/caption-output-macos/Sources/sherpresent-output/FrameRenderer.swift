import CoreGraphics
import CoreText
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

/// Draws caption rows onto a transparent frame.
///
/// Only draws when asked; the caller renders on change, never on a timer, so
/// an idle show costs nothing.
@MainActor
final class FrameRenderer {
    let device: MTLDevice
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

        self.device = device
        self.context = ctx
        self.frame = Frame(surface: surface, texture: texture, width: width, height: height)
    }

    func render(rows: [Row], layout: CaptionLayout) {
        let surface = frame.surface
        surface.lock(options: [], seed: nil)
        defer { surface.unlock(options: [], seed: nil) }

        let ctx = context
        let w = CGFloat(frame.width)
        ctx.clear(CGRect(x: 0, y: 0, width: w, height: CGFloat(frame.height)))

        let font = layout.font()
        let ascent = CTFontGetAscent(font)
        let descent = CTFontGetDescent(font)
        let rowHeight = layout.rowHeight
        // CSS half-leading: the glyph box sits centred in its row.
        let baselineInRow = (rowHeight - (ascent + descent)) / 2 + descent

        if layout.settings.background {
            // Closed-caption boxes, one per row, sized like the web overlay's
            // inline background: the glyph box plus 0.3em each side. Opaque,
            // matching the web overlay, and drawn before the shadow is set so
            // the boxes themselves cast none.
            let pad = (0.3 * layout.fontSize).rounded()
            let boxHeight = ascent + descent
            ctx.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
            for (i, row) in rows.reversed().enumerated() where row.width > 0 {
                let rowBottom = layout.bottomInset + CGFloat(i) * rowHeight
                ctx.fill(
                    CGRect(
                        x: ((w - row.width) / 2 - pad).rounded(),
                        y: (rowBottom + (rowHeight - boxHeight) / 2).rounded(),
                        width: (row.width + 2 * pad).rounded(),
                        height: boxHeight.rounded()))
            }
        }

        ctx.saveGState()
        ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        if layout.settings.shadow {
            // One pass of the web overlay's three-layer text-shadow; close
            // enough on a key, and cheap.
            ctx.setShadow(
                offset: CGSize(width: 0, height: -0.035 * layout.fontSize),
                blur: 0.18 * layout.fontSize,
                color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.9))
        }

        // Core Graphics is bottom-up: row 0 from the bottom is the newest.
        for (i, row) in rows.reversed().enumerated() {
            let y = layout.bottomInset + CGFloat(i) * rowHeight + baselineInRow
            let x = (w - row.width) / 2
            ctx.textPosition = CGPoint(x: x.rounded(), y: y.rounded())
            CTLineDraw(row.line, ctx)
        }
        ctx.restoreGState()
        ctx.flush()
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
