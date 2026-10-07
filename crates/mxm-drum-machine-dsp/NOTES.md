# NOTES.md — crates/mxm-drum-machine-dsp/

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked
examples. AGENTS.md is the contract; this file is the reference it links to.

## Contents

- [Catalogue, reference and pitch](#catalogue-reference-and-pitch)
  - [Sixteen slots, a larger append-only catalogue](#sixteen-slots-a-larger-append-only-catalogue)
  - [Reference is zero](#reference-is-zero)
  - [A pitched model's note is its measured rest pitch](#a-pitched-models-note-is-its-measured-rest-pitch)
- [Output and modulation routing](#output-and-modulation-routing)
  - [Main and individual output routing](#main-and-individual-output-routing)
  - [Per-slot modulation routing, kit-wide LFOs](#per-slot-modulation-routing-kit-wide-lfos)
- [Shared sources and what they cost](#shared-sources-and-what-they-cost)
  - [Shared source state is load-bearing](#shared-source-state-is-load-bearing)
  - [Gating a free-running bus — jump.rs, and fixed-point phase](#gating-a-free-running-bus--jumprs-and-fixed-point-phase)
- [Velocity, choke and transitions](#velocity-choke-and-transitions)
  - [This instrument has no accent: a hit is its own velocity](#this-instrument-has-no-accent-a-hit-is-its-own-velocity)
  - [Choke is an assignment, not a wiring](#choke-is-an-assignment-not-a-wiring)
  - [Replacing or choking a sounding circuit is a bounded fade](#replacing-or-choking-a-sounding-circuit-is-a-bounded-fade)
- [Capture](#capture)
  - [Capture and frozen playback — capture.rs](#capture-and-frozen-playback--capturers)
- [Events, dependencies and realtime rules](#events-dependencies-and-realtime-rules)
  - [Deterministic events and randomness](#deterministic-events-and-randomness)
  - [Dependencies and framework boundary](#dependencies-and-framework-boundary)
  - [Realtime and numeric rules](#realtime-and-numeric-rules)
- [Evidence and warts](#evidence-and-warts)
  - [Evidence rules and deliberately absent couplings](#evidence-rules-and-deliberately-absent-couplings)
  - [IDs 1–23: the TR-808 and TR-909 analogue circuits](#ids-123-the-tr-808-and-tr-909-analogue-circuits)
  - [The 2026-09-19 comparison fit](#the-2026-09-19-comparison-fit)
  - [TR-909 PCM IDs 24–27: the NMF-resynthesis ruling](#tr-909-pcm-ids-2427-the-nmf-resynthesis-ruling)
  - [IDs 28–94: the supporting catalogue](#ids-2894-the-supporting-catalogue)
  - [The 2026-09-18 acquired-recording pass](#the-2026-09-18-acquired-recording-pass)

## Catalogue, reference and pitch

### Sixteen slots, a larger append-only catalogue

`SLOT_COUNT` is 16 and notes 36–51 map one-to-one to those slots. A slot is one retriggerable circuit,
not a sample-player polyphony bucket. Model ID 0 is permanent legacy silence; the parameter domain is
permanently 0…255. Assigned IDs never move or change meaning, and a new model takes the next unused
ID. Unassigned IDs resolve to silence rather than aliasing an existing sound.

`model::AVAILABLE_MODELS` is the executable selector subset: IDs 1–94, only circuits that render in
this build. ID 0 deliberately remains outside it because the plugin's per-slot Mute owns intentional
silence for new patches.
Appendix A reserves future IDs, but a reservation is not offered as a silent model. Product labels may
not contain manufacturer or hardware model names. As each circuit lands, its entry appends to the
available list under the already-reserved ID and gains its tested capabilities. Selector groups use
the same ordered family names as the factory audition presets: Bridge 808, Reset 909, Economy 55,
Expanded 8000, Compact 606, Snap 110, Classic 78, Discrete 66 and Early 2L. Group headings are
display metadata; changing one never changes model identity or order.

### Reference is zero

Every creative control reaches the documented source/reference behavior at zero. Pitch is
family-specific: resonator poles, reset-VCO control, metallic-bank tuning, PCM clock, an evidenced
noise/filter law, or unavailable. Never implement one universal pitch shifter or final-envelope
abstraction.

`pitch_envelope`, `pitch_decay` and `noise_decay` are additive compatibility surfaces. At zero they
preserve the existing native excursion, source timing and wire/noise timing. A source circuit with no
native sweep stays unswept at zero; any extra sweep begins only away from zero. Main Decay likewise
keeps the calibrated source setting at zero. Positive travel reaches bounded extended-decay territory
where the topology permits: Deep bridge kick reaches 4.8 s T60, Reset punch kick reaches twelve times
reference, and the common extensible laws reach eight times reference. The modification nodes are
evidenced in `research:instruments/analogue-drum-machines.md` §4.11; these exact software maxima are
bounded musical choices, not measured hardware-modification endpoints. Do not fake extension by
looping fixed PCM assets.

Unsupported axes are exact no-ops for that model. They remain in the plugin's fixed parameter
inventory so changing Model never changes host-visible parameters.

Every complete circuit then crosses one explicit **static output-adaptation plane** before slot Level
and Pan. At 48 kHz, zero deviation and velocity 0.82, each isolated model reaches a −1 dBFS mono
peak. This is an offline-measured fixed trim, not a limiter, automatic gain control, dynamic
normalisation or evidence about the source machines' relative acoustic levels; those cannot be
recovered from recordings with unrelated chains. It preserves each circuit's envelope, accent law
and internal level relationships while making Model changes usable. `engine.rs` owns the 0…94 table
and a catalogue-wide test; any renderer gain change must remeasure the complete table rather than
hand-correct one trim.

### A pitched model's note is its measured rest pitch

`ModelId::reference_pitch_hz` names the **rest pitch** of every model with a tonal body — kicks,
snares (their body), toms, congas, bongos, cowbells, claves, rims and the bell, 61 models — and is
`None` for hats, cymbals, claps, maracas, the tambourine, guiro and brush. `chromatic_reference_key`
turns it into the MIDI key a claimed channel plays the model unmoved at: concert pitch for a pitched
model (A4 = 440 Hz, equal temperament), note 60 otherwise (owner, 2026-09-18).

**The rest pitch is where the pitch settles, not an average over the hit** (the owner's method). A
kick starts with a sweep, and noise and a short decay hide the tone behind it; measured over a whole
hit, the swept kicks and toms read 8–84 cents off. `tests/musical_pitch.rs` therefore renders each
model in an **analysis patch** — noise and its decay down, the sweep's time down, the body's decay up,
and Pitch envelope at −1 or 0, whichever leaves the steadier ring, because −1 removes a native sweep but
inverts an additive one — and reads the lowest partial within 12 dB of the strongest over the late
40–95% of the ring. The two halves of that window must agree within 5 cents. The same file proves
the analysis controls do not move the rest pitch, and that every pitched model plays within 5 cents
an octave either way. **Changing a pitched model's tone or pitch law means re-running
`print_the_measured_reference_pitches` and updating the table.** The checks run per model on
threads, which keeps the file to seconds in a debug build.

The two supporting pair cowbells (46, 70) read their own oscillator pair from their machine's shared
frame at zero Tune: CR-8000 571/838 Hz and CR-78 545/775 Hz, from the comparison fit. Away from zero
they run the same two squares on free private phases, moved exactly. By the rule above, 46's note is
its upper oscillator, 838 Hz: the recording keeps 571 Hz 15 dB under it.

## Output and modulation routing

### Main and individual output routing

After the catalogue trim and slot Level/Mute/Solo gain, each slot selects
`mxm_part_routing::Destination::Main` or one of sixteen mono auxiliary destinations. The pre-Pan
mono signal enters `mxm-part-routing::DestinationRouter`; Pan is applied only when a share reaches main and is bypassed
for every individual share. Multiple slots assigned to one individual output sum there. A stable
all-main assignment is sample-identical to the pre-D7 stereo path.

A destination edit on a slot that carried audio at its last sample transfers over 2 ms as a
continuous linear partition whose two gains sum to one. At most the old and new destinations carry
a slot. Returning to the origin reverses in place; a third request replaces one latest-wins pending
target. Any other slot — idle, or muted, soloed out or at zero level while its circuit runs — has
nothing to carry and takes its destination at once. A host may stop processing an idle instrument,
and an unmute or hit can land on the edit's own sample, so a transfer that needs inaudible samples
could otherwise still be pending and leak the new sound's start to the old output. "Carried audio"
is recorded after each rendered sample (circuit active at a non-zero post-Mute/Solo level; routes
scale level multiplicatively, so zero stays zero) and read before the next, and `route_outputs`
must run before an event group's triggers, which the plugin does. Activation may likewise install
destinations directly. Voice reset, choke and panic do not change instance output assignments.

### Per-slot modulation routing, kit-wide LFOs

Each slot owns a thirteen-target by seven-source topology. Targets are `pitch`, `pitch_env`,
`pitch_decay`, `decay`, `attack`, `tone`, `body`, `noise`, `noise_decay`, `character`, `dynamics`,
`level` and `pan`; sources are `lfo1`, `lfo2`, `lfo3`, `wheel`, `pressure`, `velocity` and `random`.
A route on slot 1 cannot alter slot 2. One `mxm-modulation::SourceFrame` belongs to each slot, while
exactly three kit-wide LFO generators run once per sample and project the same values into whichever
slot frames route them. Controllers follow the slot's sounding channel; velocity/random are held
from that slot's latest trigger. An absent route contributes nothing while retaining its amount. A
newly needed source is cleared before its first read.

All three LFOs are free-running: their phase advances on every rendered sample whether or not a route
currently reads them, so emptying the routing matrix and filling it again resumes the phase the
elapsed samples imply. They were briefly gated on a non-empty matrix; removing that gate did not
move the render floor by more than `drum_machine_cpu_cost`'s own run-to-run spread.

**Every route is the collection's modulation standard** (`plans/plan-modulation-standard.md`,
2026-09-26): this is an original instrument with no hard-wired depth to keep, so every pair takes the
standard reach. Pitch is 12 semitones at full; **the `level` target is shown as Amplitude and is the
standard factor on the slot's level**, `1 + clamp(Σ, ±1)` — silence to double — where it summed up
to ±12 dB; the other targets use their one bipolar axis unit, and Pan its −1…+1. Velocity is
published as `v − 1` (resting at full before a slot's first hit), Wheel and Pressure through the
standard, Random as `standard::random`. No kit carries a route, so no kit moved. `conformance.rs`'s
`Declared` runs the standard's checks — the release check on a short model, since a hit ends by
itself — each falsified once; the plugin's tests reuse it through the `conformance` feature.
**Since 2026-10-07 the plugin's route parameters are four route slots a drum** (mxm-model-drums'
scheme, the owner's ruling; the plugin's `NOTES.md`), which it writes into this grid each block:
the grid, its reaches and these checks are unchanged, and the plugin holds its one amount
parameter's travel to every pair's offer rather than each pair's reading. This crate's `AGENTS.md`
read, until then:

> - Each slot owns its own 13-target × 7-source topology and `SourceFrame`; a route never crosses
>   slots. Three kit-wide LFOs run once per sample and always free-run. A newly needed source is
>   cleared before its first read; an absent route keeps its amount.
> - Every route is the collection's standard: Amplitude (`level`) is the standard factor, Velocity is
>   `v − 1`. `conformance.rs`'s `Declared` runs the checks; the plugin reuses it (`conformance`).
>   Randomness is seeded; the plugin resolves LFO rates and tempo sync.
Randomness has an explicit seed and reset restores the same sequence. The plugin resolves free or
host-tempo-synchronised LFO rates before this framework-free layer receives them.

## Shared sources and what they cost

### Shared source state is load-bearing

Noise, metallic oscillator banks and PCM source data are shared whenever the
catalogue declares a shared bus. Consumers at zero deviation read one generated source sample from
that machine-family state. Twin-mode snare, Bright short maraca and Triple pulse clap all read the
same 808-family white sample; all three falling toms read the one coloured derivative, because all
three recorded toms carry it (the low tom through its Noise controls, mid and high at a fixed level
with Noise an exact no-op). Do not give each slot a statistically similar private generator: phase and
cross-voice correlation are part of the machine.

A nonzero metallic Pitch deviation may take a private bank; zero rejoins the shared free-running bank
without resetting it. Shared state runs continuously according to the reference, not only while a
consumer is sounding. The 808-family transistor-noise approximation has one fixed-seed generator;
every simultaneous Twin-mode snare reads that same sample before its own wire filter and envelope.
The same bus derives one labelled coloured-noise sample for every simultaneous falling tom;
private per-slot noise remains forbidden. IDs 13–16 read one free-running six-square frame at zero
Pitch. A nonzero Pitch uses the slot's persistent private bank; returning to zero rejoins shared phase
without resetting it. A choke — which is now only ever a user-assigned group — clears the slot's
own envelopes and filters, never the machine's bank.

Running continuously has a measured price, and it is the engine's dominant cost. Every machine-shared
bus advances in `process_routed` whether or not a loaded model reads it, so the render cost with all
sixteen slots `Off` is a floor under every patch, not an idle state.
`drum_machine_cpu_cost` reports it: about **470 ns/sample, 2.3% of one core at 48 kHz and 8.2% at
192 kHz** on the 2026-09-20 development machine — roughly a third of what a sixteen-model kit costs
in total. Attributed by measuring a doubled call, `LegacySources::tick` is about **205 ns** of that
floor (six machines x up to eight squares, ticked even when no legacy model is loaded) and the 808
`MetalBank` about **38 ns**.

Do not treat that as settled. `plan:365` requires the opposite — machine buses evaluated only when a
selected or active model needs them, free-running phase preserved by analytic jump-ahead rather than
by ticking forever. Gating the buses is the open work; the figures above are the baseline it has to
beat, and any such change must keep the shared-sample correlation tests and the reference bank
bit-identical.

### Gating a free-running bus — `jump.rs`, and fixed-point phase

A shared bus has to keep the *behaviour* of a source that never stops — the hardware's oscillators
run while powered and a trigger opens an envelope rather than restarting them — but it does not
have to be **computed** every sample. Conflating those two is what made the idle floor what it is.
**Reseeding a dormant source is not the cheap version of this and is not allowed**: a seed says
nothing about oscillator phase or filter history, reusing one repeats attack textures, choosing
another invents an unrelated timeline, and either loses the hit-to-hit and simultaneous-voice
correlation that is half of why these buses are shared.

So a gated bus counts the samples it slept and jumps its state to where running would have left it.

**The 808 metal bank's phase is fixed point, and that was the enabling change** (owner, 2026-09-21).
An `f64` phase advanced by `fract(phase + increment)` rounds once per sample; the closed form
rounds once in total, so the two drift — a few ULP after 48 samples, about 1.3e-11 after half a
million, which `jump.rs`'s `floating_point_phase_is_not_exactly_jumpable` keeps on the record. A
`u64` phase makes wrapping *be* the cycle, so `phase + increment × samples` is the same arithmetic
the per-sample loop does and needs no rounding. `MetalBank::advance` is therefore one multiply per
oscillator whatever the gap, and `advancing_lands_exactly_where_ticking_does` holds it bit-exact.

**The sound did not change in any sense a person could detect.** Measured against the previous
`f64` accumulation over four seconds of the whole bank: **−179 dBFS worst sample, −201 dBFS RMS**,
which is 35 dB below a 24-bit LSB and cannot be represented in an exported pack, let alone heard.
The PolyBLEP is why — an edge moves continuously with the phase, so a phase error produces an
error of the same order instead of flipping a sample between +1 and −1. Every pitch, trim and
shared-source correlation check passed unchanged, so **no fixture needed regenerating**. Fixed
point is also the finer representation, 2⁻⁶⁴ of a cycle against `f64`'s 2⁻⁵³, and it cannot drift.

The readable fractions stay in `DUTY` and `MetalBank::START_PHASES` under `#[cfg(test)]`, and the
integer tables are **computed** from them by `fixed()` rather than transcribed —
`the_fixed_tables_match_their_fractions` exists because hand-typing them got two of six wrong in
the low digits, which moves an edge without looking like anything.

**`jump.rs` covers the other exactly jumpable class**: `xorshift64` is linear over GF(2), so N
steps is that map to the Nth power, and with the doublings precomputed a gap of any length costs
at most 64 XOR-folds. `a_jump_equals_literal_stepping` proves it against stepping.

**Every shared bus is gated, by the same rule.** `Engine::prepare` sets one `*_has_reader` flag
per bus from any slot whose selected *or* retiring model reads it — selected, not sounding — so a
tail can never be cut off from the source under it. Gating on *sounding* would save more and is
the riskier half; it is not done.

| Bus | How it catches up |
|---|---|
| 808 metal bank | Fixed-point phase: one multiply per oscillator, then five samples replayed to rebuild the band-limiting window |
| Supporting machines (`legacy`) | The same, across forty-eight squares, plus a GF(2) jump for each of six noise registers |
| 808 white/pink noise | Exact jump for the register; the colour filter is **replayed over a bounded window**, because a one-pole has no closed form but does forget — `colour_forgetting_window` is how many samples that takes, and the replay is exact rather than close once it has |
| 909 hardware noise | The fixed-point 300 kHz clock says exactly how many periods were missed; the 31-stage register jumps that far in constant time |
| DR-55 noise | A GF(2) jump. Small enough to save little alone — gated so the rule holds without exception |

**Measured, this machine: the idle floor went 484.6 → 253.1 ns/sample at 48 kHz** (2.33% → 1.21%
of a core) and 8.69% → 4.59% at 192 kHz. A full sixteen-model kit went 1365.9 → 1285.9 ns, and
sixteen struck kicks 1059 → 816, because a kit pays only for the buses its own models read.

**What remains is not bus cost.** Gating the last two noise sources moved the floor by less than
the measurement's own noise, which says the residual ~250 ns is the sixteen-slot loop itself —
the per-slot patch copy, routing check, destination routing and peak tracking. Another bus gate
will not help; that loop is the next thing to look at, if anything.

## Velocity, choke and transitions

### This instrument has no accent: a hit is its own velocity

The hardware these models come from resolves simultaneous hits through an accent mechanism — the
808's common trigger voltage, the 909's programmed accent values, the CR-78/CR-8000/DR-110 global
accent envelopes — and `research:instruments/analogue-drum-machines.md` §822 records that those
boxes have accent rather than continuous key velocity.

**None of that is modelled, by the owner's ruling of 2026-09-20.** There is no accent control on the
interface and there is not going to be one, so a hit takes its own velocity and nothing else. One
slot's level never depends on another slot's, inside a machine family or across families, and host
event order cannot reach the result because no value is shared to begin with. `Dynamics` bends that
velocity response around its reference and is the only control over it.

**Dynamics means one thing on every family** (`velocity.rs`, 2026-09-26): the velocity curve's
exponent, `1 − 0.65·d` above zero and `1 − 2·d` below. A family with a floor under the hit keeps it,
`floor + (1 − floor) · v^e`; a family linear in velocity — the analogue 909, the Economy 55, the
legacy machines, the PCM 909 — takes the curve on its velocity term at its fixed level; and a frozen
capture scales its buffer by the same curve. Before, the linear families read Dynamics as a
−3.7…0 dB trim that left the curve alone, and the metal 808 and reset-VCO 909 families had mappings
of their own. **At Dynamics 0 nothing moved**: every kit ships at 0, and a render of all 94 models at
four velocities was identical to the bit before and after (the plan's revision record);
`dynamics_bends_every_familys_velocity_curve_the_same_way` holds that ±1 bends every model's curve
one way, measured on energy because a timed-burst clap's first burst ignores velocity.

This is a deliberate departure from the source machines and from the brief's velocity law, kept
because the instrument is played from a keyboard or a DAW rather than from a step sequencer with an
accent button. A review that rediscovers the hardware's accent buses is rediscovering a decision,
not a defect. `a_hit_is_its_own_velocity_whatever_else_is_struck` holds it.

### Choke is an assignment, not a wiring

Each slot carries a `choke_group`: 0 for none, 1…`CHOKE_GROUP_MAX` for a group. A triggered slot
silences every other slot in its group through the bounded de-click of *Replacing or choking a
sounding circuit*, so a long kick cut by a short one does not click. Slots triggered on the same
sample are exempt — they are all starting, not cutting each other — which also keeps the result
independent of host event order, and preserves retrigger semantics: a second hit on a live circuit
adds to its ring rather than resetting it first.

**Nothing chokes unless the user assigns it** (owner, 2026-09-20). The source machines' hardwired
closed/open hat pairs are gone, and the factory kits ship with every slot at Off. A hardware pair
could only ever couple one machine's own two hats; a group couples any two slots, which is what a
player wants when the open hat is an 808 and the closed one a 909, or when a short kick should close
a long one. The circuits therefore own no choke of their own: `PcmMetal::choke` and
`Hat::choke_open` were removed with the wiring, and the slot-level fade ends in `Slot::reset`, which
clears each voice's filters — including the ones those methods used to forget.

### Replacing or choking a sounding circuit is a bounded fade

A slot separates `model`, the selection, from `rendering`, the circuit the render path is sounding.
They differ only while an outgoing circuit fades out, over `MODEL_RETIRE_SECONDS`. The selected model
is authoritative for triggers the instant it is set, and the two never share voice state.

The replacement policy is deterministic, and every branch of it exists because the alternative is an
audible cut:

- a model change or `NoteChoke` on an audible slot starts the fade; on a silent one it takes effect
  at once;
- a **second** change or choke during a fade only moves the selection. The circuit fading out is
  still audible and the intervening selection never sounded, so rapid A→B→C and change-during-choke
  leave A's fade running;
- a trigger during a fade ends it and sounds the selected model, so a fade never delays a hit;
- `Slot::reset` syncs `rendering` back to the selection. Panic during a fade must not leave a slot
  rendering a circuit its triggers no longer address;
- completing a fade carries `peak` across. That is a meter the host may not have read, not circuit
  state; only an explicit engine reset clears the meters.

The metallic bank moves the same way, in the priority families and in `legacy.rs`'s supporting
voices, which share `METAL_TRANSITION_SECONDS` rather than each choosing a duration. A creative Pitch deviation takes the slot-private
constellation over `METAL_TRANSITION_SECONDS` rather than on one sample, `metal_pitch` holds the
deviation the private bank is running at so the outgoing tuning fades out as itself, and parking on
shared stops ticking the private bank entirely — its phase persists and costs nothing. The blend is
exact at both ends, so a voice steady at either tuning is untouched by the crossfade existing, and
the supporting voices blend their pair path's separate direct mix alongside the bank itself.

A voice's filters are all cleared by `reset`. `metal_peak_filter` sat outside it and leaked a Q-30
ring between hits; when a voice gains a filter, it gains a line in `reset` in the same change.

Both durations are CHOSEN, named in code, and owed a click/latency measurement that has not been
run. Testing these transitions needs care: a fade one sample long still renders its first sample at
full gain, and a cymbal's own slew buries a bank switch, so a test that looks at one sample or at
`worst_step` alone will pass against a hard cut. The tests in `engine.rs` measure the worst step
across the whole transition window, and for the bank, that the divergence *grows*.

## Capture

### Capture and frozen playback — `capture.rs`

Resample (plan §4.7, D9) freezes each slot into a one-shot and plays the one-shot instead of the
circuit. `capture.rs` owns both halves and **only** those: it renders and it reads back. Writing a
sample pack is the plugin's, because *Dependencies and framework boundary* below keeps
`mxm-audio-file` out of the shipped graph.

**A capture is a recording of an approved drum, not a re-voicing of it.** All 94 models are owner
listening-approved, so the only differences a freeze may introduce are the ones the plan names.
`a_capture_equals_the_slots_isolated_live_render` holds the strict half of that: sample for
sample, a capture is what the circuit rendered.

**One pass, not sixteen.** `capture_kit` strikes every slot at the same instant and reads each from
its own `Destination::Auxiliary`, so all sixteen buffers come from one engine and one bus history.
That costs a single pass over the shared sources, and two slots struck together draw the noise the
hardware would have given them. Two slots struck *apart* do not, because a buffer rendered against
bus samples 0…N is replayed at a position it never saw — a bounded loss, not a hidden one.

**The capture context is fixed, which is what makes a capture a pure function of the patch and the
sample rate.** `CAPTURE_VELOCITY` fixes the strike; `CAPTURE_TEMPO_BPM` fixes the tempo, because
one model reads it and keying the cache on tempo would re-render the kit whenever a project's
tempo moved; routing is off, since every route that survives a freeze applies *after* the capture;
and the engine starts from its deterministic reset origin.

**`capture_patch` neutralises the mix, and the reason is where the tap sits.** The auxiliary
stream is after Level, and Mute and Solo reach the engine as Level zero rather than as fields of
their own — so neutralising Level neutralises all three and a muted slot still captures. Pan is
already bypassed on the individual outputs, and Master never reaches the engine at all. **Pitch is captured at its
reference detent; Decay is baked in.** Pitch reads correctly in both directions from one capture,
so freezing it would only add a resample. Decay cannot be lengthened by a sampler, so capturing at
the reference put every Decay above it out of reach — frozen *and* in an exported pack, which is
the owner's report of 2026-09-21. `SlotCapture::decay` carries the baseline the reader subtracts;
no saved state has to carry it, because a capture is a cache rebuilt from the parameters.

**Each buffer is trimmed to its own tail.** The pass runs until the last slot goes quiet, so a
short rim would otherwise carry seconds of silence waiting for a cymbal. Trimming is what makes a
slot's capture independent of what it was captured beside, and it keeps an exported one-shot from
being mostly padding.

**Reaching `MAX_CAPTURE_SECONDS` is truncation, not failure.** The catalogue's tails are long — a
legacy voice carries a sixty-second emergency bound — and those models must still be usable
frozen, so a capture that has not gone quiet is faded over `TRUNCATION_FADE_SECONDS` and reports
`truncated`. `CaptureError` is reserved for a refused allocation, a non-finite sample or an
unusable rate, and a failure yields nothing partial.

**A capture can be rendered in pieces, and the consumer decides how big.** `capture_kit` is the
whole thing in one call; `KitCaptureInProgress::begin`/`render`/`finish` is the same render split
into bounded chunks, and `capture_kit_until` is one call that asks a predicate whether to give up.
All three produce identical audio — the resumable form exists so a caller can bound how long it
takes to abandon a render, not to change one. The plugin uses it because its capture thread is
joined at teardown, so the chunk size is what a host waits for when a project closes. **A kit
carries an opaque request number** (`KitCapture::request`, stamped by `answering`, zero until
then) that this crate never reads: the plugin numbers its engages with it, so a kit made for one
it has left is never installed.

**A capture fits a 1 MB stack, in debug.** The plugin captures inside `activate`, on the host's
main thread, and 1 MB is the Windows default for one. `KitCaptureInProgress` therefore boxes its
`Engine`: held by value, `begin` and `capture_kit_until` each kept copies of it, and the debug
bundle overflowed `clap-validator`'s main thread under parameter fuzzing. A new value that holds an
`Engine` on this path boxes it too. `a_capture_fits_a_one_megabyte_stack` holds the bound.

**Playback is a sampler, and says so** (owner, 2026-09-21). `CaptureVoice` reads with
sample-domain Pitch and Decay: **pitch is a playback rate**, so pitch and duration move together
exactly as §3.4's PCM-metal row already describes for the one family that was always a clocked
capture; and **Decay only shortens the capture it is given**, which is why the capture is taken at
the Decay that was set rather than at the reference. Asking for more tail than the buffer holds
re-renders it; asking for less shortens it in place, so the axis stays live under the hand while
the exact version arrives. No time-stretch is built to hide the asymmetry.

**The reader is bounded before any family chooses how it retriggers.** A slot owns
`READERS_PER_SLOT` preallocated readers over the one immutable buffer; a reader is a position and
an envelope, not a copy, so overlap costs reader state and no memory, and a trigger with none free
steals the oldest. `Retrigger::Restart` is what a hard-resetting circuit does and `Overlap` what a
ringing resonator does; which one a family takes is a claim about its circuit and belongs in the
catalogue ledger.

**The unity case does not pay for the rare one.** At rate 1.0 on an integer position the read
short-circuits to a plain fetch. Off unity it is a windowed sinc whose cutoff narrows with the
rate, because mxm-kit's `docs/oscillators/14-samplers.md` §14.5 measures why upgrading the interpolator
alone is the wrong half of the aliasing problem. **The interpolator's cost has not been measured
against the circuits it replaces** — plan §4.7 owes that number, together with the shared-bus
floor it cannot remove on its own.

## Events, dependencies and realtime rules

### Deterministic events and randomness

Same-offset events are grouped and resolved by the brief's order; host iteration order must not change
accent, choke or retrigger results. CC120 dominates and clears every circuit, bus and pending event;
CC123 releases expression ownership without truncating ordinary one-shot tails.

All randomness uses explicit fixed seeds. A reset restores deterministic startup. No operating-system
entropy enters DSP or tests.

### Dependencies and framework boundary

The runtime dependencies are `mxm-modulation` and `mxm-part-routing`; both are dependency-free at
the same MSRV. There are no `nice_plug` or GUI types. Public APIs take plain values, events and sample rate.

`mxm-measure` may be a dev-dependency for rulers; verdicts and thresholds remain in this crate's
tests. `mxm-audio-file` is dev-only for listening renders; `mxm-listening` and `serde_json` are
dev-only for the `drum_machine_ab_page` comparison example, which decodes, trims and loudness-matches
through the listener's `prep` so the page and every listening report read a sound the same way. None
enters the shipped graph.

### Realtime and numeric rules

- No allocation, locks, logging, formatting, file/network I/O or blocking work on render paths.
- Fixed arrays own all slots, buses, event groups and model state; no per-trigger construction.
- `f32` carries audio; `f64` computes recursive coefficients and offline/interval setup where error
  compounds.
- A struck pole pair uses the `sin(ω)` input numerator in `Resonator::process_strike`; the raw
  all-pole gain is a numerical artefact (about 136 at 56 Hz), not circuit level. The Bridge 808
  struck bodies instead use `deep_bridge_kick.rs`'s coupled-form (phasor) resonator. A strike there
  sets the phase and amplitude directly, the amplitude survives retuning, and a retrigger still adds
  to the live ring.
- Ordinary reference hits stay clear of the final emergency bound; that bound must never become an
  implicit waveshaper. The supporting renderer's output saturator sees its signal at a quarter of
  its level at reference, and Character restores full drive at +1.
- A coefficient that does not change every sample **may** be built once and held rather than
  rebuilt on the render path; on an always-on path it **must** be. Where that line falls, and the
  measurements behind it, are the bullet below — read the two together, because holding is not free
  and this one is not a blanket requirement. Three shapes of it, all bit-exact because the held
  expression is the one the model was calibrated with: a coefficient following the **sample rate alone** is built at activation and
  in `set_sample_rate` (`Engine::noise_808_colour_pole`, and `legacy.rs`'s fixed 400/600/1000/6000
  and 3000 Hz corners); one following the **rate and the model** is memoised on `(id, sample_rate)`
  (`legacy.rs`'s strike RC and feedthrough pair); and one following a **tuning** is held beside it
  and rebuilt only when it moves (`legacy.rs`'s `mode_tuning`/`mode_pole`, which covers both
  `Resonator::configure` and `pole_terms`). A held tuning uses `NaN` as its sentinel so that
  `reset` and a rate change force the rebuild — `Resonator::reset` clears the recurrence, and a
  tuning must not outlive it. What genuinely moves every sample — a corner following an envelope,
  a pole following Tune — stays on the render path.
- **That rule is about the always-on paths, and it is not a licence to memoise every expression in
  a voice.** It was written from `Engine::noise_808_colour_pole`, which runs every sample whatever
  is loaded, and then over-generalised. What measurement actually shows, on
  `drum_machine_cpu_cost`'s scene holding sixteen voices open at 10 Hz (`retrigger_hz`, which is a
  rate so the figure is the same at every sample rate):

  | A/B | 48 kHz | 192 kHz |
  |---|---|---|
  | all per-voice holding on / off | 1604 / 1627 ns | 1515 / 1544 ns |
  | `MetalBank` increments held / not | 472 / 471 ns idle | 428 / 431 ns idle |

  Roughly 1–2% for the per-voice holding and nothing resolvable for the shared bank, against a
  run-to-run spread of 1–4%, with earlier pairs coming out the other way round. A held coefficient
  costs a tuple comparison, a branch and a read from a larger struct — the same order as the `exp`
  it replaces. So: hold what is on an always-on path, because that cost is paid by an empty patch
  and the rule is cheap to obey there; hold a per-voice coefficient where it is free; and **do not
  add memo state to a voice on the strength of this rule alone.** Several per-voice sites are
  deliberately left rebuilding. Measure first, on a scene where the voices are running.
- Every recursive state flushes denormals and recovers deterministically from non-finite input/state.
- Internal frequency and time bounds remain valid at every accepted sample rate.
- Saturators are explicitly bounded in `f32`; stated output bounds are constants with arguments.
- Reset leaves exact silence and no stale shared-source or choke state.

## Evidence and warts

### Evidence rules and deliberately absent couplings

All initial catalogue entries are schematic-derived and fidelity unverified. Comments cite the
research page by `research:` reference, never a private path. Every chosen calibration constant says
that it is chosen and names the measurement that could replace it.

Machine couplings are deliberate: shared oscillator/noise phase, the TR-606 tempo-dependent
open-hat law, reset-VCO strike phase and PCM pitch/duration coupling. Label them in code and tests
so nobody “fixes” them into generic drum behavior.

Two couplings the hardware has are deliberately **absent**, by the owner's rulings of 2026-09-20,
and each has its own section above: machine accent buses, because this instrument has no accent and
a hit is its own velocity; and the hardwired closed/open hat choke, replaced by a choke group the
user assigns. Do not label those in code as couplings, and do not restore them from the hardware
evidence — the evidence is not in dispute, the product decision is.

### IDs 1–23: the TR-808 and TR-909 analogue circuits

IDs 1–23 are the TR-808 and TR-909 analogue circuits:
- struck bridged-T bodies;
- the 808 family's shared noise and six-square bank;
- hard-reset triangle VCOs;
- one 31-stage, taps-31-and-13, approximately 300 kHz machine-shared 909 noise source.

Their calibrations are the comparison fit below. The owner released the earlier settings for fitting
on 2026-09-19 — *not married to the current drum settings* — so the 2026-09-17 Twin-mode snare and
Reset punch kick verdicts did not freeze those intermediate renders. The owner listening-approved
all 94 current models on 2026-09-20 after the comparison fit; that approval is recognisability, not
hardware verification.

### The 2026-09-19 comparison fit

**The 2026-09-19 comparison fit.** Every model with an acquired comparison recording was fitted to
it by onset, sweep, rest pitch, envelope and spectrum, with
[`../../docs/drum-model-fitting.md`](../../docs/drum-model-fitting.md). Code comments name each
fitted constant. A fit is recognisability against one recording chain, not hardware verification:
fidelity stays UNVERIFIED. Where it differs, it supersedes the service values and the 2026-09-18
calibrations below.

- **Onsets.** No reference hit starts at its peak unless its recording does, as claves and rims
  do. Attacks come from the trigger pulse's shape, the VCA's rise and filter build-up.
- **808.**
  - Kick: a 1 ms trigger feed-through spike, a 5.1 ms held pitch lift with its transition strike,
    rest 48.8 Hz, T60 460 ms.
  - Snare: modes at 168.3/331.9 Hz.
  - Toms and congas: measured rest pitches and T60s, with per-voice fitted excursions in place of
    the 12 ms control decay.
  - Rim and clave: the rim is 1761/472 Hz through an asymmetric clipper and a 9 ms switch, and its
    trigger reaches the output 0.45 ms before the strike. The clave is 2497 Hz with a 33 ms switch.
  - Maraca: an 18.5 ms gate with a VCA threshold.
  - Clap: bursts at 0/8/19.5/29.5 ms and a 2.13 s reverb tail.
  - Metal bank: this unit's six oscillators, not the service nominals.
    - Oscillators 1–4 are fitted to the spectral lines of three cymbal tails: 245.17 / 420.76 /
      339.79 / 565.30 Hz, at 44 % duty. They explain 96 % of the line power, against 48 % for the
      nominal set.
    - Oscillators 5/6 are the cowbell's trim: 824.65 / 552.12 Hz at 46 % duty.
    - The squares have four-point B-spline PolyBLEP edges.
    - The nominal set's edge pattern was the closed hat's audible periodicity.
  - Hats and cymbal: both hats take one shared 7.1 kHz Q 5 band. The closed hat's swing VCA passes
    one polarity into a third-order 7.5 kHz high-pass; the open hat's saturates softly. The cymbal's
    lower path is high-passed at 1.2 kHz (third order).
  - The struck bodies use `deep_bridge_kick.rs`'s coupled-form resonator (see *Realtime and
    numeric rules*).
- **909.**
  - Kick: the sweep is linear in Hz, `54·(1 + 4.95·e^(−t/12.4 ms))`. The oscillator holds for the
    1.9 ms trigger and releases from its centre, and a VCA threshold ends the hit in finite time.
  - Snare: 178/262 Hz, with its noise low-passed at 11 kHz.
  - Toms: 88.5/120/138.7 Hz, carrying the recorded quiet low partial (0.62–0.69×) and short high
    one (1.66–1.83×).
    - They hold through the trigger, and only the dominant oscillator follows Decay.
    - On the mid and high toms, the high oscillator opens at the trigger's release, and the release
      rings an attack transient.
    - Tom noise waits for that oscillator's amplifier.
    - The low tom's trigger release also clicks, and each tom's second oscillator sits at the
      recorded ratio: 0.684 low, 0.661 high.
    - The output stages of the toms and the clap stay near-linear at reference.
  - Rim: one strike under the amplifier's rise, into 220/480/1132 Hz networks. Striking from both
    trigger edges notched its spectrum, and rendered every partial about 10 % low.
  - Clap: the tail starts at the trigger; the bursts start at 8.9 ms, 10.9 ms apart. The burst band
    is centred near 1.45 kHz, and the tail band near 1.04 kHz. Once the tail falls below 30 % of
    its start, the tail band darkens with it, level-compensated (owner: the original's late hiss is
    darker).
- **Economy 55.**
  - An 8–10 ms trigger strikes the kick and snare again on its falling edge. The snare also carries
    the kick's roughly 20 Hz trigger leak through the coupling capacitors.
  - Fitted envelopes replace the §11.1 T60s:
    - kick 40 ms;
    - snare body 30 ms at a 350 Hz rest, plus a noise fade that follows Noise decay only;
    - rim 13.8 ms;
    - a hat fade that ends at 57 ms, and holds its last window up: `(1 − t/T)^p` with p = 1.3.
  - The hat's LC resonance drifts with the voice's own envelope, behind a two-stage 4 kHz
    high-pass (owner: the model sounded lower; the recording falls about 18 dB per octave from 4
    to 2 kHz). The recording opens high, dips as the strike passes and rises again as it fades —
    smoothed peaks of 9232, 8480 and 8802 Hz over 0–10, 10–30 and 30–60 ms, by
    `tools/ab_resonance.py` — where one fixed centre gave 8000, 8377 and 7846 Hz. Three terms
    with their own time constants carry it: a base near the recording's body, a strike lift that
    falls away in 5 ms, and a late rise that arrives only once the envelope is well down, as
    `(1 − envelope)²`; the damping opens with the envelope. It renders 9716, 8382 and 8643 Hz.
    One curve fitted to all three windows instead of three terms pinned the body at 9475 and
    9497 Hz, a fifth of an octave high where the hat is loudest after the strike. The
    coefficients are recomputed every 32 samples, which the drift is far slower than.
  - The hat's noise arrives in bursts, amplitude-modulated by the same machine-shared sample: the
    recording's burstiness sits four standard deviations outside what plain noise gives, and the owner heard it as "something periodic ... a
    little more snappy". The bursts are slow. Two 600 Hz poles put the flutter at 150–600 Hz where
    the recording's sits at 60–150, which the owner heard as a "lower frequency modulation" the
    model lacked. Burst noise in a transistor is a two-level random telegraph, so a comparator on
    the smoothed shared sample drives one, flipping about every 14 ms, with a floor pole keeping
    what is left inside the recording's band; a fast wander stays under it, because the telegraph
    alone moved the loudest millisecond off the strike and cost the hit its crest.
    - The peaks inside those windows are the machine-shared sample's own lines. They hold their
      frequencies whatever the centre does, so a single line cannot be placed by tuning.
  - A small 150 Hz–2.5 kHz share of the same noise leaks around the resonant high-pass and falls
    with its own 80 ms T60. The acquired recording keeps that broadband bed through the attack
    where the resonant path is nearly empty; without it the owner heard less low end, and the
    spectrogram was a solid deficit below about 2 kHz. It deliberately excludes the recording's
    50/100 Hz mains hum.
- **Supporting renderer.**
  - Noise colour is a per-voice TPT state-variable high-pass plus band-pass. The Chamberlin
    band-pass it replaces capped every centre near 7.3 kHz.
  - Voices have per-voice attacks, holds and two-stage envelopes.
  - Native sweeps sound at zero on 32, 36, 47–50, 54, 60 and 85, and Pitch envelope −1 removes them.
  - A snare's noise gate follows Noise decay only, which keeps the analysis patch's rest pitch
    invariant.
  - The TR-606 kick and toms carry the VCA's phase-locked switching ticks. The 606 metal and the
    FR-2L cymbal carry the VCA's envelope thump.
  - Metal duty is per oscillator: CR-78 40 %, CR-8000 cowbell pair 47/49 %.
  - The CR-8000, TR-606 and DR-110 banks use this unit's oscillators, fitted to the line spectra of
    their cymbal and open-hat tails, not the service nominals:
    - CR-8000: 311.55 / 574.35 / 780.93 / 902.3 / 917.55 / 1149.3 Hz;
    - TR-606: 401.44 / 439.6 / 480.5 / 553.32 / 679.74 / 971.58 Hz;
    - DR-110: 304.26 / 448.97 / 789.28 / 1114.3 Hz, within 2 % of §5.8's four printed values.
      The review pass's 371.44 and 607.84 Hz were other squares' harmonics (a third of 1114.3 Hz
      and the even harmonics of 304.26 Hz).

    All three are band-limited (PolyBLEP) and start from a fixed phase spread.
  - The CR-8000 hats pass one polarity above a threshold before a 6 kHz high-pass.
  - Per-voice mechanisms at reference:
    - the trigger's falling edge strikes the diode toms and congas again;
    - the congas have an early pitch pull;
    - a flat clip of 34/35's first trough, after the output bend;
    - a damping ramp on 45 (2.38 kHz settling to 2246 Hz);
    - a VCA threshold ends the 44 clap.
  - 40 is rebuilt as a smooth ring of about 1.3 kHz, struck again at 2.29, 2.9 and 4.02 ms and then
    by a jittered 2.1 ms train until 25 ms.
  - The 68 bongo keeps its 2.5 kHz partial: it is the voice's own, starting with the strike and
    decaying with the body.
  - Fitted per-voice tables in `legacy.rs`, each applying at reference:
    - `output_sign` inverts 62, 70 and 79. The research is silent on output polarity, so the
      recordings decide.
    - `trigger_pulse` feeds the trigger into tonal outputs (34, 35, 49, 60, 69).
    - `third_mode` and `mode_phase` give up to three resonators, each with its own start phase (90).
    - `wire_delay` holds off snare wires (61, 81, 91).
    - `late_ring` changes a ring's decay partway through (40).
    - `body_harmonics` adds a circuit's own harmonic series on the ring's phase, each harmonic a
      fixed share of the ring (Chebyshev polynomials of the ring normalised by the amplitude its
      quadrature pair gives, so the series is exact and band-limited): 33, 45 and 54 carry it at
      every level, and the diode toms 34–36 only while the ring stands above their diodes'
      conduction, where the recordings' H2–H8 hold their level and then fall 20–30 dB within
      20 ms. A polynomial bend cannot do that: its series follows the ring down.
    - `metal_noise` mixes noise into a metal bank before its filters and adds a high-passed top
      beside the band (CR-8000 41, TR-606 51, DR-110 56–58); its fourth entry chooses the top's
      order, one pole (DR-110) or two (the 606 cymbal, whose band sits an octave under the 6 kHz
      corner, where a one-pole skirt fills the band below it: +11…+14 dB from 0.8 to 2.5 kHz).
      The one-pole top owns `top_hp`, the two-pole one `filters[2]`; neither may borrow `aux_lp`,
      which a mixed-noise low-pass and the CR-8000 hats' third order already use.
      The recordings are noise-like
      where the bank alone is line-like: spectral flatness −3…−8 dB against −13…−18 dB, and the
      CR-8000 cymbal's lines stood 80 dB over the noise beside them against the recording's 30.
      **The balance between a metal voice's lines and the noise around them is what the owner
      hears**: fit each voice's line prominence (the peak over the median of its ±8 %
      neighbourhood) against its recording — 41 and 51 stood too pure, 56 too buried.
    - **Higher and lower, in the owner's notes, are the perceived tone, not brightness** (owner,
      2026-09-20). What decides it is **which line leads** — its position and its level over the
      rest — so fit the line table, not the centroid. A voice can measure brighter and still be
      heard as lower. In a noise voice there is no oscillator to tune and the tone is the
      resonance the noise is shaped by, so fit that resonance's centre **and its width**: a broad
      one reads as a colour, a narrow one as a pitch.
    - `resonance_drift` moves a voice's band-pass colour and its `noise_resonance` as its own
      envelope falls, recomputed every 64 samples (`Voice::drift`), from the peak the hit reached:
      `[centre × at the strike, centre × when it has gone, Q × at the strike, Q × when it has gone,
      curve, time constant s]`. A zero time constant rides the envelope's fall; a voice whose
      envelope holds flat gives that nothing to ride — the tambourine holds 200 ms and has lost 5 %
      by 45 ms, so its band stayed at the strike's tuning through the whole window the owner was
      listening to — and drifts on its own clock instead. **A hardware resonance is narrow at the strike and climbs through the tail**,
      because the lower modes die first; one fixed band cannot follow a recording that climbs while
      it decays. Measured per voice over 0–10 / 10–30 / 30–60 / 60–120 / 120–250 / 250–500 ms, as
      the energy-weighted centre of the voice's own band: the CR-78 cymbal's recording climbs
      8234 → 8725 Hz where ours sat flat at 8270 → 8243, the CR-78 tambourine's 4551 → 4698 where
      ours sat at 4677 → 4635. A drift costs measured width — a band that sweeps inside a window
      reads broader — so a voice whose strike must stay narrow takes a slow curve, not a fast one.
      41 measures the same fault in the other direction (its recording falls 6463 → 6179 Hz and
      sits 480 Hz over ours at every window) and is left bit-identical because the owner has
      approved it; one row would fit it.
    - `body_modulation` is a voice's own measured amplitude modulation, `[rate Hz, depth]`, on its
      output. Only the TR-606 cymbal carries one: 24.5 Hz at 4.5 % of the mean from 20 ms and 8–9 %
      through the tail, coherent across its 4–10 kHz envelope where the model's carried only the
      incoherent 130–270 Hz beating of its own bank. Measured on the acquired recording's envelope,
      not copied from it, the way the metal banks' frequencies are.
    - `metal_peak` adds the output network's own resonance to a metal voice, where the recording
      leads on one line the band alone does not reach (41's 5.75 kHz and 51's 6.80 kHz). It owns
      a dedicated filter because 51 already uses the old colour slot for its two-pole top.
    - `metal_floor` puts a noise floor beside the band, with its own decay and colour where the
      recording's low content falls faster than its band (TR-606 52/53) and riding the voice
      elsewhere (51, DR-110 57/58).
    - `metal_edge` passes the trigger's own click into a metal voice's output (TR-606 52/53).
    - `metal_darkening` closes a one-pole low-pass as a metal voice falls, level-compensated: the
      CR-8000 cymbal's recording holds its 6.1 kHz peak while its centroid falls from 8.3 to
      5.9 kHz, which one fixed band cannot do.
    - `attack_noise` opens a noise voice with a burst of wider, lower noise (63, 64), or, with a
      zero T60, rides the voice's envelope as a low floor under the whole hit (82, 92).
      **Narrowing a band takes the top with it**, and this owner hears missing top as "more
      harmonics" in the recording, so a voice that narrows pays it back somewhere.
    - **A noise voice's high-pass corner is its body, not a colour.** The CR-78 cymbal sat behind
      a resonant 7 kHz corner that cut its own 1.6–3.2 kHz noise: that octave measured 11–16 dB
      light in every window while 6.4 kHz and up matched within 1.6 dB (owner: "original sounds
      like it has a bit more low band noise"). Opening it to 4 kHz at Q 0.707 fills the octave and
      drops the voice's third-octave distance from 5.2 to 2.4 — a band-pass added at 1.6 kHz
      cannot, because its skirt lands in the 800 Hz octave, where both measures already show us
      heavy.
    - `noise_resonance` puts a narrow resonance beside a noise voice's band: 71's 4.7 kHz whistle
      and 64's 8.87 kHz ring, both of which the owner hears as a clear metal pitch. Each opens
      with the voice.
    - `cowbell_direct_weights` mixes a pair's direct path separately from its band-passed one (46).
    - `clap_voicing` can give a tail its own two-slope decay from its opening (59).
    - `hat_output` adds the CR-8000 hats' output resonance at 6.2 kHz after their rectifier.
  - The 40 strike train keeps a fixed fade that ignores Decay. It ends by about 25 ms, before the
    rest-pitch analysis window; a longer train moved the rest pitch by up to 36 cents.
  - 58 is gated: it holds 0.82 s, then releases over 40 ms.
  - 91's body decay stays at 25 ms or more; shorter, the rest pitch sinks under the thump.
  - 61's body rests at 302 Hz and starts about 5 % above it: its recording's strongest partial is
    316 Hz in the first 10 ms and 306 by 30 ms. The 148/198/253 Hz partials its recording keeps
    after 60 ms are the 50 Hz hum series, not the voice.
  - 59's tail decays from its own opening in two slopes, and opens as loud as its bursts.
  - Each voice's band sits where its recording's leading line is: 41 leads on 5746 Hz and falls
    away upward, 53 on 6802 Hz with 8–9.5 kHz 9 dB under it, 64 on 8.42 kHz, 71 on 4.65 kHz, 82 on
    7.8 kHz and 92 on 11.1 kHz beside a second line at 7.95 kHz.
  - **The three TR-606 metal voices sit on nearly one centre and must stay apart in width and
    time, not in tuning**: the recordings put the cymbal at 7.20–7.23 kHz and both hats at
    7.21–7.28, so the models do too (51 at 7.3 kHz Q 7, the hats at 7.25 Q 4). What separates them
    is the cymbal's Q, its 3.44 kHz low band, its 24.5 Hz ripple and its seconds-long tail against
    the hats' Q 4, their 500 Hz floor, their click and their 50 ms and 450 ms decays.
  - 82's gate is forty-five milliseconds of hold and a 0.12 s decay, not the 0.065 s it carried:
    its recording holds −15.4 dB at 30–60 ms and −34.8 dB at 60–120 ms where the model had fallen
    to −18.3 and −46.9. A VCA conduction threshold then closes the exponential between about 80
    and 95 ms: shortening the decay instead made the 30–60 ms body too quiet, while leaving it
    open carried the model to 150 ms against the recording's 90 ms. Its label still names the
    hold, which is what the ear hears as the gate. Its low-floor share is 0.0007 rather than 0.004;
    the larger value left 0.4–3.2 kHz 14–16 dB heavy while its 6.4 kHz band already matched.
  - **The TR-606's two hats share one band, one noise mix, one floor colour, one trigger click,
    one high-passed top and one strike narrowing**, because the hardware gives them one audio path
    through a common VCA and resonant high-pass (§5.7): the 7.25 kHz Q 4 resonance over a 4.5 kHz
    cut, a noise mix low-passed at 4 kHz and the two-pole top. Each keeps its own envelope, attack,
    level and floor time (owner, 2026-09-20: "give the 606 closed hat the same as the open").
    The band sits at 7.25 kHz, not the 6.85 the A/B pass had chosen: both recordings are anchored
    at 7210–7283 Hz in every window from 10 ms, where the shared band rendered 6328–6580, one
    error of 680–995 Hz that the owner heard on the closed hat ("resonant frequency higher on the
    original").
  - **The cymbal (51) takes the hats' resonant high-pass**, since §5.7 gives it the same one, over
    its own 3.44 kHz band, and adds the two-pole high-passed top: a Q 4 band alone leaves its
    recording's 10–16 kHz 5–18 dB short. Its band sits at 7.3 kHz Q 7, not the hats' 6.85 Q 4: the
    recording's smoothed resonance is anchored at 7.20–7.27 kHz through its whole life, and a band
    on its 6802 Hz leading line alone pulled ours to 6.1–6.5 kHz through the body. A dedicated
    Q 30 output-network peak therefore lifts the existing 6.80 kHz bank line without moving the
    fitted broad band; before it, 7.2 and 7.75 kHz led the model's first 60 ms. Its low band is
    held at 0.06 because the narrower high band lifts everything under it: at 0.09 its 3.4 kHz line
    re-enters the tail's leading six, which the recording's does not. Its noise floor is low-passed
    at 2.5 kHz: white noise under the whole hit widened the strike and put the measured peak
    anywhere from 6.4 to 8.2 kHz.
  - **Roughness is energy concentration, not the fitted modulation.** The owner heard "a grrr
    sound" on the cymbal where "the original is cleaner". Measured as amplitude-modulation depth
    per band over 15–300 Hz, our 500 Hz–16 kHz envelope carried 76 % against the recording's 36 %,
    with its peaks at 33 and 270–284 Hz — the bank's own beats. Removing `body_modulation`
    entirely changed that by 1.3 points, so the fitted 24.5 Hz ripple is not what is heard; the
    recording is smoother because its energy is spread across bands that fluctuate independently,
    while ours sits in one Q 7 band. Spreading energy above the band with the high-passed top
    is the lever that moves it (76 % → 67 % at the level the recording's own 12–16 kHz allows,
    and → 49 % at a level that overshoots it), and more in-band noise makes it worse, not better,
    because narrowband noise is what a slow deep envelope is made of.
  - **A metal voice's first 60 ms depends on the bank's phase at the trigger**, because the bank
    free-runs and the hit takes whatever beat it lands in. Measured across seven trigger phases,
    51's smoothed peak moved 6361–8207 Hz in 0–10 ms and 6719–7237 Hz in 10–30 ms with one fixed
    band, so a single render says nothing about an early window: **measure a metal voice over
    several phases and fit the mean**. The cure for a swing that large is a band tight enough to
    pin the peak whatever the lines do — Q 4 → Q 7 took 51's strike spread from ±612 to ±137 Hz
    and its 60–120 ms spread from ±120 to ±5 Hz — not more noise, which flattens the spectrum
    without holding the peak.
  - The DR-110's metal filters are its §5.8 bands, 3.45 and 7.1 kHz, not the 8.5 kHz the A/B pass
    had chosen.
  - No ring may tick after its onset: `smooth_rings_do_not_tick_after_their_onset` bounds the
    prominence of an isolated step in the third difference of sixteen smooth voices, including the
    whole FR-2L family, at 12 dB over the median of the 25 ms around it. The voices with a fitted
    tick, train or restrike (34, 35, 40, 47–50) are out of its scope. **The measure needs its
    absolute gate**: an otherwise smooth ring's third difference sits at the f32 rounding floor, so
    a purely relative bound reads 17–20 dB on a clean 51 Hz sine and means nothing.
  - **Open, for the owner:** the CR-8000 service table puts the high conga (39) at 690 Hz with a
    15 ms decay, and puts 37's recorded 299 Hz in the mid-conga range. The recording mapped to 37
    may therefore be the mid conga.
  - **Open, for the owner:** the FR-2L kick (85) has no click the measurements can find. Its
    reference render is smooth sample by sample after the onset; its events sit at −105 dB and its
    whole band above 3 kHz at −120 dB, where the recording's own noise floor is −75 dB. The
    recording's 100/150/200 Hz partials, which look like the harmonics the model lacks, are the
    50 Hz hum series. The listening example fades each hit's last 5 ms, where the catalogue's
    0.65 s window cut the kick 45 dB down.
  - **Open, for the owner:** the TR-606 cymbal's recording pulses at about 24 Hz, which is the
    bank's own beating — its 7.2 kHz region holds five lines 20–40 dB apart where the six squares'
    harmonics give three, and a fine refit puts three lines within 8 Hz of 480 Hz, the signature of
    a supply-ripple frequency modulation the model's fixed squares do not have. Reproducing the
    rate needs a frequency-counter measurement of that unit's oscillators.
- **Recording-chain artefacts are not copied:** 50 Hz hum series (CR-78, FR-2L), pack noise floors,
  truncation, pre-roll.
- **Low conga:** 37 follows its recording to 299.4 Hz (owner, 2026-09-19: "Pitch is much lower in
  model"). Its unrecorded siblings 38/39 scale by the same factor, to 436.3 / 633.0 Hz, a labelled
  inference that keeps the circuit's intervals.

Listening was deferred until all 94 models rendered, by owner direction; it did not pause
implementation.

### TR-909 PCM IDs 24–27: the NMF-resynthesis ruling

TR-909 PCM IDs 24–27 use the owner's 2026-09-18 **NMF-resynthesis ruling**. Three fixed 44.1 kHz
sources serve four articulations: both hats restart one gently phase-rotated coherent KL soft-mask
source, with separate open and closed address/envelope paths; crash and ride each use one source made
by the same method. Closed uses a measured 13,920-frame gain table and the owner-selected light 0.03
deterministic white residual under that same envelope; neither is a second PCM source.
`assets/reset-909-nmf/README.md` records the offline method, closed articulation, representation and
checksums. Zero controls play the resynthesized sources without applying the
former procedural source's DAC and reconstruction path a second time. A 256-phase, sixteen-tap
windowed-sinc table preserves the top octave across host sample rates; the playback clock still
couples Pitch and duration. The hats no longer choke one another on their own: that is a choke group
the user assigns, per the ruling above. Owner listening accepted the closed
NMF trials as targets, found the selected shared-hat source indistinguishable from the open reference
apart from one uncertain tonal impression, judged the selected crash exactly the same as its
comparison, and selected the light full-envelope residual for closed hat because it retained the
shared open-hat identity while bringing the comparison closer. Ride remains open.

### IDs 28–94: the supporting catalogue

IDs 28–94 complete the admitted supporting catalogue. DR-55 has its own transistor-noise source;
CR-8000, TR-606, DR-110, CR-78, TR-66 and the early transistor family each own a separate
continuously advancing noise/metal source frame. `legacy.rs` centralises only repeated electrical
primitives: every ID has an explicit topology, tuning, decay, capabilities and machine source.
The post-build comparative pass measured isolated renders against the allowed private folders,
installed Bitwig content and public-domain CR-78 one-shots; Ableton's `able` AIFC codec was
unreadable and contributed no evidence. It calibrated available T60s, 909 tom centres and the
project-authored PCM spectrum/tails, but does not verify hardware fidelity; the comparison fit
above supersedes its analogue calibrations. Every available model is
covered by two catalogue-wide capability tests: unsupported axes are bit-exact no-ops and supported
axes change audio, including the separate pitch-depth, pitch-time and noise-time capability rows. ID 53 alone reads host tempo; slower tempo lengthens its open-hat envelope.

### The 2026-09-18 acquired-recording pass

The owner directed the 2026-09-18 acquired-recording pass to **make do with the evidence available**
(its analogue numbers are superseded by the comparison fit where they differ):
missing serials and chains do not block practical correction, but they do keep fidelity UNVERIFIED.
`research:instruments/analogue-drum-machines.md` §11.2 owns the ruler and measured conclusions. The
pass corrected the TR-808 service “decay” values from mislabeled T60s to their observed −20 dB
meaning, fitted its clap/cowbell/cymbal/hat curves and spectral balances, moved the 909 tom centres
to the acquired centre-setting tracks, restored bright 909 PCM metal, and replaced the supporting
renderer’s one 2.5 kHz noise colour with per-voice regions. Supporting cymbals/hats consume their
machine-shared bank at zero Pitch; evidenced cowbells select their oscillator pair from that same
shared frame. A nonzero creative Pitch deviation alone takes the slot-private path. The confirmed
FR-2L comparison moves the early kick/clave references to 48/355 Hz. That pass itself moved no
recording bytes; the later NMF-resynthesis ruling above deliberately supersedes the PCM family's
procedural-source contract.
