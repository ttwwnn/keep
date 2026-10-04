#!/usr/bin/env python3
"""Types into a tab of the test's daemon, as a person at its keyboard would:
attached at the tab's own size, the text sent, a moment for it to land.

    digita.py <socket> <workspace> <tab> <cols> <rows> <text> [--enter]
"""
import socket
import struct
import sys
import threading
import time

path, ws, tab, cols, rows, text = sys.argv[1:7]
enter = "--enter" in sys.argv[7:]


def frame(tag, payload):
    return bytes([tag]) + struct.pack(">I", len(payload)) + payload


def s(value):
    b = value.encode()
    return struct.pack(">I", len(b)) + b


c = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
c.connect(path)
c.sendall(frame(0x02, s(ws) + struct.pack(">IHH", int(tab), int(cols), int(rows))))


def drain():
    try:
        while c.recv(65536):
            pass
    except OSError:
        pass


threading.Thread(target=drain, daemon=True).start()
time.sleep(0.3)
c.sendall(frame(0x14, text.encode()))
if enter:
    time.sleep(0.3)
    c.sendall(frame(0x14, b"\r"))
time.sleep(0.5)
c.close()
