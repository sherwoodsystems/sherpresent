# Companion Mode Plan

## Context

sher-present's rpi-osc-bridge translates USB clicker presses (arrow keys) into OSC commands (`/clicker/{channel}/next`, `/clicker/{channel}/prev`) sent to sher-present-settings. In "Companion mode," those presses go to Bitfocus Companion instead, letting Companion orchestrate multi-system workflows (slides + lights + cameras + ProPresenter, etc.) from the same two-button clicker.

## MVP: HTTP API

Bridge POSTs to Companion's REST API to press buttons. User configures two Companion buttons with whatever actions they want.

### Companion API

```
POST http://<companion-ip>:8000/api/location/<page>/<row>/<column>/press
```

No auth required by default. Returns 200 on success.

### Bridge Code (`CompanionSender`)

```python
import urllib.request
import json

class CompanionSender:
    """Sends clicker events to Companion via HTTP API."""

    def __init__(self, host: str, port: int = 8000,
                 next_button: tuple = (1, 0, 0),
                 prev_button: tuple = (1, 0, 1)):
        self.base_url = f"http://{host}:{port}/api/location"
        self.next_button = next_button  # (page, row, col)
        self.prev_button = prev_button

    def _press(self, button: tuple):
        page, row, col = button
        url = f"{self.base_url}/{page}/{row}/{col}/press"
        try:
            req = urllib.request.Request(url, method='POST')
            urllib.request.urlopen(req, timeout=2)
        except Exception as e:
            logging.error(f"Companion press failed: {e}")

    def send_next(self, **kwargs):
        self._press(self.next_button)

    def send_prev(self, **kwargs):
        self._press(self.prev_button)
```

### Config Addition (v4)

```json
{
  "version": 4,
  "mode": "companion",
  "companion": {
    "host": "192.168.1.100",
    "port": 8000,
    "next_button": [1, 0, 0],
    "prev_button": [1, 0, 1]
  }
}
```

When `mode` is `"companion"`, the bridge instantiates `CompanionSender` instead of `BroadcastSender`. The existing keyboard listener (`listen_keyboard`) stays the same — it just calls `sender.send_next()` / `sender.send_prev()` on a different sender object.

### What the User Sets Up in Companion

1. Create a page (e.g., page 1)
2. Add two buttons:
   - Button at row 0, col 0 → "Next" actions (advance ProPresenter, send OSC to sher-present, trigger lighting cue, etc.)
   - Button at row 0, col 1 → "Prev" actions
3. Set the bridge config to point at those coordinates

---

## Future: Satellite Protocol (2-Button Virtual Surface)

The Companion Satellite protocol lets the bridge register as a **2-button hardware surface** that appears natively in Companion's UI — just like a tiny Stream Deck. Users drag actions onto the buttons directly in Companion, no button coordinates to configure.

### How It Works

- TCP connection to Companion on port **16622**
- ASCII line protocol: `COMMAND ARG=VALUE\n`
- Register with `ADD-DEVICE DEVICEID=... KEYS_TOTAL=2 KEYS_PER_ROW=2`
- Send `KEY-PRESS` / `KEY-RELEASE` when clicker buttons are pressed
- Must send `PING` every ~2 seconds to stay connected

### Satellite Code Sketch

```python
import socket
import threading
import time

class CompanionSatellite:
    """Registers as a 2-button Companion surface via Satellite protocol."""

    DEVICE_ID = "rpi-osc-bridge"
    PRODUCT_NAME = "USB Clicker"

    def __init__(self, host: str, port: int = 16622):
        self.host = host
        self.port = port
        self.sock = None
        self._running = False

    def connect(self):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.connect((self.host, self.port))
        # Read the BEGIN line
        self._read_line()
        # Register as a 2-button device
        self._send(f'ADD-DEVICE DEVICEID={self.DEVICE_ID} '
                   f'PRODUCT_NAME="{self.PRODUCT_NAME}" '
                   f'KEYS_TOTAL=2 KEYS_PER_ROW=2')
        # Start keepalive thread
        self._running = True
        threading.Thread(target=self._keepalive, daemon=True).start()

    def _send(self, msg: str):
        self.sock.sendall((msg + '\n').encode())

    def _read_line(self) -> str:
        data = b''
        while not data.endswith(b'\n'):
            data += self.sock.recv(1)
        return data.decode().strip()

    def _keepalive(self):
        while self._running:
            time.sleep(2)
            try:
                self._send('PING')
            except:
                self._running = False

    def send_next(self, **kwargs):
        # Button 0 = "next"
        self._send(f'KEY-PRESS DEVICEID={self.DEVICE_ID} KEY=0')
        time.sleep(0.05)
        self._send(f'KEY-RELEASE DEVICEID={self.DEVICE_ID} KEY=0')

    def send_prev(self, **kwargs):
        # Button 1 = "prev"
        self._send(f'KEY-PRESS DEVICEID={self.DEVICE_ID} KEY=1')
        time.sleep(0.05)
        self._send(f'KEY-RELEASE DEVICEID={self.DEVICE_ID} KEY=1')

    def disconnect(self):
        self._running = False
        if self.sock:
            self._send(f'REMOVE-DEVICE DEVICEID={self.DEVICE_ID}')
            self.sock.close()
```

### What the User Sees in Companion

The bridge shows up as a surface called "USB Clicker" with two buttons in Companion's Settings → Surfaces. The user clicks each button in the Companion UI and assigns actions — no coordinates to configure, no bridge config needed beyond the Companion host IP.

### Satellite Config

```json
{
  "version": 4,
  "mode": "satellite",
  "companion": {
    "host": "192.168.1.100",
    "port": 16622
  }
}
```

---

## Bridge Integration Point

Both senders share the same interface as `BroadcastSender`:

```python
# In bridge.py, where sender is created:
if config.get("mode") == "companion":
    sender = CompanionSender(
        host=config["companion"]["host"],
        port=config["companion"].get("port", 8000),
        next_button=tuple(config["companion"].get("next_button", [1, 0, 0])),
        prev_button=tuple(config["companion"].get("prev_button", [1, 0, 1])),
    )
elif config.get("mode") == "satellite":
    sender = CompanionSatellite(
        host=config["companion"]["host"],
        port=config["companion"].get("port", 16622),
    )
    sender.connect()
else:
    sender = BroadcastSender(...)  # existing behavior
```

The keyboard listener doesn't change — it calls `sender.send_next()` / `sender.send_prev()` regardless of mode.

## Implementation Order

1. Add `CompanionSender` class to bridge.py
2. Add mode selection to config (v4 migration)
3. Wire up sender selection in bridge startup
4. Add mode toggle to bridge web UI
5. (Later) Add `CompanionSatellite` class for native surface support

## Verification

- Run Companion on a machine on the same network
- Set bridge to companion mode, point at Companion IP
- Press USB clicker → verify Companion buttons flash/trigger
- Configure Companion buttons with test actions (e.g., send OSC back to sher-present) and verify end-to-end
