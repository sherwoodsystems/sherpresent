import AVFoundation
import Foundation

/// Converts incoming s16le PCM into whatever format the analyzer asked for.
///
/// `SpeechAnalyzer.bestAvailableAudioFormat` is expected to be Float32 rather
/// than Int16, and may not be 16 kHz, so this always goes through
/// `AVAudioConverter` instead of assuming a memcpy will do.
///
/// Not an actor: `AVAudioPCMBuffer` is not `Sendable`, so every buffer is built
/// and handed to the analyzer inside the same task that owns this object.
final class AudioBridge {
    private let inputFormat: AVAudioFormat
    private let outputFormat: AVAudioFormat
    private let converter: AVAudioConverter?

    var formatDescription: String {
        "\(outputFormat.sampleRate)Hz \(outputFormat.channelCount)ch "
            + formatName(outputFormat.commonFormat)
    }

    init?(sampleRate: Double, channels: UInt32, outputFormat: AVAudioFormat) {
        guard
            let input = AVAudioFormat(
                commonFormat: .pcmFormatInt16,
                sampleRate: sampleRate,
                channels: AVAudioChannelCount(channels),
                interleaved: true)
        else { return nil }

        self.inputFormat = input
        self.outputFormat = outputFormat
        self.converter = AVAudioConverter(from: input, to: outputFormat)
        if converter == nil { return nil }
    }

    /// Convert one blob of little-endian i16 bytes. `data.count` is always even.
    func convert(_ data: Data) -> AVAudioPCMBuffer? {
        let frames = AVAudioFrameCount(data.count / 2 / Int(inputFormat.channelCount))
        guard frames > 0,
            let input = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: frames),
            let channel = input.int16ChannelData
        else { return nil }

        input.frameLength = frames
        data.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                memcpy(channel[0], base, data.count)
            }
        }

        guard let converter else { return nil }

        // Size the output for the rate change; equal rates give a 1:1 ratio.
        let ratio = outputFormat.sampleRate / inputFormat.sampleRate
        let capacity = AVAudioFrameCount((Double(frames) * ratio).rounded(.up) + 16)
        guard let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else {
            return nil
        }

        let feed = OneShotInput(input)
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, outStatus in
            guard let buffer = feed.take() else {
                outStatus.pointee = .noDataNow
                return nil
            }
            outStatus.pointee = .haveData
            return buffer
        }

        if status == .error {
            logErr("audio conversion failed: \(error?.localizedDescription ?? "unknown")", level: "error")
            return nil
        }
        return output.frameLength > 0 ? output : nil
    }
}

/// Hands `AVAudioConverter`'s input block its buffer exactly once.
///
/// The block is typed `@Sendable`, but `convert(to:error:withInputFrom:)` calls
/// it synchronously on the caller's thread before returning, so nothing here is
/// ever touched concurrently — which is what makes `@unchecked` sound.
private final class OneShotInput: @unchecked Sendable {
    private var buffer: AVAudioPCMBuffer?

    init(_ buffer: AVAudioPCMBuffer) { self.buffer = buffer }

    func take() -> AVAudioPCMBuffer? {
        defer { buffer = nil }
        return buffer
    }
}

private func formatName(_ format: AVAudioCommonFormat) -> String {
    switch format {
    case .pcmFormatFloat32: "float32"
    case .pcmFormatFloat64: "float64"
    case .pcmFormatInt16: "int16"
    case .pcmFormatInt32: "int32"
    default: "other"
    }
}
