import Foundation
import Translation

/// On-device translation with a debounce.
///
/// Translation is local and free, so partial hypotheses are worth translating
/// as they arrive — but only on a debounce, and never more than one call in
/// flight, or a fast speaker produces a queue of stale work.
actor Translator {
    private let session: TranslationSession?
    private let debounce: Duration
    private var scheduled: Task<Void, Never>?
    private var inFlight = false
    private var lastSource = ""

    /// Set by the owner when a turn closes; results tagged with an older turn
    /// are dropped rather than overwriting the new line.
    private var currentTurn: UInt64 = 1

    init(session: TranslationSession?, debounceMilliseconds: Int = 250) {
        self.session = session
        self.debounce = .milliseconds(debounceMilliseconds)
    }

    func setTurn(_ id: UInt64) {
        currentTurn = id
        scheduled?.cancel()
        scheduled = nil
        lastSource = ""
    }

    /// Translate a volatile hypothesis after the debounce settles.
    ///
    /// `deliver` is only called if the turn is still current.
    func requestDebounced(
        _ text: String, turnID: UInt64, deliver: @escaping @Sendable (String, UInt64) async -> Void
    ) {
        guard session != nil, !text.isEmpty, text != lastSource else { return }
        scheduled?.cancel()
        scheduled = Task { [debounce] in
            try? await Task.sleep(for: debounce)
            guard !Task.isCancelled else { return }
            await self.run(text, turnID: turnID, deliver: deliver)
        }
    }

    /// Translate finalized text immediately, cancelling any pending partial.
    func requestImmediate(_ text: String, turnID: UInt64) async -> String? {
        scheduled?.cancel()
        scheduled = nil
        return await translate(text, turnID: turnID)
    }

    private func run(
        _ text: String, turnID: UInt64, deliver: @Sendable (String, UInt64) async -> Void
    ) async {
        guard !inFlight else { return }
        if let translated = await translate(text, turnID: turnID) {
            await deliver(translated, turnID)
        }
    }

    private func translate(_ text: String, turnID: UInt64) async -> String? {
        guard let session, turnID == currentTurn else { return nil }
        inFlight = true
        defer { inFlight = false }

        do {
            let response = try await session.translate(text)
            // The turn may have closed while this was in flight; a late result
            // would otherwise overwrite the line that just started.
            guard turnID == currentTurn else { return nil }
            lastSource = text
            return response.targetText
        } catch {
            logErr("translation failed: \(error.localizedDescription)", level: "warn")
            return nil
        }
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
