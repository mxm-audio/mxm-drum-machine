"""How a voice's resonance moves through its own decay, model against original.

python ab_resonance.py <site dir> <id> [<id> ...]

One table per model id, six windows from the onset out to half a second. Per window and per file:

    median      the A-weighted median frequency — where half the audible energy sits below
    res         the resonance peak, the spectrum smoothed over +-8 % first so a noise voice's own
                lines cannot decide it
    Q           that peak's -3 dB width as res / bandwidth
    level       the window's RMS against the file's peak, in dB

A single fixed band-pass lands the average and misses both ends of a hit, which reads to a
listener as too high at the strike and too low in the tail from the same model. This is the tool
that tells the two apart, and the width from the centre: a broad band reads as a vaguer, lower
pitch than a narrow one with the same centre. Read it with "A hardware resonance is narrow at the
strike and drifts as it decays" in docs/drum-model-fitting.md.

Needs numpy. Reads the site's rendered pairs, so run it after a regeneration.
"""

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ab_review  # noqa: E402

WINDOWS = ((0, 0.010), (0.010, 0.030), (0.030, 0.060), (0.060, 0.120), (0.120, 0.250), (0.250, 0.500))
# Wide enough to carry a hat's whole resonance, narrow enough to leave out rumble and the
# ultrasonic shelf.
BAND = (300.0, 18_000.0)
SMOOTH = 0.08
FFT = 1 << 16


def a_weighting_db(f):
    f = np.maximum(f, 1.0)
    f2 = f * f
    ra = (12194.0**2 * f2**2) / (
        (f2 + 20.6**2) * np.sqrt((f2 + 107.7**2) * (f2 + 737.9**2)) * (f2 + 12194.0**2)
    )
    return 20.0 * np.log10(ra) + 2.0


def onset(x):
    """Both files are measured from their own onset, or every window compares different sounds."""
    return int(np.argmax(np.abs(x) >= 0.01 * np.abs(x).max()))


def window_rows(path):
    x, rate = ab_review.read(path)
    x = x[onset(x):]
    peak = np.abs(x).max()
    rows = []
    for start, end in WINDOWS:
        segment = x[int(start * rate):int(end * rate)]
        if len(segment) < 64 or not np.any(segment):
            rows.append(None)
            continue
        spectrum = np.abs(np.fft.rfft(segment * np.hanning(len(segment)), n=FFT)) ** 2
        freqs = np.fft.rfftfreq(FFT, 1.0 / rate)
        keep = (freqs > BAND[0]) & (freqs < BAND[1])
        spectrum, freqs = spectrum[keep], freqs[keep]

        audible = spectrum * 10 ** (a_weighting_db(freqs) / 10.0)
        median = freqs[np.searchsorted(np.cumsum(audible) / audible.sum(), 0.5)]

        db = 10.0 * np.log10(spectrum + 1e-20)
        smoothed = np.empty_like(db)
        for i, f in enumerate(freqs):
            lo = np.searchsorted(freqs, f * (1.0 - SMOOTH))
            hi = np.searchsorted(freqs, f * (1.0 + SMOOTH))
            smoothed[i] = db[lo:hi].mean()
        peak_bin = int(np.argmax(smoothed))
        top = smoothed[peak_bin]
        low = next((freqs[i] for i in range(peak_bin, 0, -1) if smoothed[i] < top - 3.0), freqs[0])
        high = next((freqs[i] for i in range(peak_bin, len(freqs)) if smoothed[i] < top - 3.0), freqs[-1])

        level = 20.0 * np.log10(np.sqrt(np.mean(segment.astype(np.float64) ** 2)) / peak + 1e-12)
        rows.append((median, freqs[peak_bin], freqs[peak_bin] / max(high - low, 1.0), level))
    return rows


def report(site, ident):
    model = window_rows(site / "audio" / f"{ident:03d}-model.wav")
    original = window_rows(site / "audio" / f"{ident:03d}-original.wav")
    print(f"== {ident:03d}" + " " * 12 + "model                        original                     difference")
    print(" " * 14 + "median   res    Q  level    median   res    Q  level    median   res     Q")
    for (start, end), m, o in zip(WINDOWS, model, original):
        label = f"{start * 1000:6.0f}-{end * 1000:3.0f} ms"
        if m is None or o is None:
            print(f"{label}  (silent)")
            continue
        print(
            f"{label} {m[0]:7.0f}{m[1]:7.0f}{m[2]:5.1f}{m[3]:7.1f}  "
            f"{o[0]:8.0f}{o[1]:7.0f}{o[2]:5.1f}{o[3]:7.1f}  "
            f"{m[0] - o[0]:+8.0f}{m[1] - o[1]:+7.0f}{m[2] - o[2]:+6.1f}"
        )


if __name__ == "__main__":
    site = Path(sys.argv[1]).resolve()
    for argument in sys.argv[2:]:
        report(site, int(argument))
