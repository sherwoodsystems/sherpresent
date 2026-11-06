# Canva Presenter Notes Protocol

How Canva's remote control webview delivers presenter notes over WebSocket. Derived from captured network logs in `ref/canva-capture-notes-returning-after-login.txt`.

## Overview

Canva presenter notes are **not** delivered via XHR/REST. They arrive as **binary WebSocket messages** on Canva's `/_stream` WebSocket connection. The flow requires authentication (Canva login) before notes are available.

## Connection Flow

### 1. Authentication

The webview loads `https://www.canva.com/presentation/control?id2={session_id}`. If not logged in, Canva redirects to `/signup/` (with `signupRedirect` and `loginRedirect` query params pointing back to the control page). After login, the user lands back on `/presentation/control`.

### 2. WebSocket Connection

After the page loads, Canva opens a WebSocket to `/_stream` using a custom binary protocol called `cwsp`. The second protocol string is a base64-encoded JSON auth token containing session info (user ID, team ID, locale, feature flags).

```
WS_OPEN /_stream protocols: ["cwsp", "<base64-auth-json>"]
```

### 3. Controller Registration

Once connected, the client sends a binary message containing:

```json
{"A":"remote-control","B":"connectController"}{"A":"{session_id}"}
```

This binds the WebSocket to a specific presentation session. The session ID is the `id2` query parameter from the URL (e.g., `8f5f83a5-6137-4e43-bc99-faeb236d17f3`).

### 4. Initial State (Snapshot)

The server responds with a type `"C"` message containing the initial presentation state:

```json
{
  "A?": "C",
  "Bk": {
    "A": "{session_id}",
    "B": {"A?": "C"},
    "C": 1,
    "E": "DAHD2_nLTsk",
    "H": "D"
  }
}
```

This triggers a `STATE_UPDATE` event with `{"type":"snapshot"}` in the app.

## Message Types

All WS messages are binary. The first 6 bytes are a binary header (typically `00 00 00 01 28 20` for data messages), followed by a JSON payload.

### Type A: Slide Change Notification

Minimal message indicating the current slide changed.

```json
{"A?": "A", "Bk": 1}
```

- `Bk` = current slide index (0-based)

This triggers `STATE_UPDATE` with `{"type":"slide_change","currentPage":1}`.

### Type B: Slide State with Notes

The main message type carrying presenter notes.

```json
{
  "A?": "B",
  "Bk": {
    "A?": "A",
    "Bk": 3,          // current slide index (0-based)
    "Bl": 15,         // total slide count
    "Bm": 0,          // sub-page / animation step (always 0 in logs)
    "Bn": 0,          // unknown (always 0)
    "Bp": [           // notes array for nearby slides
      {"A": 2, "B": "This is a page without a title"},
      {"A": 3, "B": ""},
      {"A": 4, "B": "This is a page with line breaks\n\nLine break \n\nDouble breaks up to now\nSingle break\nSingle break\n\nLONG LONG LINE WIHT NO BREAKS LONE LINE IWTH NO BREAKS"}
    ],
    "Bo": 2           // optional, present in some messages
  },
  "Bl": "d8ff2e8f-8063-48ec-9a34-c91d62f1af34",  // page UUID (optional)
  "Bm": 2            // optional
}
```

### Type C: Connection/Snapshot State

Initial state after connecting (see section 4 above).

## Notes Data Structure (`Bp` Array)

The `Bp` field is an array of note objects for **the current slide and its neighbors** (typically a window of ~3 slides around the current one).

Each entry:
```json
{"A": <slide_index>, "B": "<note_text>"}
```

- `A` = 0-based slide index
- `B` = note text as a plain string. Empty string `""` means no notes on that slide. Line breaks are represented as `\n`.

Key behaviors observed:
- Notes arrive for the **current slide and 1-2 adjacent slides** (a sliding window)
- When navigating, **two messages arrive in sequence**:
  1. First: immediate acknowledgment with `"Bp": []` (empty notes) and `"Bo": 2`
  2. Second: delayed follow-up with the actual `Bp` notes array populated
- The notes for slide 0 had text `"These are presenter notes"`, proving this is where Canva notes live

## Navigation (Outbound)

Slide navigation is done via XHR POST:

```
POST /_ajax/remotecontrol/remote/{session_id}/navigate
```

Body:
```json
{
  "A": "{session_id}",
  "B": 3,                                           // target slide index (0-based)
  "C": 0,                                           // sub-page index
  "D": "d8ff2e8f-8063-48ec-9a34-c91d62f1af34"      // target page UUID
}
```

Note: When navigating to slide 0, the `"B"` field is **omitted** from the body (not set to 0).

The server responds `{}` (empty JSON). The actual state update comes via the WebSocket (type B message).

## Binary Frame Format

WS messages use a binary framing protocol. Observed header patterns:

| Hex Header | Purpose |
|---|---|
| `00 00 00 01 28 20` | Data message (JSON payload follows) |
| `00 00 00 01 1d 00...` | Client subscription message (connectController) |
| `00 00 00 02 10 00` | Request acknowledgment (contains request ID) |
| `00 00 00 01 20 00...` | Connection setup response |
| `00 00 00 00 0c 80...` / `00 00 00 00 0c 00...` | Keepalive/ping-pong (14 bytes) |
| `00 00 00 01 04 00...` | Initial capabilities/codec negotiation |
| `00 00 00 02 28 60` | Client acknowledgment |

## How to Extract Notes in sher-present

To pull notes from the Canva webview:

1. **Intercept WS binary messages** on the `/_stream` WebSocket connection
2. **Skip the 6-byte binary header** and parse the remaining bytes as JSON
3. **Filter for type B messages** (`"A?": "B"`)
4. **Read `Bk.Bp`** for the notes array
5. **Match `Bp[n].A`** to slide indices and **read `Bp[n].B`** for the note text
6. **Handle the two-message pattern**: after navigation, ignore the first empty `Bp` and wait for the second message with populated notes
7. **Get current slide** from `Bk.Bk` and total from `Bk.Bl`

### Extracting the Current Slide's Notes

```
current_slide = message.Bk.Bk
total_slides  = message.Bk.Bl
notes_window  = message.Bk.Bp

for entry in notes_window:
    if entry.A == current_slide:
        current_notes = entry.B  // may be "" if no notes
```

### Important Caveats

- Notes are only available **after login** (requires Canva account auth)
- The WebSocket uses Canva's proprietary `cwsp` binary protocol
- The JSON field names are obfuscated single/double letter keys that may change between Canva deployments
- Notes arrive asynchronously after navigation - there is a brief period where `Bp` is empty
- The binary header format is not fully documented; the 6-byte skip works for data messages but may need adjustment
