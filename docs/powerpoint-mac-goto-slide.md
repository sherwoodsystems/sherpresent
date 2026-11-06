# Go-to-slide for PowerPoint on macOS

Last updated: 2026-10-05

**Status:** disabled. The go-to box in Live Status is hidden, and slide rows in
the notes view can't be clicked, when the adapter is PowerPoint on a Mac
(`appStore.supportsGoto`). The macOS `PowerPointAdapter` doesn't implement
`goto_slide`, so OSC `/clicker/goto` and the web `goto` command return the
trait's "Go-to-slide not supported" error.

Keynote and PowerPoint on Windows (COM `SlideShowView.GotoSlide`) are unaffected.

## Why it isn't trivial

PowerPoint for Mac's AppleScript dictionary
(`/Applications/Microsoft PowerPoint.app/Contents/Resources/PowerPoint.sdef`)
has a `go to slide <view> number <int>` command, but its direct parameter is the
edit-mode `view` class. During a show it fails on every reference to the
show's view:

```applescript
tell application "Microsoft PowerPoint"
    go to slide (slide show view of slide show window 1) number 2
    -- error: Parameter error. (-50)
end tell
```

`go to slide (view of document window 1) number 2` succeeds, but it only moves
the editor; the running show doesn't change.

A slide show view only supports `go to first slide`, `go to last slide`,
`go to next slide` and `go to previous slide`. The presenter tool adds `next` and
`previous`. Nothing in the dictionary touches the "See All Slides" grid either.

## Options

### 1. Step through (tested, rejected for now)

Rewind with `go to first slide` if the target is behind, then call
`go to next slide` until `current show position` reaches the target. Tested
against a live 3-slide show: 1→3, 3→2, 2→1 and 1→3 all landed correctly.

```applescript
if target < (current show position of ssView) then go to first slide ssView
set steps to 0
repeat while (current show position of ssView) < target and steps < 1000
    go to next slide ssView
    set steps to steps + 1
end repeat
```

Builds don't move `current show position`, which is why the cap is a step count,
not the slide count.

**Why it's rejected:** the audience briefly sees every slide in between, and
on-click animations play on the way past. You always land on the target before
any of its animations.

### 2. VBA add-in (`.ppam`), the recommended next step

PowerPoint for Mac's VBA has a real `SlideShowView.GotoSlide`, and AppleScript
can call a macro with arguments:

```text
run VB macro  macro name <text>  list of parameters <list of text>  → integer
```

That gives a true jump: no intermediate slides, no keystrokes, no focus change,
and no Accessibility permission.

**The add-in** (`SherPresent.ppam`, one standard module):

```vb
' Jumps the running show of the named presentation to slide n.
' Returns the new show position, or -1 when there is no running show.
Public Function SherGotoSlide(presName As String, n As String) As Long
    Dim pres As Presentation
    Set pres = Presentations(presName)
    If pres.SlideShowWindow Is Nothing Then
        SherGotoSlide = -1
        Exit Function
    End If
    With pres.SlideShowWindow.View
        .GotoSlide CLng(n)
        SherGotoSlide = .CurrentShowPosition
    End With
End Function
```

**Calling it from the adapter:**

```applescript
tell application "Microsoft PowerPoint"
    run VB macro macro name "SherGotoSlide" list of parameters {"Deck.pptx", "5"}
end tell
```

**Adapter shape:** have `goto_slide` try the macro first. On success, read the
position and total with the same script `get_slide_info` uses. If the macro is
missing, fall back to the "not supported" error, or to option 1 if that's
acceptable by then. Expose a `supports_goto` flag through a Tauri command and
use it to drive `appStore.supportsGoto`, replacing today's `isMac` check.

**Open questions (none of this is tested yet):**

- **Name resolution:** check whether `run VB macro` finds a function in an
  add-in by bare name. If not, check whether `"SherPresent.ppam!SherGotoSlide"`
  or a module-qualified name works.
- **Return value:** check whether the integer result carries the function's
  return value or is always 0. If it's always 0, read the position back
  separately.
- **Hanging:** check whether calling a macro during a show can hang Apple
  Events. The notes code has been disabled before for "slow/hung Apple Events"
  (see `notes-adapter-status.md`).
- **Trust:** check whether macro security prompts on first run. Signing the
  add-in may avoid that.
- **Hidden slides:** `GotoSlide` takes a slide *index*, while the app shows
  `current show position`. These differ when slides are hidden.

**Installation:** PowerPoint → Tools → PowerPoint Add-ins… → `+` → select the
`.ppam`, then tick it. It stays loaded across launches. Add-ins live in
`~/Library/Group Containers/UBF8T346G9.Office/User Content.localized/Add-Ins.localized/`.
This is a one-time step per machine and per user, so it belongs in the
getting-started docs. The app could also detect whether the add-in is present
and offer to open that folder.

**Building:** VBA can't be authored from the repo. Build the `.ppam` once in
PowerPoint (Save As → PowerPoint Add-in), commit the binary, and keep the
`.bas` source alongside it so changes are reviewable.

### 3. Keystrokes (not recommended)

During a show, typing the slide number and pressing Return jumps directly. `G`
or `-` opens the slide grid. Sending these through System Events needs
Accessibility permission. It also brings PowerPoint's show window to the front,
which is risky mid-talk, and it breaks silently if focus is in the wrong window.

## Re-enabling

1. Implement `goto_slide` in `apps/desktop/src-tauri/src/adapters/powerpoint.rs`.
2. Make `supportsGoto` in `apps/desktop/src/lib/state.svelte.ts` true for
   PowerPoint on a Mac, ideally from a backend capability flag rather than the
   user agent.
