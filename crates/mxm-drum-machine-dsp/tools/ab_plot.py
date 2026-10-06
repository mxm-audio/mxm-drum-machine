"""Plots for the local drum A/B page: model against original, four views per model.

python ab_plot.py <mapping.json> <path to src/model.rs> <site dir> [ids...]

Needs numpy and Pillow. Reads <site dir>/audio/NNN-model.wav and NNN-original.wav as written by the
`drum_machine_ab_page` example and writes <site dir>/images/NNN-{onset,cycles,whole,spectrogram}.png.
The grid is half a wavelength of each model's `reference_pitch_hz`, parsed from model.rs. The method
and how to read each view are docs/drum-model-fitting.md.

Each image is 1560 x 736, the largest that stays under the ~1.15 megapixel limit at which Claude's
image input starts downscaling, so both the owner and Claude read the same pixels.

Time zero is each file's own onset, the first sample within 40 dB of its peak. Every view starts
2 ms before it, so the step from silence into the hit is on the page; where a file has no samples
the strip is shaded rather than silently drawn as zero.
"""

import json
import re
import sys
import wave
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 1560, 736
LEFT, RIGHT, TOP = 70, 16, 34
MODEL = (31, 122, 140)
ORIGINAL = (192, 98, 43)
GRID = (222, 226, 231)
GRID_PERIOD = (190, 197, 205)
ONSET = (120, 128, 136)
NO_DATA = (238, 238, 238)
AXIS = (120, 128, 136)
TEXT = (30, 34, 40)
PRE_ROLL = 0.002


def font(size, bold=False):
    for name in (("segoeuib.ttf" if bold else "segoeui.ttf"), "arial.ttf"):
        try:
            return ImageFont.truetype(f"C:/Windows/Fonts/{name}", size)
        except OSError:
            pass
    return ImageFont.load_default()


def read_wav(path):
    with wave.open(str(path), "rb") as w:
        rate, width, frames = w.getframerate(), w.getsampwidth(), w.readframes(w.getnframes())
    if width == 3:
        raw = np.frombuffer(frames, dtype=np.uint8).reshape(-1, 3)
        ints = (raw[:, 0].astype(np.int32) | (raw[:, 1].astype(np.int32) << 8)
                | (raw[:, 2].astype(np.int32) << 16))
        ints = np.where(ints >= 1 << 23, ints - (1 << 24), ints)
        x = ints / float(1 << 23)
    else:
        x = np.frombuffer(frames, dtype=np.int16) / 32768.0
    peak = np.abs(x).max() or 1.0
    onset = int(np.argmax(np.abs(x) >= 0.01 * peak))
    return {"x": x, "rate": rate, "onset": onset}


def pitch_table(model_rs):
    text = Path(model_rs).read_text(encoding="utf-8")
    body = text[text.index("pub const fn reference_pitch_hz"):]
    body = body[:body.index("_ => None")]
    return {int(i): float(hz) for i, hz in re.findall(r"(\d+) => Some\(([\d.]+)\)", body)}


def to_x(area, t, t0, t1):
    return area[0] + (t - t0) / (t1 - t0) * (area[2] - area[0])


def trace(draw, sound, t0, t1, area, colour, scale, shade=True):
    """Draws a sound over [t0, t1) seconds of its own onset time into area."""
    x, rate, onset = sound["x"], sound["rate"], sound["onset"]
    left, top, right, bottom = area
    mid = (top + bottom) / 2
    half = (bottom - top) / 2 - 2
    start = onset + int(round(t0 * rate))
    end = onset + int(round(t1 * rate))
    seg = np.zeros(max(end - start, 1))
    lo, hi = max(start, 0), min(end, len(x))
    if hi > lo:
        seg[lo - start:hi - start] = x[lo:hi]
    if shade:
        # Before the file's first sample or after its last there is nothing to draw: say so.
        if start < 0:
            edge = min(to_x(area, (0 - onset) / rate, t0, t1), right - 1)
            if edge > left + 1:
                draw.rectangle([left + 1, top + 1, edge, bottom - 1], fill=NO_DATA)
        if end > len(x):
            edge = max(to_x(area, (len(x) - onset) / rate, t0, t1), left + 1)
            if edge < right - 1:
                draw.rectangle([edge, top + 1, right - 1, bottom - 1], fill=NO_DATA)
    columns = right - left
    if len(seg) / columns < 2:
        xs = left + np.arange(len(seg)) / max(len(seg) - 1, 1) * columns
        ys = mid - seg / scale * half
        draw.line(list(zip(xs.tolist(), ys.tolist())), fill=colour, width=2)
        return
    edges = np.linspace(0, len(seg), columns + 1).astype(int)
    for c in range(columns):
        chunk = seg[edges[c]:max(edges[c + 1], edges[c] + 1)]
        y_hi = mid - chunk.max() / scale * half
        y_lo = mid - chunk.min() / scale * half
        draw.line([(left + c, y_hi), (left + c, max(y_lo, y_hi + 1))], fill=colour, width=1)


def grid(draw, area, t0, t1, spacing, label_every, fonts, labels=True):
    """Lines every `spacing` from the onset onwards, a darker line at the onset itself."""
    k = 0
    while k * spacing < t1:
        t = k * spacing
        if t >= t0:
            x = to_x(area, t, t0, t1)
            colour = ONSET if k == 0 else (GRID_PERIOD if k % 2 == 0 else GRID)
            draw.line([(x, area[1]), (x, area[3])], fill=colour, width=1)
            if labels and k % label_every == 0 and k > 0:
                draw.text((x + 2, area[3] - 15), str(k), fill=AXIS, font=fonts["tiny"])
        k += 1


def frame(draw, area):
    draw.rectangle(area, outline=AXIS)


def midline(draw, area):
    mid = (area[1] + area[3]) / 2
    draw.line([(area[0], mid), (area[2], mid)], fill=GRID_PERIOD)


def time_axis(draw, area, t0, t1, fonts):
    span_ms = (t1 - t0) * 1000
    step = next(s for s in (0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500) if span_ms / s <= 16)
    tick = np.ceil(t0 * 1000 / step) * step
    while tick <= t1 * 1000 + 1e-9:
        x = to_x(area, tick / 1000, t0, t1)
        draw.line([(x, area[3]), (x, area[3] + 4)], fill=AXIS)
        draw.text((x - 12, area[3] + 4), f"{tick:g} ms", fill=AXIS, font=fonts["tiny"])
        tick += step


def envelope_db(sound, t1, pitch):
    # One period of the rest pitch (at least 2 ms), so a low drum's own cycles do not ripple it.
    x, rate, onset = sound["x"], sound["rate"], sound["onset"]
    window = int(rate * max(0.002, 1.0 / pitch if pitch else 0.002))
    seg = x[onset:onset + int(t1 * rate)]
    n = len(seg) // window
    if n == 0:
        return np.array([]), np.array([])
    rms = np.sqrt((seg[:n * window].reshape(n, window) ** 2).mean(axis=1))
    return (np.arange(n) + 0.5) * window / rate, rms


def sheet(title, view):
    img = Image.new("RGB", (WIDTH, HEIGHT), "white")
    draw = ImageDraw.Draw(img)
    return img, draw


def areas_for(rows):
    bottom = HEIGHT - 26
    row_h = (bottom - TOP) // rows
    return [(LEFT, TOP + i * row_h + 4, WIDTH - RIGHT, TOP + (i + 1) * row_h - 4) for i in range(rows)]


def waveform_view(entry, sounds, scale, t1, spacing, label_every, heading, note, path, fonts):
    """Model, original and both overlaid, from 2 ms before the onset to t1."""
    t0 = -PRE_ROLL
    model, original = sounds
    img = Image.new("RGB", (WIDTH, HEIGHT), "white")
    draw = ImageDraw.Draw(img)
    draw.text((LEFT, 6), heading, fill=TEXT, font=fonts["title"])
    names = ["Model", "Original", "Both"] if original else ["Model"]
    areas = areas_for(len(names))
    for name, area in zip(names, areas):
        draw.text((6, area[1] + 4), name, fill=TEXT, font=fonts["label"])
    trace(draw, model, t0, t1, areas[0], MODEL, scale)
    if original:
        trace(draw, original, t0, t1, areas[1], ORIGINAL, scale)
        trace(draw, original, t0, t1, areas[2], ORIGINAL, scale, shade=False)
        trace(draw, model, t0, t1, areas[2], MODEL, scale, shade=False)
    for area in areas:
        grid(draw, area, t0, t1, spacing, label_every, fonts)
        midline(draw, area)
        frame(draw, area)
    # Redraw the traces over the grid so neither hides a waveform.
    trace(draw, model, t0, t1, areas[0], MODEL, scale, shade=False)
    if original:
        trace(draw, original, t0, t1, areas[1], ORIGINAL, scale, shade=False)
        trace(draw, original, t0, t1, areas[2], ORIGINAL, scale, shade=False)
        trace(draw, model, t0, t1, areas[2], MODEL, scale, shade=False)
    time_axis(draw, areas[-1], t0, t1, fonts)
    draw.text((LEFT + 300, HEIGHT - 16), note, fill=AXIS, font=fonts["tiny"])
    img.save(path, optimize=True)


# Spectrogram: a magma-like level map, and a blue-white-red difference map.
MAGMA = np.array([(0, 0, 4), (40, 11, 84), (101, 21, 110), (159, 42, 99), (212, 72, 66),
                  (245, 125, 21), (250, 193, 39), (252, 255, 164)], dtype=float)
FLOOR_DB = -84.0
DIFF_DB = 24.0
F_LO, F_HI = 30.0, 20000.0
# Three analysis windows, each serving the band it resolves best: long for the low partials, short
# for the highs, so a kick's pitch and a hat's attack are both readable in one picture.
BANDS = ((0.0, 400.0, 0.043), (400.0, 3000.0, 0.0107), (3000.0, 1e9, 0.00267))


def colour_map(values, anchors):
    values = np.clip(values, 0.0, 1.0) * (len(anchors) - 1)
    lo = np.floor(values).astype(int).clip(0, len(anchors) - 2)
    frac = (values - lo)[..., None]
    return anchors[lo] * (1 - frac) + anchors[lo + 1] * frac


def row_frequencies(rows):
    return F_HI * (F_LO / F_HI) ** (np.arange(rows) / (rows - 1))


def spectrogram_db(sound, t0, t1, columns, freqs):
    """Level in dB (0 = this file's loudest cell) on a log-frequency, linear-time grid."""
    x, rate, onset = sound["x"], sound["rate"], sound["onset"]
    centres = onset + np.round((t0 + (np.arange(columns) + 0.5) / columns * (t1 - t0)) * rate).astype(int)
    out = np.full((len(freqs), columns), -200.0)
    for lo, hi, seconds in BANDS:
        rows = (freqs >= lo) & (freqs < hi) & (freqs < rate / 2)
        if not rows.any():
            continue
        length = max(int(seconds * rate) // 2 * 2, 16)
        window = np.hanning(length)
        pad = np.concatenate([np.zeros(length), x, np.zeros(length)])
        index = centres[:, None] + np.arange(length)[None, :] - length // 2 + length
        index = index.clip(0, len(pad) - 1)
        frames = pad[index] * window
        n_fft = 1 << int(np.ceil(np.log2(length * 4)))
        mag = np.abs(np.fft.rfft(frames, n=n_fft, axis=1)) / (window.sum() / 2)
        bins = np.fft.rfftfreq(n_fft, 1 / rate)
        # Linear interpolation of each column's magnitude at the band's row frequencies.
        picked = np.array([np.interp(freqs[rows], bins, m) for m in mag]).T
        out[rows] = 20 * np.log10(np.maximum(picked, 1e-12))
    # A three-row maximum, so a window sidelobe null between two steady partials does not draw a
    # black line that reads as a real notch.
    out = np.maximum(out, np.maximum(np.roll(out, 1, axis=0), np.roll(out, -1, axis=0)))
    return out - out.max()


def spectrogram_view(entry, pitch, model, original, t_end, path, fonts, title):
    t0 = -PRE_ROLL
    img = Image.new("RGB", (WIDTH, HEIGHT), "white")
    draw = ImageDraw.Draw(img)
    draw.text((LEFT, 6), title + "  ·  SPECTROGRAM", fill=TEXT, font=fonts["title"])
    names = ["Model", "Original", "Model − original"] if original else ["Model"]
    areas = areas_for(len(names))
    left, top, right, bottom = areas[0]
    columns, rows = right - left - 1, bottom - top - 1
    freqs = row_frequencies(rows)
    maps = [spectrogram_db(model, t0, t_end, columns, freqs)]
    if original:
        maps.append(spectrogram_db(original, t0, t_end, columns, freqs))
    for area, level in zip(areas, maps):
        rgb = colour_map((level - FLOOR_DB) / -FLOOR_DB, MAGMA)
        img.paste(Image.fromarray(rgb.astype(np.uint8)), (area[0] + 1, area[1] + 1))
    if original:
        a, b = np.maximum(maps[0], FLOOR_DB), np.maximum(maps[1], FLOOR_DB)
        diff = np.clip((a - b) / DIFF_DB, -1, 1)
        blue, white, red = np.array([33, 102, 172.]), np.array([247, 247, 247.]), np.array([178, 24, 43.])
        rgb = np.where(diff[..., None] < 0,
                       white + (blue - white) * (-diff[..., None]),
                       white + (red - white) * diff[..., None])
        silent = (maps[0] < FLOOR_DB + 6) & (maps[1] < FLOOR_DB + 6)
        rgb[silent] = (255, 255, 255)
        area = areas[2]
        img.paste(Image.fromarray(rgb.astype(np.uint8)), (area[0] + 1, area[1] + 1))
    draw = ImageDraw.Draw(img)
    ticks = [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]
    for name, area in zip(names, areas):
        frame(draw, area)
        draw.text((6, area[1] + 4), name.replace(" − ", "\n− "), fill=TEXT, font=fonts["label"])
        h = area[3] - area[1] - 1
        for f in ticks:
            y = area[1] + 1 + np.log(F_HI / f) / np.log(F_HI / F_LO) * (h - 1)
            if y < area[1] + 40:
                continue
            draw.line([(area[0] - 4, y), (area[0], y)], fill=AXIS)
            draw.text((area[0] - 36, y - 7), f"{f // 1000}k" if f >= 1000 else str(f), fill=AXIS, font=fonts["tiny"])
        if pitch and F_LO < pitch < F_HI:
            y = area[1] + 1 + np.log(F_HI / pitch) / np.log(F_HI / F_LO) * (h - 1)
            for x0 in (area[0] + 1, area[2] - 12):
                draw.line([(x0, y), (x0 + 11, y)], fill=(80, 220, 235), width=3)
        x = to_x(area, 0.0, t0, t_end)
        draw.line([(x, area[1]), (x, area[1] + 6)], fill=(80, 220, 235), width=2)
    time_axis(draw, areas[-1], t0, t_end, fonts)
    note = ("level: each file against its own loudest cell, black = −84 dB"
            + ("; difference: red = model louder, blue = model quieter, full colour = 24 dB" if original else "")
            + ("; cyan ticks = rest pitch" if pitch else "") + "  ·  windows 43 / 11 / 2.7 ms below 400 Hz / to 3 kHz / above")
    draw.text((LEFT, HEIGHT - 16), note, fill=AXIS, font=fonts["tiny"])
    img.save(path, optimize=True)


def render(entry, pitch, model, original, out, fonts):
    sounds = (model, original)
    scale = max(np.abs(model["x"]).max(), np.abs(original["x"]).max() if original else 0) or 1.0
    title = f"{entry['id']:02d} {entry['label']}  ·  {entry['machine']} {entry['voice']}"
    ident = f"{entry['id']:03d}"
    tail = "time 0 = each file's onset (first sample within 40 dB of its peak); grey = no samples in the file"

    # Onset: the attack itself, the first half-waves or 5 ms.
    if pitch:
        half = 0.5 / pitch
        t1 = max(6 * half, 0.005)
        note = f"grid: half wavelength of the {pitch:g} Hz rest pitch ({half * 1000:.3f} ms)  ·  {tail}"
        waveform_view(entry, sounds, scale, t1, half, 1 if t1 / half < 20 else 4,
                      title + "  ·  ONSET", note, out / f"{ident}-onset.png", fonts)
    else:
        note = f"no pitch (unpitched voice): grid every 0.5 ms  ·  {tail}"
        waveform_view(entry, sounds, scale, 0.010, 0.0005, 2,
                      title + "  ·  ONSET", note, out / f"{ident}-onset.png", fonts)

    # Cycles: sixteen periods of the rest pitch on the half-wave grid, or 40 ms.
    if pitch:
        half = 0.5 / pitch
        note = f"grid: half wavelength of the {pitch:g} Hz rest pitch ({half * 1000:.3f} ms); numbers count half-waves  ·  {tail}"
        waveform_view(entry, sounds, scale, 32 * half, half, 4,
                      title + "  ·  CYCLES", note, out / f"{ident}-cycles.png", fonts)
    else:
        note = f"no pitch (unpitched voice): grid every 1 ms  ·  {tail}"
        waveform_view(entry, sounds, scale, 0.040, 0.001, 5,
                      title + "  ·  CYCLES", note, out / f"{ident}-cycles.png", fonts)

    # Whole hit: both waveforms and their envelopes in dB, to -60 dB.
    def length(sound):
        x = sound["x"]
        peak = np.abs(x).max() or 1.0
        idx = np.nonzero(np.abs(x) >= peak * 10 ** (-60 / 20))[0]
        return ((idx[-1] if len(idx) else sound["onset"]) - sound["onset"]) / sound["rate"]
    t_end = max(length(model), length(original) if original else 0)
    t_end = min(max(t_end * 1.05, 0.02), 3.0)
    t0 = -PRE_ROLL
    img = Image.new("RGB", (WIDTH, HEIGHT), "white")
    draw = ImageDraw.Draw(img)
    draw.text((LEFT, 6), title + "  ·  WHOLE HIT", fill=TEXT, font=fonts["title"])
    names = ["Model", "Original", "Level dB"] if original else ["Model", "Level dB"]
    areas = areas_for(len(names))
    if pitch:
        per_px = (WIDTH - LEFT - RIGHT) / (t_end / (0.5 / pitch))
        every = int(np.ceil(10 / per_px))
        spacing = every * 0.5 / pitch
        note = (f"grid: every {every} half-wave{'s' if every > 1 else ''} of the {pitch:g} Hz rest pitch"
                if every > 1 else f"grid: half wavelength of the {pitch:g} Hz rest pitch")
    else:
        spacing = next(s for s in (0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1) if t_end / s <= 150)
        note = f"no pitch (unpitched voice): grid every {spacing * 1000:g} ms"
    note += f"  ·  {tail}"
    for name, area in zip(names, areas):
        draw.text((6, area[1] + 4), name, fill=TEXT, font=fonts["label"])
    trace(draw, model, t0, t_end, areas[0], MODEL, scale)
    if original:
        trace(draw, original, t0, t_end, areas[1], ORIGINAL, scale)
    for name, area in zip(names, areas):
        grid(draw, area, t0, t_end, spacing, 10, fonts, labels=False)
        if name != "Level dB":
            midline(draw, area)
        frame(draw, area)
    trace(draw, model, t0, t_end, areas[0], MODEL, scale, shade=False)
    if original:
        trace(draw, original, t0, t_end, areas[1], ORIGINAL, scale, shade=False)
    env = areas[-1]
    for db in (-20, -40):
        y = env[1] + (-db / 60) * (env[3] - env[1])
        draw.line([(env[0], y), (env[2], y)], fill=GRID_PERIOD)
        draw.text((env[0] - 34, y - 8), f"{db}", fill=AXIS, font=fonts["tiny"])
    for sound, colour in [(model, MODEL)] + ([(original, ORIGINAL)] if original else []):
        times, rms = envelope_db(sound, t_end, pitch)
        if len(times) < 2:
            continue
        db = 20 * np.log10(np.maximum(rms / scale, 1e-6))
        ys = env[1] + np.clip(-db / 60, 0, 1) * (env[3] - env[1])
        xs = [to_x(env, t, t0, t_end) for t in times]
        draw.line(list(zip(xs, ys.tolist())), fill=colour, width=2)
    time_axis(draw, areas[-1], t0, t_end, fonts)
    draw.text((LEFT + 300, HEIGHT - 16), note, fill=AXIS, font=fonts["tiny"])
    img.save(out / f"{ident}-whole.png", optimize=True)

    spectrogram_view(entry, pitch, model, original, t_end, out / f"{ident}-spectrogram.png", fonts, title)


def main():
    mapping, model_rs, site = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    entries = json.loads(Path(mapping).read_text(encoding="utf-8"))
    pitches = pitch_table(model_rs)
    out = site / "images"
    out.mkdir(exist_ok=True)
    for stale in out.glob("*-attack.png"):
        stale.unlink()
    fonts = {"title": font(17, True), "label": font(14, True), "tiny": font(11)}
    only = {int(a) for a in sys.argv[4:]}
    for entry in entries:
        if only and entry["id"] not in only:
            continue
        model = read_wav(site / "audio" / f"{entry['id']:03d}-model.wav")
        original_path = site / "audio" / f"{entry['id']:03d}-original.wav"
        original = read_wav(original_path) if original_path.exists() else None
        render(entry, pitches.get(entry["id"]), model, original, out, fonts)
    print(f"plotted {len(only) or len(entries)} models into {out}")


if __name__ == "__main__":
    main()
