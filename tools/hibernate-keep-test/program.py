#!/usr/bin/env python3
"""A tab's program for tools/hibernate-keep-test.sh.

Started by the daemon through a wrapper named for a version number, which is
how Keep tells Claude Code. It does what the test leaves in `cmd-<pid>` in the
work directory, one command a line, then writes `done-<pid>`:

  print N            N numbered lines, `P<pid>-L0000001 …`, counting on
  title TEXT         OSC 2
  ask                the questions programs ask their terminal; the answers
                     go to `answers-<pid>-<k>`, one a line, as Python reprs
  claude on          the alternate screen, drawn as Claude Code draws it
                     there, with the Kitty keyboard pushed
  claude count N     the counter on that screen, drawn again
  claude off         back to the main screen, as Claude Code leaves
"""

import os
import select
import sys
import time
import tty

WORK = os.environ["KEEP_HIB_WORK"]
ME = os.getpid()
QUERIES = [
    b"\x1b[c", b"\x1b[>c", b"\x1b[=c", b"\x1b[>q", b"\x1b[?u",
    b"\x1b]10;?\x1b\\", b"\x1b]11;?\x1b\\", b"\x1b]12;?\x1b\\", b"\x1b]4;1;?\x1b\\", b"\x1b]4;200;?\x1b\\",
    b"\x1b[?996n", b"\x1b[14t", b"\x1b[16t", b"\x1b[18t", b"\x1b[5n", b"\x1b[6n",
] + [b"\x1b[?%d$p" % m for m in (
    1, 3, 4, 5, 6, 7, 8, 9, 12, 25, 40, 45, 47, 66, 67, 69, 1000, 1001, 1002, 1003, 1004, 1005, 1006, 1007,
    1015, 1016, 1034, 1035, 1036, 1039, 1042, 1047, 1048, 1049, 2004, 2026, 2027, 2031, 2048, 5522, 9001,
)]

tty.setraw(0)
n = 0
asked = 0
count = 0


def write(data):
    os.write(1, data)


def ask(query):
    write(query)
    got = b""
    end = time.time() + 0.5
    while time.time() < end:
        ready, _, _ = select.select([0], [], [], 0.05)
        if ready:
            got += os.read(0, 4096)
    return got


def claude_screen():
    write(b"\x1b[H\x1b[2J")
    write(b"\x1b[2;3H\x1b[38;5;208m\xe2\x95\xad\xe2\x94\x80 Claude Code \xe2\x94\x80\xe2\x95\xae\x1b[0m")
    write(b"\x1b[4;3H> type here")
    write(b"\x1b[6;3Hcount %d" % count)


with open(os.path.join(WORK, "tabs", str(ME)), "w"):
    pass

while True:
    path = os.path.join(WORK, "cmd-%d" % ME)
    if os.path.exists(path):
        commands = open(path).read().splitlines()
        os.unlink(path)
        for command in commands:
            op, _, arg = command.partition(" ")
            if op == "print":
                out = []
                for _ in range(int(arg)):
                    n += 1
                    out.append(b"P%d-L%07d line of output\r\n" % (ME, n))
                write(b"".join(out))
            elif op == "title":
                write(b"\x1b]2;" + arg.encode() + b"\x07")
            elif op == "ask":
                asked += 1
                answers = [repr(ask(q)) for q in QUERIES]
                with open(os.path.join(WORK, "answers-%d-%d" % (ME, asked)), "w") as f:
                    f.write("\n".join(answers) + "\n")
            elif op == "claude":
                what, _, value = arg.partition(" ")
                if what == "on":
                    write(b"\x1b[?1049h\x1b[>1u\x1b[?2004h")
                    claude_screen()
                elif what == "count":
                    count = int(value)
                    claude_screen()
                elif what == "off":
                    write(b"\x1b[?2004l\x1b[<u\x1b[?1049l")
        with open(os.path.join(WORK, "done-%d" % ME), "w") as f:
            f.write("%d\n" % n)
    time.sleep(0.05)
