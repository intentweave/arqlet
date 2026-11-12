"""Loopback-only prototype server. No dependencies; native jobs are bounded."""
import argparse
import json
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from pathlib import Path
from urllib.parse import unquote, urlsplit
import queue
import secrets
import subprocess
import threading

ROOT = Path(__file__).resolve().parent
BINARY = ROOT.parent.parent / "target/debug/layout-command"
# Workspace packages are served read-only at this prefix; nothing else outside apps/playground is exposed.
PACKAGES_PREFIX = "/packages/"
PACKAGES_ROOT = (ROOT.parent.parent / "packages").resolve()
SLOTS = threading.BoundedSemaphore(2)
# Every operation handled by the Rust dispatcher; the backend adds no graph semantics.
OPERATIONS = {"import", "add", "toggle", "reveal", "trace", "direction", "refine", "structuredRefine", "configure", "optionDefaults", "measure", "shape", "scene", "move", "strategy", "routing", "metrics", "routeConflicts", "componentRefine", "containerRefine", "elkLayout", "overview", "refineProgressive", "intent", "restore", "port", "settle", "referenceCreate", "referenceRefresh", "referenceReroute", "referenceValidate", "referenceCanonical", "referenceSvg", "referenceFromSvg", "referenceReproduce", "referenceRepair"}
MAX_BODY = 64_000_000
# Operations a session runs on its own view (besides OPERATIONS): start it from a snapshot, hand its snapshot out.
SESSION_OPERATIONS = {"load", "export"}
MAX_SESSIONS = 32
SESSION_TIMEOUT = 15


class SessionHost:
    """Provisional (ADR 0019, step 3): sessions whose snapshot stays in one long-running native process
    (`layout-command --sessions`). Messages go to it one at a time; the browser sees random keys, never the process's
    numbers. A message that takes longer than SESSION_TIMEOUT restarts the process: every session is lost then, and
    the next message to one is answered as an unknown session."""

    def __init__(self):
        self.lock = threading.Lock()
        self.process = None
        self.answers = None
        self.keys = {}

    def _start(self):
        self.process = subprocess.Popen([str(BINARY), "--sessions"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL, text=True)
        self.answers = queue.Queue()
        stdout, answers = self.process.stdout, self.answers
        threading.Thread(target=lambda: [answers.put(line) for line in stdout], daemon=True).start()
        self.keys = {}

    def _stop(self):
        if self.process:
            self.process.kill()
            self.process.wait()
        self.process = None
        self.keys = {}

    def _ask(self, message):
        if self.process is None or self.process.poll() is not None:
            self._start()
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()
        try:
            return json.loads(self.answers.get(timeout=SESSION_TIMEOUT))
        except queue.Empty:
            self._stop()
            raise TimeoutError

    def handle(self, body):
        """Answers one session message: (status, body)."""
        session = body.get("session")
        with self.lock:
            try:
                if session == "open":
                    if len(self.keys) >= MAX_SESSIONS:
                        return 503, {"ok": False, "error": "Too many sessions; close one"}
                    opened = self._ask({"session": "open"})
                    key = secrets.token_hex(16)
                    self.keys[key] = opened["session"]
                    return 200, {"ok": True, "session": key}
                native = self.keys.get(session) if isinstance(session, str) else None
                if native is None:
                    return 404, {"ok": False, "error": "Unknown session"}
                if body.get("close") is True:
                    self._ask({"session": native, "close": True})
                    del self.keys[session]
                    return 200, {"ok": True}
                request = body.get("request")
                if not isinstance(request, dict) or request.get("op") not in OPERATIONS | SESSION_OPERATIONS:
                    return 400, {"ok": False, "error": "Unsupported operation"}
                answer = self._ask({"session": native, "request": request})
                return (200 if answer.get("ok") else 400), answer
            except TimeoutError:
                return 504, {"ok": False, "error": "Time budget exceeded; the backend's sessions were restarted"}
            except (OSError, ValueError):
                self._stop()
                return 500, {"ok": False, "error": "Native binary missing or invalid result; rebuild demo"}


SESSIONS = SessionHost()

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def translate_path(self, path):
        route = unquote(urlsplit(path).path)
        if route.startswith(PACKAGES_PREFIX):
            target = (PACKAGES_ROOT / route[len(PACKAGES_PREFIX):]).resolve()
            if PACKAGES_ROOT in target.parents and target.is_file():
                return str(target)
            return str(PACKAGES_ROOT / ".not-found")  # no such file: the base class answers 404
        return super().translate_path(path)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def reply(self, status, body):
        raw = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        try:
            self.wfile.write(raw)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_POST(self):
        if self.path not in ("/api/op", "/api/session"):
            return self.reply(404, {"ok": False, "error": "Unknown endpoint"})
        host = f"127.0.0.1:{self.server.server_port}"
        if self.headers.get("Host") not in (host, f"localhost:{self.server.server_port}"):
            return self.reply(403, {"ok": False, "error": "Invalid host"})
        origin = self.headers.get("Origin")
        if origin and origin not in (f"http://{host}", f"http://localhost:{self.server.server_port}"):
            return self.reply(403, {"ok": False, "error": "Cross-origin request rejected"})
        if self.headers.get_content_type() != "application/json":
            return self.reply(415, {"ok": False, "error": "JSON required"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length <= MAX_BODY:
                return self.reply(413, {"ok": False, "error": "Request too large or empty"})
            self.connection.settimeout(10)
            request = json.loads(self.rfile.read(length))
            if self.path == "/api/session":
                if not isinstance(request, dict):
                    return self.reply(400, {"ok": False, "error": "Invalid request"})
                return self.reply(*SESSIONS.handle(request))
            if not isinstance(request, dict) or request.get("op") not in OPERATIONS:
                return self.reply(400, {"ok": False, "error": "Unsupported operation"})
        except (ValueError, TimeoutError):
            return self.reply(400, {"ok": False, "error": "Invalid request"})
        if not SLOTS.acquire(blocking=False):
            return self.reply(503, {"ok": False, "error": "Backend busy; retry later"})
        try:
            result = subprocess.run([str(BINARY)], input=json.dumps(request), text=True, capture_output=True, timeout=15)
            if result.returncode:
                return self.reply(500, {"ok": False, "error": "Native computation failed"})
            response = json.loads(result.stdout)
            self.reply(200 if response.get("ok") else 400, response)
        except subprocess.TimeoutExpired:
            self.reply(504, {"ok": False, "error": "Refinement time budget exceeded"})
        except (OSError, ValueError):
            self.reply(500, {"ok": False, "error": "Native binary missing or invalid result; rebuild demo"})
        finally:
            SLOTS.release()

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    print(f"Arqlet: http://127.0.0.1:{args.port}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
