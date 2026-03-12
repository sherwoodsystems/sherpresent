"""Safe JSON file I/O with file locking for rpi-osc-bridge."""

import os
import json
import fcntl


def safe_write_json(filepath, data):
    """Atomic JSON write with file locking."""
    temp_file = filepath + ".tmp"
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    with open(temp_file, 'w') as f:
        fcntl.flock(f.fileno(), fcntl.LOCK_EX)
        json.dump(data, f, indent=2)
        fcntl.flock(f.fileno(), fcntl.LOCK_UN)
    os.rename(temp_file, filepath)


def safe_read_json(filepath, default=None):
    """Read JSON file with shared lock."""
    if not os.path.exists(filepath):
        return default
    with open(filepath, 'r') as f:
        fcntl.flock(f.fileno(), fcntl.LOCK_SH)
        data = json.load(f)
        fcntl.flock(f.fileno(), fcntl.LOCK_UN)
    return data
