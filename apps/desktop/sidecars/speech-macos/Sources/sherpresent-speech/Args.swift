import Foundation

struct Args: Sendable {
    var probe = false
    var source = ""
    var target = ""
    var sampleRate: Double = 16000
    var channels: UInt32 = 1
    var format = "s16le"
    var translate = true
    var turnMaxChars = 180
    var turnSilenceMs = 1200

    enum ParseError: Error, CustomStringConvertible {
        case unknownFlag(String)
        case missingValue(String)
        case badValue(String, String)
        case protocolMismatch(Int)
        case missingRequired(String)

        var description: String {
            switch self {
            case let .unknownFlag(f): return "unknown flag '\(f)'"
            case let .missingValue(f): return "'\(f)' needs a value"
            case let .badValue(f, v): return "'\(f)' got an invalid value '\(v)'"
            case let .protocolMismatch(v):
                return
                    "protocol \(v) is not supported (this helper speaks \(Protocol.version)); rebuild the speech helper"
            case let .missingRequired(f): return "'\(f)' is required"
            }
        }
    }

    /// Strict on purpose: a stale bundled helper meeting newer Rust must fail
    /// loudly rather than run with silently-dropped options.
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
            case "--probe": args.probe = true
            case "--no-translate": args.translate = false
            case "--protocol":
                let raw = try value(flag)
                guard let v = Int(raw) else { throw ParseError.badValue(flag, raw) }
                guard v == Protocol.version else { throw ParseError.protocolMismatch(v) }
            case "--source": args.source = try value(flag)
            case "--target": args.target = try value(flag)
            case "--sample-rate":
                let raw = try value(flag)
                guard let v = Double(raw), v > 0 else { throw ParseError.badValue(flag, raw) }
                args.sampleRate = v
            case "--channels":
                let raw = try value(flag)
                guard let v = UInt32(raw), v > 0 else { throw ParseError.badValue(flag, raw) }
                args.channels = v
            case "--format":
                let raw = try value(flag)
                guard raw == "s16le" else { throw ParseError.badValue(flag, raw) }
                args.format = raw
            case "--turn-max-chars":
                let raw = try value(flag)
                guard let v = Int(raw), v > 0 else { throw ParseError.badValue(flag, raw) }
                args.turnMaxChars = v
            case "--turn-silence-ms":
                let raw = try value(flag)
                guard let v = Int(raw), v > 0 else { throw ParseError.badValue(flag, raw) }
                args.turnSilenceMs = v
            case "--log-level":
                _ = try value(flag)
            default:
                throw ParseError.unknownFlag(flag)
            }
            i += 1
        }

        guard !args.source.isEmpty else { throw ParseError.missingRequired("--source") }
        if args.translate && !args.probe && args.target.isEmpty {
            throw ParseError.missingRequired("--target")
        }
        return args
    }
}
