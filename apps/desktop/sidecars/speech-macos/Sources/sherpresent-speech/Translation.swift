import Foundation
import Translation

// TranslationSession ships without a Sendable conformance, but the actor below
// serializes every call through `inFlight`/`scheduled`, so it's never touched
// concurrently in practice.
// swift-format-ignore: AvoidRetroactiveConformances
extension TranslationSession: @unchecked @retroactive Sendable {}

/// On-device translation that keeps up with the speaker.
///
/// At most one call is in flight, and partials start no more often than
/// `interval`; requests that arrive meanwhile replace each other, so the next
/// call always takes the newest text. Translating every hypothesis kept up
/// but read as jitter; a trailing debounce starved instead (hypotheses arrive
/// faster than it settles, so translation only landed at pauses). A steady
/// cadence sits between the two. Finals skip the wait.
actor Translator {
    private let session: TranslationSession?
    private var inFlight = false
    private var pending: (text: String, turnID: UInt64)?
    private var lastSource = ""

    /// Set by the owner when a turn closes; results tagged with an older turn
    /// are dropped rather than overwriting the new line.
    private var currentTurn: UInt64 = 1

    /// Calls are numbered so a slow partial can't land after, and overwrite,
    /// a newer result (the final's immediate call runs alongside the queue).
    private var started: UInt64 = 0
    private var accepted: UInt64 = 0

    private let interval: Duration
    private var lastPartial: ContinuousClock.Instant?

    init(session: TranslationSession?, intervalMilliseconds: Int = 400) {
        self.session = session
        self.interval = .milliseconds(intervalMilliseconds)
    }

    func setTurn(_ id: UInt64) {
        currentTurn = id
        pending = nil
        lastSource = ""
    }

    /// Translate a volatile hypothesis as soon as the previous call is done.
    ///
    /// `deliver` is only called if the result is still the newest for a
    /// current turn.
    func request(
        _ text: String, turnID: UInt64, deliver: @escaping @Sendable (String, UInt64) async -> Void
    ) {
        guard session != nil, !text.isEmpty, text != lastSource else { return }
        pending = (text, turnID)
        guard !inFlight else { return }
        inFlight = true
        Task { await self.drain(deliver: deliver) }
    }

    /// Translate finalized text immediately, superseding any pending partial.
    func requestImmediate(_ text: String, turnID: UInt64) async -> String? {
        pending = nil
        return await translate(text, turnID: turnID)
    }

    private func drain(deliver: @Sendable (String, UInt64) async -> Void) async {
        while pending != nil {
            if let last = lastPartial {
                try? await Task.sleep(until: last + interval, clock: .continuous)
            }
            // Re-read after the wait: newer text may have arrived, or a final
            // or turn change may have cleared it.
            guard let next = pending else { break }
            pending = nil
            lastPartial = .now
            if let translated = await translate(next.text, turnID: next.turnID) {
                await deliver(translated, next.turnID)
            }
        }
        inFlight = false
    }

    private func translate(_ text: String, turnID: UInt64) async -> String? {
        guard let session, turnID == currentTurn else { return nil }
        started &+= 1
        let ticket = started

        do {
            let response = try await session.translate(text)
            // The turn may have closed, or a newer call finished first, while
            // this was in flight; either way this result is stale.
            guard turnID == currentTurn, ticket > accepted else { return nil }
            accepted = ticket
            lastSource = text
            return response.targetText
        } catch {
            logErr("translation failed: \(error.localizedDescription)", level: "warn")
            return nil
        }
    }
}

/// The line in progress, as last sent.
///
/// Source hypotheses and translations land from different tasks. Routing both
/// through here means every message carries the newest of each, so neither
/// half of the line ever steps backwards — previously each new hypothesis was
/// re-sent with the translation from the last *final*, undoing every partial
/// translation in between.
actor LiveLine {
    private let emitter: Emitter
    private var source = ""
    private var translated = ""
    private var turnID: UInt64 = 1

    init(emitter: Emitter) {
        self.emitter = emitter
    }

    func setSource(_ text: String) {
        source = text
        emitter.send(.partial(source: source, translated: translated))
    }

    func setTranslation(_ text: String, turnID: UInt64) {
        guard turnID == self.turnID, text != translated else { return }
        translated = text
        emitter.send(.partial(source: source, translated: translated))
    }

    /// `translated` is nil when translation failed; the last good one stays.
    func finalize(source text: String, translated newTranslation: String?) {
        source = text
        if let newTranslation { translated = newTranslation }
        emitter.send(.final(source: source, translated: translated))
    }

    /// End the line. Late translations for it are dropped from here on.
    func close(nextTurn: UInt64) {
        emitter.send(.turnComplete)
        source = ""
        translated = ""
        turnID = nextTurn
    }
}

enum TranslationSetup {
    /// Map `LanguageAvailability.Status` onto the wire vocabulary.
    static func statusName(_ status: LanguageAvailability.Status) -> String {
        switch status {
        case .installed: return "installed"
        case .supported: return "notInstalled"
        case .unsupported: return "unsupported"
        @unknown default: return "unsupported"
        }
    }

    static func availability(source: Locale.Language, target: Locale.Language) async -> String {
        let status = await LanguageAvailability().status(from: source, to: target)
        return statusName(status)
    }
}
