import Foundation

/// Wire protocol with `captions/output/` in Rust.
///
/// - **stdin**: NDJSON. The same messages the web overlay's
///   `/api/captions/ws` socket carries (`segment`, `replay`, `status`,
///   `settings`), so there is one caption message format for every output.
///   EOF = graceful stop.
/// - **stdout**: NDJSON — `ready`, `sinks`, `error`.
/// - **stderr**: plain-text logs.
enum WireProtocol {
    static let version = 1
}

// MARK: - Inbound

struct Segment: Decodable, Equatable {
    var id: UInt64
    var source: String
    var translated: String
    var final: Bool
}

/// Mirrors `OverlaySettings` in `captions/mod.rs`. Rust has already clamped
/// and sanitized every field.
struct OverlaySettings: Decodable, Equatable {
    var fontSize: Double = 56
    var maxLines: Int = 2
    var chromaColor: String = "#00B140"
    var safeArea: Double = 5
    var width: Double = 80
    var shadow: Bool = false
    var background: Bool = false
    /// Seconds of silence before the lines clear; 0 = never
    var clearAfter: Double = 8

    init() {}

    /// Field by field with defaults, so a newer or older app that adds or
    /// drops a field can't make the whole settings message undecodable.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = OverlaySettings()
        fontSize = try c.decodeIfPresent(Double.self, forKey: .fontSize) ?? d.fontSize
        maxLines = try c.decodeIfPresent(Int.self, forKey: .maxLines) ?? d.maxLines
        chromaColor = try c.decodeIfPresent(String.self, forKey: .chromaColor) ?? d.chromaColor
        safeArea = try c.decodeIfPresent(Double.self, forKey: .safeArea) ?? d.safeArea
        width = try c.decodeIfPresent(Double.self, forKey: .width) ?? d.width
        shadow = try c.decodeIfPresent(Bool.self, forKey: .shadow) ?? d.shadow
        background = try c.decodeIfPresent(Bool.self, forKey: .background) ?? d.background
        clearAfter = try c.decodeIfPresent(Double.self, forKey: .clearAfter) ?? d.clearAfter
    }

    private enum CodingKeys: String, CodingKey {
        case fontSize, maxLines, chromaColor, safeArea, width, shadow, background, clearAfter
    }
}

struct StatusPayload: Decodable {
    var translateEnabled: Bool?
}

enum InMessage {
    case segment(Segment)
    case replay([Segment])
    case status(StatusPayload)
    case settings(OverlaySettings)
    case unknown

    private struct Envelope: Decodable {
        var type: String
        var segment: Segment?
        var segments: [Segment]?
        var status: StatusPayload?
        var settings: OverlaySettings?
    }

    /// Unknown types and malformed lines are dropped, never fatal: a version
    /// skew or stray write must not take the output down mid-show.
    static func parse(_ line: String) -> InMessage {
        guard let data = line.data(using: .utf8),
            let env = try? JSONDecoder().decode(Envelope.self, from: data)
        else { return .unknown }

        switch env.type {
        case "segment": return env.segment.map(InMessage.segment) ?? .unknown
        case "replay": return .replay(env.segments ?? [])
        case "status": return env.status.map(InMessage.status) ?? .unknown
        case "settings": return env.settings.map(InMessage.settings) ?? .unknown
        default: return .unknown
        }
    }
}

// MARK: - Outbound

struct SinkStatus: Encodable {
    var kind: String
    var name: String
    var hasClients: Bool
}

enum OutMessage {
    case ready(width: Int, height: Int, sinks: [SinkStatus])
    case sinks([SinkStatus])
    case error(code: String, fatal: Bool, message: String)

    private struct Wire: Encodable {
        var type: String
        var `protocol`: Int?
        var width: Int?
        var height: Int?
        var sinks: [SinkStatus]?
        var code: String?
        var fatal: Bool?
        var message: String?
    }

    var line: String {
        let wire: Wire
        switch self {
        case let .ready(w, h, sinks):
            wire = Wire(type: "ready", protocol: WireProtocol.version, width: w, height: h, sinks: sinks)
        case let .sinks(sinks):
            wire = Wire(type: "sinks", sinks: sinks)
        case let .error(code, fatal, message):
            wire = Wire(type: "error", code: code, fatal: fatal, message: message)
        }
        let data = (try? JSONEncoder().encode(wire)) ?? Data("{}".utf8)
        return String(decoding: data, as: UTF8.self)
    }
}

func emit(_ message: OutMessage) {
    FileHandle.standardOutput.write(Data((message.line + "\n").utf8))
}

func logErr(_ text: String, level: String = "info") {
    FileHandle.standardError.write(Data("[\(level)] \(text)\n".utf8))
}
