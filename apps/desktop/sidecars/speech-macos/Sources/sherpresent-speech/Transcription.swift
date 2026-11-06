import AVFoundation
import Foundation
import Speech
import Translation

/// Owns one SpeechAnalyzer session for the life of the process.
///
/// `SpeechAnalyzer` and `SpeechTranscriber` have unverified `Sendable`
/// conformance, so everything here stays on one actor.
actor Transcription {
    private let args: Args
    private let emitter: Emitter

    init(args: Args, emitter: Emitter) {
        self.args = args
        self.emitter = emitter
    }

    func run() async {
        let locale = await SpeechLocale.resolve(args.source)
        let sourceLang = Locale.Language(identifier: args.source)
        let targetLang = Locale.Language(identifier: args.target)

        // --- Translation preflight -----------------------------------------
        // Apple cannot download translation packs on our behalf, so this has to
        // be a clean fatal with instructions rather than a silent degradation.
        var session: TranslationSession?
        if args.translate {
            let status = await LanguageAvailability().status(from: sourceLang, to: targetLang)
            switch status {
            case .installed:
                session = TranslationSession(installedSource: sourceLang, target: targetLang)
            case .supported:
                emitter.send(
                    .error(
                        code: .translationNotInstalled, fatal: true,
                        message:
                            "The \(args.source) → \(args.target) translation language pack is not installed. "
                            + "Install it in System Settings › General › Language & Region › Translation Languages."
                    ))
                exit(3)
            default:
                emitter.send(
                    .error(
                        code: .translationUnsupported, fatal: true,
                        message:
                            "macOS cannot translate \(args.source) → \(args.target). Pick a different language pair."
                    ))
                exit(3)
            }
        }

        // --- Speech setup ---------------------------------------------------
        let transcriber = SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            // `fastResults` trades a little accuracy for responsiveness — the
            // right call for live captions, and what Apple's own
            // `progressiveTranscription` preset turns on.
            reportingOptions: [.volatileResults, .fastResults],
            attributeOptions: []
        )

        let supported = await SpeechTranscriber.supportedLocales
        guard SpeechLocale.contains(supported, locale) else {
            emitter.send(
                .error(
                    code: .localeUnsupported, fatal: true,
                    message:
                        "macOS speech recognition does not support \(args.source). Supported: \(supported.prefix(12).map { $0.identifier(.bcp47) }.joined(separator: ", "))"
                ))
            exit(3)
        }

        if !(await downloadSpeechModelIfNeeded(for: transcriber, locale: locale)) { exit(4) }

        guard
            let outputFormat = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [
                transcriber
            ]),
            let bridge = AudioBridge(
                sampleRate: args.sampleRate, channels: args.channels, outputFormat: outputFormat)
        else {
            emitter.send(
                .error(
                    code: .audioError, fatal: false,
                    message: "Could not build an audio converter for the analyzer's format."))
            exit(5)
        }

        // Captions are what the user is waiting on, so don't let the analyzer
        // run at background priority.
        let analyzer = SpeechAnalyzer(
            modules: [transcriber],
            options: .init(priority: .userInitiated, modelRetention: .processLifetime))
        let (inputSequence, inputBuilder) = AsyncStream<AnalyzerInput>.makeStream()

        do {
            // Load the model before audio flows, so the first words of a talk
            // aren't delayed by a cold start.
            try await analyzer.prepareToAnalyze(in: outputFormat)
            try await analyzer.start(inputSequence: inputSequence)
        } catch {
            emitter.send(
                .error(
                    code: .analyzerError, fatal: false,
                    message: "Could not start the speech analyzer: \(error.localizedDescription)"))
            exit(5)
        }

        emitter.send(
            .ready(
                sourceLocale: args.source,
                targetLocale: args.translate ? args.target : args.source,
                translate: args.translate,
                analyzerFormat: bridge.formatDescription))

        // --- Pump audio ------------------------------------------------------
        let audioTask = Task {
            for await data in StdinPCM.stream() {
                if let buffer = bridge.convert(data) {
                    inputBuilder.yield(AnalyzerInput(buffer: buffer))
                }
            }
            // EOF: flush whatever is mid-utterance rather than dropping it.
            inputBuilder.finish()
            try? await analyzer.finalizeAndFinishThroughEndOfInput()
        }

        // --- Consume results -------------------------------------------------
        var turns = TurnBuilder(maxChars: args.turnMaxChars)
        let translator = Translator(session: session)
        let live = LiveLine(emitter: emitter)

        do {
            for try await result in transcriber.results {
                let text = String(result.text.characters)

                if result.isFinal {
                    let shouldClose = turns.finalize(text)
                    let line = turns.line(volatile: "")

                    // The source never waits on translation: show the
                    // finalized words now, then the translation when ready.
                    var translated: String?
                    if args.translate {
                        await live.setSource(line)
                        translated = await translator.requestImmediate(line, turnID: turns.turnID)
                    }
                    await live.finalize(source: line, translated: translated)

                    if shouldClose {
                        turns.close()
                        await translator.setTurn(turns.turnID)
                        await live.close(nextTurn: turns.turnID)
                    }
                } else {
                    let line = turns.line(volatile: text)
                    await live.setSource(line)

                    if args.translate {
                        await translator.request(line, turnID: turns.turnID) { translated, turnID in
                            await live.setTranslation(translated, turnID: turnID)
                        }
                    }
                }
            }
        } catch {
            emitter.send(
                .error(
                    code: .analyzerError, fatal: false,
                    message: "Speech recognition stopped: \(error.localizedDescription)"))
            audioTask.cancel()
            exit(6)
        }

        // Results ended: stdin closed and the analyzer drained.
        if !turns.isEmpty {
            emitter.send(.turnComplete)
        }
        audioTask.cancel()
    }

    /// Speech models, unlike translation packs, can be fetched programmatically.
    private func downloadSpeechModelIfNeeded(
        for transcriber: SpeechTranscriber, locale: Locale
    ) async -> Bool {
        // `assetInstallationRequest` hands back a request even when the model
        // is already on disk, so check first — otherwise every start reports a
        // phantom download to the UI.
        if SpeechLocale.contains(await SpeechTranscriber.installedLocales, locale) {
            return true
        }

        do {
            guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber])
            else {
                return true  // Already installed.
            }

            let progress = request.progress
            let ticker = Task {
                while !Task.isCancelled {
                    emitter.send(
                        .assetProgress(stage: "speechModel", fraction: progress.fractionCompleted))
                    try? await Task.sleep(for: .milliseconds(500))
                }
            }
            defer { ticker.cancel() }

            logErr("downloading speech model for \(args.source)")
            try await request.downloadAndInstall()
            return true
        } catch {
            // Retryable: a venue's flaky Wi-Fi is exactly this case, and Rust
            // will respawn with backoff.
            emitter.send(
                .error(
                    code: .assetDownloadFailed, fatal: false,
                    message: "Could not download the speech model: \(error.localizedDescription)"))
            return false
        }
    }
}
