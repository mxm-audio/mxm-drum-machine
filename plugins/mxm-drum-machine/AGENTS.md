# AGENTS.md — plugins/mxm-drum-machine

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug CLAP shell and editor for `mxm-drum-machine`: sixteen MIDI-addressed slots selecting
from an append-only pool of machine-specific drum circuits. All 94 admitted models render in the
permanent framework and are owner listening-approved as of 2026-09-20. New circuits may append only
as complete, evidence-backed and individually auditable rows.

The measurements, history and reasoning behind every rule below are in [NOTES.md](NOTES.md); read
the linked section before changing what a rule governs.

# Ownership

Owns `Cargo.toml`, `README.md`, `control-map.json`, `src/`, `presets/`, `tests/` and `host-tests/`;
the licence is the repository's `LICENSE`. It owns
CLAP/parameter identity, parameter smoothing, host event translation, telemetry and editor.
DSP and model behavior belong to [`../../crates/mxm-drum-machine-dsp/AGENTS.md`](../../crates/mxm-drum-machine-dsp/AGENTS.md).

# Local Contracts

## Identity, outputs and MIDI ([NOTES.md](NOTES.md#identity-and-current-implementation-status))

- Product name: `mxm-drum-machine`; permanent CLAP id `dk.mxm.mxm-drum-machine`.
- No audio input. Configuration 0 is stereo main `L+R` plus sixteen mono outputs `Slot 01`…`Slot 16`;
  configuration 1 is stereo compatibility. Main is port 0 in both; auxiliary IDs stay 1…16 in
  declaration order. A name is not identity, and port names carry no model name.
- **Hosts do not report which outputs are patched**: switching-jack normalling (a patched output
  leaving `L+R`) is impossible and must not be attempted.
- Each slot's MIDI channel defaults to `Kit` (notes 36–51 on every unclaimed channel); channel
  1…16 makes the slot chromatic and claims the channel. A pitched model plays at concert pitch via
  `ModelId::chromatic_reference_key`, a noise voice around note 60. Kit slots ignore notes on a
  claimed channel; slots may share a chromatic channel and layer.
- A hit's owner has two lifetimes: NoteOff and CC123 end its per-note expression but leave the
  tail; NoteChoke matched against the owner terminates the tail. CC120 is global panic and CC123
  releases expression globally, regardless of claims.
- The selector offers every available model. ID 0 stays a legacy silence value for old state but is
  not offered; IDs 95…255 are safe unavailable values. Init is the brief's sixteen-slot mixed
  assignment, distinct from the nine audition presets.

## Drums ([NOTES.md § Implemented drums](NOTES.md#implemented-drums-deliberate-behavior))

- **Quick repeated strikes continue from live state: never reset phase/state on NoteOn.**
- A route reaches what the collection's modulation standard reaches (the DSP's `FULL_SCALE`,
  held by its `conformance`). One amount serves every target, so the host reads it as a percentage
  of its target's reach, and `routing_checks`' pair-by-pair readings do not apply: a **recorded
  deviation** (2026-10-07), as model-drums' (`a_route_amount_travels_what_every_pair_is_offered`).
- **A hit is its own velocity**: no accent bus or control. Notes landing on one slot at one sample
  reduce to one strike at the **winning owner's own** velocity (`ArbitrationResult::owner_strike`).
- Model behaviour, shared sources and calibration belong to the DSP crate; fidelity remains
  hardware-unverified.

## Parameter surface ([NOTES.md § Parameters](NOTES.md#parameter-and-routing-surface))

- **mxm-model-drums' scheme: the host holds what the panel can show at once** (the owner,
  2026-09-30, for this machine; done 2026-10-07). A slot is `model`, twenty general controls
  `c01`…`c20` (host name `Control k`), `level`, `pan`, `mute`, `solo`, `choke_group`, `output`,
  `midi_channel` and four route slots `route<r>_{source,target,amount}`; kit-wide are `master`,
  `lfo{1,2,3}_{rate,shape,sync}` and `resample` (`params::slot_ids`, `GLOBAL_IDS`;
  `the_ids_are_complete_unique_and_651`). Nested arrays add `_<slot>`, one-based.
- **The controls are the named ones of before, mechanically** (the owner, 2026-10-07): `params::control`
  holds which is which — model-drums' common seven first (Tune, Decay, Tone, Attack, the velocity
  response, the pitch drop, its time), then Body, Noise, Noise decay, Character. Each keeps its
  range, default, unit, smoothing and path into the DSP; every kit sounds bit-identical
  ([NOTES.md § the move](NOTES.md#the-move-to-general-controls-and-route-slots-2026-10-07)).
  Controls 12–20 mean nothing on any model: never read, never drawn.
- **A model shows only the controls its code reads, under its own names** (the owner, 2026-10-07:
  honest controls; decided from the code, never from renders). `editor::controls` is the table —
  each model's knobs, names and help, a control that waits on another saying so — held to the DSP's
  `ModelId::capabilities` (`each_model_shows_exactly_the_controls_its_code_reads`). A general name
  stays where honest; one that misleads names what the control does (*Soft hits*, *Gain*, *Snappy
  decay*, …). The table and the reasons: [NOTES.md § Each model's controls](NOTES.md#each-models-controls-2026-10-07).
- **IDs are free to change during pre-alpha** (the owner, 2026-10-07), a recorded deviation from
  `../AGENTS.md`'s *Permanent identifiers* until the first release; nothing migrates old IDs ("There
  are no saved projects - we are in pre alpha", 2026-09-30). Changing Model never changes the host
  parameter inventory; every default is zero, the reference sound.
- Output is stepped `L+R`, `1`…`16`; MIDI channel `Kit`, `Ch 1`…`Ch 16`; Choke group `Off`, `1`…`16`.
- **Choke group is a kit setting** (presets and Init carry it): Init and the audition kits `Off`,
  the creative kits grouped as a real kit (the owner, 2026-10-07).
  **Output, MIDI channel and `resample` are instance settings**, excluded from preset capture, apply,
  Init, completeness, identity baseline and dirty comparison through
  `mxm-preset::Instrument::is_instance_setting`. `master` is global.
- Mute and Solo are post-circuit: circuits keep running; with any Solo on, only unmuted soloed slots
  sound, so Mute wins.
- Model stores 0…255; the editor maps the available catalogue onto the grouped `mxm-ui` caret
  selector. An unavailable ID displays `Unavailable N` and renders silence.
- Exactly three kit LFOs, never per slot; `sync` is one button snapping Rate to a division; no valid
  tempo means the free Rate.
- **Four route slots a drum**: source `Off`, LFO 1–3, Wheel, Pressure, Velocity, Random; target
  `Off`, `Control 1`…`20`, `Level`, `Pan` (`routes::RouteTarget`). A route is in use when neither is
  Off; switching its source off keeps its target and amount; two on one source and target add; one
  aimed at a control the model does not read (12–20 on every model) does nothing.
  `Routes::routing_from` fills the DSP's unchanged per-slot grid; an in-use route at settled zero
  leaves its per-sample list. Controls, routes and the shell's
  DSP engine are boxed, so no host thread's stack overflows.
- The 3,227-parameter surface's IDs (`pitch_N`…, `route_<target>_<source>_{on,amount}_N`) and the
  older `lfo{1,2,3}_division` and unsuffixed route IDs are gone; their history is in
  [NOTES.md](NOTES.md#the-surface-before-2026-10-07).

## Interface ([NOTES.md § Interface](NOTES.md#interface))

- The editor implements [`../../docs/briefs/mxm-drum-machine.md`](../../docs/briefs/mxm-drum-machine.md).
  No standalone Routes card: each knob's routes are rows under it, over the selected slot's four
  route slots, as model-drums' editor draws them (`route_stack`): `‹ modulate ›` offers the sources
  not on it while a slot is free, adding takes a free slot (one aimed at that knob first) in one
  bracketed gesture, removing switches the source off. **Only a shown knob offers sources**; a route
  already aimed at a control the model does not show is drawn under `<name> (unused)`, removable,
  never offered more. Each LFO row is Rate, Sync, then six **drawn** shapes.
- Output is the last card: Level/Pan, Output, Choke group, MIDI channel, then **All slots**
  (`binding::set_together`), which is not a parameter and stores nothing.
- **Kit-wide controls sit in the app bar**: Master, then Resample and Export samples, sized from the
  widest label (`resample_pair_width`) and the last thing a narrow bar folds into `…`
  (`resample_menu_items`). The button reads "Export samples", never "Export pack".
- **The panel names a control by its model's name, the host by its number** (`Axis::name`); route
  rows read their knob's name (Level, not *Amplitude*). An Off or unavailable id keeps the eleven
  general controls, disabled. **No engineering prose in editor copy**; the model's description is
  the selector's hover text, in the sound's words.
- Selection is transient editor state; clicking a slot never auditions it and selection never moves
  a control. Durable edits are bracketed gestures through one binding module.
- The Model card's sound trace renders off the audio thread from the actual DSP model; never a
  decorative curve. Every card body is a `mxm_ui::tree` with computed floors; the one declared
  number is `SLOT_MODEL_MIN`.
- An original interface: no pads, step-key row, hardware layout, product colours or typography.

## Presets ([NOTES.md § Presets](NOTES.md#presets-and-the-audition-kits), [§ creative kits](NOTES.md#the-fifty-creative-kits))

- Nine factory audition kits, one source machine each, named with a numeric source token and no
  manufacturer names (`README.md` maps them), on one fixed role map: 1–5 Kick/Snare/low/mid/high
  drum; 6–8 pitched percussion; 9 Rim; 10 Clap; 11/12 closed/open hat; 13 Cymbal; 14 Cowbell; 15
  Clave; 16 auxiliary. A missing role is muted, never filled. All 94 models appear exactly once.
- **Fifty creative kits follow** (`src/preset/creative.rs`, *Percussion*), on the same map, named in
  original words without machine numbers. A key moves only what its model shows, never a *Gain*;
  every closed hat chokes its open hat (the owner, 2026-10-07). The design is the table; an ignored
  generator writes the files, and the default suite compares them in memory and renders the bank.
- Kits never carry Output or MIDI channel. Routes are sparse: a route not in use stores nothing, an
  in-use one its source and target, and its amount when nonzero; an omission loads as the default.
  Kits carry Controls 12–20, at zero.

## Resample and the sample pack ([§ Resample](NOTES.md#resample-is-an-instance-setting-and-the-capture-happens-at-activation), [§ pack](NOTES.md#the-sample-pack--packrs))

- `resample` is off in Init and every kit; no kit engages it. `refresh_captures` runs in `activate`,
  never in `process`; a failed capture leaves the instrument live.
- **One capture per engage**: nothing re-renders while engaged, because installing a kit cuts
  sound. To change a frozen kit: off, edit, on. Tune, Decay and Soft hits (Controls 1, 2 and 5)
  stay live as playback controls.
- Running toggles go to `capture_worker::CaptureWorker`, one thread per instance joined in its
  `Drop`. **Never use nice-plug's `AsyncExecutor::execute_background`; `task_executor` stays a no-op.**
- `service_captures` is a pointer swap with no allocation, lock or drop. `CaptureBank` hands kits
  over by ownership and keeps two retired slots. Only a kit carrying the current engage number
  (`MxmDrumMachine::engagement`) and rate installs; the worker abandons overtaken renders.
- A test standing in for the worker stops it first (`plugin.worker = None`).
- `pack.rs` writes 24-bit WAV one-shots and a manifest through mxm-kit's `mxm-audio-file`, never
  `mxm-audio-file-decode`. Files stop before the mix: unity gain, centred, Tune and Decay applied
  (`the_mix_cannot_change_an_exported_byte`). A pack's rate is fixed (`editor::PACK_SAMPLE_RATE`),
  whatever the host runs at; the capture that plays follows the host.
- Export requires Resample engaged (otherwise disabled, with its reason), runs on the capture thread
  through `capture_worker::Gate`, and picks its folder through `mxm_ui::offthread`.
- **A test names a temporary directory; nothing may resolve the real pack root inside one**
  (`pack::root_under`). An existing pack is never overwritten: write a unique sibling, then rename.

## Realtime ([NOTES.md § Realtime](NOTES.md#realtime))

- Host tempo is read once per block (ID 53, synced LFOs). No allocation, locks, formatting, logging,
  I/O or blocking in `process()`.
- Event groups use fixed arrays, built only where an event is due, bounded at 256 NoteOns and 256
  other events; past a bound only an order-independent result survives. Note ownership is
  order-independent; same-offset controller events keep host order. Output selectors apply before
  triggers; buffers are capped at 64 samples between event checks.
- Smoothers advance once per rendered sample; telemetry publishes once per internal block;
  `ProcessStatus::Tail` while any circuit is active, otherwise `Normal`.

# Work Guidance

- Implement one evidence-backed drum at a time; complete its DSP tests, parameters, editor
  capability explanation and listening status before adding the next selector entry.
- Keep manufacturer/model references in documentation, never product labels, preset names or
  parameter values.
- Do not add an internal sequencer, sample import, generic effect rack or a universal pitch engine.
- The editor uses `mxm-ui` controls; do not create a plugin-local dropdown or styling system.

# Verification

```bash
cargo test -p mxm-drum-machine
cargo clippy -p mxm-drum-machine --all-targets
cargo xtask bundle mxm-drum-machine --release
clap-validator validate "target/bundled/mxm-drum-machine.clap"
cargo test -p mxm-drum-machine-host-tests      # through MXM Player, against that bundle
```

**A change that must not move the sound** is proved with the same-sound harness, run before and
after and compared (`cargo test -p mxm-drum-machine --lib same_sound_digests -- --ignored
--nocapture`), and with the host test's recorded kit render. The recorded-kit fixture, the editor's
standard and tree checks, review pictures
(`MXM_PICTURES=after cargo test -p mxm-drum-machine --lib tree_pictures -- --ignored`) and the full
coverage list: [NOTES.md § Verification evidence](NOTES.md#verification-evidence). Hardware
fidelity, real multi-output DAW restoration, Linux and macOS remain unverified. *Since the split
(2026-10-06):* CI builds and tests Windows, macOS and Linux when started by hand, and Linux and macOS are checked later, together (root `AGENTS.md`, *Verification*).

# Child DOX Index

No child AGENTS.md files. This document covers the plugin.
