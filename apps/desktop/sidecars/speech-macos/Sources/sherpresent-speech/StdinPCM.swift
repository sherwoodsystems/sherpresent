import Foundation

/// Reads raw little-endian i16 PCM from stdin.
///
/// A `read(2)` can return any byte count, including one that splits a sample in
/// half. Carrying the odd byte across reads is the difference between captions
/// and garbled noise, so the accumulator here is the part worth testing.
enum StdinPCM {
    /// Bridge blocking reads on a dedicated thread into an async stream.
    ///
    /// A real `Thread` rather than `FileHandle.readabilityHandler`, which is
    /// fragile under sustained load.
    static func stream(bufferSize: Int = 16384) -> AsyncStream<Data> {
        AsyncStream(Data.self, bufferingPolicy: .unbounded) { continuation in
            let thread = Thread {
                var leftover: UInt8?
                var buffer = [UInt8](repeating: 0, count: bufferSize)

                while true {
                    let n = buffer.withUnsafeMutableBytes { raw in
                        read(0, raw.baseAddress, bufferSize)
                    }
                    if n < 0 {
                        if errno == EINTR { continue }
                        logErr("stdin read failed: \(String(cString: strerror(errno)))", level: "error")
                        break
                    }
                    if n == 0 { break }  // EOF: the parent closed stdin.

                    var chunk = Data()
                    if let held = leftover {
                        chunk.append(held)
                        leftover = nil
                    }
                    chunk.append(contentsOf: buffer[0..<n])

                    // Only whole samples go downstream; hold a trailing odd byte
                    // until its partner arrives.
                    if chunk.count % 2 == 1 {
                        leftover = chunk.removeLast()
                    }
                    if !chunk.isEmpty {
                        continuation.yield(chunk)
                    }
                }
                continuation.finish()
            }
            thread.name = "sherpresent-stdin"
            thread.start()
        }
    }
}
