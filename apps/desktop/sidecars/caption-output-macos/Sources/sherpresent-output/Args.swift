import Foundation

/// Command-line options. Everything that changes during a show (styling,
/// slides, timer...) arrives over stdin instead, so these are only the things
/// fixed for the life of the process.
struct Args {
    var protocolVersion = 0
    /// Frame sinks to publish to. Only `syphon` exists today; NDI would be
    /// another value here plus another `FrameSink`.
    var sinks: [String] = ["syphon"]
    var serverName = "SherPresent Captions"
    /// What to draw: `captions` or `notes`. See `makeContent`.
    var content = "captions"
    /// Captions only: which language(s) to show. The web overlay's `?text=`.
    var text = CaptionText.translated
    /// Fixed frame size; not a flag because nothing needs another size yet.
    static let width = 1920
    static let height = 1080
    /// Render the final state to a PNG at EOF instead of publishing anywhere.
    /// Used by tests and for eyeballing layout against the web overlay.
    var renderPNG: String?

    enum ParseError: Error, CustomStringConvertible {
        case missingValue(String)
        case badValue(String, String)
        case unknownFlag(String)
        case missingRequired(String)

        var description: String {
            switch self {
            case .missingValue(let f): "\(f) needs a value"
            case .badValue(let f, let v): "bad value for \(f): \(v)"
            case .unknownFlag(let f): "unknown flag \(f)"
            case .missingRequired(let f): "\(f) is required"
            }
        }
    }

    static func parse(_ argv: [String]) throws -> Args {
        var args = Args()
        var i = 0
        func value(_ flag: String) throws -> String {
            i += 1
            guard i < argv.count else { throw ParseError.missingValue(flag) }
            return argv[i]
        }

        while i < argv.count {
            let flag = argv[i]
            switch flag {
            case "--protocol":
                let v = try value(flag)
                guard let n = Int(v) else { throw ParseError.badValue(flag, v) }
                args.protocolVersion = n
            case "--sink":
                args.sinks = try value(flag).split(separator: ",").map { String($0) }
            case "--name": args.serverName = try value(flag)
            case "--content": args.content = try value(flag)
            case "--text":
                let v = try value(flag)
                guard let t = CaptionText(rawValue: v) else { throw ParseError.badValue(flag, v) }
                args.text = t
            case "--render-png": args.renderPNG = try value(flag)
            default: throw ParseError.unknownFlag(flag)
            }
            i += 1
        }

        guard args.protocolVersion != 0 else { throw ParseError.missingRequired("--protocol") }
        return args
    }
}
