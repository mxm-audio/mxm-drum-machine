"""Serves a local drum A/B site on 127.0.0.1:8777, saves drawings and keeps the owner's review state.

python ab_serve.py <site dir>

A page opened from disk cannot POST, and browser automation refuses file:// URLs, so the page is
served. Each "Send to Claude" lands in <site dir>/annotations/ as the annotated PNG and a .txt with
the note. Approve / Needs work / Reset update <site dir>/review.json (see ab_review.py) and append to
<site dir>/review-log.jsonl with the time; an approval stores the fingerprint of the audio approved,
so a later change to that model's sound is detected. See
docs/drum-model-fitting.md.
"""

import base64
import json
import re
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ab_review  # noqa: E402

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path.cwd()
OUT = ROOT / "annotations"
REVIEW = ROOT / "review.json"
# Every review action with its time, so "did I approve that?" has an answer.
LOG = ROOT / "review-log.jsonl"
LOCK = threading.Lock()


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def end_headers(self):
        # The page and review.json change under the owner; never serve a stale copy.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def reply(self, payload):
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        if not 0 < length <= 20_000_000:
            self.send_error(413)
            return
        data = json.loads(self.rfile.read(length))
        if self.path == "/annotate":
            self.annotate(data)
        elif self.path == "/review":
            self.review(data)
        else:
            self.send_error(404)

    def annotate(self, data):
        ident = int(data["id"])
        view = re.sub(r"[^a-z]", "", str(data.get("view", "")))[:10] or "view"
        png = base64.b64decode(str(data["png"]).split(",", 1)[1])
        OUT.mkdir(exist_ok=True)
        name = f"{ident:03d}-{view}-{time.strftime('%Y%m%d-%H%M%S')}"
        (OUT / f"{name}.png").write_bytes(png)
        note = f"{ident:02d} {data.get('label', '')} ({view})\n{data.get('note', '')}\n"
        (OUT / f"{name}.txt").write_text(note, encoding="utf-8")
        with LOCK:
            state = json.loads(REVIEW.read_text(encoding="utf-8")) if REVIEW.exists() else {}
            entry = state.setdefault(str(ident), {"status": "review", "reason": "", "notes": [], "approved_fp": None})
            entry["notes"].append({"date": time.strftime("%Y-%m-%d"), "view": view, "note": str(data.get("note", ""))})
            REVIEW.write_text(json.dumps(state, indent=1), encoding="utf-8")
        self.reply({"saved": f"annotations/{name}.png"})

    def review(self, data):
        ident = int(data["id"])
        action = data.get("action")
        with LOCK:
            state = json.loads(REVIEW.read_text(encoding="utf-8")) if REVIEW.exists() else {}
            entry = state.setdefault(str(ident), {"status": "review", "reason": "", "notes": [], "approved_fp": None})
            if action == "approve":
                entry.update(status="approved", reason=f"You approved it on {time.strftime('%Y-%m-%d')}.",
                             approved_fp=ab_review.fingerprint(ab_review.model_wav(ROOT, ident)))
            elif action == "reset":
                entry.update(status="review", reason="You reset it to review.", approved_fp=None)
            elif action == "needs_work":
                entry.update(status="needs_work", reason="You marked it as needing work.", approved_fp=None,
                             heard_fp=ab_review.fingerprint(ab_review.model_wav(ROOT, ident)))
                note = str(data.get("note", "")).strip()
                if note:
                    entry["notes"].append({"date": time.strftime("%Y-%m-%d"), "view": "row", "note": note})
            REVIEW.write_text(json.dumps(state, indent=1), encoding="utf-8")
            with LOG.open("a", encoding="utf-8") as log:
                log.write(json.dumps({"time": time.strftime("%Y-%m-%d %H:%M:%S"), "id": ident,
                                      "action": action, "note": str(data.get("note", ""))}) + "\n")
        self.reply(state)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 8777), Handler).serve_forever()
