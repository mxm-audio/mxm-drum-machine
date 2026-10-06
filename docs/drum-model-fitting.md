# Fitting drum models to hardware recordings

How to compare every model of a drum machine with a recording of the hardware it copies, see where
they differ, and fit the models until they match. Written after the `mxm-drum-machine` pass of
2026-09-19 and extended through the review rounds that followed, so the next drum machine starts
from the method rather than rediscovering it. Section 6 is what the circuits turned out to do;
section 7 is how to move a model the last hundred hertz once it is already close, which is where
most of the work went. Its last part, *Acoustic drum models in particular*, is what fine-tuning
`mxm-model-drums`' physical models against acoustic recordings added in 2026-09.

This is a howto and owns no rules. What a model may and may not do is
[`crates/mxm-drum-machine-dsp/AGENTS.md`](../crates/mxm-drum-machine-dsp/AGENTS.md), and for the
acoustic models [`crates/mxm-model-drums-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-model-drums/blob/main/crates/mxm-model-drums-dsp/AGENTS.md). What may cross
from a recording into the repository is the root [`AGENTS.md`](../AGENTS.md) *Research citations*,
with the full text in mxm-kit's
[`docs/collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md#research-boundary),
*Research boundary*.
Where this page and an owner disagree, the owner wins.

**When to do it:** once a machine's circuits render and pass their own tests, and before owner
listening. A model built from service notes alone can pass every synthetic test and still start at
the wrong point in its cycle, sit a fifth off, or be an octave too dark. Only a recording shows that.

## 1. What you need

**One recording per model, at a known setting that the model's reference matches.** Zero deviation
is the model's reference setting, so the recording has to be of the same knob positions. Examples:
- The TR-808 set was recorded at knob position 5 (files named like `BD T5 D5`), so the 808 models'
  references are the middle of each knob.
- The 909 crash and ride came as *Full* / *Max* decay files, so those models calibrate at full decay.

Record the setting in the mapping; a model fitted to the wrong setting is fitted wrong.

**A mapping file.** It is a JSON array in catalogue order, one object per model:

| Field | What it holds |
|---|---|
| `id` | The model ID |
| `label` | The product label |
| `group` | The selector group, which becomes a section of the page |
| `machine` | The hardware, named as a reference |
| `voice` | The hardware voice |
| `reference` | The recording's absolute path, or `null` where none exists |
| `source` | Which set it came from |
| `note` | Setting, doubts, mislabels |

Build it with an agent that reads the acquisition record
(`research:instruments/analogue-drum-machines.md` §12.4), opens candidate files and measures them.
It must write every doubt into `note` rather than guess. The first pass found:
- a sample set whose "tom/conga" names were likely mislabelled;
- a reference an octave from its model;
- 17 voices with no recording at all.

**The mapping stays local**, next to the site (for example `<site dir>/ab-mapping.json`). It holds
absolute paths into private folders, which are tool state in both repositories.

## 2. The comparison site

The tools are all in `crates/mxm-drum-machine-dsp`. Python needs numpy and Pillow.

```bash
# Render every model, decode every recording, write <site>/audio and <site>/index.html
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_ab_page -- <mapping.json> <site>
# Four figures per model into <site>/images (ids optional)
python crates/mxm-drum-machine-dsp/tools/ab_plot.py <mapping.json> crates/mxm-drum-machine-dsp/src/model.rs <site> [ids...]
# One row of numbers per model (ids optional)
python crates/mxm-drum-machine-dsp/tools/ab_metrics.py <mapping.json> <site> [ids...]
# Listening-proxy flags: only what an owner would hear as different (ids optional)
python crates/mxm-drum-machine-dsp/tools/ab_residuals.py <mapping.json> <site> [ids...]
# Serve it on http://127.0.0.1:8777/, receive drawings and keep the review state
python crates/mxm-drum-machine-dsp/tools/ab_serve.py <site>
# After every regeneration: an approved drum whose sound changed goes back to "changed"
python crates/mxm-drum-machine-dsp/tools/ab_review.py refresh <site>
# What the listener hears on every row, before anyone listens: <site>/listen/index.md, and a report
# per row beside it (NNN.md, NNN.json). Run in the mxm-tools repository, whose crate the listener is
cargo run -p mxm-listening --release --bin listen -- site <site>
```

**Run the listener first.** `listen site` reads every row's model against its original with
mxm-tools' `crates/mxm-listening` — each difference with its window, band, both values, its size in audibility
thresholds and the owner's words for it, most audible first, and the regions where the two differ
and no reading says why. Read `listen/index.md` before looking at a picture and before the owner
hears the page; the Python tools stay the owner's views. For one pair, `listen compare <original>
<render>`; against every round robin of a layer, `listen compare --set <manifest> --group <layer>
<render>`, which counts a difference only when it holds across the takes.

**Keep the owner's review state on the page.** Over several passes the owner could not remember
which drums were already approved, and was asked to re-listen from memory. Each row now shows one
of four states:
- **Approved.**
- **Changed, re-listen:** it changed after the owner last heard it.
- **To review.**
- **Needs work.**

Each row also shows the owner's own earlier notes, with **Approve** and **Needs work** buttons. The
default filter shows only what needs the owner's ear.

`<site>/review.json` holds the state:
- An approval stores a scale-free fingerprint of the audio approved: the 2 ms level envelope and
  32 band levels.
- `ab_review.py refresh` flips an approved drum back to "changed" only when its sound actually
  moved, so a loudness-matching change or a rebuild never costs an approval.
- `ab_review.py seed` builds a first state from a render of what the owner last heard. Seed
  approvals only from the owner's explicit words. The first seed inferred approvals from "heard it
  with no note", and the owner rejected them.
- `<site>/review-log.jsonl` records every Approve, Needs work and Reset with its time.
- **Restart the server after changing `ab_review.py`, and raise `FINGERPRINT_VERSION`.** The
  server imports the module once at startup and then keeps writing fingerprints in the format it
  started with. A fingerprint change made mid-review left the running server writing the old format
  for a whole round; the next `refresh` compared the two formats, and eight approvals the owner had
  just given flipped to "changed". Every fingerprint now carries its version, and `refresh` reports
  a version it cannot read as stale and leaves the owner's verdict alone instead of taking it away.
  When approvals do flip, check the audio before believing it: recompute with the old code from
  git, and only a real difference in the band levels is a real change of sound.
- Keep the page's element classes distinct from its status names. A status called `review` on a
  badge, beside a cell found by `.review`, broke every redraw after the first click, and the
  owner's clicks then saved without the page showing them.
- Never hide a row under the owner's cursor. Filtering applies on load and on a filter change only,
  so an approved row stays in place, dimmed.

**What the generator does.** Its decode, trim and body loudness live in
mxm-tools' `crates/mxm-listening/src/prep.rs`, which the page calls, so the page and every listening report read
a sound the same way; the steps below are those definitions.
1. It renders each model the way every calibration measures it: zero deviation, velocity 0.82,
   48 kHz, alone in slot 1, panned hard to one side so the channel is the mono circuit, eight
   seconds.
2. It trims both files from 5 ms before the onset to the last sample within 70 dB of the peak, plus
   50 ms.
3. It **loudness-matches** the recording to the model by perceived loudness over the hit's loud
   body. That is BS.1770 K-weighted power in 5 ms windows over the first second, counting only
   windows within 20 dB of the loudest.
   - The first version matched plain energy down to −40 dB. Recordings with a sub-audio trigger
     leak, hum, a noise floor or a file-end artefact then counted that junk as body, and played about
     4.5 dB louder than their models.
   - Plain energy also lets a model whose energy sits where the ear is less sensitive sound quieter
     at "equal" level.
   - The owner heard "the original is louder" twice before the cause was found, and later said
     "lower" may have meant quieter. When the owner hears a level difference, or "lower", suspect
     the matching before the model, and don't retune a pitch unless the rendered partials or the
     early pitch measure off.
4. It gives both one common gain that puts the louder peak at −1 dBFS.
   - **When several rows play one recording** (a fine-tuning page's settings side by side, §7), match
     the model to the recording instead, and give each recording one gain on every row that plays it.
     The snare page matched each recording to its own row's model: the same file then played up to
     0.7 dB apart from row to row, and the owner heard the original "sound different in these two
     tests". A reference must not move while the model does.
5. **Neither end may step.** Each model renders after 5 ms of silence, and both files fade in over
   1 ms and out over 5 ms. Trimming to the onset used to leave a model's first sample at −39 dB,
   and the owner heard that step as a click on a kick with no noise to mask it — twice, before the
   cause was found. A fingerprint is taken from the onset, so such a page-side change never costs
   an approval.

The page has one section per selector group and a row per model: **Model**, **Original** and
**Both**, which plays the model then the original. Under each row are the four figures, each with
a drawing tool.

**It is a comparison, not a blind test.** The owner rejected a guess-which-is-which A/B: they need
to know what they are hearing to say what is wrong. They also asked for just the original and the
new model, with no "before" version cluttering the row.

**Drawing is how the owner points.** They draw on a figure, type a note and press **Send to
Claude**. The server saves `<site>/annotations/NNN-view-time.png`, the figure with the strokes, and
a `.txt` with the note. Read both.
- A page opened from disk cannot POST, and browser automation refuses `file://`, so always serve
  it.
- Rebuild the page after every DSP change you want the owner to hear, and make the owner reload.
  The player decodes each file once and keeps the buffer for the life of the page, so a page left
  open across a rebuild keeps playing the build it loaded — the owner approved a drum while hearing
  the build before it, and the approval had to be taken back. The generator writes the build stamp
  to `<site>/build.txt`, the page re-reads it every twenty seconds, and a page older than the build
  says so in red across the header.

**Play through Web Audio, not an `<audio>` element.** The element clicked at the end of a quiet,
decaying drum, in a different place each play. The owner spent most of a review chasing that click
in the model, and the model never had it: a decoded buffer, scheduled with a 5 ms gain ramp at stop,
removes the decoder and the element's own stop from the path. It also keeps one stream open, which
matters on a Bluetooth headset, whose codec clicks when a stream starts or stops.

**When the owner reports a click, measure the file before touching the model.** Sample steps,
curvature, and the high-passed envelope's prominence against its neighbourhood, in absolute dBFS.
If the file is clean at the level the owner listens at, the click is in the playback chain: test it
with variants — silence appended, silence both sides, a long fade — rather than changing the DSP.

**Every rebuild versions its URLs.** Audio and images carry a `?v=<build time>` query, because a
browser that cached the last render plays yesterday's drum and the owner hears no difference after
a fix. The server also sends `Cache-Control: no-store`.

**Never publish the site.** The recordings are third-party, and two of the sets forbid
redistribution. It is not an artifact and not a commit.

## 3. Reading the four views

Every figure is **1560 × 736**: the largest size that stays under the ~1.15-megapixel point where
Claude's image input starts downscaling, so the owner and Claude see the same pixels. Each has three
rows (Model, Original, and both overlaid or their difference) and shares one time axis:

- **Time zero is each file's own onset:** its first sample within 40 dB of its peak.
- **Every view starts 2 ms before it**, so the step out of silence is visible.
- **Grey means the file has no samples there.** A missing sample is never drawn as a zero.

**Never assume two files share a pre-onset offset.** The first plotter assumed every file started
5 ms before its hit. The recordings started two or three samples before theirs, so the plotter cut
the first 5 ms off every original, and every model looked as if its attack was wrong. The owner
caught it. Find each file's onset.

**The grid is half a wavelength of the model's rest pitch** (`reference_pitch_hz`), so a line falls
on every zero crossing of a model at its rest pitch.
- Against the grid, a pitch sweep shows as crossings arriving early and converging.
- A wrong rest pitch shows as a steady drift.
- The original's crossings against the same grid give its sweep and rest pitch by eye.
- An unpitched voice gets a fixed 0.5 ms or 1 ms grid.

| View | Span | Best for |
|---|---|---|
| **Onset** | Six half-waves or 5 ms (10 ms unpitched) | First-sample step, click or trigger spike, which way the first half-cycle goes, attack rise |
| **Cycles** | 32 half-waves (40 ms unpitched) | Sweep, rest pitch, wave shape (sine against triangle against squarish), upper partials |
| **Whole hit** | The hit to −60 dB | Envelope shape, holds, two-stage decays, T20/T40, tails, clap bursts |
| **Spectrogram** | The same as Whole hit | Missing partials, noise colour, per-band decay, a click band, and where the model is too bright or too dark |

**The spectrogram's rows** are the model, the original, and **model − original**: red where the
model is louder, blue where it is quieter, full colour at 24 dB.
- Each level map is relative to its own loudest cell, and black is −84 dB.
- The frequency axis is logarithmic from 30 Hz to 20 kHz. Cyan ticks mark the rest pitch.
- Three analysis windows each serve the band they resolve: 43 ms below 400 Hz, 11 ms to 3 kHz and
  2.7 ms above. A kick's pitch and a hat's attack are both readable, and the seams at 400 Hz and
  3 kHz are window changes, not content.
- A three-row maximum hides window-sidelobe nulls between steady partials, which otherwise draw
  black lines that look like notches.

## 4. The numbers

The listener's reports (§2) carry these numbers and more, each against its audibility threshold;
the table below is the older ranking, kept for the owner's page and for parity. Two readings differ
by name there: `late Hz` is the listener's *strongest mode* in the ring, and the rest pitch by the
rule of §6 (the lowest mode within 12 dB of the strongest) is its own reading, which names the modes
it chose between when two sounds disagree.

`ab_metrics.py` ranks where to look. It never decides.

| Column | Meaning | What a gap usually means |
|---|---|---|
| `jump` | Largest of the first two samples, relative to the peak | Near 1 against a small original: the model starts at full amplitude |
| `pk ms` | Time of the absolute peak after onset | A missing attack, or a slow VCA or filter build-up |
| `t20`, `t40` | ms from the loudest 2 ms RMS window until the RMS falls 20 / 40 dB | Wrong decay, or a missing hold or second stage |
| `cent early`, `cent body` | Spectral centroid over 0–20 ms, and over 20 ms to min(300 ms, t40) | Wrong balance of body and noise, or a filter too dark or too bright |
| `late Hz`, `dc` | The strongest partial over the ring (−12 to −40 dB), and the cents from model to original | Rest pitch |
| `early Hz` | From the first four zero crossings | Sweep start |
| `tE`, `tB` | Mean absolute ⅓-octave band difference in dB, each spectrum normalised to its loudest band, early and body | Timbre distance: lower is closer, and under about 8 dB is close |

`ab_resonance.py <site> <id>` answers a different question: not what the voice averages, but how
its resonance moves through its own decay. Six windows from the onset to half a second, each with
the A-weighted median frequency, the resonance peak (the spectrum smoothed over ±8 % first, so a
noise voice's own lines cannot decide it), that peak's −3 dB Q, and the window's level. Use it on
every hat, cymbal and noise voice before touching a filter, and on any note about pitch: a model
that is too high at the strike and too low in the tail measures as a contradiction until you see
the two ends separately.

**Traps.**
- `late Hz` reads 25–50 Hz on a short original with hum under it.
- A centroid over a window that starts after a short hit has decayed measures the noise floor.
- Sample-pack recordings may have been through a processing chain.

The table is for ranking; the pictures and the owner's ears are the judgement.

## 5. The fitting loop

**Split the work by circuit-family file, one agent per family, in parallel.**
- Each agent works in its own git worktree made from the same commit, with its own copy of the
  tools, whose Cargo path dependencies point at *its* worktree, and its own site directory.
- Split by file, so the branches touch disjoint code. A family that shares one renderer goes to one
  agent.
- One iteration (rebuild, render all, plot, measure) takes about 13 s.
- Give every agent the same written brief:
  - the owner's words;
  - the files it owns;
  - the contracts from the DSP `AGENTS.md`, restated;
  - the metric definitions, and its family's baseline rows;
  - your own observations from the figures;
  - what to report.
- **Make every agent measure with the repository's tools, not its own.** Two agents that each fit
  their own metric produced reports that could not be compared and conclusions that contradicted
  each other and mine — one reported a voice's strike as the broader of the pair where the same
  files measured the other way round, because the two width definitions disagree (§7). Name the
  tool and the columns in the brief, and ask for the numbers in that form.
- **Tell each agent which of its voices the owner has approved**, and re-send that list whenever it
  changes. An approval can land while an agent works.

**Each model, in this order:**
1. **The onset:** first sample, click, direction of the first half-cycle, rise time.
2. **The sweep and the rest pitch.**
3. **The decay:** shape, stages, holds.
4. **The spectral balance:** body against noise, filter colour, missing partials.

Stop when the gains are marginal. A noise realisation will never match sample for sample.

**Shared tables are not hand-merged; they are re-measured after the merge.**
- `REFERENCE_OUTPUT_TRIM` (the −1 dBFS plane):
  `cargo test -p mxm-drum-machine-dsp --lib -- --ignored print_the_reference_output_trims --nocapture`
  prints the whole table corrected onto the plane.
- `reference_pitch_hz` (the rest pitch):
  `cargo test -p mxm-drum-machine-dsp --test musical_pitch -- --ignored print_the_measured_reference_pitches --nocapture`.
- An agent may correct its own rows locally so its tests pass.

**Before the owner listens, run `listen site` and `ab_residuals.py` over every model and clear or
explain every difference and flag.** The owner's ear is for what the numbers cannot catch, not for what a run of the checks would
have found. In the first review the owner found by ear a 10 % partial error, a missing early sweep,
a pitch 900 cents off and a late attack. All four were already in the metrics table, unread.

What each flag in that tool means:
- **Harmonics.** Its A-weighted audible-harmonic deficit is the owner's "the original has more
  harmonics" and "the model sounds lower", on pitched voices. It is calibrated so the approved 808
  set reads 9 dB or less.
- **Metallic and noisy voices.** No whole-hit spectral comparison catches the owner's "lower
  pitched" on these: cross-correlation, centroid and roll-off all left the approved 808 hats as far
  off as the complained-of CR-8000 ones. What does catch it is the windowed resonance profile —
  `ab_resonance.py`, and §7 *Noise voices in particular*. Run that on every hat, cymbal, clap and
  tambourine before the owner listens; `ab_residuals.py` will not flag what they hear on these.

**Then:**
1. Merge.
2. Re-measure both tables.
3. Run the crate's tests, and every downstream test that pins a render.
4. Regenerate the site and hand it to the owner.

A test that pins a model's audio, such as a compatibility fixture, is re-captured deliberately,
with the reason recorded beside the fixture. It is never loosened.

## 6. What the recordings taught

These were found on `mxm-drum-machine`, but they are properties of how the circuits work, and the
next machine's models will get the same things wrong.

**Nothing starts at its peak.** The most common error by far was an envelope that begins at full
amplitude, so the first sample is the loudest (`jump` ≈ 1). Real voices rise over 0.1–15 ms:
- the VCA's attack;
- the trigger pulse's RC shape;
- a filter ringing up.

Give each voice its recorded rise. The exceptions are real and per voice: claves and rims do jump.

**A struck resonator's starting phase is audible.**
- An impulse into a pole pair starts at zero, like a sine. A widened, RC-shaped trigger pulse of
  about 0.4 ms rises the way the recordings do.
- A coupled-form (phasor) resonator lets a strike set the phase and amplitude directly, and keeps
  its amplitude through retuning. The 808 bodies use one.
- Some drums start closer to a cosine: at their peak, with the first half-cycle going the other way.
- Fit the phase from the onset view.
- A trigger pulse that leaks to the output is the click on a kick's first millisecond
  (`research:instruments/analogue-drum-machines.md` §2.1). The 808 kick also holds a pitch lift
  for the trigger's 5 ms and strikes again as it releases, which makes its deep, fast first
  half-cycle.

**Sweeps belong to the reference when the recording shows one.**
- Several kicks and toms sweep into their rest pitch.
- Where the original's early crossings run ahead of the grid, the sweep is native at zero
  deviation, and Pitch Env at −1 removes it.
- Measure the sweep's start frequency and time constant from the onset and cycles views.

**The Chamberlin state-variable filter has a frequency ceiling.**
- `f = 2 sin(π fc / fs)` clamped below 1 capped every band-pass centre near 7.3 kHz at 48 kHz, and
  made every hat and cymbal an octave too dark.
- Use a topology-preserving (TPT/ZDF) SVF (Zavalishin, *The Art of VA Filter Design*), which
  reaches fs/2.
- Noise colours are usually a high-pass plus a band-pass, not one narrow band-pass.

**A hardware resonance is narrow at the strike and drifts as it decays.** No model got this until
the owner's last review round, and it accounts for five separate notes that read as contradictions
until they were measured:
- The recordings' hats and cymbals open as a tight whistle — Q 9–14 over the first 10 ms — and
  broaden to Q 6–8 for the body. One voice opened the other way, broad at Q 2.8 and narrowing to
  Q 5.1, so fit the direction per voice rather than assuming.
- The centre moves too, usually upward through the tail (7635 → 8759 Hz over half a second on one
  hat), because the resonator's lower modes die first and the surviving energy sits higher.
- A single fixed band-pass therefore lands the average and misses both ends: it reads as too high
  at the strike and too low in the tail, from the same model, and that is exactly what the owner
  reported on two voices in one sitting.
- Drive centre and Q from the voice's own envelope and recompute the SVF coefficients on a block
  boundary. Every 32–64 samples is ample for a drift this slow, and it stays allocation-free and
  sample-rate independent.
- Measure it as a profile, never as one number: A-weighted median frequency, the resonance peak
  smoothed over ±8 % so the noise's own lines do not decide it, its −3 dB Q, and the window level,
  over about six windows from 0–10 ms out to 250–500 ms.

**"Higher" and "lower" are about width as often as centre.** When the owner said "model has lower
resonance" on a voice whose centre already matched to the hertz, the fault was that our Q 5.4 band
had a skirt reaching 200 Hz below the recording's Q 14.4 whistle. A broad band reads as a vaguer,
lower pitch than a narrow one at the same centre. Check the width before moving the centre, and in
a noise voice treat the perceived pitch as the resonance's centre *and* its width together.

**Snares lead with the body.**
- The recordings' first 20 ms are tone, and the wire noise arrives gated.
- Gate the noise with the noise-decay time, not the main decay. Otherwise the analysis patch that
  measures the rest pitch moves the pitch.

**Envelopes have stages.**
- Cymbals and cowbells fall in two stages.
- The CR-78, TR-66 and FR-2L voices hold, then release.
- Claps are separated bursts whose tail opens on the last burst.
- Read all of these off the whole-hit dB curve, not a T60.

**Fit the metal bank's frequencies from the tails.**
- A six-square bank at its service nominals made the 808 closed hat audibly periodic. The six
  squares' edges cluster at a rate set by their frequencies, and the wrong frequencies gave the
  wrong clusters.
- The fix is this unit's own frequencies: fit each square's odd-harmonic line series to a long FFT
  of a cymbal or open-hat tail.
- Judge it objectively, by the autocorrelation and the coefficient of variation of the band-passed
  envelope, model against original.
- Band-limited edges (PolyBLEP) are right to have, but they did not remove the periodicity.

**Measure oscillator pairs from the recording.** A cowbell's pair can sit 5–8 % from the service
note's nominal frequencies, and the listener hears the interval. Per-oscillator duty matters too:
40–46 % against a nominal 48 % changed the cowbells' even harmonics visibly.

**A current-controlled oscillator sweeps linearly in hertz.** The 909 kick's sweep fitted
`f(t) = f_rest · (1 + k·e^(−t/τ))`, which places the first eighteen zero crossings within 0.15 ms.
An exponential sweep in semitones does not fit.

**Reset oscillators are held during the trigger.** The 909 kick and toms hold their oscillators
through the trigger (about 1.9 ms), then release from the waveform's centre, rising. That is the
original's small first bump, where a free-running start is a step.

**A VCA with a threshold ends a hit in finite time.** When the envelope capacitor falls below the
amplifier's conduction point, the decay steepens and stops (909 kick, DR-55 hat and snare). An
exponential tail to the idle threshold is wrong there, and it is audible as a long faint tail.

**The spectrogram finds hidden partials.** The 909 toms carry a quiet tone at 0.62–0.69× and a
short one at 1.66–1.83× their main pitch. None of this was in the research page, and it is most of
their character. Only the dominant oscillator follows Decay.

**A long trigger strikes twice.** The DR-55's trigger of about 8–10 ms re-excites the kick and
snare on its falling edge.

**The reference-level saturator must not be working.** A final bound that bends the reference hit
is an implicit waveshaper (`crates/mxm-drum-machine-dsp/AGENTS.md`). The supporting renderer's did,
slightly, on every voice. Check at reference that no bound is active, and leave the drive to
Character.

**Tell the machine from the recording chain by phase.**
- **The machine:** ticks every half-period of a 606 tom's ring, which stop when the ring decays,
  are locked to the drum's phase. They are the transistor VCA switching, and they belong in the
  model.
- **The chain:** a 50/100/150 Hz series under a CR-78 snare's tail is mains hum, and so is a flat
  floor at −80 dB. Neither belongs.
- **The research:** can be wrong about the unit too. All three 808 toms carry noise, where the page
  says only the low tom does. The 808 snare rings at 168/332 Hz in this set and 172/338 Hz in
  another, which is unit variation, not the wrong voice.

**The rest-pitch rule decides the note, not the label.** The note is the lowest partial within
12 dB of the strongest. A cowbell whose lower oscillator sits 15 dB under the upper one plays at
the upper one's pitch. A label naming a nominal frequency ("Classic 340 snare") is identity, and
does not have to match.

**Recordings disagree with the circuit, and sometimes with themselves.** The first pass found:
- a snare with almost no wire noise, where the snappy control was probably at minimum;
- a file with 6 ms of low-level pre-roll;
- a file truncated mid-tail;
- a partial that looks like bleed from another voice;
- a large sub-audio thump that is either the gate leaking into the audio or processing.

Record each doubt in the mapping and in the code comment. Reproduce a wart only when it is the
machine's (*A copy is warts and all*, in mxm-kit's
[`docs/collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md#the-goal-and-how-much-licence-a-copy-has)),
and never fit a model to a recording chain's artefact.

## 7. Fine-tuning: moving a model the last hundred hertz

Section 5 is the loop and section 6 is what the circuits do. This is what the last rounds cost,
where a model is already close, every change is worth tens of hertz, and the only judge is an owner
whose listening is the scarcest thing in the process.

### Reading the owner's words

Every phrase below cost at least one round before it was understood. The measurement is what the
phrase turned out to mean on this machine, with this owner. The listener speaks these phrases
(mxm-tools' `crates/mxm-listening/data/vocabulary.tsv` is this table's machine-readable form, so a new phrase
goes into both), and what the owner said about which sounds is kept as its golden cases
(mxm-tools' `crates/mxm-listening/data/golden.tsv`, scored by `listen golden`). Before acting on a note, run
`listen explain "<phrase>" <reference> <candidate>`: it lists what the phrase has meant and those
readings' differences in the two sounds.

| They say | It usually means | Measure |
|---|---|---|
| "the original has more harmonics" | An A-weighted audible-harmonic deficit on a pitched voice, or a missing octave band on a noise one | `ab_residuals.py` harmonics flag; energy per octave against the window's total |
| "sounds lower / higher" (pitched voice) | Rest pitch, or a missing early sweep | `late Hz`, `early Hz` |
| "sounds lower / higher" (noise voice) | The resonance's centre **and** its width, in one window — often the strike alone | `ab_resonance.py` |
| "the model has higher resonance" on a voice measuring low | The first 10 ms, which is the opposite of the body | The 0–10 ms row, separately |
| "the original has a more narrow bandpass" | Tail Q, and usually a tail dying too early underneath it | The Q and level columns together |
| "the original changes resonance over time" | The drift: narrow at the strike, centre moving through the decay | The profile across all six windows |
| "the original is louder" | The page's loudness match counting junk — or a real window-level gap | K-weighted body match first, then the level column |
| "lower" | Sometimes loudness, not pitch. They said so themselves | Rule out level before retuning anything |
| "LF modulation on the harmonics" | Amplitude modulation at a few tens of hertz | Per-band Hilbert envelope; sideband spacing |
| "there is a grrrr sound" | Excess modulation depth in the roughness range | Depth across 15–300 Hz, per band |
| "a click" | The playback chain, until the file proves otherwise | The file's own samples; then the page, the browser, the headphones |
| "more low band noise" | A whole octave the voice does not produce | Energy per octave, all windows |

**Measure a profile, never a single number.** `ab_resonance.py` reports six windows from the onset
to half a second, both files measured from their own onsets. A model that is too high at the strike
and too low in the tail averages out to correct, and reads as a contradiction until the windows are
separated.
- Ignore a window where the file is below about −45 dB of its own peak: it measures the floor. One
  tambourine's 250–500 ms window put the model and the recording 2.4 kHz apart, and neither was
  audible there.

**Two honest width measures disagree, so fit the curve rather than the number.** The −3 dB crossing
outward from the peak and the equivalent bandwidth (peak over the area under the smoothed curve)
gave Q 5.6 and Q 2.1 for the same file, and ranked model against recording in opposite directions.
A shoulder on the curve breaks the crossing measure; a long skirt breaks the area measure. An agent
and I each trusted our own and reached opposite conclusions about the same hat. Fit the whole
smoothed curve, and quote both widths whenever a width is the point.

**When the peak and the centroid disagree, the listener follows the peak.** A tambourine's band
centroid improved from 78 Hz mean error to 65 while its smoothed peak went from within 30–104 Hz to
209–289 Hz low — in the exact direction the owner had already complained about. The centroid is
pulled by content far from the resonance, and a definite pitch lives at the peak.

**A sweeping band reads broader than it is.** A resonance that moves inside a measurement window
measures wider than it ever is at one instant, so a voice whose strike must stay narrow needs a
*slow* drift curve, not a fast one — a curve of 1.3 against 0.3 on two hats of the same machine.

**A metal voice's first 60 ms is a lottery, so never fit it from one render.** The noise and metal
banks free-run, and what the strike window contains depends on where they are when the trigger
lands. Rendering one cymbal at seven trigger phases put its smoothed strike peak anywhere between
6361 and 8207 Hz, and its 10–30 ms peak between 6719 and 7237 — a spread far larger than the errors
being chased. Two people measuring the same model from their own renders will disagree and both be
right.
- Render several trigger phases and fit the mean, and quote the spread beside it.
- The cure for a wandering early window is a band tight enough to pin the peak whatever the lines
  do, not more noise. Raising one cymbal's mix made every window worse; taking its band from Q 4 to
  Q 7 brought the strike from −250 ±612 Hz to −43 ±137.

**When a smoothed spectrum has two humps, the peak is a knife-edge and the median is the stable
column.** On one hat a strike lift of 12 % put the peak at 8035 Hz and 14 % put it at 9716: the two
humps swap which one wins, and no amount of tuning lands the peak in between. Read the median there,
and say which column you fitted.

### Calibrating the ear

"Audible" in the listener's reports means the literature's thresholds until the owner has sat a few
calibration sessions; after that it means the owner's. A session takes the owner's own sound and
asks, trial after trial, which of three is different, one of them changed by a known amount — a
pitch shift, a shorter decay, a brighter top octave — and walks the amount down until the owner
guesses right about 70 % of the time.

```bash
# In the mxm-tools repository, whose crate the listener is
cargo run -p mxm-listening --release --bin listen -- session <a sound the owner knows> [--operators pitch,decay,band-3200-6400] [--minutes 6]
cargo run -p mxm-listening --release --bin listen -- calibrate
```

- Run it on a sound the owner is tuning against, on the playback the owner judges with (headphones),
  and let the page record it.
- A session reports its check trials. A session with wrong checks measured attention, not the ear:
  throw it away.
- When a difference is too small to hear, guess: the forced choice is how the test finds the limit,
  and a wrong guess makes the next trial easier. Every answer counts, across sessions.
- The results live in `.listening/`, which never enters the repository, and the listener prefers them
  over the literature from then on. Rerun `calibrate` after every session.

### Noise voices in particular

Hats, cymbals, claps, tambourines and snare wires took more rounds than every pitched voice
together. They fail in ways a spectrum of the whole hit does not show, and the owner's words for
those failures are about pitch even though nothing in the voice oscillates.

**A noise voice has a perceived pitch, and it is the resonance.** The owner's ruling, in their
words: "higher and lower means the perceived tone or pitch", and "in noise sources that is most
likely the resonance of the overtones". So a complaint about pitch on a hat is a statement about the
resonance's centre *and* its width, in some window. Answer it with `ab_resonance.py`, never by
retuning an oscillator the voice does not have.

**A width number on noise is meaningless without its smoothing span.** On one tambourine, ±2 %
smoothing read Q ≈ 20 for both model and recording, ±4 % read ≈ 12 for both, and ±8 % read 5.4
against 14.4. The narrow spans are measuring the lines of one noise realisation, not the filter.
Quote the span with the Q, compare only like with like, and lean on centre and level — two
independent measures agreed on those to within 50 Hz and 0.1 dB while disagreeing on Q by a factor
of two.

**Fit the band structure before the resonance.** A noise colour is usually a high-pass plus a
band-pass, sometimes two-stage, and a missing band is invisible to a resonance measurement. Compare
energy per octave, each window against its own total: one cymbal was 13–16 dB short in the
1.6–3.2 kHz octave in *every* window while everything above it matched within 1.6 dB. That is a band
the voice never produces, and no filter move fixes it.

**A broadband floor under a resonant voice widens its strike.** One cymbal's white-noise floor was
what made its strike too broad and lifted 0.8–2.5 kHz; low-passing that floor at 2.5 kHz and halving
it fixed both at once. Check the floor before widening or narrowing the band that sits on it.

**Cyclical faults have their own measurements, and each has its own cure.**
- **Periodicity** — the bank's square edges clustering, which is heard as a buzz at a definite rate:
  autocorrelation and the coefficient of variation of the band-passed envelope, model against
  recording. The cure is the bank's frequencies (§6), not the filter.
- **Amplitude modulation** — a slow wobble on the harmonics: per-band Hilbert envelope, or the
  spacing of the sidebands around each line. Fit rate and depth from the recording; one cymbal's
  was 24.5 Hz at 4.5 % over 20–150 ms, 8.5 % over 150–400 and 6.8 % over 400–800.
- **Roughness** — the owner's "there is a grrrr sound to the model", against a recording they called
  cleaner: measure modulation depth across the whole roughness range, about 15–300 Hz, not only at
  the rate you fitted. Excess depth there is the grrr.
- **Check what your own bank already does before adding any of it.** One model's envelope ripple was
  36–38 % where its recording's was 21–25 %: the bank was beating harder than the hardware, so a
  correctly fitted modulation on top still did not dominate the way it does in the recording, and
  the fix was to take beating away rather than add modulation.
- **Burstiness** — a gated or burst-driven noise source: envelope swing and autocorrelation against
  the recording (0.35 / 0.77 against 0.43 / 0.75 on one hat, which is four standard deviations away
  from plain noise and audibly right). Both are *fast*-modulation measures — 1 ms windows and
  1–20 ms lags — so they necessarily fall when you slow a flutter. They cannot be held at their old
  values while the rate is corrected; when the fault is a rate, the modulation table above is the
  instrument, not these two.
- **Do not chase a modulation cell to a number: it is realisation-dependent.** Across sixteen random
  noise realisations of one configuration the 60–150 Hz cell spanned 5.7–16.7 %, sigma 3.2, and the
  machine's own sample sat near the bottom of that range. Fit the configuration's expected value,
  measured over several realisations, and treat a single render's cell as a sample of it.
- **Slowing a modulation can eat the attack.** One hat's fast wander had been supplying the hit's
  peak; replacing it with a slower telegraph moved the loudest millisecond from 3.5 ms to 14.5 and
  took 2 dB off the crest, which is the owner's "snappy" in reverse. The two faults live in
  different parts of the hit, so gate the slow modulation in after the attack rather than trading
  one against the other.

**The source is shared; the colour is per voice.** Every voice draws on the machine's own noise or
metal bank. A private per-slot generator is forbidden by the crate's contract, and it also hides
exactly the periodicity and phase effects above.

**Never fit the recording's own floor.** Hum at 50/100/150 Hz, a flat floor at −80 dB, and +13 to
+23 dB of 100–200 Hz in a tail where the hit is long gone all belong to the recording chain. One
cymbal's recording carried all three, and fitting them would have put a hum in the model.

**Know when a noise voice is done.** A noise realisation never matches sample for sample, and the
difference map on the spectrogram will always be speckled. It is done when the resonance profile,
the octave balance and the window levels match, and what is left is the realisation.

**Fix the envelope before the filter.** "The original has more narrow bandpass" was mostly a tail
12 dB down by 60 ms: a hold of 35 → 45 ms and a decay of 0.065 → 0.12 s answered most of the note,
and the Q change that the words seemed to ask for was the small part.

**Anchor the body first, then the strike.** The body is nearly all of the duration and the strike is
one window. Moving a cymbal's band onto the hats' 7.2 kHz resonance made everything from 120 ms
onward exact, and left one visible fault instead of six vague ones.

**One window's error is not the voice's error.** Use separate terms with separate time constants — a
fast lift for the strike, a slow rise for the body, over an anchored base — rather than moving the
whole centre to fix one window. Every attempt to fit one curve through all six windows overshot the
far end.

**A modulation source must actually move during the window you care about.** A drift driven by the
envelope's fall does nothing on a voice whose envelope holds: a tambourine that holds for 200 ms had
lost 5 % of its amplitude by 45 ms, so its band stayed at the strike tuning through every window the
owner listens to and only moved once the voice was inaudible. The fix was a time constant of its
own, with zero keeping the envelope driver for the voices that decay. Before fitting any
envelope-driven modulation, plot the driver over the windows you are fitting and check it has moved.

**Every change costs something somewhere. Measure the cost and say it.** Narrowing one voice's band
took 2–5 dB out of its 8–16 kHz and moved its ⅓-octave distance the wrong way, 2.8 → 3.1, while its
centre improved. On this owner missing top is "the original has more harmonics", one of the two
phrases they use most. Report the regression beside the improvement, every time.

**A line the filter cannot move belongs to the bank.** When the peaks inside a window hold their
frequencies at every filter centre you try, they are the shared noise or metal sample's own lines.
The lever is then the bank — fit its oscillators from a long tail (§6) — and no amount of filter
tuning will place them.

**Fit a modulation from the recording, not from the hardware's schematic.** A cymbal's low-frequency
modulation was measured straight off the recording: the per-band Hilbert envelope gives 24.5 Hz at
4.5 % over 20–150 ms, 8.5 % over 150–400 and 6.8 % over 400–800. No access to the unit and no
frequency counter is needed, and our own measurement of a recording crosses the research boundary
where the recording does not.
- Then check that your own model does not drown it. That bank's envelope ripple was 36–38 % against
  the recording's 21–25 %, so a correctly fitted 24.5 Hz still did not dominate the way it does in
  the hardware.

**Build the shared mechanism, not four private fixes.** Five separate notes across two renderers
were one fault, and one table — a per-voice row of centre and Q multipliers at the strike and at the
end, plus a curve — answered all of them. The default row multiplies by exactly 1.0, so every voice
without a row stays bit-identical, and a test pins that.

**Prove the blast radius by rendering everything.** Take SHA-256 of all model renders before and
after a merge: only the intended IDs may differ. That is how a reverted change to two approved hats
was proved reverted, and how a one-voice merge was proved to touch one voice.

**Re-measure every reported improvement yourself, in the owner's terms, before spending their ear.**
An agent's own metric improved on a voice that had regressed on the measure the owner's complaint
was about, and another disclosed a residue of "about 700 Hz" on its median where the smoothed peak
said 995 Hz. Neither was dishonest; each had fitted its own number. A listening round is expensive
and an extra agent round is cheap.

**A note that contradicts the measurement is still right.** In one review round: "model has higher
ressonance" on a voice measuring 300–600 Hz *low* — it was the first 10 ms, which was 514 Hz high;
"model has lower ressonance" on a voice whose centre matched to the hertz — it was the width, Q 5.4
against 14.4; "the original has more narrow bandpass" on a voice whose strike Q already matched — it
was the tail Q, and the missing tail under it. Find which window, which width, or which link in the
chain the note is about. Never dismiss it, and never argue it down.

**Before believing a model is wrong, rule out the chain.** In this pass the chain produced, in
order: originals playing 4.5 dB loud because the loudness match counted sub-audio junk; a click that
survived two rounds of hunting in the DSP and was a Bluetooth headset; a browser serving cached
audio after a rebuild; a page left open across a rebuild playing its own decoded buffers, which cost
an approval; and a review server holding a stale copy of the fingerprint code, which cost eight. The
model was at fault in none of them.

**The owner's ear outranks your number, including when your number is right.** A tambourine measured
209–289 Hz low on the peak, in the direction the owner had complained about, and it was sent back
for another round. While that round ran, the owner approved it. The measurement was not wrong — the
later fit does measure closer — but it was a prediction of what they would dislike, and predictions
lose to the person listening. The better-measuring fit stays recorded in a comment beside the row,
waiting for them, and the approved sound ships.

**Never spend an approval.**
- An approved drum is frozen. If an agent is already changing one, tell it to revert before it
  rebuilds, and verify the revert by fingerprint.
- An approval can land *while* an agent works, so check the log before merging anything, not only
  before dispatching it. Two drums were approved mid-flight in one afternoon.
- When a fix has to cross into an approved drum — one shared band under an approved voice and a
  complained-of one — put the choice to the owner with the measurement, and let them decide whether
  to spend it. They may well say yes; it is still theirs to spend.
- When an approved drum carries a complaint in its note, propose the fix and ask. Do not take the
  owner's approval back on your own initiative.
- After every rebuild, refresh the review state and then check that each flip is a real change of
  sound before handing the page over.

**Know when to stop.** This owner has heard, and correctly named, errors of about 5 % of a centre
frequency and 3 dB of tail. Do not hand over a voice that is worse than that in a direction they
have already named; do hand over one whose remaining fault is a noise realisation, a strike width of
a few percent, or anything the four views show as identical.

### Acoustic drum models in particular

Learned fine-tuning `mxm-model-drums`' snare against Frankensnare's 14-inch Szpaderski on
2026-09-25/26 (the plan's revisions 20-21). A physical model has fewer knobs than a circuit and more
coupled physics, and the owner's words mapped onto it differently.

**Fine-tune, don't fit.** An automated corpus fit scored 17 of 21 snares closer than their nearest
neighbour and every one of them was still flagged by the tools here: a ring a fifth low, the loudest
moment at 46 ms, a body 6-14 dB too close to the strike. It had bought its score with soft strikes
at the bottom of their range, microphones at their limits and the wrong modes. A corpus fit places a
drum; the sound is then fine-tuned one drum at a time, by this section, with the owner. Settings
pinned at a range's edge are a warning, not a result.

**Iterate on one hit, one part of the sound at a time, in seconds** (the owner, 2026-09-28, on
Big Rusty's damped kick: "You cannot find a faster way to iterate on this? … Would it not be better to
be more focused. And get one right at a time?"; then "It seems that you iterate faster with this new
approach, so make sure to write it down."). Searches over three layers — every candidate rendered and
read against twelve takes — took minutes each and optimised a blend of a hundred readings, so a 9 dB
miss in the part being worked on weighed no more than a few small ones elsewhere, and three of them
pulled the settings the wrong way. What worked:
- **One layer, one part.** Take the middle layer. Work §5's parts in turn, reading only that part's
  readings. Move to the other layers only when the part is right.
- **The fast loop.** The kick fit's `render` mode (mxm-model-drums'
  [`crates/mxm-model-drums-dsp/examples/model_drums_kick_fit.rs`](https://github.com/mxm-audio/mxm-model-drums/blob/main/crates/mxm-model-drums-dsp/examples/model_drums_kick_fit.rs))
  reads one layer's takes once. It then renders the setting and each `--try a=v,b=w` on top of it,
  about 4 s a try, and prints the listener's established differences, most audible first — only
  `--parts` asked for, nothing of the room (`--room-after`). `--shape yes` prints the first 15 ms a
  millisecond at a time beside a take: the attack's shape, finer than any window. Set by hand; a
  search only over a part's own settings, and only once the cost counts what the part needs.
- **Say how far, not how much better.** Report the established differences against the takes beside
  the takes against each other (each take against the other three, `listen compare --set`,
  leave-one-out), never a falling loss alone. Beside both, the real drum one velocity step away (the
  fast loop's `--neighbours 7,9`: the other layers' takes, level-matched and counted as the renders
  are). Big Rusty's middle layer reads 5–16 against itself but 18–20 from layer 9 and 13–21 from
  layer 6, so a render at 21 is as far as the drum struck a little harder or softer; the takes'
  own count is a much stricter bar than one layer's step.
- **Take the velocity from the takes' loudness first** (*Map velocity* below; the fit's
  `--map-velocity`): a soft layer struck three times too hard moved every soft-to-hard reading.
- **Fix what the level match couples first.** Renders are matched to the takes' body loudness, so a
  body that rings too long moves every attack reading. On a damped kick the decay came before the
  onset.
- **Take the room out of the comparison** (*Tell the drum from the room* below). A damped kick is
  dead in about 170 ms, and what its takes hold after that is the studio. The room's readings count
  one way only — the model may not ring longer or louder — and the rest pitch read off the room's
  tail not at all. Counted both ways, a search gave the pillow back a second of ring.
- **Hear the recording chain before the drum.** A microphone and its preamplifier pass no steady
  pressure. A model without their low cut kept the net push of a head driven into the shell: its first
  half-cycle stood as high as the swing back, where the takes' stood 7 dB under. The cut is the listening
  point's (`DrumSpec::mic_low_cut_hz`); at 25 Hz it took Big Rusty's three layers from 25, 24 and 34
  differences to 24, 20 and 32 before anything was retuned — more than any law about the drum itself
  that day. Look at the first 15 ms (`--shape yes`) and the mean against the RMS it prints.
- **When a setting cannot move a reading, look inside the model.** The fast loop's `force` mode
  prints the beater's contact force through the strike. It showed a force chattering at a fixed
  number of samples per swing, which a change of sample rate proved numerical. The residual's input
  filter let a slow push into every band. Each finding became a law, off by default and with its
  test, not a setting.

**Measure first, then one step each side** (the owner, 2026-09-29: "I would analyse the real drum. See
what the measurements said and then try to hit that sound in first try. Then do something on each side
of that parameter wise and see if I got close or further to the original. Then I would take another
parameter and do the same again. And continue with every parameter. Then I would start from the
beginning and do it again. Until it gets no better."; then "If my approach works, then remember to
write it down"). It works, and it is the default search now:
- **The first try is read off the takes**, not guessed (the synth fit's `--start measured`): the rest
  pitch, the glide, the rise, the decay slopes, the lines every take holds.
- **Then each setting in turn, one step to each side** (`kick_fit::step_search`, the fits' `steps`
  mode): both sides rendered at once; the setting moves to the closer side, or its step halves; passes
  repeat until one brings nothing and every step is fine. Two renders a setting a pass where a
  whole-range scan takes 48. It took the synthesized kick's hard layer on Virtuosity from 70 to 38
  established differences, and set a click by hand the same way (four levels, the closest kept).

**Read the engine from the take** (2026-09-30, Virtuosity's hard hit; the owner: "You can use any kind
of synthesis as far as I care. Only rule is the goal", then "an animated parametric filter"). The synth
fit's `analyse` mode reads the whole patch from one take, no search, and its render stood as close to
that take as the listener's own resynthesis with the take's first 10 ms spliced in (43 audible
differences against 44; two takes of the drum differ by 12). What it took:
- **Split lines from noise, then describe each compactly.** The listener's self-test rebuild (its
  modes as tracked sines, the rest as 2 ms third-octave envelopes) sounded "very close"; the same
  noise described by 13 breakpoints a band read closer (49 against 63), laws per band (level, rise,
  two stages) 51, and one animated parametric filter — a broad band darkening and three rings whose
  centres move, 32 numbers — 45. A kick's ringing lives in the noise part: a narrow ring near 560 Hz
  that builds up for 30 ms and sinks toward 450 Hz as it rings over the whole sound.
- **Fit the lines to the waveform, not to the windows' estimates.** A line's frequency from one window
  was a turn out of phase by 0.4 s; its level from the first window's decay ran 20 dB high by 160 ms.
  Fitted to the take's waveform in its band (analysis by synthesis), the body turned out to be two
  lines — one gliding 90 → 82 Hz within 80 ms, a steady one at 83.4 Hz holding the ring — and the
  lines left −21 to −25 dB of the body band where one window-read line left +7 dB.
- **Whatever the lines miss, the noise takes.** A line the fit cannot hold as a steady sine (271 Hz
  here) becomes noise where the take is tonal (200–400 Hz: 60 % lines in the take, none in the render)
  and inflates the noise law about 6 dB. Turning the filter down 6 dB was worth 9 differences. Lines
  that follow the take window by window, as the self-test's do, would fix it but are stored
  per-partial pitch and amplitude envelopes separated from a recording — Roland US 11,127,387's claim
  1 almost word for word (the owner asked, 2026-09-30). A better law per line (a glide, two stages)
  is the road; recordings stay the ruler, not the source of what a preset stores.
- **Fit the ring and the strike apart, then together.** Fitted from the strike, the rings were spent
  on the strike (+33 dB bursts dying in 3 ms). The ring's law is fitted from 8 ms; the strike is a
  contact of its own — a level and a T60 per band — and a short thump line beside the body (41 Hz);
  then both together over every reading, each reading counted only as far as its window holds two
  periods of its band (a quarter-millisecond window cannot read a 40 Hz band).
- **A filter bank plays what it is told only if each band has its own noise and is calibrated as the
  analysis reads it.** Bands filtering one noise cancel where they overlap (12 dB low, notched); a band
  read alone misses its neighbours' leakage (1.5 dB). A test holds the voice to the law.
- **Or build forward from set overtones** (the owner's tables, 2026-09-30; the whole route, to a model
  in the plugin, is [`adding-a-drum-model.md`](https://github.com/mxm-audio/mxm-model-drums/blob/main/docs/adding-a-drum-model.md), the technique itself
  [`drum-synthesis.md`](https://github.com/mxm-audio/mxm-model-drums/blob/main/docs/drum-synthesis.md)): wavetables of chosen,
  editable overtones — each rounded to a whole multiple of a common step, so one period loops — in
  groups that die together (body, strike and second skin, long ring), each with a glide, an envelope
  and a moving low-pass; tuned by the step method against the takes. First tuned: 51 established
  differences against the fitted engine's 30, the owner: "almost perfect". What the groups cost: every
  overtone from phase 0 makes one sharp first swing (the strike, about 10 differences), and one
  envelope per group cannot give neighbours their own lives (about 20). Nothing is taken from the
  recording; the listener and the ear are the ruler.
- **Before a preset stores curves measured from a recording, counsel reads Roland's US 11,127,387**
  (stored sine envelopes plus a residual split from a recording; `plans/plan-mxm-model-drums.md`
  revision 11, in the private archive). Laws — decay times, centres, tilts — set through the editor are the other road.

**Reading the owner's words on an acoustic drum.**

| They say | It turned out to mean | Measure |
|---|---|---|
| "the attack is too soft" | The body after the strike too loud against it: the 30-100 ms window 6-14 dB too close to the peak | Window levels (`ab_resonance.py`), `ab_residuals.py`'s level flag, T20 |
| "the attack is washy" | Noise the recording lacks in the first 10-20 ms: a dense noise residual, or the wires' buzz starting early | First-20-ms centroid; the top octave's level at 5 ms against 15 ms |
| "higher pitched", "the tone of the drum skin" | The lowest strong partial, not the loudest: the recording had lines 10 dB under its ring that the model lacked (§6's rest-pitch rule) | Partials per window; the lowest within 12 dB of the strongest; the tone zoom below |
| "the attack is lower pitched than the tail" | A low mode lingering 100-200 ms where the recording hands over to its ring within 20 ms | A line track of each band, 40 ms windows every 20 ms |
| "the skin rings a little long" | Not the decay: the ring is a steady tone where the recording's wobbles (2 % against 9 % modulation over 15-300 Hz); a steady tone reads as ringing | The ring line's width, its 15-300 Hz modulation depth, its share of the tail |
| "the tail's pitch is too clean" | One steady line where the recording has several, and wobble | The tone zoom; modulation depth |
| "the overtones are too high" (drawn on the tone zoom) | Overtones too strong against the ring, not in absolute level: the octave bands matched within 3 dB | 450-750 Hz against 150-300 Hz per window |
| "the original has a longer wire decay" | Not a slower-fading hiss: a longer buzz decay "did nothing good". The recording's wires keep colliding after the model's stop (2-15 dB above them at 450-600 ms) and come in clusters: their 1 ms level correlates over 1-20 ms (+0.1 to +0.4, steady noise about 0), most on soft hits. The model needed wires that keep landing (resting on the head) and a buzz that follows the landings instead of storing hiss | The 2-8 kHz band's level at 300/450/600 ms; its 1 ms level's swing and autocorrelation after removing the decay; the buzz's share of 30-150 ms per layer |
| "the slapping is great but a little too pronounced" | Each slap too crisp: the buzz 2-18 dB over the recording above 6.4 kHz, where the recording's wires fall 6 dB and then 17 dB an octave. Not the slaps' sharpness (kurtosis and crest per 2 ms matched Gaussian noise in both) nor the 2-8 kHz level (within 0.5 dB). A roll-off at 8 kHz matched it, and the owner rejected it: a user can EQ the top away but cannot get it back (the next paragraphs) | Octave levels over 30-150 and 120-500 ms against the recording's round-robin mean; the 2-8 kHz level's swing at 4-16 Hz |
| "the rattle from the springs is a little more pronounced on the original"; "the original sounds a little more like white noise" (the finished snare, 2026-09-27) | The recording's wires rattle in distinct slaps where the model's are a plain hiss: their kurtosis over the 5 ms envelope reads 2.91–3.06 against the model's 2.71–2.79, which is white noise's own 2.75; the rattle's octaves, 1.6–6.4 kHz, stand 0.6–1.9 dB higher in the whole sound, which the buzz share hid because the model's surplus above 6.4 kHz made up for it; and below 800 Hz the recording is noise where the model rings clean lines. Not a flatter spectrum: the model's wire band is the flatter one | `listen`: `texture.rattle`, `texture.slap_kurtosis`, `tonality.tonal_share` |

**Look closer than the four views.** The spectrogram's resolution is its windows', not its pixels':
every short-time transform trades time for frequency (about Δt·Δf ≥ 1/4π at best). Two zoomed views
settled what the full-hit spectrogram blurred:
- **Attack zoom:** -2 to 60 ms, 100 Hz-20 kHz, windows of 8 ms below 1 kHz, 3 ms to 4 kHz and 1.3 ms
  above, a column every 0.04 ms. The crack, the buzz's onset and single wire slaps.
- **Tone zoom:** 0-900 ms, 80 Hz-1 kHz on a linear axis, one 90 ms window (about 16 Hz). The skin's
  lines a few tens of hertz apart, each with its own decay and wobble. The owner drew on this view.

**The numbers that decided things.**
- **Octave-band envelopes, model minus original,** eight octaves at eleven times from 5 to 700 ms, each
  against the file's loudest 10 ms. It shows which band is wrong when; one number per hit hides it.
- **Partials per window** (0-30, 30-100, 100-300, 300-800, 800-1500 ms) and **line tracks** of a band:
  which modes exist, which one leads, and when the lead changes.
- **Soft to hard over every round robin**, mean and spread per layer. One strike per layer is noise.

**Map velocity from the recordings' loudness.** The layers' body loudness spanned 20 dB (vl1 -9.3,
vl9 +11 dB about vl5); the model spans that between velocities of about 0.1 and 1. So the middle
layer was velocity 0.31, and a day of fine-tuning had run it at twice its stick speed. The nonlinear
parts, contact, glide and wires, all change with velocity.

**One drum, one setting.** A pad, a strike point or a buzz level the owner picks on one layer applies
to every layer of that drum; check them all before handing over.

**Settings are parameters; offer them as ladders.** Tuning, damping, strike point and material
(the loss above the ring) are the plan's §4.5 axes. When the owner hears a difference one of them
covers ("can it be tuned lower?", "it rings a little long", "we should get those overtones as
parameters"), put a ladder of it on the page and let the ear pick: they chose a medium pad (T60 1 s)
from three, and the hard hit over the medium one as the closest. What a setting cannot reach is
physics to add.

**Tell the drum from the room.** Late lines that recur at the same frequencies, within 1-2 Hz, across
a set's drums of every size are the room or the kit around them ringing in sympathy, not the drum:
Frankensnare shares about 152, 167, 188, 204 and 220 Hz from its 10-inch to its 22-inch snares, Unruly
145, 160, 173 and 218 Hz. List each drum's strongest partials over 0.8-1.5 s before chasing a low
line. Mechanisms fitted to them will not generalise; a half-day of vented shells and wire damping
proved it. When the owner wants a fair A/B anyway, add the room to the comparison only: two-pole
resonators at the recurring lines, each with its measured late T60, driven by the model's output,
gains fitted on one layer and checked unchanged on the others (they held within about 5 dB).

**A stored tail hides what the source does.** When every variant of a drive falls at the same rate late, the late
sound is an early store fading, not the source: the snare's buzz stored 0.6 s of hiss, and five drives and two
contact laws all fell 107 dB/s. Shorten the store until the late sound follows the source, then fix the source.

**A filter bank's skirts set its top.** A buzz or residual built from half-octave band-passes cannot
be shaped above a few kHz by its tilt or its top band: each band's skirt falls 6 dB an octave, and the
lower, louder bands' skirts outweigh the upper bands. Measure which band the excess comes from before
turning a knob; only a roll-off on the sum shapes it.

**Don't take out what an EQ can take out.** The owner, on that roll-off: "The user can do that
themselves with an EQ if the want. But they cannot get it back if it is never there." Brightness,
top-end level and anything else a user's EQ or filter reaches stays in the model, even where it runs
over the recording. Spend fine-tuning on what no EQ can fix: the physics, the timing, how the parts
balance and how they change from soft to hard.

**Keep the page small.** After a day of ladders the snare page held 24 rows, and the owner said "There
are too many to compare, I cannot tell them apart anymore." Put up the current pick and one or two
alternatives per question, three layers each at most, and retire a row as soon as it is judged.

**A single render's modulation measure is not a trend.** The wires' slow swing jumped between 0.3 and
1.1 dB as one setting moved, differently per layer. Compare it against the recording's round-robin
mean and move it only with a setting that averages (the slap's ring), not by chasing one render.

**Traps met here.**
- `ab_residuals.py` looks up each row's pitch in the drum machine's `model.rs` by row id. On another
  instrument's site that is some drum-machine voice's pitch, and the harmonic, partial and early-pitch
  checks are wrong or silently skipped. Give it the site's own pitch table.
- A render tool that takes the last fit's shared constants on every job line overrides the model's
  new defaults; say every constant a test depends on on the line itself.
- A tool built against the crate in its own directory needs rebuilding after every crate change.
  Two runs of this pass compared a new law against an old build.
- A rendered comparison needs the same trimming and loudness match as the page (§2), or its levels
  mislead.

## 8. What crosses into the repository

**Only the fitted constants cross**, each with a comment saying:
- what it is;
- that it was fitted to *the acquired comparison recording* on the date of the pass;
- what measurement could replace it.

**Research is cited** as `` `research:<path>` `` §. **A recording is never** committed, copied,
linked or named by its path. The site, the mapping and the annotations stay local.
