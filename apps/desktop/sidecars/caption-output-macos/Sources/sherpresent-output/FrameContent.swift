import CoreGraphics

/// What one helper process draws, chosen by `--content`. Each content parses
/// its own stdin messages and owns everything about how the frame looks;
/// `OutputApp` only renders when it says the picture changed.
@MainActor
protocol FrameContent: AnyObject {
    /// Apply one stdin line. Returns whether the picture changed.
    func handle(_ line: String) -> Bool
    /// Draw onto a frame that has already been cleared to transparent.
    func draw(in ctx: CGContext, width: CGFloat, height: CGFloat)
}

@MainActor
func makeContent(_ kind: String, text: CaptionText = .translated) throws -> FrameContent {
    switch kind {
    case "captions": return CaptionContent(text: text)
    case "notes": return NotesContent()
    default: throw Args.ParseError.badValue("--content", kind)
    }
}
