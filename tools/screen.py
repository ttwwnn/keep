#!/usr/bin/env python3
"""What a tab's screen holds, asked of the daemon over its socket.

The daemon owns every tab's terminal, so it can answer this for a tab nobody
is looking at — which is what a test needs: drive the app, then ask the far
end what arrived, without a screenshot and without the app's cooperation.

    screen.py <workspace> <tab>
"""

import os
import socket
import struct
import sys


def socket_path() -> str:
    override = os.environ.get("KEEP_SOCKET")
    if override:
        return override
    base = os.environ.get("XDG_RUNTIME_DIR") or os.environ.get("TMPDIR") or "/tmp"
    return os.path.join(base, f"keep-{os.environ.get('USER', 'default')}.sock")


def string(value: str) -> bytes:
    raw = value.encode()
    return struct.pack(">I", len(raw)) + raw


def preview(workspace: str, tab: int) -> str:
    payload = string(workspace) + struct.pack(">I", tab)
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.connect(socket_path())
    # T_PREVIEW, then a framed reply: [tag][length][payload]
    client.sendall(bytes([0x08]) + struct.pack(">I", len(payload)) + payload)

    header = b""
    while len(header) < 5:
        chunk = client.recv(5 - len(header))
        if not chunk:
            raise SystemExit("daemon closed the connection")
        header += chunk
    tag, length = header[0], struct.unpack(">I", header[1:5])[0]

    body = b""
    while len(body) < length:
        chunk = client.recv(length - len(body))
        if not chunk:
            break
        body += chunk
    if tag != 0x89:
        raise SystemExit(f"unexpected reply tag {tag:#x}: {body[:120]!r}")
    return body[4:].decode("utf-8", "replace")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    text = preview(sys.argv[1], int(sys.argv[2]))
    print("\n".join(line.rstrip() for line in text.split("\n") if line.strip()))
