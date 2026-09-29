import CoreGraphics
import Foundation
import ImageIO
import Metal
import Syphon

// Dev tool, not shipped: connect to a Syphon server by name, grab one frame,
// and report what arrived — size, and how many pixels are transparent,
// opaque, or partially covered. Proves alpha survives the round trip.
//
//   syphon-probe "SherPresent Captions" [out.png] [--timeout 5]

let argv = Array(CommandLine.arguments.dropFirst())
guard let name = argv.first else {
    FileHandle.standardError.write(Data("usage: syphon-probe <server name> [out.png]\n".utf8))
    exit(2)
}
let outPath = argv.dropFirst().first { !$0.hasPrefix("--") }
let deadline = Date().addingTimeInterval(5)
let device = MTLCreateSystemDefaultDevice()!

enum Probe {
@MainActor static func findServer(_ name: String) -> [String: Any]? {
    SyphonServerDirectory.shared().servers.first {
        ($0[SyphonServerDescriptionNameKey] as? String) == name
    }
}

@MainActor static func report(_ texture: MTLTexture, device: MTLDevice, outPath: String?) -> Never {
    let w = texture.width, h = texture.height
    // Copy through a shared buffer: the client's texture may be private.
    let queue = device.makeCommandQueue()!
    let buffer = device.makeBuffer(length: w * h * 4, options: .storageModeShared)!
    let cmd = queue.makeCommandBuffer()!
    let blit = cmd.makeBlitCommandEncoder()!
    blit.copy(
        from: texture, sourceSlice: 0, sourceLevel: 0,
        sourceOrigin: MTLOrigin(x: 0, y: 0, z: 0), sourceSize: MTLSize(width: w, height: h, depth: 1),
        to: buffer, destinationOffset: 0, destinationBytesPerRow: w * 4, destinationBytesPerImage: w * h * 4)
    blit.endEncoding()
    cmd.commit()
    cmd.waitUntilCompleted()

    let px = buffer.contents().bindMemory(to: UInt8.self, capacity: w * h * 4)
    var clear = 0, opaque = 0, partial = 0, notPremultiplied = 0
    var topmostText = h
    for y in 0..<h {
        for x in 0..<w {
            let i = (y * w + x) * 4
            let a = px[i + 3]
            if a == 0 { clear += 1 } else if a == 255 { opaque += 1 } else { partial += 1 }
            if a > 0 { topmostText = min(topmostText, y) }
            // Premultiplied means no channel exceeds alpha.
            if px[i] > a || px[i + 1] > a || px[i + 2] > a { notPremultiplied += 1 }
        }
    }

    if let outPath {
        let ctx = CGContext(
            data: px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)!
        let dest = CGImageDestinationCreateWithURL(
            URL(fileURLWithPath: outPath) as CFURL, "public.png" as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
        CGImageDestinationFinalize(dest)
    }

    let json: [String: Any] = [
        "width": w, "height": h, "pixelFormat": texture.pixelFormat.rawValue,
        "clear": clear, "opaque": opaque, "partial": partial,
        "notPremultiplied": notPremultiplied, "topmostTextRow": topmostText,
    ]
    let data = try! JSONSerialization.data(withJSONObject: json, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
    exit(0)
}
}

var client: SyphonMetalClient?

Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
    MainActor.assumeIsolated {
        if Date() > deadline {
            FileHandle.standardError.write(Data("timed out waiting for \(name)\n".utf8))
            exit(1)
        }
        if client == nil, let desc = Probe.findServer(name) {
            client = SyphonMetalClient(serverDescription: desc, device: device, options: nil, newFrameHandler: nil)
        }
        if let c = client, let tex = c.newFrameImage() {
            Probe.report(tex, device: device, outPath: outPath)
        }
    }
}
RunLoop.main.run()
