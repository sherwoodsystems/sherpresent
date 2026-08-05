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
        let locale = Locale(identifier: args.source)
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
                await emitter.emit(
                    .error(
                        code: .translationNotInstalled, fatal: true,
                        message:
                            "The \(args.source) → \(args.target) translation language pack is not installed. "
                            + "Install it in System Settings › General › Language & Region › Translation Languages."
                    ))
                exit(3)
            default:
                await emitter.emit(
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
            reportingOptions: [.volatileResults],
            attributeOptions: []
        )

        let supported = await SpeechTranscriber.supportedLocales.map { $0.identifier(.bcp47) }
        guard supported.contains(where: { $0.caseInsensitiveCompare(args.source) == .orderedSame })
        else {
            await emitter.emit(
                .error(
                    code: .localeUnsupported, fatal: true,
                    message:
                        "macOS speech recognition does not support \(args.source). Supported: \(supported.prefix(12).joined(separator: ", "))"
                ))
            exit(3)
        }

        if !(await downloadSpeechModelIfNeeded(for: transcriber)) { exit(4) }

        guard
            let outputFormat = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [
                transcriber
            ]),
            let bridge = AudioBridge(
                sampleRate: args.sampleRate, channels: args.channels, outputFormat: outputFormat)
        else {
            await emitter.emit(
                .error(
                    code: .audioError, fatal: false,
                    message: "Could not build an audio converter for the analyzer's format."))
            exit(5)
        }

        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let (inputSequence, inputBuilder) = AsyncStream<AnalyzerInput>.makeStream()

        do {
            try await analyzer.start(inputSequence: inputSequence)
        } catch {
            await emitter.emit(
                .error(
                    code: .analyzerError, fatal: false,
                    message: "Could not start the speech analyzer: \(error.localizedDescription)"))
            exit(5)
        }

        await emitter.emit(
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
        var lastTranslation = ""
        let emitter = self.emitter

        do {
            for try await result in transcriber.results {
                let text = String(result.text.characters)

                if result.isFinal {
                    let shouldClose = turns.finalize(text)
                    let line = turns.line(volatile: "")

                    if args.translate {
                        lastTranslation =
                            await translator.requestImmediate(line, turnID: turns.turnID)
                            ?? lastTranslation
                    } else {
                        lastTranslation = line
                    }

                    await emitter.emit(.final(source: line, translated: lastTranslation))

                    if shouldClose {
                        await emitter.emit(.turnComplete)
                        turns.close()
                        lastTranslation = ""
                        await translator.setTurn(turns.turnID)
                    }
                } else {
                    let line = turns.line(volatile: text)
                    await emitter.emit(.partial(source: line, translated: lastTranslation))

                    if args.translate {
                        // Debounced so a fast speaker doesn't queue stale work;
                        // the source line still updates every hypothesis.
                        await translator.requestDebounced(line, turnID: turns.turnID) {
                            translated, _ in
                            await emitter.emit(.partial(source: line, translated: translated))
                        }
                    } else {
                        await emitter.emit(.partial(source: line, translated: line))
                    }
                }
            }
        } catch {
            await emitter.emit(
                .error(
                    code: .analyzerError, fatal: false,
                    message: "Speech recognition stopped: \(error.localizedDescription)"))
            audioTask.cancel()
            exit(6)
        }

        // Results ended: stdin closed and the analyzer drained.
        if !turns.isEmpty {
            await emitter.emit(.turnComplete)
        }
        audioTask.cancel()
    }

    /// Speech models, unlike translation packs, can be fetched programmatically.
    private func downloadSpeechModelIfNeeded(for transcriber: SpeechTranscriber) async -> Bool {
        do {
            guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber])
            else {
                return true  // Already installed.
            }

            let progress = request.progress
            let emitter = self.emitter
            let ticker = Task {
                while !Task.isCancelled {
                    await emitter.emit(
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
            await emitter.emit(
                .error(
                    code: .assetDownloadFailed, fatal: false,
                    message: "Could not download the speech model: \(error.localizedDescription)"))
            return false
        }
    }
}
