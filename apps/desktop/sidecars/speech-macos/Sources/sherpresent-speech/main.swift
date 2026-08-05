import Foundation
import Speech
import Translation

let emitter = Emitter()

let args: Args
do {
    args = try Args.parse(Array(CommandLine.arguments.dropFirst()))
} catch {
    await emitter.emit(.error(code: .badArguments, fatal: true, message: "\(error)"))
    exit(2)
}

// Belt-and-braces: the deployment target already prevents launching here, but a
// clear message beats "Bad CPU type" or a dyld abort.
if #unavailable(macOS 26.0) {
    await emitter.emit(
        .error(
            code: .unsupportedOS, fatal: true,
            message: "The Apple on-device caption provider requires macOS 26 (Tahoe) or later."))
    exit(1)
}

if args.probe {
    await Availability.probe(args: args, emitter: emitter)
    exit(0)
}

await Transcription(args: args, emitter: emitter).run()
exit(0)
