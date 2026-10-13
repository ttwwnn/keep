#!/usr/bin/env python3
"""A tab kept by its client while there is no surface for it (crates/keep/src/keeper.rs).

Drives the client binary the way the app does — a pty standing in for each
surface, the attach file in a directory of its own, `hibernate` written before
the surface goes and `take=` naming the keeper when the next one comes — against
a daemon of its own, and checks what each surface was sent:

  - every line the tab printed reaches the surface that wakes it, once and in
    order: the history before the first surface went (thousands of lines, far
    more than the daemon keeps), what came while nobody was drawing it, and
    what came after;
  - while kept, the program is answered as the surface would answer it, and
    what it says about itself (title, directory, clipboard, background) is
    left for the app;
  - a program on the alternate screen comes back on it, with the shell's
    screen and history underneath when it leaves;
  - the keeper goes when the app says it will not come back, when the app is
    gone, and when the tab ends; a surface that finds no keeper attaches the
    ordinary way.

Usage: driver.py <keep> <keepd> <replay> [--no-mirror]
Exit 0 when every check passed; prints one line per check.
"""

import fcntl
import os
import pty
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time

KEEP, KEEPD, REPLAY = (os.path.abspath(a) for a in sys.argv[1:4])
NO_MIRROR = "--no-mirror" in sys.argv
COLS, ROWS = 105, 38
WORK = tempfile.mkdtemp(prefix="keeper-test-")
SOCK = "/tmp/kpt-%d.sock" % os.getpid()
PASSED = 0
FAILED = 0

FACTS = """foreground=ffffff
background=282c34
cursor=ffffff
scheme=light
cell=16,34
xtversion=ghostty 1.3.2-HEAD-+a53771a
da=62;22;52/1;10;0
"""


def check(what, wanted, got):
    global PASSED, FAILED
    if wanted == got:
        PASSED += 1
        print("  ok    %s" % what)
    else:
        FAILED += 1
        print("  FAIL  %s" % what)
        print("        wanted: %r" % (wanted,))
        print("        got:    %r" % (got,))
        # A sabotage needs only to be caught: the first failure says so, and
        # the waits the rest would time out on are spared.
        if os.environ.get("KEEPER_FAIL_FAST"):
            raise SystemExit(1)


def wait_for(seconds, predicate):
    end = time.time() + seconds
    while time.time() < end:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


# --- the tab's program ----------------------------------------------------------

PROGRAM = r'''#!/usr/bin/env python3
import os, select, sys, time, tty
work = os.environ["KEEPER_WORK"]
tty.setraw(0)
n = 0
asked = 0
open(os.path.join(work, "program"), "w").write(str(os.getpid()))
def ask(query):
    os.write(1, query)
    got = b""
    end = time.time() + 0.8
    while time.time() < end:
        r, _, _ = select.select([0], [], [], 0.05)
        if r:
            got += os.read(0, 4096)
    return got
while True:
    path = os.path.join(work, "cmd")
    if os.path.exists(path):
        commands = open(path).read().splitlines()
        os.unlink(path)
        for command in commands:
            op, _, arg = command.partition(" ")
            if op == "print":
                out = []
                for _ in range(int(arg)):
                    n += 1
                    out.append(b"L%07d line of output\r\n" % n)
                os.write(1, b"".join(out))
            elif op == "pace":
                # Lots of lines, a pause after each: output a reader keeps
                # up with, as the daemon's queue for a client does not hold
                # a burst of thousands of lines at once.
                lots, per, pause = arg.split()
                for _ in range(int(lots)):
                    out = []
                    for _ in range(int(per)):
                        n += 1
                        out.append(b"L%07d line of output\r\n" % n)
                    os.write(1, b"".join(out))
                    time.sleep(float(pause))
            elif op == "raw":
                os.write(1, arg.replace("\\e", "\x1b").replace("\\a", "\x07").replace("\\r", "\r").replace("\\n", "\n").encode())
            elif op == "ask":
                asked += 1
                answers = []
                for query in arg.split("|"):
                    answers.append(repr(ask(query.replace("\\e", "\x1b").replace("\\a", "\x07").encode())))
                open(os.path.join(work, "answers-%d" % asked), "w").write("\n".join(answers))
            elif op == "bigosc":
                os.write(1, b"\x1b]52;c;" + b"A" * int(arg))
            elif op in ("listen", "measure"):
                asked += 1
                got = bytearray()
                end = time.time() + float(arg or "1.5")
                while time.time() < end:
                    r, _, _ = select.select([0], [], [], 0.05)
                    if r:
                        got += os.read(0, 65536)
                got = bytes(got)
                heard = repr(got) if op == "listen" else "%d %r %r" % (len(got), got[:8], got[-2:])
                open(os.path.join(work, "answers-%d" % asked), "w").write(heard)
            elif op == "exit":
                sys.exit(0)
        open(os.path.join(work, "done"), "w").write(str(n))
    time.sleep(0.02)
'''

program = os.path.join(WORK, "program.py")
with open(program, "w") as f:
    f.write(PROGRAM)
os.chmod(program, 0o755)

commands = 0


def send(*lines):
    """Have the program do these; `finished` waits until it has."""
    done = os.path.join(WORK, "done")
    if os.path.exists(done):
        os.unlink(done)
    with open(os.path.join(WORK, "cmd.tmp"), "w") as f:
        f.write("\n".join(lines) + "\n")
    os.rename(os.path.join(WORK, "cmd.tmp"), os.path.join(WORK, "cmd"))
    return lines


def finished(lines):
    if not wait_for(90, lambda: os.path.exists(os.path.join(WORK, "done"))):
        print("the program did not do: %r" % (lines,))
        sys.exit(2)


def command(*lines):
    """Have the program do these, and wait until it has."""
    finished(send(*lines))


# --- the daemon -------------------------------------------------------------------

env = dict(os.environ, KEEP_SOCKET=SOCK, KEEP_STATE_DIR=os.path.join(WORK, "state"), KEEPER_WORK=WORK,
           KEEP_WAKE_PATIENCE="5", KEEP_CONTINUATION_LIMIT="4096")
os.makedirs(env["KEEP_STATE_DIR"])
daemon = subprocess.Popen([KEEPD], env=dict(env, SHELL=program), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
wait_for(30, lambda: os.path.exists(SOCK))
subprocess.run([KEEP, "new", "k"], env=env, stdout=subprocess.DEVNULL, check=True)
wait_for(30, lambda: os.path.exists(os.path.join(WORK, "program")))
PROGRAM_PID = int(open(os.path.join(WORK, "program")).read())

# The app's own process, as far as a keeper knows: something that can die.
app = subprocess.Popen(["/bin/sleep", "3600"])


# --- surfaces -----------------------------------------------------------------------

class Surface:
    """A pty standing in for a surface, with the client running in it."""

    count = 0

    def __init__(self, take=None, mirror=True, app_pid=None, showing=True, where="keep-attach"):
        Surface.count += 1
        # Where the app puts a client: a directory of its own inside one
        # named keep-attach. Anywhere else a client never keeps.
        self.dir = os.path.join(WORK, where, "surface-%d" % Surface.count)
        os.makedirs(self.dir)
        self.showing = os.path.join(self.dir, "showing")
        with open(self.showing, "w") as f:
            f.write("1" if showing else "0")
        lines = ["k", "1", self.showing]
        if mirror and not NO_MIRROR:
            lines.append("mirror=10000000")
            lines.append("grapheme=1")
            lines.append("blink=1")
        if take:
            lines.append("take=" + take)
        lines.append("app=%d" % (app_pid or app.pid))
        # Whose directory it is, as the app says it beside each client.
        with open(os.path.join(self.dir, "app"), "w") as f:
            f.write("%d\n" % (app_pid or app.pid))
        with open(os.path.join(self.dir, ".keep-attach"), "w") as f:
            f.write("\n".join(lines) + "\n")
        self.received = bytearray()
        self.closing = False
        self.pid, self.master = pty.fork()
        if self.pid == 0:
            # The child is the client, or nothing: never the rest of this script.
            try:
                fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
                os.chdir(self.dir)
                os.execve(KEEP, [KEEP], env)
            finally:
                os._exit(127)
        # Not inherited by the clients of the surfaces made after this one,
        # as a surface's is not: a client holding another's pty open would
        # keep that one's surface from ever going.
        fcntl.fcntl(self.master, fcntl.F_SETFD, fcntl.FD_CLOEXEC)
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        import select
        while not self.closing:
            try:
                ready, _, _ = select.select([self.master], [], [], 0.05)
                if not ready:
                    continue
                data = os.read(self.master, 65536)
            except (OSError, ValueError):
                return
            if not data:
                return
            self.received.extend(data)
            # What a surface answers when asked dark or light: light, as
            # the facts say.
            if b"\x1b[?996n" in data:
                try:
                    os.write(self.master, b"\x1b[?997;2n")
                except OSError:
                    pass

    def let_go(self):
        """Close the surface's end, as a surface going away does."""
        self.closing = True
        self.reader.join(2)
        try:
            os.close(self.master)
        except OSError:
            pass
        # What a surface does to its process group as it goes.
        try:
            os.killpg(self.pid, signal.SIGHUP)
        except (ProcessLookupError, PermissionError):
            # Gone already, the pty's own hang-up having ended it: macOS
            # says EPERM of a group left with only a zombie in it.
            pass

    def has(self, text):
        return text.encode() in self.received

    def hide(self):
        with open(self.showing, "w") as f:
            f.write("0")

    def hibernate(self, wait=0.3):
        """What the app does: hide it, say it will come back, let go of it."""
        self.hide()
        time.sleep(wait)
        with open(os.path.join(self.dir, "facts"), "w") as f:
            f.write(FACTS)
        with open(os.path.join(self.dir, "hibernate"), "w") as f:
            f.write("1")
        self.let_go()

    def ttys(self):
        """The terminals the client still has open, as lsof sees them."""
        out = subprocess.run(["lsof", "-p", str(self.pid)], capture_output=True, text=True).stdout
        return [line.split()[-1] for line in out.splitlines() if "/dev/ttys" in line]

    def kept(self):
        return os.path.exists(os.path.join(self.dir, "kept"))

    def alive(self):
        try:
            pid, _ = os.waitpid(self.pid, os.WNOHANG)
            return pid == 0
        except ChildProcessError:
            return False


    def replay(self, more=None):
        path = os.path.join(WORK, "received-%d.bin" % self.pid)
        with open(path, "wb") as f:
            f.write(bytes(self.received))
        args = [REPLAY, str(COLS), str(ROWS), path] + ([more] if more else [])
        out = subprocess.run(args, capture_output=True, text=True).stdout
        head, _, text = out.partition("text\n")
        report = dict(line.split(" ", 1) if " " in line else (line, "") for line in head.splitlines())
        return report, text


def pid_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def numbers(text):
    import re
    return [int(m) for m in re.findall(r"L(\d{7}) line of output", text)]


def in_order_once(found, upto):
    return found == list(range(1, upto + 1))


try:
    print("a shell with a long history, kept and handed over")
    one = Surface()
    wait_for(30, lambda: len(one.received) > 0)
    command("print 3000", "raw \\e]2;first title\\a")
    wait_for(90, lambda: one.has("L0003000"))
    check("the first surface was sent the tab's 3000 lines", True, one.has("L0003000"))
    one.hibernate()
    wait_for(30, one.kept)
    check("its client keeps the tab when the surface goes", True, one.kept())
    check("and is still running", True, one.alive())
    check("holding on to no terminal of the surface's", [], one.ttys())

    print("while kept")
    command(
        "print 2000",
        "raw \\e]2;second title\\a\\e]7;file://host/tmp/elsewhere\\a\\e]52;c;aGVsbG8=\\a\\e]11;rgb:01/02/03\\a",
        "ask \\e[c|\\e[>c|\\e[=c|\\e[>q|\\e]11;?\\e\\\\|\\e[?996n|\\e[14t|\\e[18t|\\e[?u"
        "|\\eP+q544E\\e\\\\|\\e_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\\e\\\\",
    )
    answers = open(os.path.join(WORK, "answers-1")).read().splitlines()
    check("CSI c is answered as the surface answers it", repr(b"\x1b[?62;22;52c"), answers[0])
    check("CSI > c too", repr(b"\x1b[>1;10;0c"), answers[1])
    check("CSI = c is left unanswered, as the surface leaves it", repr(b""), answers[2])
    check("XTVERSION names the surface's terminal", repr(b"\x1bP>|ghostty 1.3.2-HEAD-+a53771a\x1b\\"), answers[3])
    check("OSC 11 reports the background the program set", repr(b"\x1b]11;rgb:0101/0202/0303\x1b\\"), answers[4])
    check("the colour scheme is the surface's", repr(b"\x1b[?997;2n"), answers[5])
    check("the text area in pixels", repr(b"\x1b[4;1292;1680t"), answers[6])
    check("and in cells", repr(b"\x1b[8;38;105t"), answers[7])
    check("the keyboard protocol", repr(b"\x1b[?0u"), answers[8])
    check("the terminfo entry it runs as (XTGETTCAP TN)", repr(b"\x1bP1+r544E=787465726D2D67686F73747479\x1b\\"),
          answers[9])
    check("and pictures spoken, as a surface answers asked (the Kitty graphics query)",
          repr(b"\x1b_Gi=31;OK\x1b\\"), answers[10])
    read = lambda name: open(os.path.join(one.dir, name), "rb").read() if os.path.exists(os.path.join(one.dir, name)) else b""
    check("the title is left for the app", b"second title", read("title"))
    check("and the directory", b"file://host/tmp/elsewhere", read("pwd"))
    check("and the clipboard", b"hello", read("clipboard"))
    check("and the background", b"010203\n", read("background"))
    # The window turns dark: the app writes the facts again. The program
    # that asked to be told (2031) is, and is answered dark from then on.
    command("raw \\e[?2031h")
    time.sleep(0.5)
    with open(os.path.join(one.dir, "facts"), "w") as f:
        f.write(FACTS.replace("scheme=light", "scheme=dark"))
    command("listen 1.5", "ask \\e[?996n")
    told = open(os.path.join(WORK, "answers-2")).read()
    check("a program asking to be told of the scheme is told it turned dark", repr(b"\x1b[?997;1n"), told)
    asked = open(os.path.join(WORK, "answers-3")).read().splitlines()
    check("and asking, is answered dark", repr(b"\x1b[?997;1n"), asked[0])
    with open(os.path.join(one.dir, "facts"), "w") as f:
        f.write(FACTS)
    command("listen 1.5", "raw \\e[?2031l")
    check("and light again", repr(b"\x1b[?997;2n"), open(os.path.join(WORK, "answers-4")).read())

    # The program reads the clipboard (OSC 52): its keeper asks the app,
    # played here, which answers with the pasteboard; where the config
    # denies it, nobody is asked and nothing is answered.
    def play_the_app(directory, text, seen, taking=0.0):
        request = os.path.join(directory, "clipboard-request")
        if wait_for(5, lambda: os.path.exists(request)):
            ident = open(request).read().split("\n")[0]
            seen.append(ident)
            time.sleep(taking)
            with open(os.path.join(directory, ".answer"), "w") as f:
                f.write(ident + "\n" + text)
            os.rename(os.path.join(directory, ".answer"), os.path.join(directory, "clipboard-answer"))
    seen = []
    app_thread = threading.Thread(target=play_the_app, args=(one.dir, "pasted", seen), daemon=True)
    app_thread.start()
    command("ask \\e]52;c;?\\a")
    app_thread.join(5)
    check("a program reading the clipboard is answered with what the app read", repr(b"\x1b]52;c;cGFzdGVk\x1b\\"),
          open(os.path.join(WORK, "answers-5")).read().splitlines()[0])
    check("the app was asked once", 1, len(seen))
    os.unlink(os.path.join(one.dir, "clipboard-request"))
    with open(os.path.join(one.dir, "facts"), "w") as f:
        f.write(FACTS + "clipboard-read=deny\n")
    time.sleep(0.5)
    command("ask \\e]52;c;?\\a|\\e[5n")
    answers = open(os.path.join(WORK, "answers-6")).read().splitlines()
    check("where the config denies it, nothing is answered and nobody asked",
          (repr(b""), repr(b"\x1b[0n"), False),
          (answers[0], answers[1], os.path.exists(os.path.join(one.dir, "clipboard-request"))))
    with open(os.path.join(one.dir, "facts"), "w") as f:
        f.write(FACTS)

    print("a surface comes back for it")
    two = Surface(take=one.dir)
    wait_for(90, lambda: not os.path.exists(one.dir))
    check("the keeper is gone, and its directory with it", False, os.path.exists(one.dir))
    wait_for(10, lambda: os.path.exists(os.path.join(two.dir, "took")))
    check("the new client says it took the pane", b"1\n", open(os.path.join(two.dir, "took"), "rb").read() if os.path.exists(os.path.join(two.dir, "took")) else b"")
    command("print 500")
    wait_for(90, lambda: two.has("L0005500"))
    report, text = two.replay()
    found = numbers(text)
    check("every line reached it, once and in order (5500)", True, in_order_once(found, 5500))
    if not in_order_once(found, 5500):
        print("        lines: %d, first %s, last %s" % (len(found), found[:3], found[-3:]))
    check("with the title the program set while kept", "second title", report.get("title"))
    check("it is attached the way the daemon counts", True,
          "attached" in subprocess.run([KEEP, "ls"], env=env, capture_output=True, text=True).stdout)

    print("a client the app did not put where it puts them")
    elsewhere = Surface(where="elsewhere")
    wait_for(30, lambda: elsewhere.has("L0005500"))
    elsewhere.hibernate()
    wait_for(10, lambda: not elsewhere.alive())
    check("goes with its surface, whatever the files beside it say", (False, False),
          (elsewhere.alive(), elsewhere.kept()))

    print("a surface that finds a keeper that does not answer")
    ghost = os.path.join(WORK, "keep-attach", "ghost")
    os.makedirs(ghost)
    with open(os.path.join(ghost, "hibernate"), "w") as f:
        f.write("1")
    lost = Surface(take=ghost)
    wait_for(10, lambda: not os.path.exists(os.path.join(ghost, "hibernate")))
    wait_for(30, lambda: lost.has("L0005500"))
    check("gives up on it, tells it to go, and attaches the ordinary way", (False, True),
          (os.path.exists(os.path.join(ghost, "hibernate")), lost.has("L0005500")))
    lost.let_go()

    print("a program on the alternate screen, over the shell")
    command("raw \\e[?1049h\\e[H\\e[2JFULL SCREEN PROGRAM\\e[>1u")
    two.hibernate()
    wait_for(30, two.kept)
    check("kept again", True, two.kept())
    command("raw \\e[3;1Hdrawn while kept")
    three = Surface(take=two.dir)
    wait_for(90, lambda: three.has("drawn while kept"))
    report, text = three.replay()
    check("it comes back on the alternate screen", "yes", report.get("alternate"))
    check("with what the program drew there, kept or not", True,
          "FULL SCREEN PROGRAM" in text and "drawn while kept" in text)
    check("and the program's keyboard", "1", report.get("kitty"))
    report, text = three.replay("\\e[<u\\e[?1049l")
    found = numbers(text)
    if not in_order_once(found, 5500):
        print("        lines: %d, first %s, last %s, gaps %s" % (
            len(found), found[:3], found[-3:],
            [(a, b) for a, b in zip(found, found[1:]) if b != a + 1][:5]))
    check("when it leaves, the shell's screen and history are under it (5500 lines)", True,
          in_order_once(found, 5500))
    check("on the main screen", "no", report.get("alternate"))
    command("raw \\e[<u\\e[?1049l", "print 100")

    print("a surface that comes for the pane before its keeper listens")
    # The app let go of a surface a moment ago and shows the tab again at
    # once: the new client is there before the old one keeps.
    three.hide()
    time.sleep(0.3)
    with open(os.path.join(three.dir, "facts"), "w") as f:
        f.write(FACTS)
    with open(os.path.join(three.dir, "hibernate"), "w") as f:
        f.write("1")
    four = Surface(take=three.dir)
    time.sleep(0.8)
    check("the new client waits for the keeper", True, four.alive() and not os.path.exists(os.path.join(four.dir, "took")))
    lflag = termios.tcgetattr(four.master)[3]
    check("with its terminal raw already", (0, 0), (lflag & termios.ICANON, lflag & termios.ISIG))
    os.write(four.master, b"\x03")
    time.sleep(0.3)
    check("a ctrl-c meanwhile is the program's, not a signal for the client", True, four.alive())
    three.let_go()
    wait_for(90, lambda: four.has("L0005600"))
    found = numbers(four.replay()[1])
    check("and takes the pane from it: every line, once and in order (5600)", True, in_order_once(found, 5600))
    wait_for(30, lambda: not three.alive())
    check("the keeper is gone", False, three.alive())
    command("listen 0.5")
    check("and the ctrl-c reached the program", "b'\\x03'", open(os.path.join(WORK, "answers-7")).read())

    print("a surface let go of while its client waits to take the pane")
    four.hibernate()
    wait_for(30, four.kept)
    # Shown for a moment and out of sight again: the client waits to be
    # shown, and the app lets go of its surface meanwhile, meaning to come
    # back. Its process group is hung up, as a surface going does.
    five = Surface(take=four.dir, showing=False)
    time.sleep(0.4)
    with open(os.path.join(five.dir, "facts"), "w") as f:
        f.write(FACTS)
    with open(os.path.join(five.dir, "hibernate"), "w") as f:
        f.write("1")
    five.let_go()
    wait_for(30, five.kept)
    check("the client outlives its surface, takes the pane and keeps it", True, five.kept() and five.alive())
    check("from the keeper before it, which is gone", False, os.path.exists(four.dir))
    command("print 100")
    six = Surface(take=five.dir)
    wait_for(90, lambda: six.has("L0005700"))
    found = numbers(six.replay()[1])
    check("the next surface has every line, once and in order (5700)", True, in_order_once(found, 5700))

    print("a sequence cut in two")
    # The surface goes out of sight while the program is in the middle of
    # a title: the client's own question waits for the end of it.
    command("raw \\e]2;cut")
    wait_for(30, lambda: six.has("\x1b]2;cut"))
    six.hide()
    time.sleep(0.6)
    command("raw  in two\\a")
    wait_for(30, lambda: six.has(" in two\x07"))
    wait_for(5, lambda: six.has(" in two\x07\x1b[?996n"))
    tail = bytes(six.received[six.received.rfind(b"\x1b]2;cut"):])
    check("nothing is written into the middle of the program's sequence", True,
          tail.startswith(b"\x1b]2;cut in two\x07"))
    check("the question comes after it", True, b"\x1b[?996n" in tail)
    check("so the title is the program's", "cut in two", six.replay()[0].get("title"))
    # And handed over in the middle of one: the rest finishes it there.
    command("raw \\e]2;handed")
    six.hibernate()
    wait_for(30, six.kept)
    seven = Surface(take=six.dir)
    wait_for(30, lambda: os.path.exists(os.path.join(seven.dir, "took")))
    command("raw  over\\a", "print 1")
    wait_for(30, lambda: seven.has("L0005701"))
    check("a sequence the handover cut off is finished on the new surface", "handed over", seven.replay()[0].get("title"))
    # One too long to say the start of (a clipboard of over a megabyte; a
    # few kilobytes, as the tests have the limit): the keeper hands over
    # once the program has ended it.
    command("bigosc 20000")
    seven.hibernate()
    wait_for(30, seven.kept)
    seven = Surface(take=seven.dir)
    time.sleep(1.0)
    check("the keeper waits for the end of a sequence too long to hand over", False,
          os.path.exists(os.path.join(seven.dir, "took")))
    command("raw \\a", "print 1")
    wait_for(30, lambda: seven.has("L0005702"))
    check("and hands over after it, none of it written as text", False, "AAAAAAAAAA" in seven.replay()[1])

    print("an image drawn while kept")
    # The one thing a handover cannot carry: the keeper holds what the
    # program writes from the image on and asks the app for a surface,
    # played here, which is handed the pane and then that, as it came.
    seven.hibernate()
    wait_for(30, seven.kept)
    command("raw before the image\\r\\n\\e_Ga=T,f=24,s=1,v=1;AAAA\\e\\ after the image\\r\\n", "print 1")
    wait_for(10, lambda: os.path.exists(os.path.join(seven.dir, "wake")))
    check("the keeper asks the app for a surface", True, os.path.exists(os.path.join(seven.dir, "wake")))
    woken = Surface(take=seven.dir)
    wait_for(30, lambda: woken.has("L0005703"))
    check("the image reaches the surface made for it, with what came before and after it", True,
          b"before the image\r\n\x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\ after the image\r\nL0005703" in bytes(woken.received))
    found = numbers(woken.replay()[1])
    check("and every line, once and in order (5703)", True, in_order_once(found, 5703))
    check("its client says the pane is not to be let go of again", True,
          wait_for(10, lambda: os.path.exists(os.path.join(woken.dir, "unkeepable"))))
    took = os.path.join(woken.dir, "took")
    wait_for(10, lambda: os.path.exists(took))
    check("having said so before it says it took the pane", True,
          os.stat(os.path.join(woken.dir, "unkeepable")).st_mtime_ns <= os.stat(took).st_mtime_ns)

    print("an image no surface comes for")
    # The keeper holds the program's output for a while, then gives it to
    # its mirror: the question asked after the image is answered then, and
    # a second image asks for a surface anew.
    woken.hibernate()
    wait_for(30, woken.kept)
    wake = os.path.join(woken.dir, "wake")
    sent = send("raw \\e_Ga=T,f=24,s=1,v=1;AAAA\\e\\ \\e[6n", "listen 8")
    wait_for(10, lambda: os.path.exists(wake))
    first = open(wake).read() if os.path.exists(wake) else ""
    finished(sent)
    check("the program's question after it is answered once the keeper gives up waiting", True,
          b"R" in eval(open(os.path.join(WORK, "answers-8")).read()))
    command("raw \\e_Ga=T,f=24,s=1,v=1;AAAA\\e\\ ")
    wait_for(10, lambda: os.path.exists(wake))
    second = open(wake).read() if os.path.exists(wake) else ""
    check("and a second image asks anew, in words the app has not heard", (True, True, True),
          (first != "", second != "", second != first))

    print("a program reading the clipboard while the app takes its time")
    # The keeper does not wait for the app's answer: it goes on reading
    # the program, which goes on writing; the answer reaches the program
    # when the app gives it. One still unanswered when a surface takes the
    # pane is that surface's to answer, and one bigger than the daemon's
    # biggest frame goes to the program all the same.
    ten = Surface(take=woken.dir)
    wait_for(90, lambda: ten.has("L0005703"))
    ten.hibernate()
    wait_for(30, ten.kept)
    seen = []
    slow = threading.Thread(target=play_the_app, args=(ten.dir, "later", seen, 4.0), daemon=True)
    slow.start()
    # Paced, as a program that goes on working writes: no more than a
    # reader keeps up with — and far more than the daemon holds for one
    # that stops reading for the four seconds the app takes.
    command("raw \\e]52;c;?\\a", "pace 200 200 0.02", "listen 4")
    slow.join(10)
    check("a program reading the clipboard is answered once the app answers, however long that takes",
          repr(b"\x1b]52;c;bGF0ZXI=\x1b\\"), open(os.path.join(WORK, "answers-9")).read())
    eleven = Surface(take=ten.dir)
    wait_for(90, lambda: eleven.has("L0045703"))
    found = numbers(eleven.replay()[1])
    whole = len(found) > 5000 and found == list(range(found[0], 45704))
    if not whole:
        print("        lines: %d, first %s, last %s, gaps %s" % (
            len(found), found[:3], found[-3:], [(a, b) for a, b in zip(found, found[1:]) if b != a + 1][:5]))
    check("and its keeper read the program meanwhile: no line lost (to 45703)", True, whole)
    eleven.hibernate()
    wait_for(30, eleven.kept)
    command("raw \\e]52;c;?\\a", "print 1")
    wait_for(10, lambda: os.path.exists(os.path.join(eleven.dir, "clipboard-request")))
    twelve = Surface(take=eleven.dir)
    wait_for(90, lambda: twelve.has("L0045704"))
    check("a read unanswered when the pane is taken is asked of the surface that takes it", True,
          wait_for(10, lambda: b"\x1b]52;c;?\x1b\\" in bytes(twelve.received)))
    twelve.hibernate()
    wait_for(30, twelve.kept)
    big = threading.Thread(target=play_the_app, args=(twelve.dir, "x" * (13 << 20), [], 0.5), daemon=True)
    big.start()
    command("raw \\e]52;c;?\\a", "measure 15")
    big.join(10)
    check("an answer bigger than the daemon's biggest frame reaches the program whole",
          "18175329 b'\\x1b]52;c;e' b'\\x1b\\\\'", open(os.path.join(WORK, "answers-10")).read())
    woken = twelve

    print("the keeper goes when it should")
    keeper_dir = woken.dir
    os.unlink(os.path.join(keeper_dir, "hibernate"))
    wait_for(30, lambda: not woken.alive())
    check("when the app says it will not come back", False, woken.alive())
    check("the tab is untouched", True, pid_alive(PROGRAM_PID))
    eight = Surface(take=keeper_dir)
    wait_for(90, lambda: eight.has("L0045704"))
    check("a surface that finds no keeper attaches the ordinary way", True, eight.has("L0045704"))
    eight.hibernate()
    wait_for(30, eight.kept)
    app.kill()
    app.wait()
    wait_for(30, lambda: not eight.alive())
    check("when the app is gone", False, eight.alive())
    app2 = subprocess.Popen(["/bin/sleep", "3600"])
    nine = Surface(take=eight.dir, app_pid=app2.pid)
    wait_for(90, lambda: nine.has("L0045704"))
    nine.hibernate()
    wait_for(30, nine.kept)
    with open(os.path.join(nine.dir, "somebody's"), "w") as f:
        f.write("not the keeper's")
    command_nowait = os.path.join(WORK, "cmd")
    with open(command_nowait, "w") as f:
        f.write("exit\n")
    wait_for(30, lambda: not nine.alive())
    check("when the tab ends", False, nine.alive())
    check("taking only its own files with it", (True, False),
          (os.path.exists(os.path.join(nine.dir, "somebody's")), os.path.exists(os.path.join(nine.dir, "kept"))))
    app2.kill()

finally:
    for proc in (app,):
        try:
            proc.kill()
        except Exception:
            pass
    daemon.terminate()
    try:
        daemon.wait(5)
    except Exception:
        daemon.kill()
    try:
        os.unlink(SOCK)
    except OSError:
        pass
    if os.environ.get("KEEP_WORK"):
        print("kept: " + WORK)
    else:
        shutil.rmtree(WORK, ignore_errors=True)

print("passed %d, failed %d" % (PASSED, FAILED))
sys.exit(0 if FAILED == 0 else 1)
