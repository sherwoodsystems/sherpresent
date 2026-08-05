import Foundation

/// Messages written to stdout, one JSON object per line.
///
/// The Rust side (`captions/provider/apple.rs`) ignores unknown `type` values
/// and unparseable lines, so adding a message here is backwards compatible.
enum OutMessage {
    case ready(sourceLocale: String, targetLocale: String, translate: Bool, analyzerFormat: String)
    case assetProgress(stage: String, fraction: Double)
    case partial(source: String, translated: String)
    case final(source: String, translated: String)
    case turnComplete
    case error(code: ErrorCode, fatal: Bool, message: String)
    case availability(Availability)

    var json: [String: Any] {
        switch self {
        case let .ready(source, target, translate, format):
            return [
                "type": "ready",
                "protocol": Protocol.version,
                "sourceLocale": source,
                "targetLocale": target,
                "translate": translate,
                "analyzerFormat": format,
            ]
        case let .assetProgress(stage, fraction):
            return ["type": "assetProgress", "stage": stage, "fraction": fraction]
        case let .partial(source, translated):
            return ["type": "partial", "source": source, "translated": translated]
        case let .final(source, translated):
            return ["type": "final", "source": source, "translated": translated]
        case .turnComplete:
            return ["type": "turnComplete"]
        case let .error(code, fatal, message):
            return ["type": "error", "code": code.rawValue, "fatal": fatal, "message": message]
        case let .availability(a):
            return a.json
        }
    }
}

/// Error codes shared with `apple.rs`. `fatal` decides whether Rust respawns.
enum ErrorCode: String {
    case unsupportedOS
    case badArguments
    case localeUnsupported
    case translationNotInstalled
    case translationUnsupported
    case assetDownloadFailed
    case analyzerError
    case audioError
}

enum Protocol {
    static let version = 1
}

/// Answer to `--probe`, rendered in Settings.
struct Availability {
    var osSupported: Bool
    var osVersion: String?
    var archSupported: Bool
    var speechLocaleSupported: Bool
    var speechModelInstalled: Bool
    var speechLocale: String
    var supportedLocales: [String]
    /// "installed" | "notInstalled" | "unsupported"
    var translationStatus: String
    var message: String?

    var json: [String: Any] {
        var out: [String: Any] = [
            "type": "availability",
            "protocol": Protocol.version,
            "osSupported": osSupported,
            "archSupported": archSupported,
            "speech": [
                "localeSupported": speechLocaleSupported,
                "modelInstalled": speechModelInstalled,
                "locale": speechLocale,
                "supportedLocales": supportedLocales,
            ],
            "translation": ["status": translationStatus],
        ]
        if let osVersion { out["osVersion"] = osVersion }
        if let message { out["message"] = message }
        return out
    }
}

/// Serializes stdout writes.
///
/// Results, the translation debounce, and the asset-download ticker all emit
/// concurrently; interleaved partial writes would corrupt the NDJSON stream.
actor Emitter {
    private let handle = FileHandle.standardOutput

    func emit(_ message: OutMessage) {
        guard
            let data = try? JSONSerialization.data(
                withJSONObject: message.json, options: [.withoutEscapingSlashes])
        else {
            logErr("could not encode message", level: "error")
            return
        }
        var line = data
        line.append(0x0A)  // \n
        handle.write(line)
    }
}

/// Human-readable logging on stderr. Rust forwards these and keeps a tail to
/// append to failure messages, which is the main diagnostic channel when this
/// is being debugged from a machine that cannot run it.
func logErr(_ message: String, level: String = "info") {
    FileHandle.standardError.write(Data("[\(level)] \(message)\n".utf8))
}
