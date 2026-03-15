#!/usr/bin/env python3
"""Generate language-specific constant files from spec/protocol-constants.json.

Usage:
    python scripts/generate-constants.py

Outputs:
    apps/desktop/src-tauri/src/generated_constants.rs
    apps/desktop/src/lib/generated-constants.ts
    apps/bridge/src/rpi_osc_bridge/generated_constants.py
"""

import json
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT_DIR = os.path.dirname(SCRIPT_DIR)

SOURCE = os.path.join(ROOT_DIR, "spec", "protocol-constants.json")

TARGETS = {
    "rust": os.path.join(ROOT_DIR, "apps", "desktop", "src-tauri", "src", "generated_constants.rs"),
    "ts": os.path.join(ROOT_DIR, "apps", "desktop", "src", "lib", "generated-constants.ts"),
    "py": os.path.join(ROOT_DIR, "apps", "bridge", "src", "rpi_osc_bridge", "generated_constants.py"),
}

HEADER = "DO NOT EDIT — generated from spec/protocol-constants.json by scripts/generate-constants.py"


def generate_rust(channels: list[str], port: int) -> str:
    items = ", ".join(f'"{ch}"' for ch in channels)
    return f"""\
// {HEADER}

/// Valid channel names for broadcast mode
pub const VALID_CHANNELS: &[&str] = &[{items}];

/// Default broadcast port for channel communication
pub const DEFAULT_BROADCAST_PORT: u16 = {port};
"""


def generate_ts(channels: list[str], port: int) -> str:
    items = ", ".join(f"'{ch}'" for ch in channels)
    return f"""\
// {HEADER}

/** Valid channel names for broadcast mode */
export const VALID_CHANNELS = [{items}] as const;

export type ChannelName = typeof VALID_CHANNELS[number];

/** Default broadcast port for channel communication */
export const DEFAULT_BROADCAST_PORT = {port};
"""


def generate_py(channels: list[str], port: int) -> str:
    items = ",\n    ".join(f'"{ch}"' for ch in channels)
    return f'''\
# {HEADER}

VALID_CHANNELS = [
    {items},
]

DEFAULT_BROADCAST_PORT = {port}
'''


def main() -> None:
    with open(SOURCE) as f:
        data = json.load(f)

    channels = data["validChannels"]
    port = data["defaultBroadcastPort"]

    generators = {
        "rust": generate_rust,
        "ts": generate_ts,
        "py": generate_py,
    }

    for lang, path in TARGETS.items():
        content = generators[lang](channels, port)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
        print(f"  wrote {os.path.relpath(path, ROOT_DIR)}")

    print("Done.")


if __name__ == "__main__":
    main()
