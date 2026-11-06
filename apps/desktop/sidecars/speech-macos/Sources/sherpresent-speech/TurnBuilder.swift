import Foundation

/// Decides where one caption line ends and the next begins.
///
/// Pure and synchronous so the boundary policy can be unit-tested without audio
/// or the Speech framework. The thresholds are CLI flags because they can only
/// really be tuned by watching a live talk.
struct TurnBuilder {
    let maxChars: Int
    let terminators: Set<Character> = [".", "!", "?", "。", "！", "？", "…"]

    /// Text finalized so far in the current turn.
    private(set) var committed = ""
    /// Monotonic turn id, used to discard translations that arrive too late.
    private(set) var turnID: UInt64 = 1

    init(maxChars: Int) {
        self.maxChars = maxChars
    }

    /// Full text of the line as it currently stands, including the live guess.
    func line(volatile: String) -> String {
        if committed.isEmpty { return volatile }
        if volatile.isEmpty { return committed }
        return committed + " " + volatile
    }

    /// Fold a finalized fragment in. Returns true when the line should close.
    mutating func finalize(_ text: String) -> Bool {
        let fragment = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !fragment.isEmpty else { return false }

        committed = committed.isEmpty ? fragment : committed + " " + fragment

        if let last = committed.last, terminators.contains(last) { return true }
        return committed.count >= maxChars
    }

    /// Close the line and start a new one.
    mutating func close() {
        committed = ""
        turnID &+= 1
    }

    var isEmpty: Bool { committed.isEmpty }
}
