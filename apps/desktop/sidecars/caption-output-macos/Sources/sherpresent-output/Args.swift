import Foundation

/// Command-line options. Everything that changes during a show (font size,
/// lines, safe area...) arrives over stdin as `settings` instead, so these are
/// only the things fixed for the life of the process.
struct Args {
    var protocolVersion = 0
    /// Frame sinks to publish to. Only `syphon` exists today; NDI would be
    /// another value here plus another `FrameSink`.
    var sinks: [String] = ["syphon"]
    var serverName = "SherPresent Captions"
    var width = 1920
    var height = 1080
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
        func int(_ flag: String) throws -> Int {
            let v = try value(flag)
            guard let n = Int(v), n > 0 else { throw ParseError.badValue(flag, v) }
            return n
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
            case "--width": args.width = try int(flag)
            case "--height": args.height = try int(flag)
            case "--render-png": args.renderPNG = try value(flag)
            default: throw ParseError.unknownFlag(flag)
            }
            i += 1
        }

        guard args.protocolVersion != 0 else { throw ParseError.missingRequired("--protocol") }
        return args
    }
}
