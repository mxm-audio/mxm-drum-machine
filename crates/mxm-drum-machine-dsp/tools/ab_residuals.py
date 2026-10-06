"""Listening-proxy residuals for the local drum A/B site: flags what an owner would hear as different.

python ab_residuals.py <mapping.json> <site dir> [ids...]

Needs numpy. Reads <site dir>/audio/NNN-model.wav and NNN-original.wav as written by the
`drum_machine_ab_page` example (loudness-matched). Every check compares the model with the original
the way a listener does, and prints only the checks past their threshold, so an empty line means
nothing to report. Run it over every model before the owner listens; see docs/drum-model-fitting.md.

Checks, and what a flag means to the ear:
- peaks   the strongest rendered partial near a pitched voice's note (0.6-4x), early and in the
          body, in cents: the rendered sound, not the configured value.
- early   the pitch of the first cycles, low-passed at three times the note, in cents.
- attack  each file's level at the other's loudest millisecond (5 ms-smoothed envelope).
- level   window RMS in dB (0-3, 3-10, 10-30, 30-100 ms), relative to each file's loudest.
- tail    how abruptly the tail ends: ms from -20 dB to -40 dB, model against original.
- period  the envelope's autocorrelation peak between 1 and 20 ms (a periodic burst pattern).
- harm    audible-harmonic deficit of a pitched voice: harmonics 2-8 over the hit's loud body,
          A-weighted so the ear's low-frequency insensitivity counts, floored at -40 dB below the
          loudest, counting only true lines after the first 20 ms; the dB the original has above
          the model, summed. "More harmonics" or "sounds
          lower" to the owner. The approved 808 set reads 9 dB or less, except its kick (44): a
          kick's few low harmonics are over-counted by the weighting.
- rolloff where 85 % of the energy lies below, early (0-20 ms), in cents. Advisory only: the
          approved 808 hats read up to +-400 c.

Calibrated against owner verdicts, the approved 808 set as negatives. Not a check: the perceived
pitch of a metallic or noisy voice. Spectral cross-correlation did not separate the approved 808
voices from the CR-8000 voices the owner heard as lower; fit a metal bank's oscillator frequencies
from the line spectrum of a long cymbal or open-hat tail instead (docs/drum-model-fitting.md).
"""

import json
import sys
import wave
from pathlib import Path

import numpy as np

# Each threshold is the smallest that leaves the owner-approved 808 set (IDs 1-16) unflagged.
THRESHOLDS = {"harm": 12.0, "rolloff": 450.0, "peaks_early": 90.0, "peaks_body": 35.0, "early": 150.0,
              "attack_db": 3.0, "level": 6.5, "tail": 1.8, "period": 0.12}


def read(path):
    with wave.open(str(path), "rb") as w:
        rate, width, frames = w.getframerate(), w.getsampwidth(), w.readframes(w.getnframes())
    if width == 3:
        raw = np.frombuffer(frames, dtype=np.uint8).reshape(-1, 3)
        ints = raw[:, 0].astype(np.int32) | (raw[:, 1].astype(np.int32) << 8) | (raw[:, 2].astype(np.int32) << 16)
        x = np.where(ints >= 1 << 23, ints - (1 << 24), ints) / float(1 << 23)
    else:
        x = np.frombuffer(frames, dtype=np.int16) / 32768.0
    peak = np.abs(x).max() or 1.0
    onset = int(np.argmax(np.abs(x) >= 0.01 * peak))
    return x[onset:], rate


def partials(x, rate, a, b, lo, hi, count=4):
    seg = x[int(a * rate): int(b * rate)]
    if len(seg) < 256:
        return []
    n = 1 << 17
    mag = 20 * np.log10(np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n=n)) + 1e-12)
    freqs = np.fft.rfftfreq(n, 1 / rate)
    mag -= mag.max()
    sel = np.nonzero((freqs > lo) & (freqs < hi))[0]
    found = [i for i in sel[1:-1] if mag[i] > mag[i - 1] and mag[i] > mag[i + 1] and mag[i] > -24]
    found = sorted(found, key=lambda i: -mag[i])[:count]
    return sorted(float(freqs[i]) for i in found)


def early_hz(x, rate):
    """The pitch of the first real cycles: crossings whose half-wave between them reaches 5 % of the
    peak, skipping the onset's own crossing, so a wiggle or a click does not count."""
    seg = x[: int(rate * 0.05)]
    zc = np.nonzero(np.diff(np.signbit(seg)))[0]
    floor = 0.05 * np.abs(seg).max()
    real = [a for a, b in zip(zc, zc[1:]) if np.abs(seg[a:b + 1]).max() >= floor]
    real = real[1:]
    return rate / (2 * np.mean(np.diff(real[:5]))) if len(real) >= 5 else None


def envelope(x, rate, ms=1.0):
    w = max(int(rate * ms / 1000), 1)
    n = len(x) // w
    return np.sqrt((x[: n * w].reshape(n, w) ** 2).mean(axis=1)) + 1e-12


def fall_ms(env, level_db, ms=1.0):
    db = 20 * np.log10(env / env.max())
    top = int(np.argmax(db))
    below = np.nonzero(db[top:] < level_db)[0]
    return (top + below[0]) * ms if len(below) else len(db) * ms


def periodicity(x, rate):
    seg = x[: int(0.15 * rate)]
    env = envelope(seg, rate, 0.25)
    env = env - env.mean()
    ac = np.correlate(env, env, "full")[len(env) - 1:]
    ac /= ac[0] + 1e-12
    lo, hi = 4, 80  # 1-20 ms at 0.25 ms per window
    return float(ac[lo:hi].max()) if len(ac) > hi else 0.0


def cents(a, b):
    return 1200 * np.log2(a / b)


def low_passed(x, rate, cutoff, seconds=0.06):
    seg = x[: int(seconds * rate)]
    spec = np.fft.rfft(seg)
    spec[np.fft.rfftfreq(len(seg), 1 / rate) > cutoff] = 0
    return np.fft.irfft(spec, n=len(seg))


def a_weight_db(f):
    """IEC 61672 A-weighting in dB."""
    f2 = np.asarray(f, dtype=float) ** 2
    ra = (12194.0 ** 2 * f2 * f2) / ((f2 + 20.6 ** 2) * np.sqrt((f2 + 107.7 ** 2) * (f2 + 737.9 ** 2)) * (f2 + 12194.0 ** 2))
    return 20 * np.log10(ra) + 2.0


def loud_body_s(x, rate):
    env = envelope(x, rate, 2.0)
    db = 20 * np.log10(env / env.max())
    top = int(np.argmax(db))
    below = np.nonzero(db[top:] < -20)[0]
    return max((top + below[0]) * 0.002 if len(below) else len(db) * 0.002, 0.03)


def harmonic_deficit(m, mr, o, orr, f0, count=8, floor=-40.0):
    """Counts only harmonics that are lines in the original (10 dB over their neighbourhood's
    median), over the loud body after the first 20 ms, where a swept voice has settled: a noise band
    or an inharmonic partial that happens to sit on k·f0 is not a harmonic."""
    def profile(x, rate):
        seg = x[int(0.02 * rate): int(max(loud_body_s(x, rate), 0.05) * rate)]
        n = 1 << 17
        mag = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n=n))
        freqs = np.fft.rfftfreq(n, 1 / rate)
        level, line = [], []
        for k in range(1, count + 1):
            sel = (freqs > k * f0 * 0.97) & (freqs < k * f0 * 1.03)
            around = (freqs > k * f0 * 0.85) & (freqs < k * f0 * 1.15)
            if sel.any() and k * f0 < rate / 2:
                peak = mag[sel].max()
                level.append(20 * np.log10(peak + 1e-12))
                line.append(20 * np.log10(peak / (np.median(mag[around]) + 1e-12)) >= 10.0)
            else:
                level.append(-200.0)
                line.append(False)
        out = np.array(level) + a_weight_db(f0 * np.arange(1, count + 1))
        return out - out.max(), np.array(line)
    (hm, _), (ho, lines) = profile(m, mr), profile(o, orr)
    cm, co = np.maximum(hm[1:], floor), np.maximum(ho[1:], floor)
    counted = (ho[1:] > floor) & lines[1:]
    return float(np.clip(co - cm, 0, None)[counted].sum())


def rolloff_hz(x, rate, a, b, fraction=0.85):
    seg = x[int(a * rate): int(b * rate)]
    n = 1 << 15
    p = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n=n)) ** 2
    freqs = np.fft.rfftfreq(n, 1 / rate)
    keep = (freqs > 40) & (freqs < 18000)
    c = np.cumsum(p[keep])
    return float(freqs[keep][np.searchsorted(c, fraction * c[-1])])


def check(ident, pitch, model, original):
    (m, mr), (o, orr) = model, original
    flags = []
    if pitch:
        d = harmonic_deficit(m, mr, o, orr, pitch)
        if d > THRESHOLDS["harm"]:
            flags.append(f"original has more audible harmonics: {d:.0f} dB")
        # The strongest partial near the note, early and in the body.
        for a, b in ((0.0, 0.03), (0.03, 0.2)):
            pm = partials(m, mr, a, b, 0.6 * pitch, 4 * pitch, count=1)
            po = partials(o, orr, a, b, 0.6 * pitch, 4 * pitch, count=1)
            limit = THRESHOLDS["peaks_early"] if a == 0.0 else THRESHOLDS["peaks_body"]
            if pm and po and abs(cents(pm[0], po[0])) > limit:
                flags.append(f"strongest partial {a*1000:.0f}-{b*1000:.0f}ms {pm[0]:.0f}/{po[0]:.0f} Hz ({cents(pm[0], po[0]):+.0f} c)")
        # The first cycles' pitch, below three times the note, so clicks and noise do not count.
        em = early_hz(low_passed(m, mr, 3 * pitch), mr)
        eo = early_hz(low_passed(o, orr, 3 * pitch), orr)
        # Only a stable reading counts: the same estimate one crossing later must agree within 100 c.
        stable = all(
            e and e2 and abs(cents(e, e2)) < 100
            for e, e2 in ((em, early_hz(low_passed(m, mr, 3 * pitch)[int(0.5 * mr / pitch):], mr)),
                          (eo, early_hz(low_passed(o, orr, 3 * pitch)[int(0.5 * orr / pitch):], orr)))
        )
        if stable and em and eo and abs(cents(em, eo)) > THRESHOLDS["early"]:
            flags.append(f"early pitch {em:.0f}/{eo:.0f} Hz ({cents(em, eo):+.0f} c)")
    # The attack's shape: each file's level at the other's loudest millisecond, on a 1 ms envelope
    # smoothed over 5 ms. Comparing peak times alone flagged every flat noise plateau.
    em_, eo_ = envelope(m, mr), envelope(o, orr)
    smooth = lambda e: np.convolve(e[:100], np.ones(5) / 5, mode="same")
    sm, so = smooth(em_), smooth(eo_)
    pk_m, pk_o = int(np.argmax(sm)), int(np.argmax(so))
    drop_m = 20 * np.log10(sm.max() / sm[pk_o])
    drop_o = 20 * np.log10(so.max() / so[pk_m])
    if max(drop_m, drop_o) > THRESHOLDS["attack_db"]:
        flags.append(f"attack shape: loudest ms {pk_m}/{pk_o}, level at the other's peak -{drop_m:.1f}/-{drop_o:.1f} dB")
    lv = []
    for a, b in ((0, 0.003), (0.003, 0.01), (0.01, 0.03), (0.03, 0.1)):
        rm = np.sqrt(np.mean(m[int(a * mr): int(b * mr)] ** 2) + 1e-20)
        ro = np.sqrt(np.mean(o[int(a * orr): int(b * orr)] ** 2) + 1e-20)
        lv.append((a, b, 20 * np.log10(rm / np.abs(m).max()), 20 * np.log10(ro / np.abs(o).max())))
    bad = [f"{a*1000:.0f}-{b*1000:.0f}ms {x:.1f}/{y:.1f}" for a, b, x, y in lv if abs(x - y) > THRESHOLDS["level"] and max(x, y) > -40]
    if bad:
        flags.append("level dB model/orig: " + ", ".join(bad))
    tm = fall_ms(em_, -40) - fall_ms(em_, -20)
    to = fall_ms(eo_, -40) - fall_ms(eo_, -20)
    if max(tm, to) > 5 and (tm / max(to, 1e-3) > THRESHOLDS["tail"] or to / max(tm, 1e-3) > THRESHOLDS["tail"]):
        flags.append(f"tail -20->-40 dB {tm:.0f}/{to:.0f} ms ({'model ends more abruptly' if tm < to else 'original ends more abruptly'})")
    pm_, po_ = periodicity(m, mr), periodicity(o, orr)
    if pm_ - po_ > THRESHOLDS["period"]:
        flags.append(f"envelope periodicity {pm_:.2f}/{po_:.2f}")
    ro = cents(rolloff_hz(m, mr, 0, 0.02), rolloff_hz(o, orr, 0, 0.02))
    if abs(ro) > THRESHOLDS["rolloff"]:
        flags.append(f"early roll-off {ro:+.0f} c ({'model darker' if ro < 0 else 'model brighter'})")
    return flags


def pitch_table(model_rs):
    import re
    text = Path(model_rs).read_text(encoding="utf-8")
    body = text[text.index("pub const fn reference_pitch_hz"):]
    body = body[: body.index("_ => None")]
    return {int(i): float(hz) for i, hz in re.findall(r"(\d+) => Some\(([\d.]+)\)", body)}


def main():
    mapping = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    site = Path(sys.argv[2])
    model_rs = Path(__file__).resolve().parent.parent / "src" / "model.rs"
    pitches = pitch_table(model_rs)
    only = {int(a) for a in sys.argv[3:]}
    for e in mapping:
        if not e["reference"] or (only and e["id"] not in only):
            continue
        flags = check(e["id"], pitches.get(e["id"]),
                      read(site / "audio" / f"{e['id']:03d}-model.wav"),
                      read(site / "audio" / f"{e['id']:03d}-original.wav"))
        print(f"{e['id']:3d} {e['label'][:28]:28} " + ("; ".join(flags) if flags else "-"))


if __name__ == "__main__":
    main()
