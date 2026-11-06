import Foundation

let args: Args
do {
    args = try Args.parse(Array(CommandLine.arguments.dropFirst()))
} catch {
    emit(.error(code: "badArguments", fatal: true, message: "\(error)"))
    exit(2)
}

guard args.protocolVersion == WireProtocol.version else {
    emit(
        .error(
            code: "protocolMismatch", fatal: true,
            message: "App speaks protocol \(args.protocolVersion), helper speaks \(WireProtocol.version)"))
    exit(2)
}

let app: OutputApp
do {
    app = try OutputApp(args: args)
} catch {
    emit(.error(code: "startupFailed", fatal: true, message: "\(error)"))
    exit(3)
}
app.start()

// stdin on a background task; every message hops to the main actor.
Task.detached {
    do {
        for try await line in FileHandle.standardInput.bytes.lines {
            await app.handle(line)
        }
    } catch {
        logErr("stdin read failed: \(error.localizedDescription)", level: "warn")
    }
    await app.finish()
}

// A real run loop, not just the main dispatch queue: Syphon's server
// discovery answers clients via distributed notifications, which are delivered
// through the main run loop.
RunLoop.main.run()
