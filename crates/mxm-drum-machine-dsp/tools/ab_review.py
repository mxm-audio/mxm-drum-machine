"""The owner's review state for the local drum A/B site: what is approved, and what changed since.

python ab_review.py refresh <site dir>
    Re-fingerprints every model's audio. An approved model whose sound changed becomes "changed",
    and so does a "needs work" model once its sound differs from what the owner marked, so the
    owner re-listens to exactly what moved. Run it after every regeneration.

python ab_review.py seed <site dir> <baseline site dir> <state.json>
    One-off: builds <site dir>/review.json from a hand-written state file and a baseline render
    (the site as the owner last heard it).

Needs numpy. <site dir>/review.json holds, per model id: status (approved, changed, review),
a reason, the owner's notes, and the fingerprint of the audio the owner approved. ab_serve.py
reads and writes it from the page. A fingerprint is scale-free: the 2 ms level envelope in dB and
32 log-spaced band levels, each against its own maximum, so the page's loudness matching never
reads as a change. See docs/drum-model-fitting.md.
"""

import json
import sys
import wave
from pathlib import Path

import numpy as np

class StaleFingerprint(Exception):
    """A stored fingerprint that a different version of fingerprint() wrote."""


TOLERANCE_DB = 0.5
# Stamped into every fingerprint. Raise it whenever fingerprint() changes what it measures, so a
# fingerprint written by an older copy of this file is recognised as unreadable rather than compared
# against a newer one. The server holds this module in memory from the moment it starts: after a
# change here it keeps writing the old format until it is restarted, and the two formats disagreeing
# once cost the owner eight approvals they had to be given back by hand.
FINGERPRINT_VERSION = 2


def read(path):
    with wave.open(str(path), "rb") as w:
        rate, width, frames = w.getframerate(), w.getsampwidth(), w.readframes(w.getnframes())
    raw = np.frombuffer(frames, dtype=np.uint8).reshape(-1, 3)
    ints = raw[:, 0].astype(np.int32) | (raw[:, 1].astype(np.int32) << 8) | (raw[:, 2].astype(np.int32) << 16)
    return np.where(ints >= 1 << 23, ints - (1 << 24), ints) / float(1 << 23), rate


def fingerprint(path):
    """Always over the same two seconds, zero-padded: a trim or fade change must not read as a
    different sound and cost the owner an approval."""
    x, rate = read(path)
    # From the hit itself, over a fixed two seconds: a change to the page's leading silence, trim or
    # fade must not read as a different sound and cost the owner an approval.
    onset = int(np.argmax(np.abs(x) >= 0.01 * (np.abs(x).max() or 1.0)))
    x = x[onset:]
    x = np.pad(x[: int(2.0 * rate)], (0, max(0, int(2.0 * rate) - len(x))))
    w = int(0.002 * rate)
    n = len(x) // w
    env = np.sqrt((x[: n * w].reshape(n, w) ** 2).mean(axis=1)) + 1e-12
    env_db = np.maximum(20 * np.log10(env / env.max()), -60.0)
    spec = np.abs(np.fft.rfft(x, n=1 << 16)) ** 2
    freqs = np.fft.rfftfreq(1 << 16, 1 / rate)
    edges = np.geomspace(30, 16000, 33)
    bands = np.array([spec[(freqs >= a) & (freqs < b)].sum() + 1e-20 for a, b in zip(edges, edges[1:])])
    band_db = np.maximum(10 * np.log10(bands / bands.max()), -60.0)
    return {"version": FINGERPRINT_VERSION,
            "env": np.round(env_db, 2).tolist(), "bands": np.round(band_db, 2).tolist()}


def same(a, b, tol=TOLERANCE_DB):
    if not a or not b:
        return False
    # Two formats are not comparable, and "not comparable" is not "changed": say so instead, and
    # never take an approval away over it.
    if a.get("version") != b.get("version"):
        raise StaleFingerprint(a.get("version"))
    n = min(len(a["env"]), len(b["env"]))
    env = np.abs(np.array(a["env"][:n]) - np.array(b["env"][:n]))
    # Only where either is audible: below -50 dB the tail is floor.
    loud = (np.array(a["env"][:n]) > -50) | (np.array(b["env"][:n]) > -50)
    bands = np.abs(np.array(a["bands"]) - np.array(b["bands"]))
    return float(env[loud].max(initial=0)) <= tol and float(bands.max()) <= tol


def model_wav(site, ident):
    return Path(site) / "audio" / f"{ident:03d}-model.wav"


def refresh(site):
    path = Path(site) / "review.json"
    state = json.loads(path.read_text(encoding="utf-8"))
    flipped, stale = [], []
    for key, entry in state.items():
        current = fingerprint(model_wav(site, int(key)))
        try:
            if entry["status"] == "approved" and not same(entry.get("approved_fp"), current):
                entry["status"] = "changed"
                entry["reason"] = "Changed since you approved it."
                flipped.append(int(key))
            elif entry["status"] == "needs_work" and entry.get("heard_fp") and not same(entry["heard_fp"], current):
                entry["status"] = "changed"
                entry["reason"] = "Changed after your note. Re-listen."
                flipped.append(int(key))
        except StaleFingerprint:
            # Written by an older copy of this file, so it cannot be compared. The owner's verdict
            # stands and the judgement is left to a human.
            stale.append(int(key))
    path.write_text(json.dumps(state, indent=1), encoding="utf-8")
    print("changed since approval or note:", flipped or "none")
    if stale:
        print("fingerprints from an older format, left alone:", stale)
        print("restart ab_serve.py so new clicks are written in this format.")


def seed(site, baseline, state_file):
    plan = json.loads(Path(state_file).read_text(encoding="utf-8"))
    state = {}
    for key, entry in plan.items():
        ident = int(key)
        current = fingerprint(model_wav(site, ident))
        heard = fingerprint(model_wav(baseline, ident))
        unchanged = same(heard, current)
        status, reason = entry["status"], entry.get("reason", "")
        if status == "approved" and not unchanged:
            status, reason = "changed", "Changed since you approved it."
        if status == "heard":
            status, reason = ("approved", "You heard it with no note, and it has not changed.") if unchanged \
                else ("changed", "Changed since you heard it.")
        if status == "heard_ahead":
            status, reason = ("review", "Not reviewed on its own yet.") if unchanged \
                else ("changed", "Changed since you listened ahead.")
        state[key] = {"status": status, "reason": reason, "notes": entry.get("notes", []),
                      "approved_fp": current if status == "approved" else None}
    Path(site, "review.json").write_text(json.dumps(state, indent=1), encoding="utf-8")
    counts = {}
    for e in state.values():
        counts[e["status"]] = counts.get(e["status"], 0) + 1
    print("seeded", len(state), counts)


if __name__ == "__main__":
    if sys.argv[1] == "refresh":
        refresh(sys.argv[2])
    elif sys.argv[1] == "seed":
        seed(sys.argv[2], sys.argv[3], sys.argv[4])
    elif sys.argv[1] == "diff":
        # diff <site> <baseline>: which models sound different between two renders.
        changed = [i for i in range(1, 95) if not same(fingerprint(model_wav(sys.argv[2], i)), fingerprint(model_wav(sys.argv[3], i)))]
        print(changed)
