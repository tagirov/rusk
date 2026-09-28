# Drives one program in a pseudo-terminal for the tests (see
# `Sandbox::in_pty` in mod.rs). Bytes travel as hex both ways.
import json, os, pty, select, struct, sys, time, fcntl, termios

# argv[1]: {"argv": [...], "steps": [[pause_ms, hex bytes], ...],
#          "wait_for": hex marker, "kitty": bool, "timeout_ms": int}
# A step [pause_ms, hex bytes, rows, cols] also resizes the terminal after
# typing its bytes (the program gets SIGWINCH).
spec = json.loads(sys.argv[1])
argv = spec["argv"]
steps = [(s[0] / 1000.0, bytes.fromhex(s[1]), s[2:]) for s in spec.get("steps", [])]
marker = bytes.fromhex(spec.get("wait_for", ""))
kitty = spec.get("kitty", False)
deadline = time.time() + spec.get("timeout_ms", 10000) / 1000.0

try:
    pid, fd = pty.fork()
except OSError:
    # No pseudo-terminal to be had here: the test is skipped.
    print(json.dumps({"no_pty": True}))
    sys.exit(0)
if pid == 0:
    os.execv(argv[0], argv)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))

out = b""
answered = 0
started = not marker
next_at = None
status = None


def reply(data):
    try:
        os.write(fd, data)
    except OSError:
        pass


while time.time() < deadline:
    # Answer the terminal queries the way a terminal would: primary device
    # attributes, and the kitty keyboard flags when asked to act as kitty.
    while True:
        at = out.find(b"\x1b[c", answered)
        if at < 0:
            break
        if kitty and b"\x1b[?u" in out[max(0, at - 8):at]:
            reply(b"\x1b[?0u")
        reply(b"\x1b[?62;22c")
        answered = at + 3
    if not started and marker in out:
        started = True
    if started and steps and next_at is None:
        next_at = time.time() + steps[0][0]
    if started and steps and next_at is not None and time.time() >= next_at:
        _, data, size = steps.pop(0)
        reply(data)
        if size:
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", size[0], size[1], 0, 0))
        next_at = time.time() + steps[0][0] if steps else None
    r, _, _ = select.select([fd], [], [], 0.02)
    if r:
        try:
            data = os.read(fd, 65536)
        except OSError:
            data = b""
        if data:
            out += data
            continue
    done, st = os.waitpid(pid, os.WNOHANG)
    if done:
        status = st
        try:
            while True:
                r, _, _ = select.select([fd], [], [], 0.05)
                if not r:
                    break
                data = os.read(fd, 65536)
                if not data:
                    break
                out += data
        except OSError:
            pass
        break

timed_out = status is None
if timed_out:
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass
    _, status = os.waitpid(pid, 0)
if os.WIFSIGNALED(status):
    code = -os.WTERMSIG(status)
else:
    code = os.WEXITSTATUS(status)
print(json.dumps({
    "code": code,
    "timed_out": timed_out,
    "output": out.hex(),
}))
