# Configuration Formats

## Desktop App Config

Stored in the OS-specific app config directory as `config.json`.

### AppConfig

```json
{
  "osc": { /* OscConfig */ },
  "adapter": "powerpoint",
  "presentationName": "",
  "logging": { "enabled": true, "verbose": false }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `osc` | OscConfig | see below | OSC port configuration |
| `adapter` | string | `"powerpoint"` (macOS), `"libreoffice"` (Linux) | Presentation software adapter |
| `presentationName` | string | `""` | Target presentation filename |
| `logging` | LoggingConfig | `{enabled: true, verbose: false}` | Logging settings |

### OscConfig

```json
{
  "receivePort": 9000,
  "feedbackPort": 9001,
  "feedbackHost": "127.0.0.1",
  "host": "0.0.0.0",
  "feedbackDestinations": []
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `receivePort` | u16 | 9000 | Port to listen for incoming OSC commands |
| `feedbackPort` | u16 | 9001 | Legacy single feedback port |
| `feedbackHost` | string | `"127.0.0.1"` | Legacy single feedback host |
| `host` | string | `"0.0.0.0"` | Bind address for receive socket |
| `feedbackDestinations` | FeedbackDestination[] | `[]` | Additional feedback destinations |

## Bridge Config

Stored at `/etc/rpi-osc-bridge/config.json` on the Raspberry Pi.

### Version 3 (Current)

```json
{
  "version": 3,
  "feedback_port": 9001,
  "log_level": "INFO",
  "devices": {
    "usb_1": null,
    "usb_2": null,
    "usb_3": null
  }
}
```

When a device slot has a channel assignment:

```json
{
  "devices": {
    "usb_1": { "channel": "main" },
    "usb_2": { "channel": "backup" },
    "usb_3": null
  }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | int | 3 | Config format version |
| `feedback_port` | int | 9001 | Port for receiving feedback |
| `log_level` | string | `"INFO"` | Python logging level |
| `devices` | object | 3 null slots | Per-slot device-to-channel mapping |

### Version Migration

**v1 → v3**: Had `target_ip`, `target_port`, `channel` fields. Migrates by setting all device slots to the global channel.

**v2 → v3**: Had per-device entries but different format. Migrates by preserving device-channel assignments.

Invalid channel names are silently reset to `"main"`.

### Bridge Runtime Files

**`/var/run/rpi-osc-bridge/feedback.json`** — Written by bridge when feedback is received:

```json
{
  "main": {
    "presenting": 1,
    "slide": 5,
    "total": 20
  }
}
```
