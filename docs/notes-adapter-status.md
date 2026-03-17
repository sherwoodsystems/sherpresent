# Notes Across Adapters — Status & Next Steps

Last updated: 2026-03-16

## Current Status

| Adapter | Notes Support | Method | Bulk Fetch |
|---------|--------------|--------|------------|
| **Keynote (macOS)** | Full | AppleScript `presenter notes of slide i` | Yes — loops all slides |
| **PowerPoint (Windows)** | Full | COM `Slides(idx).NotesPage.Shapes.Placeholders(2).TextFrame.TextRange.Text` | Yes — iterates all slides |
| **PowerPoint (macOS)** | **DISABLED** | AppleScript (commented out) | No |
| **LibreOffice Impress** | Current slide only | TCP Impress Remote Protocol (`slide_notes` message) | No |
| **Canva** | Full | WebSocket binary frame interception | Yes — batch via `Bp` array |

## TODO: PowerPoint macOS Notes via NSAppleScript

**Priority: Tomorrow**

PowerPoint macOS notes are commented out in `src-tauri/src/adapters/powerpoint.rs` (lines ~169-285) with the reason:

> "Slow/hung Apple Events. Re-enable once the basics are working reliably."

Now that we've replaced `osascript -e` with in-process NSAppleScript FFI (status: ~5ms, commands: ~60ms vs old 160-175ms), we should re-enable notes.

### What needs to happen

1. **Uncomment the disabled code** in `powerpoint.rs`:
   - `get_presenter_notes(name)` — gets notes for current slide
   - `get_all_presenter_notes(name)` — loops all slides, returns `HashMap<i32, String>`
   - `get_notes_zoom()` / `set_notes_zoom()` — zoom controls

2. **Test with NSAppleScript** — the old Apple Event hanging issues were caused by process-spawn overhead stacking up. With FFI, individual calls are ~60ms, so bulk-fetching notes for a 30-slide deck should be ~2s instead of the old ~5s+.

3. **Watch for**: Apple Event serialization — PowerPoint can only handle one Apple Event at a time. The polling suppression (skip 3s after UI commands) should still apply. If bulk note fetch overlaps with navigation commands, we may get hangs again.

4. **Acceptance criteria**: Open a PowerPoint deck with notes on several slides, start presenting, verify notes appear in the UI and on the web server view.

## LibreOffice Impress Notes — Research Summary

### What works today

The Impress Remote Protocol (TCP port 1599) already pushes `slide_notes` as HTML when slides change. The adapter strips HTML tags and caches the plain text. This works during slideshow mode — no extra work needed for current-slide notes.

### What doesn't work: bulk fetch

The Remote Protocol has **no command to request notes for arbitrary slides**. Notes only arrive on slide transitions. So `get_all_presenter_notes()` returns an empty HashMap.

### Options for bulk fetch

**Option A: Accumulate progressively (simplest)**
- As the user navigates, cache each slide's notes as they arrive
- After a full pass through the deck, you have everything
- Downside: incomplete until every slide has been visited

**Option B: Python UNO script (full access)**
- LibreOffice's UNO API can read any slide's notes without a running slideshow
- Path: `DrawPages.getByIndex(n).getNotesPage().getByIndex(1).getString()`
- Would need to shell out to a Python script that connects on port 2002
- Requires LibreOffice started with `--accept="socket,host=0,port=2002;urp;"`
- Adds a Python dependency and a different connection mechanism
- **Not recommended for MVP** — too much complexity

**Option C: Slide scanning (existing pattern)**
- The `scan_notes` command in `presentation.rs` already has a pattern for this: it navigates to each slide via `goto_slide()` and waits for notes to arrive
- For LibreOffice, this would trigger `slide_notes` messages for each slide
- Downside: visibly flips through slides during the scan

### Recommendation for LibreOffice

Stick with **Option A** (progressive accumulation) for now. Current-slide notes already work. If users need all notes upfront, Option C (scan) is available as a manual trigger from the UI. Option B is overkill unless there's a strong demand.

## Data Flow Reference

```
Adapter.get_live_status()          → includes presenter_notes for current slide
    ↓ (every 2s poll)
AppState.notes_cache               → HashMap<i32, String>, 1-indexed
    ↓
Tauri emit "notes-cache-updated"   → frontend gets full cache snapshot
    ↓
notes_broadcast channel            → web server SSE clients
```

Bulk fetch (`get_all_presenter_notes()`) runs once on presentation start, merges into cache.
