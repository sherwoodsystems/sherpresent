import Foundation

/// Wire protocol with `output/` in Rust.
///
/// - **stdin**: NDJSON, depending on `--content`:
///   - `captions`: the same messages the web overlay's `/api/captions/ws`
///     socket carries (`segment`, `replay`, `status`, `settings`, `clear`),
///     so there is one caption message format for every output.
///   - `notes`: the stage view's `/api/ws` messages (`status`, `notes`), plus
///     `timer` from the app's Ontime client.
///   EOF = graceful stop.
/// - **stdout**: NDJSON — `ready`, `sinks`, `error`.
/// - **stderr**: plain-text logs.
enum WireProtocol {
    static let version = 2
}

// MARK: - Inbound

struct Segment: Decodable, Equatable {
    var id: UInt64
    var source: String
    var translated: String
    var final: Bool
}

/// The fields of `OverlaySettings` (`captions/mod.rs`) that affect the
/// picture; the rest (key colour, silence timeout) don't apply here. Rust has
/// already clamped every field.
struct OverlaySettings: Decodable, Equatable {
    var fontSize: Double = 56
    var maxLines: Int = 2
    var safeArea: Double = 5
    var width: Double = 80
    var shadow: Bool = false
    var background: Bool = false
    /// Fill of the background box, `0xRRGGBB`
    var boxColor: UInt32 = 0x000000

    init() {}

    /// Field by field with defaults, so a newer or older app that adds or
    /// drops a field can't make the whole settings message undecodable.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = OverlaySettings()
        fontSize = try c.decodeIfPresent(Double.self, forKey: .fontSize) ?? d.fontSize
        maxLines = try c.decodeIfPresent(Int.self, forKey: .maxLines) ?? d.maxLines
        safeArea = try c.decodeIfPresent(Double.self, forKey: .safeArea) ?? d.safeArea
        width = try c.decodeIfPresent(Double.self, forKey: .width) ?? d.width
        shadow = try c.decodeIfPresent(Bool.self, forKey: .shadow) ?? d.shadow
        background = try c.decodeIfPresent(Bool.self, forKey: .background) ?? d.background
        boxColor = try c.decodeIfPresent(String.self, forKey: .boxColor).flatMap(hexColor) ?? d.boxColor
    }

    private enum CodingKeys: String, CodingKey {
        case fontSize, maxLines, safeArea, width, shadow, background, boxColor
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
    case clear
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
        case "clear": return .clear
        default: return .unknown
        }
    }
}

// MARK: - Inbound, notes

/// The slice of Rust's `LiveStatus` the notes feed shows.
struct SlideStatus: Decodable, Equatable {
    var current = 0
    var total = 0
    var presenting = false

    private enum CodingKeys: String, CodingKey {
        case current = "current_slide"
        case total = "total_slides"
        case presenting = "is_presenting"
    }
}

/// Rust's `ontime::TimerState`.
struct TimerPayload: Decodable, Equatable {
    var connected: Bool
    /// Milliseconds remaining; negative is overtime
    var current: Double?
    /// Ontime playback state: `play`, `pause`, `stop`, `armed`, `roll`
    var playback: String?
    var title: String
}

enum NotesMessage {
    case status(SlideStatus)
    /// Every slide's notes, keyed by 1-based slide number
    case notes([Int: String])
    /// nil when Ontime isn't configured
    case timer(TimerPayload?)
    case unknown

    private struct Envelope<P: Decodable>: Decodable {
        var payload: P
    }

    private struct TypeOnly: Decodable {
        var type: String
    }

    /// Unknown types and malformed lines are dropped, never fatal.
    static func parse(_ line: String) -> NotesMessage {
        let decoder = JSONDecoder()
        guard let data = line.data(using: .utf8),
            let kind = try? decoder.decode(TypeOnly.self, from: data).type
        else { return .unknown }

        switch kind {
        case "status":
            return (try? decoder.decode(Envelope<SlideStatus>.self, from: data)).map { .status($0.payload) }
                ?? .unknown
        case "notes":
            // JSON object keys are strings; slide numbers are ints.
            guard let env = try? decoder.decode(Envelope<[String: String]>.self, from: data) else {
                return .unknown
            }
            var notes: [Int: String] = [:]
            for (k, v) in env.payload {
                if let n = Int(k) { notes[n] = v }
            }
            return .notes(notes)
        case "timer":
            return (try? decoder.decode(Envelope<TimerPayload?>.self, from: data)).map { .timer($0.payload) }
                ?? .unknown
        default:
            return .unknown
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
