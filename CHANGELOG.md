# Changelog

All notable changes to the SherPresent desktop app. Versions follow
[Semantic Versioning](https://semver.org/).

## [1.2.3] - 2026-10-21

### Changed

- **No more pause in watching the presentation after a command.** The app
  used to stop reading the presentation app for a while after each Next,
  Previous or Go To it sent (3 seconds on macOS, half a second on Windows),
  so a clicker press in that time was seen late and its QLab cue fired late.
  Now it keeps reading, and throws away only a read that overlapped a
  command, since that read might show where the show was before it.

### Fixed

- **Windows:** the regular status check no longer starts PowerPoint when it
  isn't running.

## [1.2.2] - 2026-10-21

### Changed

- **Windows:** slide and build changes made in PowerPoint itself (a clicker
  or the keyboard) are seen within a tenth of a second instead of a quarter,
  so their QLab cues fire sooner. After a command from SherPresent, the app
  starts watching PowerPoint again after half a second instead of three.

## [1.2.1] - 2026-10-21

### Changed

- **QLab slide cues no longer move the playhead.** The QLab tab option is
  now "Start the matching cue in QLab": slide 7 sends `/cue/S7/start`
  instead of GOing cue `S7`, so the playhead stays where the operator left
  it.

### Added

- **Windows:** each PowerPoint build starts its own QLab cue: `S7.1` for the
  slide's first build, `S7.2` for the next. Going back starts the cue for
  wherever the show lands. Extra OSC lines still go with slide changes only.

### Fixed

- **Windows:** build steps are tracked on every slide read, not only every
  two seconds. Next and Previous no longer guess wrong on slides with
  builds, which showed the wrong slide and fired the wrong QLab cue, and
  Next plays the builds on the last slide instead of doing nothing.

## [1.2.0] - 2026-10-20

### Added

- **QLab cue on slide change.** On the QLab tab, "GO the matching cue in
  QLab" makes each slide change during a show GO the QLab cue numbered with a
  prefix plus the slide number (prefix `S` by default: slide 7 GOes cue `S7`).
  Extra OSC can be sent alongside, with `{slide}` replaced by the slide
  number. It goes over the existing QLab connection the moment the slide
  changes; changes made in PowerPoint itself are caught within about a
  quarter second. The tab shows what was last sent, or QLab's error.
- **PowerPoint is picked automatically.** When exactly one presentation is
  presenting in PowerPoint (macOS or Windows), it's selected, and when that
  show stops and another starts, the app follows. Choosing a presentation by
  hand still sticks while a show keeps running.

### Fixed

- **QLab count-out ticks in true seconds.** The time left used to move
  whenever a poll reply landed, so seconds felt uneven. Each cue's time is
  now read from QLab when it starts and counted down on the app's own clock,
  checked against QLab every 2 seconds and corrected only if it has drifted.
  Stopping, pausing or resuming a cue still shows within a tenth of a
  second; scrubbing one shows at the next check.
- In custom QLab OSC, a quoted argument (`/go "53"`) is sent as the text
  `53`, as QLab requires for cue numbers.

## [1.1.1] - 2026-10-19

### Fixed

- **Windows:** the app builds again. The PowerPoint adapter was missing a
  field added for Keynote transitions.
- **Windows:** speaker notes are found by placeholder type rather than
  position, so decks with a customised notes master show their notes in the
  app and on the stage view.
- **Windows:** PowerPoint calls retry briefly when PowerPoint is busy (a
  dialog open, mid-transition) instead of dropping that update.
- **Windows:** new installs default to PowerPoint instead of LibreOffice.

## [1.1.0] - 2026-10-19

### Added

- **QLab count-out.** A new QLab tab connects to a QLab workspace over OSC
  (host, port, passcode and workspace) and shows the time left on the running
  video and audio cues:
  - Only cues that will actually end are counted. Looping cues, still images,
    cues with no duration, cues QLab lists as active but isn't playing, and
    cues that have finished (for example, a video holding its last frame) are
    all ignored. This keeps the count-out accurate in shows with many active
    cues.
  - When several cues of one kind run at once, the one that ends last is
    shown, with "+N more". The view sticks with that cue rather than swapping
    between cues that end at almost the same time.
  - Video and audio count out side by side in their own colours, with large
    digits and a bar that drains as time runs out. Paused cues are marked
    PAUSED.
  - With nothing playing, the cue at the playhead is shown as "Next".
  - A "Hide Cues" box takes cue numbers that are never counted. Changes apply
    immediately without reconnecting.
- **QLab outputs:** a count-out web page at `/qlab` on the stage view server,
  and an optional Syphon source ("SherPresent QLab"). Both show the same
  1920×1080 picture.
- QLab diagnostics in the app log: connection state, the active cue list, and
  why each cue does or doesn't count. Use
  `RUST_LOG=info,sherpresent_lib::qlab=trace` to log every reply as well.

### Changed

- The Syphon output helper speaks protocol version 5, which adds the `qlab`
  content. The app and the helper must be rebuilt together
  (`bun run macos:sidecar`).
- The QLab tab says when the stage view server isn't running, for example
  because another app is already using its port.
