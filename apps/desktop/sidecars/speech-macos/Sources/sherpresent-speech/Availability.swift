import Foundation
import Speech
import Translation

extension Availability {
    /// One-shot capability report for Settings.
    ///
    /// Always answers rather than failing: "this Mac can't, and here is why" is
    /// the useful result, and the operator needs it before a show rather than
    /// during one.
    static func probe(args: Args, emitter: Emitter) async {
        let os = ProcessInfo.processInfo.operatingSystemVersion
        let osVersion = "\(os.majorVersion).\(os.minorVersion)"

        var supportedLocales: [String] = []
        var localeSupported = false
        var modelInstalled = false

        supportedLocales = await SpeechTranscriber.supportedLocales.map { $0.identifier(.bcp47) }
        localeSupported = supportedLocales.contains {
            $0.caseInsensitiveCompare(args.source) == .orderedSame
        }

        let installed = await SpeechTranscriber.installedLocales.map { $0.identifier(.bcp47) }
        modelInstalled = installed.contains { $0.caseInsensitiveCompare(args.source) == .orderedSame }

        var translationStatus = "installed"
        if !args.target.isEmpty {
            translationStatus = await TranslationSetup.availability(
                source: Locale.Language(identifier: args.source),
                target: Locale.Language(identifier: args.target))
        }

        #if arch(arm64)
            let archSupported = true
        #else
            let archSupported = false
        #endif

        let report = Availability(
            osSupported: os.majorVersion >= 26,
            osVersion: osVersion,
            archSupported: archSupported,
            speechLocaleSupported: localeSupported,
            speechModelInstalled: modelInstalled,
            speechLocale: args.source,
            supportedLocales: supportedLocales,
            translationStatus: translationStatus,
            message: nil
        )

        await emitter.emit(.availability(report))
    }
}
