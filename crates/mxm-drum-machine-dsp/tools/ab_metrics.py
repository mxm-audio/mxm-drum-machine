"""Model-versus-original measurements for the local drum A/B page, to rank what differs most.

python ab_metrics.py <mapping.json> <site dir> [ids...]

Needs numpy. One row per model with a recording; the columns are defined in
docs/drum-model-fitting.md. A guide to where to look, never a verdict: judge the plots.
"""

import json
import sys
import wave
from pathlib import Path

import numpy as np


def read_wav(path):
    with wave.open(str(path), "rb") as w:
        rate, width, frames = w.getframerate(), w.getsampwidth(), w.readframes(w.getnframes())
    raw = np.frombuffer(frames, dtype=np.uint8).reshape(-1, 3)
    ints = raw[:, 0].astype(np.int32) | (raw[:, 1].astype(np.int32) << 8) | (raw[:, 2].astype(np.int32) << 16)
    ints = np.where(ints >= 1 << 23, ints - (1 << 24), ints)
    return ints / float(1 << 23), rate


def features(x, rate):
    peak = np.abs(x).max()
    onset = int(np.argmax(np.abs(x) >= 0.01 * peak))
    y = x[onset:] / peak
    f = {}
    f["jump"] = float(np.abs(y[:2]).max())  # first two samples, relative to the peak
    f["peak_ms"] = float(np.argmax(np.abs(y)) / rate * 1000)
    win = max(int(rate * 0.002), 1)
    n = len(y) // win
    rms = np.sqrt((y[: n * win].reshape(n, win) ** 2).mean(axis=1)) + 1e-12
    db = 20 * np.log10(rms / rms.max())
    top = int(np.argmax(db))

    def fall(level):
        below = np.nonzero(db[top:] < level)[0]
        return float((top + below[0]) * win / rate * 1000) if len(below) else float(n * win / rate * 1000)

    f["t20"] = fall(-20)
    f["t40"] = fall(-40)

    def spec(a, b):
        seg = y[int(a * rate): int(b * rate)]
        if len(seg) < 64:
            return None, None
        spec = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n=1 << 16)) ** 2
        freqs = np.fft.rfftfreq(1 << 16, 1 / rate)
        keep = freqs < 20000
        spec, freqs = spec[keep], freqs[keep]
        centroid = float((spec * freqs).sum() / spec.sum())
        # 1/3-octave band levels, 40 Hz .. 16 kHz, for a timbre distance
        bands = []
        lo = 40.0
        while lo < 16000:
            hi = lo * 2 ** (1 / 3)
            sel = (freqs >= lo) & (freqs < hi)
            bands.append(spec[sel].sum() + 1e-20)
            lo = hi
        bands = 10 * np.log10(np.array(bands))
        return centroid, bands

    f["c_early"], f["b_early"] = spec(0, 0.02)
    f["c_body"], f["b_body"] = spec(0.02, min(0.3, max(0.04, f["t40"] / 1000)))
    # dominant frequency over the late ring: from the peak's -12 dB point to -40 dB
    a = fall(-12) / 1000
    b = max(a + 0.03, f["t40"] / 1000)
    seg = y[int(a * rate): int(b * rate)]
    if len(seg) > 256:
        spec = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n=1 << 18))
        freqs = np.fft.rfftfreq(1 << 18, 1 / rate)
        keep = (freqs > 25) & (freqs < 12000)
        f["late_hz"] = float(freqs[keep][np.argmax(spec[keep])])
    else:
        f["late_hz"] = None
    # early instantaneous frequency from zero crossings in the first 3 half-cycles
    zc = np.nonzero(np.diff(np.signbit(y[: int(rate * 0.1)])))[0]
    if len(zc) >= 4:
        f["early_hz"] = float(rate / (2 * np.mean(np.diff(zc[:4]))))
    else:
        f["early_hz"] = None
    return f


def main():
    mapping = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    site = Path(sys.argv[2])
    only = {int(a) for a in sys.argv[3:]}
    rows = []
    for e in mapping:
        if not e["reference"] or (only and e["id"] not in only):
            continue
        m = features(*read_wav(site / "audio" / f"{e['id']:03d}-model.wav"))
        o = features(*read_wav(site / "audio" / f"{e['id']:03d}-original.wav"))
        timbre_e = float(np.abs((m["b_early"] - m["b_early"].max()) - (o["b_early"] - o["b_early"].max())).clip(0, 30).mean()) if m["b_early"] is not None and o["b_early"] is not None else float("nan")
        timbre_b = float(np.abs((m["b_body"] - m["b_body"].max()) - (o["b_body"] - o["b_body"].max())).clip(0, 30).mean()) if m["b_body"] is not None and o["b_body"] is not None else float("nan")
        cents = lambda p, q: 1200 * np.log2(p / q) if p and q else float("nan")
        rows.append((e["id"], e["label"], m, o, timbre_e, timbre_b, cents(m["late_hz"], o["late_hz"]), cents(m["early_hz"], o["early_hz"])))
    print(f"{'id':>3} {'label':28} {'jump m/o':>11} {'pk ms m/o':>11} {'t20 m/o':>13} {'t40 m/o':>13} {'cent early m/o':>15} {'cent body m/o':>15} {'late Hz m/o':>13} {'dc':>6} {'early Hz m/o':>13} {'tE':>5} {'tB':>5}")
    for i, label, m, o, te, tb, dc, de in rows:
        fmt = lambda v: "-" if v is None else f"{v:.0f}"
        print(f"{i:>3} {label[:28]:28} {m['jump']:5.2f}/{o['jump']:<5.2f} {m['peak_ms']:5.1f}/{o['peak_ms']:<5.1f} {m['t20']:6.0f}/{o['t20']:<6.0f} {m['t40']:6.0f}/{o['t40']:<6.0f} {fmt(m['c_early']):>7}/{fmt(o['c_early']):<7} {fmt(m['c_body']):>7}/{fmt(o['c_body']):<7} {fmt(m['late_hz']):>6}/{fmt(o['late_hz']):<6} {dc:6.0f} {fmt(m['early_hz']):>6}/{fmt(o['early_hz']):<6} {te:5.1f} {tb:5.1f}")


main()
