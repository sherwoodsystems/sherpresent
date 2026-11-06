import Foundation
import Speech

enum SpeechLocale {
    /// The recognition locale for `--source`. Recognition wants a regional
    /// locale ("fr-FR"); a bare language ("fr") is first expanded to its
    /// likely region, since Apple's own equivalence picks an arbitrary one
    /// (fr-BE), then matched against what this Mac supports.
    static func resolve(_ source: String) async -> Locale {
        let requested = Locale(identifier: source)
        let likely = Locale(identifier: Locale.Language(identifier: source).maximalIdentifier)
        let candidate = requested.region == nil ? likely : requested
        return await SpeechTranscriber.supportedLocale(equivalentTo: candidate) ?? requested
    }

    /// Whether `locales` (as Speech reports them) includes `locale`.
    static func contains(_ locales: [Locale], _ locale: Locale) -> Bool {
        let id = locale.identifier(.bcp47)
        return locales.contains { $0.identifier(.bcp47).caseInsensitiveCompare(id) == .orderedSame }
    }
}
