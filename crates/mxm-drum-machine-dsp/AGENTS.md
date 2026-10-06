# AGENTS.md — crates/mxm-drum-machine-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The framework-free DSP and permanent model catalogue for `mxm-drum-machine`: sixteen retriggerable
slots, machine-shared source state, circuit families, deterministic event dispatch and bounded
main-plus-individual rendering. The product contract is [`../../docs/briefs/mxm-drum-machine.md`](../../docs/briefs/mxm-drum-machine.md);
the implementation sequence is `plans/plan-mxm-drum-machine.md` (in the private archive).

The reasoning, measurements and fitting history behind every rule below are in
[NOTES.md](NOTES.md); read the linked section before changing what a rule governs.

# Ownership

Owns `Cargo.toml`, `src/`, `tests/`, crate-local examples and `tools/`, the Python half of the
hardware-comparison site (`ab_plot.py`, `ab_metrics.py`, `ab_residuals.py`, `ab_review.py`,
`ab_resonance.py`, `ab_serve.py`; numpy and Pillow, never run by a gate). It owns stable model IDs, public model labels, per-model capabilities and the
plain-value structures that the plugin maps from parameters.

It does not own nice-plug parameters, smoothing, editor state, preset files, content acquisition or
CLAP event translation. Those belong to `plugins/mxm-drum-machine`. It does not own research
material. It owns revision 32's three fixed NMF-resynthesized PCM sources for four articulations and
their technical provenance record, revision 33's reference-centred envelope-depth/time laws and
bounded extended-decay ranges, and D7's per-slot output frame and destination transfers.

# Local Contracts

## Catalogue and identity ([NOTES.md § Sixteen slots](NOTES.md#sixteen-slots-a-larger-append-only-catalogue))

- `SLOT_COUNT` is 16; notes 36–51 map one-to-one to slots. A slot is one retriggerable circuit, not
  a polyphony bucket.
- **Model IDs are permanent**: 0 is legacy silence, the domain is 0…255, assigned IDs never move or
  change meaning, a new model takes the next unused ID, and unassigned IDs resolve to silence.
- `model::AVAILABLE_MODELS` lists only circuits that render; ID 0 stays outside it (the plugin's
  Mute owns silence) and a reservation is never offered as a silent model.
- Labels never contain manufacturer or hardware model names. Selector group headings are display
  metadata: changing one never changes identity or order.

## Reference is zero ([NOTES.md](NOTES.md#reference-is-zero))

- Every creative control reaches the documented source behaviour at zero. Pitch is
  family-specific: never one universal pitch shifter or final-envelope abstraction.
- `pitch_envelope`, `pitch_decay` and `noise_decay` are additive: zero keeps the native excursion and
  timing, and an unswept circuit stays unswept. Positive Decay reaches bounded extended decay where
  the topology permits; never fake it by looping fixed PCM.
- Unsupported axes are exact no-ops, kept in the plugin's fixed inventory.
- The static output-adaptation plane is a fixed, offline-measured trim per model in `engine.rs`, not
  a limiter or AGC. A renderer gain change remeasures the whole table; never hand-correct one trim.
- `ModelId::reference_pitch_hz` is the measured **rest** pitch; `chromatic_reference_key` is concert
  pitch for a pitched model, note 60 otherwise. Changing a pitched model's tone or pitch law means
  re-running `print_the_measured_reference_pitches` and updating the table
  ([§ rest pitch](NOTES.md#a-pitched-models-note-is-its-measured-rest-pitch)).

## Routing ([§ outputs](NOTES.md#main-and-individual-output-routing), [§ modulation](NOTES.md#per-slot-modulation-routing-kit-wide-lfos))

- Each slot sends its pre-Pan mono signal to Main or one of sixteen mono auxiliaries through
  `mxm_part_routing::DestinationRouter`; Pan applies only to a main share. All-main is
  sample-identical to the pre-D7 stereo path.
- A destination edit on a slot that carried audio at its last sample transfers over 2 ms as a
  linear partition summing to one (latest-wins); any other slot switches at once. `route_outputs`
  runs before an event group's triggers. Reset, choke and panic never change assignments.
- Each slot owns its own 13-target × 7-source topology and `SourceFrame`; a route never crosses
  slots. Three kit-wide LFOs run once per sample and always free-run. A newly needed source is
  cleared before its first read; an absent route keeps its amount.
- Every route is the collection's standard: Amplitude (`level`) is the standard factor, Velocity is
  `v − 1`. `conformance.rs`'s `Declared` runs the checks; the plugin reuses it (`conformance`).
  Randomness is seeded; the plugin resolves LFO rates and tempo sync.

## Shared sources ([§ shared state](NOTES.md#shared-source-state-is-load-bearing), [§ jump.rs](NOTES.md#gating-a-free-running-bus--jumprs-and-fixed-point-phase))

- **Shared buses are load-bearing**: consumers at zero read one machine-family sample. Never give a
  slot a private, statistically similar generator. A nonzero metallic Pitch may take a private bank;
  zero rejoins shared phase without resetting it. A choke never clears the machine's bank.
- A bus is gated on `Engine::prepare`'s `*_has_reader` flags (any selected or retiring model) and
  jumps **exactly** to where running would have left it (`jump.rs`, fixed-point `u64` phase).
  **Reseeding a dormant source is not allowed.** Gating on *sounding* is not done.
- Integer phase tables are computed by `fixed()`, never typed. `advancing_lands_exactly_where_ticking_does`,
  `a_jump_equals_literal_stepping`, the shared-sample correlation tests and the reference bank stay
  bit-identical through any bus change.

## Velocity, choke and transitions ([NOTES.md § Velocity, choke and transitions](NOTES.md#velocity-choke-and-transitions))

- **No accent** (owner, 2026-09-20): a hit is its own velocity, no slot's level depends on another's
  (`a_hit_is_its_own_velocity_whatever_else_is_struck`). Dynamics is the velocity curve's exponent,
  one law on every family (`velocity.rs`, `dynamics_bends_every_familys_velocity_curve_the_same_way`).
- **Choke is a user-assigned `choke_group`** (0 none, 1…`CHOKE_GROUP_MAX`); nothing chokes unless
  assigned, and circuits own no choke. Slots triggered on the same sample are exempt.
- A model change or choke on an audible slot fades the old circuit (`rendering`) over
  `MODEL_RETIRE_SECONDS` while `model` is authoritative for triggers; a second change only moves the
  selection; a trigger ends the fade; `Slot::reset` syncs `rendering`; `peak` carries across.
- Metallic tuning moves over `METAL_TRANSITION_SECONDS`, exact at both ends. A filter a voice gains
  gets a line in `reset` in the same change.
- Both durations are chosen and owe a click/latency measurement. Transition tests measure the worst
  step over the whole window, never one sample or `worst_step` alone ([§ fade](NOTES.md#replacing-or-choking-a-sounding-circuit-is-a-bounded-fade)).

## Capture ([NOTES.md § Capture](NOTES.md#capture-and-frozen-playback--capturers))

- `capture.rs` only renders and reads back; writing a pack is the plugin's. A capture equals the
  slot's isolated live render (`a_capture_equals_the_slots_isolated_live_render`), all sixteen from
  one pass, in a fixed context (`CAPTURE_VELOCITY`, `CAPTURE_TEMPO_BPM`, routing off, reset origin).
- `capture_patch` neutralises Level (so Mute and Solo too); Pitch is captured at reference, Decay
  baked in (`SlotCapture::decay`). Buffers are trimmed to their tails; reaching
  `MAX_CAPTURE_SECONDS` truncates with a fade. `CaptureError` never yields anything partial. The
  resumable form renders identically; an `Engine` on this path is boxed
  (`a_capture_fits_a_one_megabyte_stack`).
- `CaptureVoice`: pitch is a playback rate and Decay only shortens; `READERS_PER_SLOT` readers,
  stealing the oldest. The off-unity interpolator's cost is owed a measurement.

## Events, dependencies and realtime ([NOTES.md § Events](NOTES.md#events-dependencies-and-realtime-rules))

- Same-offset events resolve in the brief's order; host iteration order never changes the result.
  CC120 clears every circuit, bus and pending event; CC123 releases expression ownership without
  truncating tails. Seeds are fixed; no OS entropy anywhere.
- Runtime dependencies are only `mxm-modulation` and `mxm-part-routing`; no `nice_plug` or GUI
  types; plain-value APIs. `mxm-measure`, `mxm-audio-file`, `mxm-listening` and `serde_json` are
  dev-only and never enter the shipped graph.
- No allocation, locks, logging, formatting, file/network I/O or blocking on render paths. Fixed
  arrays own all state; no per-trigger construction. `f32` carries audio; `f64` computes recursive
  coefficients and setup where error compounds.
- A struck pole pair uses the `sin(ω)` numerator in `Resonator::process_strike`; the Bridge 808
  bodies use `deep_bridge_kick.rs`'s coupled-form resonator. The emergency bound is never an
  implicit waveshaper.
- Hold a coefficient that does not change every sample on an always-on path (rate-only at
  activation, rate-and-model memoised on `(id, sample_rate)`, a tuning behind a `NaN` sentinel). Do
  not add memo state to a voice on that rule alone: measure first, with voices running.
- Denormals flush; non-finite state recovers deterministically; bounds hold at every accepted
  rate; saturators are bounded in `f32`; reset leaves exact silence.

## Evidence and fitting ([NOTES.md § Evidence and warts](NOTES.md#evidence-and-warts))

- Fidelity is **UNVERIFIED**: a comparison fit is recognisability against one recording chain. All
  94 models are owner listening-approved (2026-09-20). Cite research by `research:` reference,
  never a private path; every chosen constant says so and names the measurement that could replace it.
- Machine couplings (shared phase, the TR-606 tempo-dependent open hat — ID 53 alone reads host
  tempo — reset-VCO strike phase, PCM pitch/duration) are deliberate: label them in code and tests.
  Accent buses and hardwired hat chokes are deliberately absent: do not restore them.
- Recording-chain artefacts (hum, noise floors, truncation, pre-roll) are never copied. Fit a metal
  voice by which line leads and its prominence, over several trigger phases
  ([§ comparison fit](NOTES.md#the-2026-09-19-comparison-fit)); `smooth_rings_do_not_tick_after_their_onset`
  keeps its absolute gate.
- Open for the owner: the CR-8000 conga mapping, the FR-2L kick's click, the TR-606 cymbal's ripple
  and the 909 Ride.

# Work Guidance

- Read `research:instruments/analogue-drum-machines.md` and its cited primary source before adding or
  changing a circuit. Do not infer an exact transfer function from a block label.
- Fit a model against its hardware recording with [`../../docs/drum-model-fitting.md`](https://github.com/mxm-audio/newdawn-workspace/blob/main/docs/drum-model-fitting.md):
  the `drum_machine_ab_page` example and `tools/` build a local comparison site of waveforms,
  spectrograms and metrics. The site, its mapping and the recordings never enter the repository.
  Once a model is close, fit it by the profile `tools/ab_resonance.py` reports rather than by any
  single number, and read that page's *Fine-tuning* section before changing a fitted constant an
  owner has already approved.
- **Run the listener first.** `listen site <site>` (mxm-tools' `crates/mxm-listening`) writes what differs on
  every row of the comparison site, ranked by audibility and in the owner's words; read it before
  asking the owner what a sound does, and before a page goes to them.
- Implement the priority TR-808 and TR-909 families before supporting machines.
- Keep honest per-machine implementations local. Extract within this crate only after two completed
  circuits demonstrate the same API and preserve their distinct constants/couplings.
- Name nontrivial techniques and papers in code. Record rejected alternatives with measured results.
- Do not claim model fidelity from compilation or synthetic invariants.

# Verification

```bash
cargo test -p mxm-drum-machine-dsp
cargo clippy -p mxm-drum-machine-dsp --all-targets
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_deep_bridge_kick
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_twin_mode_snare
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_808_resonator_batch
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_808_noise_batch
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_808_metal_batch
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_909_reset_pair
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_909_reset_toms
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_909_analogue_pair
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_909_pcm_family
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_supporting_catalogue
# Render cost against the shared-bus floor. Wall-clock, so compare scenes within one run and
# re-measure both sides of a change; it asserts nothing and gates nothing.
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_cpu_cost
cargo run -p mxm-drum-machine-dsp --release --example drum_machine_reference_bank
```

Tests must cover catalogue identity/labels/capabilities; slot-note mapping; invalid IDs; deterministic
reset/retrigger/event order; shared-source phase correlation; exact idle and reset; finite bounded
output; sample-rate sweeps; topology-specific pitch laws; zero-reference preservation; every declared
supported axis changing sound; every unsupported axis remaining an exact audio/activity no-op; and
each model's measured calibration once implemented.

# Child DOX Index

No child AGENTS.md files. This document covers the crate.
