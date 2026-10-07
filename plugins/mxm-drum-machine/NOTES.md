# NOTES.md — plugins/mxm-drum-machine/

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked
examples. AGENTS.md is the contract; this file is the reference it links to.

## Contents

- [Identity and current implementation status](#identity-and-current-implementation-status)
- [Implemented drums' deliberate behavior](#implemented-drums-deliberate-behavior)
  - [Deep bridge kick](#deep-bridge-kick)
  - [Routes and velocity](#routes-and-velocity)
  - [Deep bridge kick's controls and reference](#deep-bridge-kicks-controls-and-reference)
  - [Twin-mode snare](#twin-mode-snare)
  - [Falling drums, rim and clave](#falling-drums-rim-and-clave)
  - [Maraca and clap](#maraca-and-clap)
  - [The metal batch](#the-metal-batch)
- [Parameter and routing surface](#parameter-and-routing-surface)
  - [The move to general controls and route slots (2026-10-07)](#the-move-to-general-controls-and-route-slots-2026-10-07)
  - [Each model's controls (2026-10-07)](#each-models-controls-2026-10-07)
  - [Choke groups, instance settings, Mute and Solo](#choke-groups-instance-settings-mute-and-solo)
  - [Model values](#model-values)
  - [Globals and the three LFOs](#globals-and-the-three-lfos)
  - [The surface before 2026-10-07](#the-surface-before-2026-10-07)
- [Interface](#interface)
  - [Layout, cards and the app bar](#layout-cards-and-the-app-bar)
  - [Labels and help text](#labels-and-help-text)
  - [Selection, telemetry and the sound trace](#selection-telemetry-and-the-sound-trace)
  - [Card trees, floors and painted names](#card-trees-floors-and-painted-names)
  - [Presets and the audition kits](#presets-and-the-audition-kits)
  - [The fifty creative kits](#the-fifty-creative-kits)
- [Resample is an instance setting, and the capture happens at activation](#resample-is-an-instance-setting-and-the-capture-happens-at-activation)
  - [Resample and Export samples live in the app bar](#resample-and-export-samples-live-in-the-app-bar)
  - [One capture per engage — a parameter edit while frozen renders nothing](#one-capture-per-engage--a-parameter-edit-while-frozen-renders-nothing)
  - [The capture thread is the plugin's own, and nice-plug's shared one is off limits here](#the-capture-thread-is-the-plugins-own-and-nice-plugs-shared-one-is-off-limits-here)
- [The sample pack — pack.rs](#the-sample-pack--packrs)
- [Realtime](#realtime)
- [Verification evidence](#verification-evidence)
  - [clap-validator's parameter fuzz on a debug bundle (2026-10-06)](#clap-validators-parameter-fuzz-on-a-debug-bundle-2026-10-06)
  - [The same sound through the move to general controls (2026-10-07)](#the-same-sound-through-the-move-to-general-controls-2026-10-07)
  - [The same sound through honest controls (2026-10-07)](#the-same-sound-through-honest-controls-2026-10-07)
  - [The recorded-kit fixture, once the pre-D7 compatibility fixture](#the-recorded-kit-fixture-once-the-pre-d7-compatibility-fixture)
  - [The editor's standard and tree checks](#the-editors-standard-and-tree-checks)
  - [What the tests cover](#what-the-tests-cover)

## Identity and current implementation status

- Product name: `mxm-drum-machine`; permanent CLAP id `dk.mxm.mxm-drum-machine`.
- No audio input. Configuration 0 is stereo main `L+R` plus sixteen mono outputs named by slot
  number only, `Slot 01`…`Slot 16` (owner, 2026-09-19; `Output 1`… before); configuration 1 is
  stereo compatibility. Full-first is the owner-approved pre-release break of 2026-09-18. Main
  remains port 0 in both; auxiliary IDs stay 1…16 in declaration order, since a name is not
  identity. MXM Player generically selects configuration 1.
- **Hosts do not report which outputs are patched.** Measured 2026-09-19 with a throwaway probe of
  this layout: Bitwig Studio 6.0 (Windows) queries `clap.audio-ports-activation/2`, sets all seventeen
  outputs active before every activation whatever chains exist, never deactivates one, and never
  tells the plugin about chains or track routing. Switching-jack normalling — a patched output
  leaving `L+R` automatically — is therefore impossible and must not be attempted: with every port
  active by default, it would empty main. Bitwig does honour `rescan(NAMES)`, but a chain keeps the
  name it was created with; that is why port names carry no model name.
- Each slot's MIDI channel defaults to `Kit`: notes 36–51 address slots 1–16 on every unclaimed
  channel. Selecting channel 1…16 makes that slot chromatic and claims the channel. A pitched model
  (kick, snare body, tom, conga, bongo, cowbell, clave, rim, bell) plays at concert pitch: key N
  sounds N's equal-tempered note at the model's measured rest pitch, via
  `ModelId::chromatic_reference_key`, within two octaves of it; hats, cymbals, claps and other noise
  voices play around note 60 (owner, 2026-09-18);
  every Kit slot ignores notes and per-note termination on a claimed channel. Slots may share a
  chromatic channel and layer. A hit's winning owner has two lifetimes: NoteOff and CC123 end its
  per-note expression but leave the one-shot tail, while the owner still addresses the hit until
  choke, retrigger or panic. So NoteChoke, by CLAP channel/key/note-ID matching against that owner,
  terminates a released tail exactly as pre-D7 key chokes did — host choke groups send it after the
  release. CC120 is global panic and CC123 releases expression globally, regardless of claims.
- The selector offers all 94 admitted catalogue models. Model ID 0 remains a permanent legacy
  silence value for old state but is no longer offered; IDs 95…255 are safe unavailable values.
  Models append without changing the fixed 0…255 domain.
- Init fills the brief's permanent sixteen-slot mixed assignment. It is distinct from the nine
  source-family audition presets.

## Implemented drums' deliberate behavior

*Since 2026-10-07 a control a model's code does not read has no knob at all
(§ [Each model's controls](#each-models-controls-2026-10-07)); "visibly unavailable" and "disabled"
below describe the panel before, when every model drew all eleven.*

### Deep bridge kick

Deep bridge kick is the schematic-derived, fidelity-unverified TR-808 bass-drum function documented
by `research:instruments/analogue-drum-machines.md` §3.2. Its public label does not use the maker or
model name.

**Quick repeated strikes continue from live resonator and output-filter state.** This is the hardware
wart that prevents sample-like machine-gun identity; never reset phase/state on NoteOn.

### Routes and velocity

**Its routes are the collection's modulation standard** (`plans/plan-modulation-standard.md`): a
route reaches what the standard reaches — Pitch twelve semitones, the **Amplitude** target the
standard factor (`+100 %` doubling the level, where it once summed decibels), every other target its
own bipolar unit — and the DSP crate's `conformance` holds that, unchanged. **Since 2026-10-07 the
route parameters are four general slots a drum** (§ [the move](#the-move-to-general-controls-and-route-slots-2026-10-07)):
one amount serves every target, so the host reads it as a percentage of its target's reach, and
`a_route_amount_travels_what_every_pair_is_offered` holds its travel to every pair's offer. *Until
then* every pair had its own `mxm_modulation_params::reading::amount_param`, reading in its target's
unit (the `level` ones in Amplitude percent, under their old ids `route_level_*`), and
`every_route_parameter_says_what_the_dsp_does` held each pair's travel and reading to
`mxm_drum_machine_dsp::conformance` through `mxm_plugin_test::routing_checks` — a check that needs a
parameter per pair, so it is retired with them: a **recorded deviation** from the modulation
standard's plugin half, as mxm-model-drums' (its brief, *Global and routing IDs*).

**A hit is its own velocity** (owner, 2026-09-20). The machines' common and global accent buses are
not modelled and there is no accent control; one slot's level never depends on another slot's, and
`Dynamics` is the only control over the response. The DSP crate's AGENTS.md and NOTES.md carry the ruling and
its reason. What survives is *collision* reduction, which is a different thing: several notes landing
on one slot at one sample are reduced to a single strike before any trigger, so host event order
cannot change the sound — but the value that sounds is the **winning owner's own** strike
(`ArbitrationResult::owner_strike`), not the combined one. One note owns the hit, so the hit is that
note's velocity: a chord on a chromatic slot strikes at the velocity of the note that also set its
pitch, never at a losing note nobody can hear.

### Deep bridge kick's controls and reference

Pitch moves resonator poles; Decay changes feedback-equivalent T60; Attack changes strike/pitch-jump;
Tone changes the passive output low-pass; Body changes sigh/leakage; Character changes local
headroom; Dynamics bends the velocity curve, as on every model (`crates/mxm-drum-machine-dsp`,
`velocity.rs`). Noise is visibly unavailable and an exact DSP no-op.
Its reference follows the 2026-09-19 comparison fit to an acquired recording:
- a trigger feed-through spike;
- a held pitch lift;
- a 48.8 Hz rest pitch.

`crates/mxm-drum-machine-dsp/NOTES.md` records the calibration. Fidelity remains hardware-unverified.

### Twin-mode snare

Twin-mode snare is the matching schematic-derived snare function from
`research:instruments/analogue-drum-machines.md` §3.3: two live-state struck bridged-T modes at the
comparison recording's 168.3 Hz and 331.9 Hz, in parallel with a wire path. Every simultaneous instance reads the same
fixed-seed machine-family white-noise sample, then applies its own filter and envelope; private
lookalike noise generators are forbidden. Noise maps the Snappy amount, Tone balances the two body
modes, and Character is visibly unavailable and an exact no-op. Fidelity remains hardware-unverified.

### Falling drums, rim and clave

The six falling drums take their rest pitches, decays and diode-control pitch excursions from the
comparison fit. All three toms read the machine-shared coloured-noise tail, as all three recorded toms
carry it; only Low falling tom exposes it on Noise, and the congas carry none.
- Layered short rim drives 1761 Hz and 472 Hz modes through an asymmetric clipper and a 9 ms
  closing switch. Its trigger reaches the output 0.45 ms before the strike.
- Pure high clave rings at 2497 Hz until a 33 ms switch.

Noise and Character availability follows the brief exactly. Hardware fidelity remains unverified.

### Maraca and clap

Both read the same machine-shared white-noise bus:
- Bright short maraca high-passes it behind a charging envelope, an 18.5 ms gate and a VCA threshold.
- Triple pulse clap runs it through a burst band, with bursts at 0/8/19.5/29.5 ms, and a separate
  reverb tail of about 2.1 s.

Pitch/Body and Pitch/Body/Character are respectively disabled exact no-ops.

### The metal batch

The metal batch owns one continuously advancing six-square bank.
- **The bank:** band-limited squares at this unit's fitted frequencies and duties, not the service
  nominals (crate `NOTES.md`).
- **Cowbell:** reads oscillators 5/6.
- **Cymbal:** reads its bands through three jointly fitted decay paths.
- **Hats:** both share one 7.1 kHz band, behind a one-polarity closed VCA and a soft-saturating open
  one. Neither articulation chokes the other unless their slots share a user-assigned choke group.
  Zero Pitch reads the shared frame, while nonzero Pitch uses a persistent private slot bank and
  rejoins shared phase at zero. Its render was accepted with the complete catalogue on 2026-09-20.

## Parameter and routing surface

*Titled "Permanent parameter and routing surface" until 2026-10-07: IDs are free to change during
pre-alpha (below), so nothing here is permanent until the first release.*

### The move to general controls and route slots (2026-10-07)

**The owner's rulings**, in order:

1. **2026-09-30** (the archive's `todo.txt`; `plans/plan-mxm-model-drums-plugin-v1.md` revision 5):
   the drum machine moves to mxm-model-drums' parameter principle — **"the host holds what the
   panel can show at once"** — with **no migration**: *"There are no saved projects - we are in pre
   alpha"*.
2. **2026-10-07: parameter IDs are free to change during pre-alpha**; the permanent-ID freeze
   applies from the first release. For this plugin and until then, that overrides
   `plugins/AGENTS.md`'s *Permanent identifiers* ("a parameter `#[id]` is never changed or
   reused"), this plugin's own frozen-surface text (below, *The surface before 2026-10-07*) and its
   list of retired IDs. A **recorded deviation**, as mxm-model-drums recorded its own in its brief.
3. **2026-10-07: mechanical mapping, same sound.** The named controls become general ones in a fixed
   order; each model keeps its panel names; every kit and preset sounds exactly as it did,
   bit-identical on Windows. Per-model redesign can come later — not now.

**The surface**: per slot `model`; twenty general controls `c01`…`c20`, named `Control k` for the
host (the module is `Slot N`); seven slot settings `level`, `pan`, `mute`, `solo`, `choke_group`,
`output`, `midi_channel`; four route slots `route<r>_{source,target,amount}`. Kit-wide `master`,
`lfo{1,2,3}_{rate,shape,sync}` and `resample`. Sixteen slots of forty and eleven kit-wide: **651**,
from 3,227 (sixteen of 19 + 13 × 7 × 2, and eleven) — `params::SLOT_PARAMETERS`, `GLOBAL_IDS`,
`the_ids_are_complete_unique_and_651`. The slot order and the route and target parameters are
model-drums' (`plugins/mxm-model-drums/src/params.rs`), Choke group now before Output.

**The mapping** (`params::control`; each keeps its range, default, unit, 15 ms linear smoothing and
its path into the DSP's `SlotPatch`):

| Control | ID | Was | Panel name | Range |
|---|---|---|---|---|
| 1 | `c01_N` | `pitch_N` | Tune | ±24 st, read in semitones |
| 2 | `c02_N` | `decay_N` | Decay | ±100 % |
| 3 | `c03_N` | `tone_N` | Tone | ±100 % |
| 4 | `c04_N` | `attack_N` | Attack | ±100 % |
| 5 | `c05_N` | `dynamics_N` | Dynamics | ±100 % |
| 6 | `c06_N` | `pitch_env_N` | Pitch envelope | ±100 % |
| 7 | `c07_N` | `pitch_decay_N` | Pitch decay | ±100 % |
| 8 | `c08_N` | `body_N` | Body | ±100 % |
| 9 | `c09_N` | `noise_N` | Noise, *Snappy* on a snare | ±100 % |
| 10 | `c10_N` | `noise_decay_N` | Noise decay | ±100 % |
| 11 | `c11_N` | `character_N` | Character | ±100 % |
| 12–20 | `c12_N`…`c20_N` | — | not drawn | ±100 %, unused |

Model-drums' common seven come first, so automation keeps its sense across the two instruments:
Tune, Decay, Tone, Attack, Velocity, Pitch drop, Pitch decay. **Dynamics takes Velocity's place
because it is velocity sensitivity** — the velocity curve's exponent on every family (the DSP's
`velocity.rs`), where model-drums' Velocity is "how far a soft stroke moves from a hard one" — and
Pitch envelope takes Pitch drop's. The machine's own follow in the order they were declared. **No
model of this machine uses 12–20**: they are exact no-ops, never read or smoothed in `process`, and
never drawn, as model-drums leaves its kick's 17–20. *Until the same day's second step* a control a
model does not support stayed visible and disabled, as before; since it, the model shows only what
its code reads, under its own names (§ [Each model's controls](#each-models-controls-2026-10-07)), and
the panel names in the table above are the general ones.

**The routes.** Each slot's 13 targets × 7 sources of presence and amount (`route_<target>_<source>_{on,amount}_N`,
182 a slot) became four route slots, each a source, a target and an amount:

- Sources, in order: `Off`, `LFO 1`, `LFO 2`, `LFO 3`, `Wheel`, `Pressure`, `Velocity`, `Random` —
  the grid's seven sources one for one (`SourceChoice::dsp`).
- Targets, in order: `Off`, `Control 1`…`Control 20`, `Level`, `Pan`. Controls 1–11 are the grid's
  targets by the table above, `Level` its `level` target (Amplitude), `Pan` its `pan`
  (`RouteTarget::dsp`); Controls 12–20 reach nothing.
- An old route `route_<target>_<source>_on_N` with its amount is one route slot with that source,
  that target and that amount. The amount keeps the old pairs' travel (−1…+1, linear) and smoothing
  (15 ms); one full route still reaches what the pair reached (the DSP's `routing::FULL_SCALE`).
- **The DSP is unchanged**: `Routes::routing_from` fills its per-slot grid from the four slots each
  block and `Routes::advance` each sample, so one route alone is exactly the pair it replaces
  (`every_grid_pair_has_exactly_one_route_spelling`, `a_route_fills_its_pair_and_an_unused_one_fills_nothing`).

**What changed in behaviour**, beyond the IDs and names:

- **Four routes a drum** is the limit, where every one of the 91 pairs could be on at once.
- **A route is off when its source or its target is Off** (model-drums' rule); removing one switches
  its source off and **keeps its target and amount**, so adding a source back to that knob restores
  the depth. Before, a route was its own presence parameter, and absence kept the amount.
- Two route slots on one source and one target **add** (`two_routes_on_one_pair_add`); before, a pair
  had one amount. A route aimed at Controls 12–20 takes a slot and does nothing.
- The host reads a route amount as a **percentage** of its target's full reach (Tune's 100 % is twelve
  semitones), not in the target's unit: one parameter serves every target. The route's host names are
  `Route r source/target/amount`, not `<Target> from <Source>`; a row on the panel still reads
  `<Target> from <Source>`.
- A knob's accessible and tooltip name is its host name, `Control k`; the panel paints its old name
  (`Bound::panel`, mxm-preset's binding rule).
- Presets store routes sparsely by the new rule (§ *Presets and the audition kits*), and every kit
  carries Controls 12–20 at zero.

**Retired with the old surface**: the plugin-side `mxm_plugin_test::routing_checks` test
(`every_route_parameter_says_what_the_dsp_does`), which needs a parameter per pair — a recorded
deviation, as model-drums' (§ *Routes and velocity*); the host test
`pre_d7_state_opens_with_routing_defaults_and_bit_exact_main_audio`, which loaded the pre-D7 state's
IDs (§ *The recorded-kit fixture*). The DSP's own `conformance` runs unchanged.

**What proved the same sound** is in § [Verification evidence](#the-same-sound-through-the-move-to-general-controls-2026-10-07).

### Each model's controls (2026-10-07)

**The owner's rulings.** 2026-09-30 (the archive's `todo.txt`): *"Map its named axes onto the seven
common controls (Tune, Decay, Tone, Attack, Velocity, Pitch drop, Pitch decay), checking each model
has an honest meaning for each."* 2026-10-07: *"do 1: drum machine"* — honest controls: a model shows
only the controls that do something for it, under its honest name. And, the same day, on the method:
*"Why do you need to measure if a parameter does anything? Can't you read the code anymore? One
parameter might not do anything depending on the setting of another parameter."*

**The method: read from the code.** A model shows a control when its circuit reads it at all, in any
setting of the other controls; a control it never reads, or reads only to discard, has no knob. Each
family's path from `SlotPatch` into its voice was read control by control (`deep_bridge_kick.rs`,
`twin_mode_snare.rs`, `falling_drum.rs`, `rim_clave.rs`, `noise_percussion.rs`, `metal_808.rs`,
`reset_vco_909.rs`, `analogue_909.rs`, `pcm_909.rs`, `economy_55.rs`, `legacy.rs`, and `engine.rs`
and `velocity.rs` around them). It agrees with the DSP's `ModelId::capabilities` everywhere: every
declared control is read, and every undeclared one is missing from the family's patch or read into
a discarded binding. So the table is held to it
(`each_model_shows_exactly_the_controls_its_code_reads`), and the DSP's own
`every_declared_unsupported_axis_is_an_exact_dsp_no_op` holds the other half. A control that acts only
while another is set is **shown**, and its help says what it waits on — *Only while Snappy is above
its bottom* — so the owner listens with that one up. *A render of every model with each control at
its ends was run first and dropped on the owner's ruling; it agreed with the code (Pitch decay on the
ten unswept models changes the hit only beside Pitch sweep), and it decided nothing.*

**The names.** Model-drums' seven keep their places — 1 Tune, 2 Decay, 3 Tone, 4 Attack, 5 the
velocity response, 6 the pitch drop, 7 its time — and the panel keeps a general name wherever it is
honest. It names what a control does where the general name would send the player to the wrong part
of the sound or the wrong way:

- **Soft hits** for Dynamics, on every model. Its top flattens the velocity curve (`velocity.rs`:
  soft hits come up), so "more Dynamics" made the drum *less* dynamic, and model-drums' *Velocity*,
  whose top widens the difference, would read backwards too.
- **Pitch drop** for Pitch envelope where the drum's pitch falls (its bottom: no drop), model-drums'
  common name; **Pitch sweep** where there is no drop of its own and the centre is none, so the top
  falls and the bottom *rises* (28, 67, 68, 74–77, 86–88). Pitch decay keeps its name; on those ten
  it acts only while Pitch sweep is off its centre.
- **Gain** for a control that only changes the level: Attack on every supporting machine (`legacy.rs`
  multiplies the whole output by `1 + 0.25·attack`) and on 10, 22, 29–31; Body on 10, 19–22, 30, 32,
  47, 73, 89; Noise where the voice is all noise or the control scales everything (11, 12, 23, 31,
  44, 59, 65, 82–84, 93, 94).
- **Snappy decay** beside Snappy on every snare, as Snappy already stood for Noise there.
- One model's own: **Bend** (1's Body, the pitch bend after the click, not body weight), **Weight**
  (17's Tone, the body against the click, not a filter), **Punch** (17's Body, a deeper drop and a
  little level), **Click length** (17's Noise decay, the click's length, not a noise envelope),
  **Snappy length** (18's Attack, which stretches the snares), **Clap length** (12's Attack), **Clap
  decay** (the claps' Noise decay, each clap's length), **Spread** (23's Attack, the gap between
  claps), **Sizzle** (14's Attack, the top layer's level), **Soft start** (24–27's Attack, whose top
  softens the start), **Drive** (28's Body; Character where it drives), **Clean** (9 and 10's
  Character, whose top is *less* drive), **Ring** and **Air** (the claps' Character, opposite ways),
  **Crunch** (24–27's Character: drive up, fewer levels down) and **Speed** (72's Tune, a scrape
  rate).

**How many each model shows**: all eleven on 3 (19–21), ten on 6, nine on 3, eight on 32, seven on
16, six on 30 and five on 4. Hidden: Noise decay on 72 models, Noise on 58, Pitch drop and Pitch
decay on 58, Character on 48, Body on 36 and Tune on 16; Decay, Tone, the fourth control and Soft
hits on none.

**The panel.** Only a model's controls are drawn, each under its name, with its help on hover; a
hidden one keeps its stored value and, being unread, its silence. A legacy Off or an unavailable id
keeps the eleven general controls, disabled, so its cards stay what they are. **A route already aimed
at a control the model does not show** — left by a change of model, or set by the host on Controls
12–20 — is drawn on the card that would hold the knob (12–20 on *Envelopes & tone*) under
"<name> (unused)", with its amount and its remove; nothing more is offered there, as model-drums
shows an unavailable current id but never offers it. Route rows read their knob's name. While
Resample freezes a slot, Tune, Decay and **Soft hits** stay live (the frozen hit is played at the
velocity curve, `velocity::curve`); Soft hits drew disabled there until this change.

**For the owner's ear — the ones I was unsure of:**

- *Soft hits* itself: a new name for the velocity curve, chosen for its direction.
- **Two knobs, one job.** Two *Gain*s on 10, 22, 30, 31, 32, 47, 73, 89, 44, 59, 65, 82–84, 93 and
  94; Clap length and Clap decay on 12 (one burst time, multiplied); Snappy length and Snappy decay
  on 18; Noise decay and Decay on 94 (one envelope, less its second fade); on the rim (9) Attack and
  Clean feed the same clipper. Named honestly rather than told apart; whether to keep them is the
  owner's.
- **Read, but faint**: 34–36's Noise and Noise decay (a hum fitted 45–70 dB under the drum); 37–39's
  Pitch drop (under a semitone) and Pitch decay; 13's Drive (about 0.5 dB); 19's Attack (its knock
  level is zero, so only a faint hiss lengthens); 1's Drive below the centre and 10's Clean above it
  (≤ 0.3 dB); Drive's lower half on the supporting machines, which only turns them down (−5 dB);
  the snares of 33, which are quiet; and Body on the resonators whose click is tiny (36–39, 45, 50,
  66–68, 74–77, 79, 80, 85–88, 90), nearly a level. Shown because the code reads them.
- **Pitch drop on the mid and high falling congas (6, 8)**: the pitch jump lasts under 2 ms even at
  the top of Pitch decay, so it is heard as a snap at the strike, not a drop.
- 18's Tone is not monotonic (its upper note flips polarity below about −0.67); 73's Tone is mostly
  a level (its partials sit far above the corner); 2 and 18's Tone balance two notes rather than
  filter, as the 808 snare's own Tone does, so the name stays.
- **Found on the way, not changed (the DSP's)**: a frozen slot follows Tune as a playback rate on
  every model, the sixteen with no Tune included (11, 12, 23, 31, 44, 59, 63–65, 71, 82–84, 92–94),
  so a Tune stored there moves a frozen hit with no knob to see it; a route on a control read only at
  the strike (Attack, Soft hits, Noise on models 1–10) does nothing, because routes reach the patch
  per sample after the trigger has read it; the maraca's and claps' Noise (now Gain) meets the
  output's hard limit inside the normal range.

**Each model's names** (`print_the_control_table`, an ignored test that prints both tables from
`editor::controls`; — is not drawn):

| Model | 1 Tune | 2 Decay | 3 Tone | 4 Attack | 5 Soft hits | 6 Pitch drop | 7 Pitch decay | 8 Body | 9 Noise | 10 Noise decay | 11 Character |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 Deep bridge kick | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Bend | — | — | Drive |
| 2 Twin-mode snare | Tune | Decay | Tone | Attack | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 3 Low falling tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | Noise | Noise decay | — |
| 4 Low falling conga | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 5 Mid falling tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 6 Mid falling conga | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 7 High falling tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 8 High falling conga | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 9 Layered short rim | Tune | Decay | Tone | Attack | Soft hits | — | — | Body | — | — | Clean |
| 10 Pure high clave | Tune | Decay | Tone | Gain | Soft hits | — | — | Gain | — | — | Clean |
| 11 Bright short maraca | — | Decay | Tone | Attack | Soft hits | — | — | — | Gain | — | — |
| 12 Triple pulse clap | — | Decay | Tone | Clap length | Soft hits | — | — | — | Gain | Clap decay | Ring |
| 13 Twin-square cowbell | Tune | Decay | Tone | Attack | Soft hits | — | — | — | — | — | Drive |
| 14 Three-path cymbal | Tune | Decay | Tone | Sizzle | Soft hits | — | — | — | — | — | Drive |
| 15 Six-square closed hat | Tune | Decay | Tone | Attack | Soft hits | — | — | — | — | — | Drive |
| 16 Six-square open hat | Tune | Decay | Tone | Attack | Soft hits | — | — | — | — | — | Drive |
| 17 Reset punch kick | Tune | Decay | Weight | Attack | Soft hits | Pitch drop | Pitch decay | Punch | Noise | Click length | — |
| 18 Reset twin snare | Tune | Decay | Tone | Snappy length | Soft hits | Pitch drop | Pitch decay | Body | Snappy | Snappy decay | — |
| 19 Low reset triad tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Gain | Noise | Noise decay | Drive |
| 20 Mid reset triad tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Gain | Noise | Noise decay | Drive |
| 21 High reset triad tom | Tune | Decay | Tone | Attack | Soft hits | Pitch drop | Pitch decay | Gain | Noise | Noise decay | Drive |
| 22 Triple-resonator rim | Tune | Decay | Tone | Gain | Soft hits | — | — | Gain | — | — | Drive |
| 23 Four-cell clap | — | Decay | Tone | Spread | Soft hits | — | — | — | Gain | Clap decay | Air |
| 24 Six-bit closed hat | Tune | Decay | Tone | Soft start | Soft hits | — | — | — | — | — | Crunch |
| 25 Six-bit open hat | Tune | Decay | Tone | Soft start | Soft hits | — | — | — | — | — | Crunch |
| 26 Six-bit crash | Tune | Decay | Tone | Soft start | Soft hits | — | — | — | — | — | Crunch |
| 27 Six-bit ride | Tune | Decay | Tone | Soft start | Soft hits | — | — | — | — | — | Crunch |
| 28 Economy 62 kick | Tune | Decay | Tone | Attack | Soft hits | Pitch sweep | Pitch decay | Drive | — | — | — |
| 29 Economy body snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 30 Economy short rim | Tune | Decay | Tone | Gain | Soft hits | — | — | Gain | — | — | — |
| 31 Inductor noise hat | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | Drive |
| 32 Dual-low kick | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Gain | — | — | Drive |
| 33 Dual-bridge snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 34 Diode low tom | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | Noise | Noise decay | — |
| 35 Diode mid tom | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | Noise | Noise decay | — |
| 36 Diode high tom | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | Noise | Noise decay | — |
| 37 Diode low conga | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 38 Diode mid conga | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 39 Diode high conga | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 40 Thirty-millisecond rim | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | Drive |
| 41 Six-square cymbal | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 42 Six-square short hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 43 Six-square long hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 44 Saw-noise clap | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | Clap decay | Drive |
| 45 Phase-shift clave | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | Drive |
| 46 Split-square cowbell | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 47 Compact dual kick | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Gain | — | — | — |
| 48 Compact body snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 49 Compact low tom | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 50 Compact high tom | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 51 Two-band cymbal | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 52 Resonant closed hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 53 Tempo-coupled open hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 54 Damped whack kick | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | Drive |
| 55 LFSR snap snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 56 Mixed-source cymbal | Tune | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 57 Mixed-source closed hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 58 Mixed-source open hat | Tune | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 59 Timed-burst clap | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | Clap decay | Drive |
| 60 Classic 62 kick | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 61 Classic 340 snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 62 Five-millisecond rim | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | — |
| 63 Sixty-millisecond noise hat | — | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 64 Long noise cymbal | — | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 65 Twenty-millisecond maraca | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | — |
| 66 High 2630 clave | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | — |
| 67 High 600 bongo | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 68 Low 400 bongo | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 69 Low 208 conga | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 70 Close-interval cowbell | Tune | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 71 Tambourine wash | — | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 72 Two-rate guiro | Speed | Decay | Tone | Gain | Soft hits | — | — | — | — | — | Drive |
| 73 Triple high bell | Tune | Decay | Tone | Gain | Soft hits | — | — | Gain | — | — | Drive |
| 74 Discrete 62 kick | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 75 Discrete 208 conga | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 76 Discrete low bongo | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 77 Discrete high bongo | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 78 Resonant 830 cowbell | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | Drive |
| 79 Discrete short rim | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | — |
| 80 Resonant 2350 clave | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | — |
| 81 Bongo-body snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 82 Forty-millisecond noise hat | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | Drive |
| 83 Forty-millisecond maraca | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | — |
| 84 Four-hundred-millisecond cymbal | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | Drive |
| 85 Early transistor kick | Tune | Decay | Tone | Gain | Soft hits | Pitch drop | Pitch decay | Body | — | — | — |
| 86 Early low conga | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 87 Early high conga | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 88 Early high bongo | Tune | Decay | Tone | Gain | Soft hits | Pitch sweep | Pitch decay | Body | — | — | — |
| 89 Early cowbell | Tune | Decay | Tone | Gain | Soft hits | — | — | Gain | — | — | Drive |
| 90 Early clave | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | — | — | — |
| 91 Early snare | Tune | Decay | Tone | Gain | Soft hits | — | — | Body | Snappy | Snappy decay | — |
| 92 Early cymbal | — | Decay | Tone | Gain | Soft hits | — | — | — | Noise | — | Drive |
| 93 Early maraca | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | — | — |
| 94 Early wire brush | — | Decay | Tone | Gain | Soft hits | — | — | — | Gain | Noise decay | Drive |

**What each does** (the hover help; what a control waits on is in it):

| Control | Name | What it does | Models |
|---|---|---|---|
| 1 | Tune | Tunes the boom; its swoop follows. | 1 |
| 1 | Tune | Tunes the drum; the snares keep their pitch. | 2, 18, 29, 33, 48, 55, 61, 81, 91 |
| 1 | Tune | Tunes the drum up or down. | 3, 4, 5, 6, 7, 8, 9, 19, 20, 21, 22, 28, 30, 32, 34, 35, 36, 37, 38, 39, 40, 45, 47, 49, 50, 54, 60, 62, 66, 67, 68, 69, 73, 74, 75, 76, 77, 78, 79, 80, 85, 86, 87, 88, 89, 90 |
| 1 | Tune | Tunes the clave; very high, it also gets quieter. | 10 |
| 1 | Tune | Tunes the cowbell. | 13, 46, 70 |
| 1 | Tune | Moves the metallic ring up or down; it has no single note. | 14, 15, 16, 41, 42, 43, 51, 52, 53, 56, 57, 58 |
| 1 | Tune | Tunes the kick; its pitch drop follows. | 17 |
| 1 | Tune | Tunes it up or down; higher is also shorter. | 24, 25, 26, 27 |
| 1 | Speed | How fast it is scraped; very fast, it turns into a hiss. | 72 |
| 2 | Decay | How long the boom rings. | 1 |
| 2 | Decay | How long the drum rings; the snares keep their length. | 2, 18, 29, 33, 48, 55, 61, 81, 91 |
| 2 | Decay | How long it rings. | 3, 4, 5, 6, 7, 8, 14, 22, 30, 32, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 45, 46, 47, 49, 50, 51, 52, 54, 56, 57, 58, 60, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77, 78, 79, 80, 82, 83, 84, 85, 86, 87, 88, 89, 90, 92, 93, 94 |
| 2 | Decay | How long it rings before it is cut off. | 9, 10 |
| 2 | Decay | How long the shake lasts. | 11 |
| 2 | Decay | How long the room tail after the claps lasts. | 12, 23, 44, 59 |
| 2 | Decay | How long it rings after the first clank. | 13 |
| 2 | Decay | How long it rings: how open it sounds. | 15, 16 |
| 2 | Decay | How long the kick lasts. | 17 |
| 2 | Decay | How long its main tone rings. | 19, 20, 21 |
| 2 | Decay | Fades it out sooner, or holds its tail up to its natural end. | 24, 25, 26, 27 |
| 2 | Decay | How long the kick lasts; it stretches the pitch sweep too. | 28 |
| 2 | Decay | How long the hat lasts. | 31 |
| 2 | Decay | How long it rings; it also follows the song's tempo. | 53 |
| 2 | Decay | How long the scrape lasts, and when it speeds up. | 72 |
| 3 | Tone | Darkens or brightens the click; the boom itself barely changes. | 1 |
| 3 | Tone | Shifts the drum between its low note and its higher ring. | 2 |
| 3 | Tone | Darkens or brightens the click at the start. | 3, 4, 5, 6, 7, 8 |
| 3 | Tone | Darkens or brightens the sound. | 9, 11, 15, 16, 19, 20, 21, 22, 24, 25, 26, 27, 28, 72, 89 |
| 3 | Tone | Softens or sharpens the click of its start and end. | 10 |
| 3 | Tone | Darkens or brightens the claps and the room. | 12, 44, 59 |
| 3 | Tone | Darker and lower, or brighter and higher. | 13 |
| 3 | Tone | A darker wash, or a brighter sizzle. | 14 |
| 3 | Weight | How heavy the body is against the click; more also adds a little saturation. | 17 |
| 3 | Tone | Shifts the drum between its lower and its upper note. | 18 |
| 3 | Tone | Moves the claps' colour down or up. | 23 |
| 3 | Tone | Brightens or darkens the snares only. | 29 |
| 3 | Tone | Darkens or brightens it; very dark, it also gets quieter. | 30 |
| 3 | Tone | Moves the hat's ringing colour down or up. | 31 |
| 3 | Tone | Darkens or brightens the overtones; the low note barely changes. | 32, 47 |
| 3 | Tone | Brightens or darkens the snares and the click; the drum's own note is untouched. | 33, 48, 55, 61, 81, 91 |
| 3 | Tone | Darkens or brightens the click and the overtones. | 34, 35, 36, 37, 38, 39, 40, 45, 49, 50, 54, 60, 62, 66, 67, 68, 69, 74, 75, 76, 77, 78, 79, 80, 85, 86, 87, 88, 90 |
| 3 | Tone | Moves the metallic colour darker or brighter. | 41, 42, 43, 51, 52, 53, 56, 57, 58 |
| 3 | Tone | Moves the cowbell's colour; it can change which of its two notes leads. | 46, 70 |
| 3 | Tone | Moves the hiss darker or brighter. | 63, 64, 65, 71, 82, 83, 84, 92, 93, 94 |
| 3 | Tone | Darkens or brightens it; on this bell, mostly louder or softer. | 73 |
| 4 | Attack | How loud the beater click is at the start. | 1 |
| 4 | Attack | How hard the drum is struck: a louder ring and a firmer thump. | 2 |
| 4 | Attack | How hard it is struck: louder, with a harder click and a bigger pitch snap. | 3, 4, 5, 6, 7, 8 |
| 4 | Attack | How hard the rim is struck: more or less crack. | 9 |
| 4 | Gain | Only makes it louder or softer, as Level does. | 10, 22, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94 |
| 4 | Attack | How quickly it gets loud. | 11 |
| 4 | Clap length | How long each clap lasts, tighter or looser, as Clap decay does over a smaller range. | 12 |
| 4 | Attack | How long the bright clank at the start lasts. | 13 |
| 4 | Sizzle | How much bright sizzle is on the hit. | 14 |
| 4 | Attack | How sharp the click at the start is. | 15, 16 |
| 4 | Attack | How loud the click at the start is. | 17 |
| 4 | Snappy length | How long the snares rattle, as Snappy decay does over a smaller range. Only while Snappy is above its bottom. | 18 |
| 4 | Attack | On this tom, only how long the faint hiss at the start lasts. | 19 |
| 4 | Attack | How hard the knock at the start is, and how long its hiss lasts. | 20, 21 |
| 4 | Spread | How far apart the claps are. | 23 |
| 4 | Soft start | Softens the start above the centre; hardens it below. | 24, 25, 26, 27 |
| 4 | Attack | How hard the thump at the start is. | 28 |
| 5 | Soft hits | Brings soft hits up towards full ones, or down, away from them. A full hit stays as it is. | 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94 |
| 6 | Pitch drop | How far the boom swoops down at the start; at the bottom it is a steady note. | 1 |
| 6 | Pitch drop | How far the pitch falls when the drum is struck; at the bottom it stays put. | 3, 4, 5, 6, 7, 8, 17, 18, 19, 20, 21, 32, 34, 35, 36, 37, 38, 39, 47, 49, 50, 54, 60, 69, 85 |
| 6 | Pitch sweep | Bends the pitch down after the strike, or up below the centre; at the centre it stays put. Harder hits bend further. | 28, 67, 68, 74, 75, 76, 77, 86, 87, 88 |
| 7 | Pitch decay | How long the swoop takes. | 1 |
| 7 | Pitch decay | How quickly the falling pitch settles. Only while Pitch drop is above its bottom. | 3, 4, 5, 6, 7, 8, 17, 18, 19, 20, 21, 32, 34, 35, 36, 37, 38, 39, 47, 49, 50, 54, 60, 69, 85 |
| 7 | Pitch decay | How quickly the pitch sweep settles; Decay stretches it too. Only while Pitch sweep is off its centre. | 28, 67, 68, 74, 75, 76, 77, 86, 87, 88 |
| 8 | Bend | How far the pitch bends down after the click, and how much the boom growls. Only while Pitch drop is above its bottom. | 1 |
| 8 | Body | How loud the drum is against the snares. | 2, 18, 29, 33, 48, 55, 61, 81, 91 |
| 8 | Body | A little more or less ring, and a bigger or smaller pitch snap at the strike. | 3, 4, 5, 6, 7, 8 |
| 8 | Body | How much of the low knock is in the hit. | 9 |
| 8 | Gain | Only makes it louder or softer, as Level does. | 10, 19, 20, 21, 22, 30, 32, 47, 73, 89 |
| 8 | Punch | A deeper pitch drop and a slightly louder body. | 17 |
| 8 | Drive | Drives the ring harder, flattening it. | 28 |
| 8 | Body | How loud the ring is against the click at the start. | 34, 35, 36, 37, 38, 39, 40, 45, 49, 50, 54, 60, 62, 66, 67, 68, 69, 74, 75, 76, 77, 78, 79, 80, 85, 86, 87, 88, 90 |
| 9 | Snappy | How loud the snares are against the drum. | 2, 18 |
| 9 | Noise | How loud the hiss under the tom is. | 3 |
| 9 | Gain | Only makes it louder or softer, as Level does. | 11, 12, 23, 31, 44, 59, 65, 82, 83, 84, 93, 94 |
| 9 | Noise | How much noise is in the click. Only while Attack is above its bottom. | 17 |
| 9 | Noise | How loud the hiss is. | 19, 20, 21 |
| 9 | Snappy | How loud the snares are against the drum; they never go away entirely. | 29, 33, 48, 55, 61, 81, 91 |
| 9 | Noise | How loud the faint noisy hum inside the drum is. | 34, 35, 36 |
| 9 | Noise | How much hiss is mixed into the metal. | 56, 57, 58 |
| 9 | Noise | How loud the hiss is against the rest of the sound. | 63, 64, 71, 92 |
| 10 | Snappy decay | How long the snares rattle. Only while Snappy is above its bottom. | 2, 18 |
| 10 | Noise decay | How long the hiss lasts. Only while Noise is above its bottom. | 3, 19, 20, 21 |
| 10 | Clap decay | How long each clap lasts, from a tight snap to one long smear. | 12, 23 |
| 10 | Click length | Turns the click from a sharp tick into a soft thump. Only while Attack is above its bottom. | 17 |
| 10 | Snappy decay | How long the snares rattle. | 29, 33, 48, 55, 61, 81, 91 |
| 10 | Noise decay | How long that hum lasts. | 34, 35, 36 |
| 10 | Clap decay | How long the last clap lasts. | 44, 59 |
| 10 | Noise decay | Shortens or lengthens the brush, as Decay does, but leaves its long second fade alone. | 94 |
| 11 | Drive | Rounds off and fattens the boom above the centre; below it, it stays clean. | 1 |
| 11 | Clean | Cleaner and more ringing above the centre; harsher and buzzier below. | 9 |
| 11 | Clean | Fuzzier and squashed below the centre; above it, it stays clean. | 10 |
| 11 | Ring | Narrower and more pitched room above the centre; airier and hissier below. | 12 |
| 11 | Drive | Adds a little grit above the centre. | 13 |
| 11 | Drive | Grittier and fuller above the centre, cleaner below. | 14, 15, 16 |
| 11 | Drive | Buzzier and squarer above the centre, rounder below. | 19, 20, 21 |
| 11 | Drive | Harsher above the centre, cleaner below. | 22 |
| 11 | Air | Airier and softer above the centre; more ringing and pitched below. | 23 |
| 11 | Crunch | Saturates it above the centre; grainier and more lo-fi below. | 24, 25, 26, 27 |
| 11 | Drive | Louder and saturated above the centre; quieter and clean below. | 31 |
| 11 | Drive | Dirtier and fatter above the centre; below it, only quieter. | 32, 40, 41, 42, 43, 44, 45, 46, 51, 52, 53, 54, 56, 57, 58, 59, 63, 64, 70, 71, 72, 73, 78, 82, 84, 89, 92, 94 |

**The contract text it replaced** — this plugin's `AGENTS.md`, as it read until 2026-10-07's second
step:

> - **The controls are the named ones of before, mechanically** (the owner, 2026-10-07): `params::control`
>   holds which is which — model-drums' common seven first (Tune, Decay, Tone, Attack, then
>   Dynamics in Velocity's place, Pitch envelope in Pitch drop's, Pitch decay), then Body, Noise,
>   Noise decay, Character. Each keeps its range, default, unit, smoothing and path into the DSP;
>   every kit sounds bit-identical ([NOTES.md § the move](NOTES.md#the-move-to-general-controls-and-route-slots-2026-10-07)).
>   Controls 12–20 mean nothing on any model: never read, never drawn. Per-model redesign is later.
>
> - **The panel names a control as before, the host by its number** (`Axis::name`): Tune, Decay, …;
>   snare Noise reads Snappy. Route rows read the DSP's target names (Tune's *Tune*, Level's
>   *Amplitude*). Unsupported controls stay visible and disabled. **No engineering prose in editor
>   copy**; the model's description is the selector's hover text, in the sound's words.
>
> - **One capture per engage**: nothing re-renders while engaged, because installing a kit cuts
>   sound. To change a frozen kit: off, edit, on. Tune and Decay (Controls 1 and 2) stay live as
>   playback controls.

### Choke groups, instance settings, Mute and Solo

**Choke is an assignment, not hardware wiring** (owner, 2026-09-20). Slots sharing a choke group cut
each other through the DSP's bounded de-click, whatever models they are — an 808 open hat closed by a
909 closed hat, or a long kick closed by a short one. The machines' hardwired closed/open hat pairs
are gone and nothing chokes unless a slot is assigned. Init and the nine audition kits ship at `Off`;
the fifty creative kits choke as a real kit would (the owner, 2026-10-07,
§ [The fifty creative kits](#the-fifty-creative-kits)).
Unlike Output and MIDI channel, Choke group is a **kit** setting: which slots cut each other is sound
design, so preset capture, apply and Init all carry it. The two new families are
instance settings: host/project state stores them, while preset capture/apply, Init, completeness,
identity baseline and dirty comparison exclude them through `mxm-preset::Instrument::is_instance_setting`.
`master` is global. Changing Model never changes the host parameter inventory.
Mute and Solo are post-circuit: muted/non-soloed circuits keep running and preserve machine-shared
interaction; when any Solo is on only unmuted soloed slots sound, so Mute wins.

### Model values

Model parameters store the fixed integer domain 0…255. The editor maps the available catalogue onto
the shared grouped `mxm-ui` caret selector, so arrows/search visit implemented models rather than
reserved or unavailable IDs. Host automation of an unavailable ID displays `Unavailable N` and
renders silence. Group headings are display-only and cannot shift a stored index.

### Globals and the three LFOs

The globals are `master` plus `lfo{1,2,3}_{rate,shape,sync}` (and the instance setting
`resample`). There are exactly three LFO generators for the kit, never three per slot. Each `sync` is
one on/off button beside its Rate knob. Off, Rate is hertz; on, the same knob snaps across 4 bars
through 1/32 and reads the selected musical division. Missing or invalid host tempo falls back to the
continuously advanced free Rate. A separate Division parameter was the clunky two-control design the
owner rejected on 2026-09-20. Realtime compaction is narrower than a route being in use: an in-use
route at settled zero stays on the panel and in a preset but leaves the per-sample DSP list until its
smoothed amount moves again. The slot's controls and routes are boxed, and the production shell boxes
its DSP engine, so neither constructing the surface nor moving the plugin through the CLAP wrapper can
overflow an ordinary host or validator thread's stack.

### The surface before 2026-10-07

*History, kept as it was written. The text below described the frozen surface the move to general
controls and route slots replaced (§ [the move](#the-move-to-general-controls-and-route-slots-2026-10-07));
its "permanent" and "never reused" were lifted for pre-alpha by the owner on 2026-10-07, and its IDs
are gone.*

**Slot parameters and their ids.** All sixteen slots carry Model, Pitch, Pitch envelope, Pitch decay,
Decay, Attack, Tone, Body, Noise, Noise decay, Character, Dynamics, Level, Pan, Mute and Solo even when
the selected model cannot use a shaping axis. The additive IDs are `pitch_env_1`…`pitch_env_16`,
`pitch_decay_1`…`pitch_decay_16` and `noise_decay_1`…`noise_decay_16`; every default is zero and
preserves the source/reference sound. They are directly automatable slot parameters and each of the
thirteen continuous slot controls is a routing target. The nice-plug nested-array
IDs are `<suffix>_<slot>` with one-based slots, including additive `mute_1`…`mute_16`,
`solo_1`…`solo_16`, `output_1`…`output_16`, `choke_group_1`…`choke_group_16` and
`midi_channel_1`…`midi_channel_16`. Output is stepped `L+R`, `1`…`16`; MIDI channel is stepped
`Kit`, `Ch 1`…`Ch 16`; Choke group is stepped `Off`, `1`…`16`.

**Globals, the three LFOs and the per-slot route grid.** The permanent globals are `master` plus
`lfo{1,2,3}_{rate,shape,sync}`. There are exactly three LFO generators for the kit, never three per
slot. Each `sync` is one on/off button beside its Rate knob. Off, Rate is hertz; on, the same knob
snaps across 4 bars through 1/32 and reads the selected musical division. Missing or invalid host
tempo falls back to the continuously advanced free Rate. The three pre-release `lfo{1,2,3}_division`
ids are retired: a separate Division parameter was the clunky two-control design the owner rejected on
2026-09-20, and none may be reused. The routing Cartesian product is per slot:
`route_<target>_<source>_on_<slot>` and `route_<target>_<source>_amount_<slot>`, for thirteen
continuous targets and seven sources. Every route starts absent at zero. Presence alone controls
whether the assignment exists and whether its row is shown; amount zero is valid and never removes
it. Realtime compaction is narrower than assignment: an assigned route at settled zero stays in the
interface and preset topology but leaves the per-sample DSP list until its smoothed amount moves
again. Removing one route leaves its dormant amount intact. The per-slot route trees and their
thirteen target groups are boxed, and the production shell boxes its DSP engine, so neither
constructing the 3,226-parameter surface nor moving the plugin through the CLAP wrapper can overflow
an ordinary host or validator thread's stack.

**The retired global route ids.** The owner's pre-release correction on 2026-09-20 retired all 108
unsuffixed global route IDs — the old nine-target/six-source `route_<target>_<source>_{on,amount}`
grid — rather than reusing them for one slot. They remain permanently retired. A state written before
this correction restores its sound parameters but cannot carry the old global modulation topology
into one arbitrary drum.

**From the brief** (`docs/briefs/mxm-drum-machine.md`), *Common per-slot parameter vocabulary* and
*Global and routing IDs* as they read until 2026-10-07, under its heading *Permanent product surfaces
fixed at D0 and extended additively*:

> Every slot has these permanent base IDs and concepts. Revision 33 appended `pitch_env`,
> `pitch_decay` and `noise_decay` without changing any earlier ID or meaning. nice-plug's nested-array
> convention makes the full IDs `<suffix>_1` through `<suffix>_16` — for example `model_1`,
> `pitch_1`, … `solo_16`. The one-based numeric suffix is part of the permanent ID.
>
> | Suffix | Canonical host name inside slot N | Kind / zero meaning |
> |---|---|---|
> | `model` | `Slot N model` | Fixed-domain stepped integer; Init chooses a useful model. |
> | `pitch` | `Slot N pitch` | Bipolar continuous pitch-law deviation; `0 st` is reference. The editor labels it Tune. Unavailable where no honest pitch law exists. |
> | `pitch_env` | `Slot N pitch env` | Additive bipolar depth deviation; zero preserves a native reference sweep or leaves a source-accurate no-sweep circuit unchanged. |
> | `pitch_decay` | `Slot N pitch decay` | Additive bipolar pitch-envelope time deviation; zero preserves source/reference timing. |
> | `decay` | `Slot N decay` | Bipolar continuous time/feedback deviation; zero is reference and positive travel extends beyond stock where the topology permits. |
> | `attack` | `Slot N attack` | Bipolar excitation/click/burst deviation; zero is reference. |
> | `tone` | `Slot N tone` | Bipolar internal spectral/filter deviation; zero is reference. |
> | `body` | `Slot N body` | Bipolar tonal-body balance/shape deviation; zero is reference. |
> | `noise` | `Slot N noise` | Bipolar noise/snappy contribution deviation; zero is reference. Snare cards label it Snappy. |
> | `noise_decay` | `Slot N noise decay` | Additive bipolar time deviation for a distinct wire/noise envelope; zero is reference and unsupported models are exact no-ops. |
> | `character` | `Slot N character` | Bipolar model-specific metal/room/nonlinear character deviation; zero is reference. |
> | `dynamics` | `Slot N dynamics` | Bipolar deviation from historical velocity/accent response; zero is reference. |
> | `level` | `Slot N level` | Linear gain formatted dB; Init balances the kit. |
> | `pan` | `Slot N pan` | Bipolar stereo placement; centre is reference. |
> | `mute` | `Slot N mute` | Boolean post-circuit silence; false by default and Mute wins over Solo. |
> | `solo` | `Slot N solo` | Boolean post-circuit isolation; if any is true, only unmuted soloed slots sound. |
> | `choke_group` | `Slot N choke group` | Stepped `Off`, `1`…`16`. Slots sharing a group cut each other through a bounded de-click, whatever models they are. `Off` by default: nothing chokes unless assigned. A **kit** setting — which slots cut each other is sound design — so presets carry it, unlike `output` and `midi_channel`. |
>
> Parameter ranges, smoothing and route full scales are measured before D2's first bundle, then
> become compatibility surface. The vocabulary and IDs above are frozen now. An unavailable axis
> remains an ordinary parameter with zero effect for that model, so model automation never changes
> the host's parameter inventory.
>
> The owner's 2026-09-20 pre-release correction replaced the global route grid with per-slot
> routing. Its 108 unsuffixed IDs are retired and never reused; the slot-suffixed IDs below are the
> permanent surface.
>
> Global parameters are: `master`; `lfo1_rate`, `lfo1_shape`, `lfo1_sync`, and the matching `lfo2_*`
> and `lfo3_*`. The rejected pre-release `lfo{1,2,3}_division` ids are retired.
>
> The fixed route sources, in evaluation order, are `lfo1`, `lfo2`, `lfo3`, `wheel`, `pressure`,
> `velocity`, `random`. The fixed targets are `pitch`, `pitch_env`, `pitch_decay`, `decay`,
> `attack`, `tone`, `body`, `noise`, `noise_decay`, `character`, `dynamics`, `level`, `pan`.
>
> For every slot/target/source combination two permanent IDs exist: `route_<target>_<source>_on_<slot>`
> and `route_<target>_<source>_amount_<slot>`. That Cartesian product is 1,456 presence/amount pairs
> across sixteen slots. Optional creative routes are all absent and zero in Init. Preset files store
> the grid sparsely: no fields for an unassigned pair, presence alone for an assigned zero route, and
> both fields for an assigned nonzero route. Velocity reaches each hit directly and is not a route.
> Per-note tuning and channel bend enter model pitch directly, not as removable routes.
>
> […] one full Pitch route reaches 12 semitones and one full Level route reaches 12 dB […]
> Presence is discrete and an absent pair contributes nothing while retaining its dormant amount.
> An assigned route at settled zero remains assigned and visible but is omitted from the compact
> per-sample DSP list; moving it away from zero activates it again.
>
> All shaping deviations and optional route amounts begin at zero; route presences are absent.

The 12 dB Level reach was already stale there: the modulation standard (2026-09-26) had made Level's
routes the Amplitude factor, as § *Routes and velocity* records.

**The contract text it replaced** — this plugin's `AGENTS.md`, and the index rows that described it,
as they read until 2026-10-07:

> Owns `Cargo.toml`, `README.md`, `control-map.json`, `src/`, `presets/`, `tests/` and `host-tests/`;
> the licence is the repository's `LICENSE`. It owns permanent CLAP/parameter identity, parameter
> smoothing, host event translation, telemetry and editor.
>
> - Routes are the collection's modulation standard: every amount is
>   `mxm_modulation_params::reading::amount_param`; `level` routes read Amplitude in percent, their
>   ids stay `route_level_*` (`every_route_parameter_says_what_the_dsp_does`).
>
> ## Parameter surface ([NOTES.md § Parameters](NOTES.md#permanent-parameter-and-routing-surface))
>
> - All sixteen slots carry Model, Pitch, Pitch envelope, Pitch decay, Decay, Attack, Tone, Body,
>   Noise, Noise decay, Character, Dynamics, Level, Pan, Mute and Solo whatever the model; changing
>   Model never changes the host parameter inventory. Every default is zero, the reference sound.
> - IDs are `<suffix>_<slot>`, one-based (`pitch_env_N`, `pitch_decay_N`, `noise_decay_N`, `mute_N`,
>   `solo_N`, `output_N`, `choke_group_N`, `midi_channel_N`). Output is stepped `L+R`, `1`…`16`;
>   MIDI channel `Kit`, `Ch 1`…`Ch 16`; Choke group `Off`, `1`…`16`.
> - Globals are `master` and `lfo{1,2,3}_{rate,shape,sync}`: exactly three kit LFOs, never per slot;
>   `sync` is one button snapping Rate to a division; no valid tempo means the free Rate.
> - Routes are `route_<target>_<source>_on_<slot>` / `_amount_<slot>`, starting absent. Presence alone
>   decides whether a route exists; amount zero never removes it; removing one keeps its dormant
>   amount. Route trees and the shell's DSP engine are boxed, so no host thread's stack overflows.
> - **Permanently retired, never reused**: `lfo{1,2,3}_division` and the 108 unsuffixed global
>   `route_<target>_<source>_{on,amount}` ids.
>
> - The editor implements [`../../docs/briefs/mxm-drum-machine.md`](../../docs/briefs/mxm-drum-machine.md).
>   No standalone Routes card: each continuous control's route stack is in the card that owns it,
>   bound to the selected slot. Each LFO row is Rate, Sync, then six **drawn** shapes.
> - Labels: Pitch reads Tune; snare Noise reads Snappy. Unsupported controls stay visible and
>   disabled. **No engineering prose in editor copy**; the model's description is the selector's
>   hover text, in the sound's words.
> - Kits never carry Output or MIDI channel. Routing is sparse: an unassigned pair stores nothing, an
>   assigned zero route presence alone, an assigned nonzero route both.
> - **One capture per engage**: nothing re-renders while engaged, because installing a kit cuts
>   sound. To change a frozen kit: off, edit, on. Pitch and Decay stay live as playback controls.
> - `pack.rs` writes 24-bit WAV one-shots and a manifest through mxm-kit's `mxm-audio-file`, never
>   `mxm-audio-file-decode`. Files stop before the mix: unity gain, centred, Pitch and Decay applied
>   (`the_mix_cannot_change_an_exported_byte`). A pack's rate is fixed (`editor::PACK_SAMPLE_RATE`),
>   whatever the host runs at; the capture that plays follows the host.
>
> `plugins/AGENTS.md`'s index: | [`mxm-drum-machine/AGENTS.md`](mxm-drum-machine/AGENTS.md) | Original
> sixteen-slot drum instrument over an append-only pool of machine-specific circuits; all 94 admitted
> models owner listening-approved, Kit or chromatic MIDI per slot, stereo main plus sixteen mono
> outputs, nine source-family audition presets on one canonical role map |
>
> The root `AGENTS.md`'s index: | [`plugins/mxm-drum-machine/AGENTS.md`](plugins/mxm-drum-machine/AGENTS.md)
> | Drum-machine identity, fixed slot/model/output/choke/channel and per-slot routing parameters, two
> output layouts, Kit/chromatic event ownership, sparse kit presets, telemetry and reflowing editor |

## Interface

### Layout, cards and the app bar

The editor implements [`../../docs/briefs/mxm-drum-machine.md`](../../docs/briefs/mxm-drum-machine.md):
the collection app bar, dynamic card paging, two eight-row slot cards, one combined card with three
stacked kit-wide LFO rows, and selected-slot Model, Excitation, Body, Envelopes/Tone and Output cards.
Each LFO row is the same compact sentence: Rate knob, adjacent Sync button, then Shape — its six
shapes **drawn**, two rows of three, not named in a menu (design system §7.3, the owner's rule of
2026-09-23 that every plugin draws its waveforms); Sync changes
the Rate reading from hertz to the snapped musical division and reveals no second selector. This row
was the collection's tempo-sync pilot; its ladder is now `mxm-tempo`'s LFO ladder
(`params::LFO_SYNC`, the same fourteen divisions at the same positions), the button the collection's
quarter note (`binding::sync_picture`), the rate resolved once per block and the reading the free
hertz when no tempo is in force (`Telemetry::tempo`). There is
no standalone Routes card or target picker: every knob's routes are rows under it, in the card that
owns it, bound to the selected slot only. *Since 2026-10-07* they are drawn over the slot's four route
slots exactly as mxm-model-drums' editor draws its own (`route_stack`, `target_line`, `route_row`,
`stack_size`): the knob's line offers `‹ modulate ›` with the sources not already on it while a slot
is free; choosing one takes a free slot — one already aimed at that knob first, so a source added
back keeps its depth — writing its source, its target and (for a slot aimed elsewhere) a zero depth
as one bracketed gesture (`adding_a_route_takes_the_first_free_slot_in_one_gesture`); a row's remove
switches its source off, one write, the depth kept. *Until then* each knob carried the collection's
`mxm-modulation-params` stack, one fixed presence-and-amount pair per source, which cannot draw a
route slot that any source and target can take.
Output is the selected
slot's output stage, so it is the last card (Tone category, design system §3.4; owner, 2026-09-18):
its Level/Pan and its Output, Choke Group and MIDI channel selectors; Pan's help says that mono individual outputs
bypass it. Under the Output selector sits the one kit-wide row, **All slots**: `L+R` sends every slot
to main and `Own outputs` sends slot N to output N, as one balanced edit of the sixteen existing
`output_*` settings through `binding::set_together`. It is not a parameter and stores nothing: the
lit cell is derived from the settings, and neither is lit when they form neither pattern, so a
per-slot exception stays in the selector above. A stored mode was rejected because an `Auto` value
would change the frozen selectors' range. It lives in this card rather than the app bar because
design system §3.1 keeps rare actions out of the bar. The whole kit's Master is not a slot control: it is an inline slider in the app bar beside
the output meter (design system §3.1), drawn through `mxm_ui::navigation::bar_card` so the keyboard
cursor reaches it. Slot rows sit at the
pointer floor (`MIN_TARGET`) with activity painted as a two-point inset inside the model button;
they keep the immutable two-digit ordinal outside the compact model button and do not display the
MIDI note number. It is an
original software interface; no pads, step-key row, source panel layout, product colours or hardware
typography.

### Labels and help text

**The host names a control by its number, `Control k`; the panel by the model's name for it**
(`editor::controls`, since 2026-10-07's second step; § [Each model's controls](#each-models-controls-2026-10-07)):
a model shows only the controls its code reads, each under its own name and with its own hover
help, and its route rows read that name. The knob's accessible name is the host's (`Bound::panel`).
*Earlier the same day* every model read the names of before — Tune, Decay, Tone, Attack, Dynamics,
Pitch envelope, Pitch decay, Body, Noise (Snappy on a snare), Noise decay, Character — with one
hover help each for every model; *until 2026-10-07* the host names were the axes' own — Pitch,
Decay, … — and only Pitch was relabelled on the card.
Pitch envelope and Pitch decay separate excursion depth from time, and Noise decay is
available only for distinct wire/noise envelopes. Decay's positive half reaches bounded extended
ranges while zero remains source/reference. *Until 2026-10-07* unsupported controls remained visible
and disabled; now they are not drawn (a route left on one is, as "(unused)"). Do not explain their
implementation contract in the editor. All visible help and tooltips use musician-facing
language. Parameter identity, exact-no-op guarantees, evidence status and other engineering prose stay
in this document and the brief, never in editor copy.

### Selection, telemetry and the sound trace

Selection of the slot being edited is transient editor state, not a parameter. Clicking a slot does
not audition it. The selected slot is clear across its whole row without shouting: selection fill,
a two-point accent outline and a narrow leading accent rail; the model text keeps the same weight.
Selected and unselected frames reserve identical stroke and margin geometry, so selection never
resizes or shifts the model/M/S controls. Clicking `S` also selects its slot. All durable edits are
bracketed host parameter gestures through one binding module.
DSP-to-editor slot activity and output peak use atomics and may drop frames. The Model card's sound
trace is not telemetry and never runs on the audio thread: the UI renders two seconds from the actual
selected DSP model and current shaping controls and caches it by a 1/64-step signature. Of its 96 peak
bins, 64 show the first 80 ms linearly so Tune, pitch contour, Tone and source texture remain visible;
32 compress the remaining tail quadratically so Decay remains visible. Do not replace it with generic
decorative curves or a tail view that hides parameter edits.

### Card trees, floors and painted names

**Every card body is a `mxm_ui::tree`** (`editor::card`), built each frame from the parameters and
the selected slot, measured for its floor and height and drawn leaf by leaf through the bindings
(`editor::paint`, `paging::editor::show`). **Floors are computed**, never typed: `page_items`
takes each from its card's tree, so a floor follows the selected slot's model — the Model
selector's widest option, *Noise* or *Snappy* — and every knob's routes at their widest row. **The
model's description is the Model selector's hover text, not a line on the card** (the owner,
2026-09-27: no help text on the panel, and hover text written for the player), in the sound's words
— *A bright open hi-hat with a long ring* — never the circuit's. Each card is exactly as wide as its floor: its ceiling is its floor
(`plans/plan-editor-standard.md` A1). **The one declared number is the model button's own minimum,
`SLOT_MODEL_MIN`**: the button truncates, and the owner's judgement of how wide a slot's name must be
to read (2026-09-18, 290 points of card) is held since R2 on the control it protects rather than as
a card minimum (A2). What the editor states for what it draws itself: a slot row is `MIN_TARGET`
tall and at its narrowest its frame's margins, the number's `SLOT_NUMBER_WIDTH`, the model button at
`SLOT_MODEL_MIN`, Mute and Solo and the row's spacing (`slot_row_min_width`). **Painted names:** the
LFO knobs read *Rate 1*–*3* (the card says *LFOs*), and the Tune knob's routes read *Tune*, as its
knob does — and since 2026-10-07's second step every route row reads its knob's name: Level, not
the standard's *Amplitude*; a snare's *Snappy*, not *Noise* (`Axis::name`). *Before*, the others
read the DSP's target names — Level's *Amplitude* — (`Axis::routed_name`, `panel_name` until
2026-10-07). The controls' names are sentence case (*Pitch drop*, *Pitch decay*, *Snappy decay*); the mechanism display is `MECHANISM_HEIGHT` tall and fills
the Model card; every knob stands in the collection's knob row or column
(`mxm_ui::control::knob_column`), the LFO rate's widest reading the longest of its hertz reading and
every musical division.

### Presets and the audition kits

The collection preset browser, generated Init, user saves, banks and favourites are live. Nine
factory audition presets each contain models from exactly one source machine. Their names carry a
recognisable numeric source token (`Bridge 808`, `Reset 909`, and peers) by owner request while
omitting manufacturer names and full model designations; `README.md` carries the exact reference
mapping. The Model selector uses those same family labels in the same order, as visually bold
non-selectable headings. Off is not offered: each slot row carries compact square `M` and `S`
buttons for the permanent Mute and Solo parameters, with full names retained in tooltips and the
accessibility tree. Clicking `S` also selects that slot, because isolating a voice normally precedes
editing it; `M` changes only Mute. Model IDs and model option order remain unchanged. The model search survives closing and
reopening the menu until its explicit cross-shaped clear action is used. Every preset uses this fixed
role map:
slots 1–5 Kick/Snare/low/mid/high drum; 6–8 second low/mid/high pitched percussion; 9 Rim; 10
Clap/brush; 11/12 closed/open hat; 13 Cymbal/crash; 14 Cowbell; 15 Clave; 16 auxiliary percussion.
Factory and user kit presets never carry Output or MIDI channel, so auditioning cannot rewire a DAW
or controller. Routes are sparse in preset files (since 2026-10-07): a route slot not in use — its
source or its target Off — stores none of its three fields, its dormant target and amount included;
an in-use route at zero depth stores its source and target; one with depth stores all three
(`routes_capture_sparsely_without_conflating_zero`). *Until then* the grid's pairs were sparse the
same way: an unassigned pair stored neither presence nor its dormant amount, an assigned zero route
presence alone, an assigned nonzero route both. Resolving every omission writes the parameter
default, so a kit load clears unrelated live routes. Every kit carries Controls 12–20, at zero; the
nine kits were rewritten to the general controls on 2026-10-07 by their generator
(`write_the_factory_presets`), every value under its new ID unchanged.
These are audition mixes, not sixteen solo levels: the DSP first places every model's isolated
zero-deviation reference hit on its documented common peak plane, then Kick and Snare stay at 0 dB;
slots 3–8 and Clap sit at −6 dB; Rim, Cowbell and Clave at −8 dB; both hats and auxiliary percussion
at −10 dB; and Cymbal/crash at −12 dB. A missing machine role is muted, never filled by moving another role. Its retained fallback model
belongs to the same family and does not move onto the missing role audibly. The tests prove all 94
models appear exactly once across the nine authored assignments and every file matches this map. The fifty
creative kits follow them (§ [The fifty creative kits](#the-fifty-creative-kits)).

### The fifty creative kits

Plan §7.2 owed **at least fifty creative factory kits**, demonstrating faithful kits of the priority
and supporting families, mixed-machine kits that expose the model pool, pitched and resonant,
metallic, low-cost, electronic and experimental ranges, choke and shared-source combinations, and
subtle against extreme deviation. They were designed on 2026-10-07 for the owner to audition, outside
the repository, then written in: the design is `src/preset/creative.rs`'s table and the files sit in
`presets/` beside the nine. Fifty, in the browser's order:

| Category | Kits |
|---|---|
| Reference (10) | Bridge Boom, Bridge Tight, Reset Club, Reset Ride, Expanded Studio, Classic Parlour, Discrete Lounge, Early Organ-Top, Compact Battery, Snap Pocket |
| Mixed machine (8) | Heavy Bottom, Bright Top; Reset Low, Bridge High; Rhythm Box Mixtape; One of Each; Bridged Pair; Lo-Fi Tops, Wooden Floor; Hand Percussion Machines; Snappy Hybrid |
| Pitched and resonant (5) | Pentatonic Toms, Diode Choir, Syn-Tom Sweeps, Woodshop Marimba, Talking Congas |
| Metallic (5) | Six-Square Foundry, Cowbell Chord, Rust and Chrome, Clockwork Hats, Noise Cymbal Wash |
| Low-cost (5) | Four-Voice Economy, Pocket Pair, Thrift Store Drive, Six-Bit Budget, Toy Box |
| Electronic (6) | Warehouse Pulse, Electro Breaks, Minimal Clicks, Trap Boom, House Shuffle, Big Snare Eighties |
| Experimental (4) | Rising Sweeps, Overdriven Wreck, LFO Drift, Glacial |
| Choke and shared source (3) | Hat Ladder, Cross-Cut, Shared Noise Section |
| Subtle against extreme (4) | Gentle Hybrid and Hybrid Unhinged; Rhythm Box Polish and Rhythm Box Meltdown, each pair one set of sixteen circuits |

Every one of the 94 models sounds in at least one kit (`every_model_sounds_in_some_kit`); the least
used, in three keys each, are 29, 73 and 91. The two machines of priority fill 274 of the 779
sounding keys. Each kit's comment in the table says what to listen for, and each key's why.

**The decisions** (the designer's, except where a ruling is dated as the owner's):

- **Choke** (the owner, 2026-10-07): *"The fifty choke as a real kit would (closed against open hat,
  plus the extra groups in your three choke kits). The nine audition kits keep choke Off."* The
  groups mean one thing across the bank: 1 the hats, 2 a bass voice (Cross-Cut's kick and the three
  kick-toms tuned to C2, D2 and F2, a monophonic line), 3 a conga's open and muted strokes, 4 a
  cymbal and the short hit that stops it (`every_kit_chokes_its_open_hat_with_its_closed_hat`,
  `the_audition_kits_ship_ungrouped`).
- **A Reference kit** is one machine's own voice: one family only; a control moves only where the
  machine had a panel control for it (Tune, Decay, Tone, Snappy, and the reset kick's Attack and the
  sweep time its panel calls Tune, here Pitch decay) or to re-pitch or re-time one of the family's
  voices for a role the machine lacked (the pocket box's kick tuned up into toms, the earliest box's
  one cymbal at three lengths); a role the family cannot play stays muted, as in the audition kits.
  No Character, Body, Pitch drop or Noise decay. Two kits each for the bridged-T and reset machines
  (their panels' two ends), one for each supporting machine but the economy box, whose four voices
  make Four-Voice Economy under Low-cost. At all-zero they would sit on top of the audition kits.
- **The role map** is the audition kits', so MXM Player's family-test beat plays every kit. A
  creative kit may put another sound on a role — a shaker on the closed-hat key, a cymbal stop on the
  clave's — never a kick on the snare's.
- **Names** are original words with no digits and no maker or model names; the audition kits keep
  their numeric tokens by the owner's request. The reference kits carry the selector's own family
  words (Bridge, Reset, Expanded, Classic, Discrete, Early, Compact, Snap)
  (`the_names_are_original_and_their_own`).
- **Category**: all fifty `Percussion`, the browser's word for a kit to play; the nine stay
  `Template`. The categories above are this note's, not browser folders.
- **Levels and pans**: the audition hierarchy one step livelier (kick and snare 0 dB, toms −6,
  percussion −7, clap −5, rim −8, hats −10, cymbal −12, cowbell and clave −9, auxiliary −10), moved
  per kit, and one drummer's-eye stereo picture (hats left, cymbal and auxiliary right, toms high
  left to low right). They were set on paper: Overdriven Wreck, Hybrid Unhinged, Rhythm Box Meltdown,
  Glacial and Big Snare Eighties want a level pass by ear.
- **A key moves only what its model reads** (§ [Each model's controls](#each-models-controls-2026-10-07)),
  never a control named *Gain* (Level does that), and never one that waits on another left where it
  waits for nothing: Pitch decay on the ten Pitch-sweep models only beside a Pitch sweep. The check
  reads the panel's own table and its help's *Only while …* (`every_key_moves_only_what_its_model_reads`).
- **Rising is Pitch sweep's, not Pitch drop's.** Below its centre, Pitch sweep bends a drum up into
  its note (Rising Sweeps); below zero, Pitch drop only shrinks a drop the circuit has.
- **Pitched kits tune by note.** A key names a note and the generator puts the model's measured rest
  pitch on it (`ModelId::reference_pitch_hz`), so a re-measured rest pitch shows up as kits to
  regenerate: Pentatonic Toms (F major pentatonic), Diode Choir (G major pentatonic), Woodshop
  Marimba (C major pentatonic), Cowbell Chord (A minor seventh over all five cowbells and the bell)
  and Cross-Cut's kick-toms.
- **Shared source** is made audible in Hat Ladder, whose six metal voices read one six-square bank
  at zero Tune, and Shared Noise Section, whose noise is brought forward on every reader of two
  noise buses. A nonzero Tune on a metal voice leaves the bank (the DSP's rule), so those kits keep
  their metal untuned.
- **Routes** in six kits: Velocity to Tone on Hand Percussion Machines' drums, Wheel and Velocity to
  Tune on Talking Congas' (the only Wheel), synced LFOs on Warehouse Pulse's open hat and ride and
  across LFO Drift, Random on Rising Sweeps' small percussion, a slow free LFO on Glacial's pans.
  Only those three set the kit-wide LFOs; every other kit leaves them at Init.
- **Open for the owner, used as they stand**: the studio machine's congas (37–39), the earliest
  kick (85), the battery box's cymbal (51) and the six-bit ride (27), each on the DSP's open list; a
  kit leaning on one is a re-listen after it moves. ID 53's open hat follows host tempo.

**Changed from the audition draft** (the scratch design the owner was sent): settings on controls
that are only a gain were dropped (Body on 22, 30, 32, 73 and 89; Noise on 11, 23 and 93, Shared
Noise Section's clap and maraca moved forward by Level instead); Rising Sweeps' kick and low conga
became circuits with a Pitch sweep (74, 75), because the draft's 60 and 69 have a drop of their own
and would only have dropped less; and two settings were dropped as unheard: Snappy on the studio
snare (33), whose wires sit far under its body (its key moved −55 dB), and Drive on Six-Square
Foundry's bank cowbell (about 0.5 dB), now untouched beside the hats it shares the bank with.

**The tests** follow plan §7.2's contract:

- **Files**: the ignored `write_the_creative_kits` writes the fifty; the runs-by-default
  `the_shipped_kits_are_their_designs` builds each in memory and compares, changing nothing in the
  checkout; `every_preset_file_is_a_listed_kit` keeps the folder and the compiled list one.
- **A default retune cannot collapse a kit's sparse overrides**, two ways. A file holds every
  parameter a kit stores, not only the design's overrides (`a_shipped_kit_never_leans_on_a_default`),
  so a changed default never reaches a shipped kit and the comparison above fails until the kit is
  regenerated on purpose; and every key that moves a control sounds apart from itself unset — same
  model, level and pan, controls at Init — by more than −60 dB of its peak
  (`every_setting_a_kit_makes_is_heard`). The faintest, measured: −45.7 dB, Trap Boom's clap room,
  shortened mostly after the window.
- **The bank render** (`every_kit_sounds_and_no_two_sound_alike`): Init, the nine and the fifty,
  each from a fresh activation with every key on an output of its own, all sixteen struck at velocity
  0.8 with the host at 120 BPM (the synced LFOs and the tempo-coupled hat follow it), ten 1,000-frame
  calls — 0.21 s, so a long-ringing kit is cut there. Every unmuted key sounds above −40 dBFS (the
  quietest, −17.9 dBFS, is in Rhythm Box Meltdown); every muted key is silent; Init rendered twice is
  one print. A key's print is four windows of three band energies against the key's own energy and
  its RMS frequency in dB, so level and pan move nothing; two kits are *alike* below 0.2 dB, the mean
  of their keys' print distances. Measured: the closest pair is Bridge 808 and Bridge Boom, 0.41 dB
  (the kick 2.7 dB of print, the cymbal 1.1, the toms 0.6–0.7: the reference kit moves only panel
  controls), then Reset 909 and Reset Ride, 0.82.
- **Cost**: every render the checks read (111: the bank, the fifty unset, Init again) is made once,
  in a `OnceLock`, spread over the machine's threads: **1.5–1.6 s** for the module on this machine
  (Windows, the fast tier's debug build, 2026-10-07). `print_the_kit_distances` (ignored) prints what
  the thresholds were chosen from. The render goes through `process()` with `full_layout`'s harness,
  now taking a host tempo (`render_at`).
- **The same sound for what was there**: after the change, `same_sound_digests` printed the 21
  digests recorded in § [The same sound through the move](#the-same-sound-through-the-move-to-general-controls-2026-10-07),
  line for line, for Init and the nine (Windows, 2026-10-07); it now also prints two lines a creative
  kit.

**The text the choke ruling replaced** (2026-10-07):

> The brief, §3: *Nothing chokes unless the user assigns it, and every factory kit ships ungrouped.*
>
> The DSP crate's `NOTES.md`: *The source machines' hardwired closed/open hat pairs are gone, and the
> factory kits ship with every slot at Off.*
>
> This plugin's `AGENTS.md`: *Choke group is a kit setting (presets, Init and every factory kit
> carry it, all `Off`).*
>
> This file, § Choke groups: *The machines' hardwired closed/open hat pairs are gone, nothing chokes
> unless the user assigns it, and every factory kit and Init ships at `Off`.*

## Resample is an instance setting, and the capture happens at activation

`resample` (plan §4.7, D9) plays each slot from a recording of itself instead of its circuit. It is
**one additive id** (taking the surface of its day from 3,226 to 3,227), and it is declared an **instance
setting** beside `output_*` and `midi_channel_*`: host and project state remember it, while preset
capture, apply, Init, completeness, the identity baseline and dirty comparison all exclude it. So
**no kit engages or disengages the mode** — loading a kit while frozen changes the kit and leaves
the mode alone, and the factory bank keeps measuring kits rather than the capture path. It is off
in Init and in every factory kit.

The sample-domain Pitch and Decay of §4.7 add **no** ids: they are the per-slot axes that already
exist, read in the captured domain by `mxm-drum-machine-dsp`'s reader — since 2026-10-07 Controls 1
and 2, Tune and Decay (`params::control::TUNE`, `DECAY`).

**`refresh_captures` is a control-thread operation and `activate` is where it runs.** It renders
seconds of audio and allocates, so it never belongs to `process`. Running it at activation is what
makes a restored session deterministic: a project that opens with Resample engaged is frozen from
its first sample, with no worker timing anywhere in the restore path — which is also the path the
player's export takes, so an offline bounce of a frozen project is reproducible.

The cache's key is `captured`: the patch the buffers were rendered from, and the rate. A capture is
a pure function of those two, so comparing them is the whole of invalidation and nothing else needs
storing. A capture that fails leaves the instrument **live** rather than silent, which is the safe
way round — the circuits are always a correct rendering of the patch.

**Toggling while the plugin runs goes to the plugin's own capture thread.** The toggle arrives as a
parameter change in `process`, where seconds of audio cannot be rendered, so `service_captures`
stores the wanted rate in `telemetry::capture_wanted` and `capture_worker` renders and publishes.
The instrument keeps playing live until the kit lands. **Disengaging is immediate**, because
dropping the mode needs no render.

### Resample and Export samples live in the app bar

**Kit-wide controls do not sit on a per-slot card** (owner, 2026-09-22). Every card but the slot
pickers and the LFOs belongs to the *selected slot*, so a global Resample on the Output card read
as "resample this drum". Design system §3.1 slot 6 already keeps Master beside the meter for the
same reason, and the pair now sits at the left of that group.

They are laid out *along* the bar, in their own left-to-right region inside its right-to-left
group, and **sized from the widest of the two labels** (`resample_pair_width`), which the toggle
takes through `mxm_ui::control::toggle_stack`. That measurement is
not decoration: the export label was renamed from a shorter string to `Export samples`, overflowed
the width the pair had reserved, wrapped the whole app bar and cost the window 335 pt of height.
mxm-kit's `crates/ui` "reserve the widest form" (*The app bar*, in its `AGENTS.md` and `NOTES.md`)
is what an under-reserved bar control breaks.

**It is "Export samples", not "Export pack".** The preset browser's footer already carries an
`Export…` that packs *presets* into a bank file, in the same window; two buttons reading "Export"
that do unrelated things is a trap.

The Resample toggle is a `navigation::bar_card` like Master, so the keyboard cursor reaches it.

**They are the last thing a narrow bar gives up** (owner, 2026-09-26). The pair is the bar's
`shell::product_actions`: past every other compact step it leaves the bar for the `…` menu, as
rows with the same words, help and enabled state (`resample_menu_items`), and its cursor card
leaves with it (`navigation::bar_card_absent`). Choosing Resample there is the same bracketed
write as the toggle; choosing Export starts the same folder pick, and the answer is collected every
frame wherever it was started (`collect_export`). Without this the pair kept the bar about 740
points wide and hid the `…` menu in any narrower window; with it the bar sets `MINIMUM`, and
`in_the_minimum_window_the_pair_is_in_the_menu_and_resample_works_from_it` holds the menu path.

### One capture per engage — a parameter edit while frozen renders nothing

**This is the rule** (owner, 2026-09-21). To change a frozen kit you turn Resample off, make the
edits, and turn it on again. `service_captures` therefore never consults the patch; `captured` is
the installed kit's **sample rate** and nothing else, and `asked` is cleared only by disengaging.

It is a correctness rule, not an economy. `Engine::set_captures` resets every capture voice,
because readers index into buffers that are about to be freed — so *installing* a kit cuts
whatever is sounding. While Decay was part of the invalidation key, one knob sweep queued a kit
per block and the kit-wide cut fired on each one: the owner heard it as cutting and crackling
across the whole instrument. Nothing re-renders while engaged, so nothing cuts.

The rate is still watched, because a kit rendered at a rate the engine has since left is
unplayable rather than merely out of date; it is retired unplayed and re-asked for.

Pitch and Decay remain live *as playback controls* under this rule — they shape the buffer that is
already there and schedule no render. Decay shortens in place; a longer tail needs a new capture,
which is what the off/on cycle is for.

### The capture thread is the plugin's own, and nice-plug's shared one is off limits here

Every other plugin in the collection that renders off the audio thread uses
`AsyncExecutor::execute_background`. This one did too, and it **crashed hosts**;
`clap-validator` reproduced it on `param-set-events`. The mechanism is in
nice-plug's `src/event_loop/background_thread.rs` and it is not about how long a task runs:

1. the worker thread is **shared by every instance of the plugin in the process**, and each task
   carries a `Weak` back to the instance that scheduled it;
2. when the worker picks up a task whose instance has since been destroyed, the upgrade fails and
   the worker **returns**, killing the shared thread and dropping the channel's receiver
   (`background_thread.rs:148`);
3. the handle outlives it, and its `Drop` then calls
   `tasks_sender.send(Message::Shutdown).expect(…)` on a disconnected channel — a panic out of a
   `Drop`, mid-teardown (`background_thread.rs:98`).

So **any task still queued when its instance goes away is a live crash**, and neither chunking nor
rate-limiting removes it; they only narrow the window. An intermediate version submitted from an
editor frame instead of from `process`, which narrowed it enough to pass the validator and
introduced a worse bug: **with the GUI closed nothing submitted at all**, so a host or a controller
moving a captured axis left the frozen kit stale indefinitely.

`capture_worker::CaptureWorker` is one thread per plugin instance, spawned in `activate` and joined
in its own `Drop`. Nothing is shared, so there is no `Weak` to fail; shutdown is a flag this module
owns; and the worker reads the request straight out of telemetry, so it does not care whether an
editor exists. `task_executor` is deliberately a no-op — **do not put work back on it**.

The costs, stated rather than hidden: one parked thread per instance, waking every `POLL` (20 ms)
to read one atomic, because `process` must not touch a lock and so cannot signal a condvar; and a
teardown that waits for at most one `CHUNK_FRAMES` render.

`service_captures` runs once per `process` call and is a pointer swap, a few comparisons and one
relaxed store: no allocation, no lock, and **nothing dropped**, because a displaced kit is
megabytes of `Vec<f32>`. `capture_bank::CaptureBank` is the handoff — a one-kit lock-free ring
where the audio thread *takes* ownership rather than borrowing, so it needs no reader counts, and
where a ready kit **waits** while a retired one is still unfreed, which is what guarantees the
audio thread always has somewhere to put what it displaces.

**Only a kit this engage asked for installs.** `MxmDrumMachine::engagement` numbers the engages:
it advances when a disengage ends one (once — the block where something was frozen or asked) and
at every activation, which supersedes whatever was asked before it and captures the current patch
itself. A request carries the number (`telemetry::CaptureRequest`), the worker stamps it on the
kit (`KitCapture::answering`), and `service_captures` installs a kit only if it carries the
current number and rate; any other is retired unplayed. Off, edit, on is the owner's way to change
a frozen kit, and without the number a kit the old engage asked for — still rendering, or waiting
in the bank because the disengage came first — installed as the new engage's: the old patch, and
the new engage, having a kit, never asked for the edited one
(`a_kit_from_an_earlier_engage_is_never_installed`,
`an_activation_supersedes_a_kit_still_rendering_and_leaves_room_to_disengage`). **The worker also
abandons a render a newer request overtakes** (`telemetry.capture_requested()`, checked between
chunks and before publishing), which spares rendering a kit that would be refused
(`a_render_a_newer_request_overtakes_is_abandoned`).

**The retired side has two slots**, because between two drains the audio thread can displace a
kit twice — once by taking a new one (installing it, or refusing it), which waits for both slots
to be empty, and once by disengaging, which must be immediate — and after disengaging it stays
live until it takes again or is activated (which drains first). **One slot was one short, and
production reached it**: a refused kit parks with nothing asking for a capture that would drain
it — after an activation has captured synchronously — and the next disengage found the slot full:
a debug assertion on the audio thread, or a leaked kit in release
(`a_take_and_a_disengage_between_two_drains_both_find_room`, and the activation test above).

**A test that stands in for the worker stops it first** (`plugin.worker = None`, which joins the
thread): a live worker answers the same request with a kit of its own, on its own clock, under
the test's assertions. Left running, it made
`toggling_resample_while_running_swaps_the_kit_in_through_the_bank` fail only under load — which
is how the slot count above was found.

**The rate travels with the request**, because only `process` knows it. A request is a single
`AtomicU64` — the rate's `f32` bits and the engage number — so repeated asks coalesce into the most
recent rather than queueing. A kit that comes back at a rate the engine is no longer running is
**retired unplayed** rather than installed, since the rate can change between asking and answering
and the wrong one plays every slot at the wrong speed.

## The sample pack — `pack.rs`

24-bit WAV one-shots and a plain-text manifest in a flat folder, written through
mxm-kit's `crates/mxm-audio-file`. **This is the collection's first plugin to ship that crate** (owner,
2026-09-21): the player already had it and every other consumer took it as a dev-dependency. No
MPL travels with it, because symphonia lives only in `mxm-audio-file-decode`, which this plugin
does not take.

**What lands in a file stops before the mix.** A frozen slot is heard as its capture, then the
sample-domain Pitch and Decay, then Level, Mute, Solo, Pan, Master and the Level/Pan routes. The
pack takes the chain **up to and including Pitch and Decay**, because those two are the one-shot
somebody shaped and auditioned, while everything after them is monitoring balance belonging to
whatever plays the pack next. So every file is unity-gain and centred, a muted slot still exports,
and `the_mix_cannot_change_an_exported_byte` holds it.

**Export requires Resample engaged** (owner, 2026-09-21) — you export the samples you have been
listening to. Disengaged, the button is present and disabled with its reason on hover, which is
the collection's treatment for a control that cannot be used rather than a hidden one.

It runs on the capture thread, reached from an editor frame through `editor::Spawn` and
`capture_worker::Gate`, and never touches the audio thread: rendering and writing files belong to
neither `process` nor an editor frame. Export is the *only* thing an editor frame asks for — a
capture is asked for by `process` — because it is the only one that begins with a person pressing
something. The gate exists from construction and outlives the thread, so a click either side of
the thread's life is inert rather than an error. `telemetry::Export` carries the outcome back for
the status line, which reserves its space at rest so a result arriving cannot move the card's
measured floor.

**The destination is chosen, and it travels with the job.** Export opens a folder chooser (owner,
2026-09-21) — a pack's natural home is an SD card, not AppData — starting at the default pack
folder so the usual place stays one click away. The dialog blocks, and blocking an editor frame
hangs the host, so it runs through `mxm_ui::offthread` exactly as the sampler's and the
convolution's file picks do; this is the collection's first *folder* pick and **its Linux portal
path is unverified from this machine**. Cancelling writes nothing.

The chosen path is resolved before the worker sees it, so `capture_worker` never consults the
environment. That is not only tidiness: while the worker called `pack::root()` itself, every run
of the export test wrote a real sixteen-file pack into the runner's own data folder — fourteen
accumulated in the owner's AppData before anyone noticed. **A test names a temporary directory;
nothing in this crate may resolve the real root inside one.**

**A pack is written at a fixed 48 kHz** (`editor::PACK_SAMPLE_RATE`), whatever the host is running.
Several trackers and grooveboxes resample anything that is not 44.1 or 48 on import, and a folder
whose rate depends on whichever session happened to export it is a trap. The capture that *plays*
follows the host instead, because it has to line up with the engine; these are two different rates
on purpose, and `kit.txt` records the one in the files.

**A default destination and no dialog** (owner, 2026-09-21), following the player: `mxm/sample-packs`
under the platform local data directory, resolved through `pack::root_under` with the root injected
so a test never writes into the folder of whoever ran it. That decision removed the `rfd`
dependency question rather than answering it.

**An existing pack is never overwritten.** Sixteen files and a manifest can fail halfway, which
per-file atomicity does not prevent, so a pack is written into a uniquely named sibling directory
and committed by one rename onto a path that does not exist — a taken name takes the next. There
is nothing to roll back, and the rename means the same thing on all three platforms, which a
rename over an existing directory would not.

**Still owed:** the frozen controls greying out, and §4.4's bus gating, which is what the CPU
claim rests on.

The editor draws the toggle as a kit-wide row on the Output card, under *All slots*: kit-wide like
its neighbour, on the output stage because a frozen kit is an output decision rather than a
per-slot sound one, and off the app bar because design system §3.1 keeps rare actions out of it.
Unlike *All slots* it **is** a parameter, so it goes through the ordinary bracketed gesture.

*(Superseded on 2026-09-22: the toggle now lives in the app bar; see* Resample and Export samples
live in the app bar*, above.)*

## Realtime

- Host tempo is read once per block. ID 53 uses it for the documented tempo-coupled open-hat decay;
  each of the three kit-wide LFOs also uses it when its own Sync is on. With no valid tempo an LFO
  remains at its free Rate. Every other model is tempo-independent.
- No allocation, locks, formatting, logging, I/O or blocking in `process()`.
- Event groups use fixed arrays/bitsets and are built only at samples where an event is due; an
  event-free sample does no collection or ownership work. A group holds up to 256 same-sample
  NoteOns and, separately, 256 releases/chokes/tunings. More NoteOns become global panic rather than
  a partial order-dependent chord. Past the second bound only a superset no host order can change
  survives: any choke in the group makes it panic; otherwise every owner releases expression and no
  tuning in the group applies, so a release flood cannot cut tails. The shell selects
  `mxm-part-routing`'s claimed-channel exclusion, removes choked NoteOns once per group with its
  `retain`, and supplies highest-key/velocity/note-ID/channel priority — a total order, so identical
  ID-less notes on two unclaimed channels still pick one owner — plus maximum-strike reduction to its
  bounded arbitrator. Controller state is reduced before note ownership; an existing owner's
  same-offset events are reduced, not replayed (choke wins; otherwise tuning, then release); then
  one winning trigger per slot. Order independence is a property of note ownership only: repeated
  same-offset bend/wheel/pressure on one channel, or repeated PolyTuning for one note, keep the host's
  event order and the last value wins, because a MIDI or CLAP event list is ordered and a later
  message at the same time (a bend returning to centre, say) is the current state; any fixed
  reduction would misstate it. The worst saturated group — sixteen slots layered on one channel,
  256 NoteOns and 256 non-matching chokes — measured 59 µs in release on the Windows development
  machine (2026-09-18), down from 725 µs with per-slot removal. Output selectors are applied before
  triggers. Process buffers are capped at 64 samples between event checks even when event-free.
- Parameter smoothers advance exactly once per rendered sample.
- Output telemetry max-combines once per internal block; slot peaks publish once per internal block.
- `ProcessStatus::Tail` is returned while any circuit remains active, otherwise `Normal`.

## Verification evidence

### clap-validator's parameter fuzz on a debug bundle (2026-10-06)

The release bundle passes `clap-validator` 35/35, which is the check this plugin's Verification
asks for. A **debug** bundle times out on `param-fuzz-basic` (clap-validator's 45-second limit),
because the fuzz sets every one of the plugin's thousands of parameters and processes audio in an
unoptimised build. It sat close to the limit already: 32–33 s on nice-plug 0.3.0, and over 45 s
after the fork's refresh onto 0.4.2. Release is unaffected: 2.93–3.04 s on 0.3.0 and 2.88–2.89 s
on 0.4.2, three runs each. Not fixed on purpose: the owner plans to lower the parameter count, the
way mxm-model-drums did, which removes the cause.

**2026-10-07, after the count was lowered** (3,227 → 651, § [the move](#the-move-to-general-controls-and-route-slots-2026-10-07)):
the debug bundle passes `clap-validator validate -t param-fuzz-basic` in **20.45 s** (20.99 s wall),
one run, Windows, nice-plug 0.4.2 — inside the 45-second limit with room to spare.

### The same sound through the move to general controls (2026-10-07)

The owner's rule for the move was *same sound, bit-identical on Windows*. Three things held it:

- **Every kit, every control, every route through `process()`**: `same_sound_digests` (an ignored
  harness in `lib.rs`'s `full_layout` tests) renders Init and the nine kits — all sixteen notes at
  once, half a second, main and sixteen individual ports — as they stand and with every used control
  moved to its own value, and Init with every slot's four routes on four different grid pairs. Run
  on the old surface and on the new, its 21 FNV-1a digests were identical, line for line:

  ```text
  Init / as-is: b01e4dbdbdb25318         Init / moved: a0dfea08809bbbaf
  Init / routes: 3da5bd969733816e
  Bridge 808 / as-is: 47e26294ed95e01d    Bridge 808 / moved: d04c7102cc40fb88
  Reset 909 / as-is: 0ebc5a34f2f6a1f3     Reset 909 / moved: 316c739781fb8c27
  Economy 55 / as-is: 77c93c8d3bbc16e5    Economy 55 / moved: dab404737d491659
  Expanded 8000 / as-is: 35f5327d3f54f00a Expanded 8000 / moved: 876893835c745540
  Compact 606 / as-is: ad8b734eb5af512e   Compact 606 / moved: 59c09dda792ebbd7
  Snap 110 / as-is: f38fb1a9eec33dd7      Snap 110 / moved: 8a39651dfdfa10f7
  Classic 78 / as-is: e50c73442cba7822    Classic 78 / moved: be760cccdbfb8f26
  Discrete 66 / as-is: 26ddb6d594e8fc31   Discrete 66 / moved: 72e815d10c0f8c3c
  Early 2L / as-is: 9eed8daa51434951      Early 2L / moved: aab983c6b37be73b
  ```

  These are a dated record, not a pin: a deliberate model change moves them.
- **The kits themselves**: each rewritten file holds every one of its 282 old values under the new ID
  the mapping gives it, `v` and `text` unchanged, plus Controls 12–20 at their defaults (144), checked
  against the files as committed before the move.
- **Through MXM Player**: the host test's recorded render, loaded through the ID mapping
  (§ *The recorded-kit fixture*), matches bit for bit.

### The same sound through honest controls (2026-10-07)

Showing each model only its controls (§ [Each model's controls](#each-models-controls-2026-10-07))
changes the editor alone: no DSP, parameter or preset file moved. Proved once, on Windows, after the
change: `same_sound_digests` printed the 21 digests recorded above, line for line, and the host
test's recorded kit (`the_recorded_kit_on_the_general_controls_renders_its_recording_bit_exact`)
matched its render bit for bit through a fresh release bundle.

### The recorded-kit fixture, once the pre-D7 compatibility fixture

`host-tests/tests/fixtures/mxm-drum-machine-pre-d7/` is project-generated D7 compatibility evidence:
a path-remapped Windows x86_64 bundle rebuilt from the fixed pre-D7 source commit, plus the original
pre-change non-default CLAP state and stereo float render and their manifest. Routine tests loaded
the state into the current bundle and compared against a current render, re-captured only for a
deliberate change to the slot's model sound (first on 2026-09-19); the retained old bundle and
original render are for manual release/DAW diagnosis.

**2026-10-07: the pre-D7 test is retired, its render kept.** The state holds the IDs of its day
(`pitch_1`, `decay_1`, …), which the plugin no longer reads and, with no migration in pre-alpha, never
will. `pre_d7_state_opens_with_routing_defaults_and_bit_exact_main_audio` was **removed** from
`host-tests/tests/behaviour.rs` for that reason (this note is its record). In its place
`the_recorded_kit_on_the_general_controls_renders_its_recording_bit_exact` rewrites the same state's
named-control IDs to the general controls by the mapping (`MAPPING` in that file — the rewrite is the
test's, not the plugin's), loads it, checks Tune restored on Control 1 and matches
`non-default-kit-main-current.wav` bit for bit on Windows, within rounding elsewhere. The fixture's
files are unchanged.

### The editor's standard and tree checks

The editor carries the collection's four standard checks, in `src/editor.rs`'s test module from
`mxm_plugin_test::paging_checks`, `mxm_plugin_test::opening_size` and
`mxm_plugin_test::keyboard_checks`:
`the_opening_size_is_the_budget_hugged`, `every_dynamic_page_fits_and_every_card_is_reachable` at
both sizes and both scales, `every_card_paints_inside_its_viewport_without_overlapping`, and
`the_keyboard_cursor_reaches_and_operates_every_parameter`. **`REFERENCE` is measured, not chosen**:
the hug check fails with the size the panel wants, and that size is the one to take. The
word-presence check in `tests/interface.rs`
complements these and does not replace them — it cannot see a control pushed off-screen.
`every_card_passes_the_tree_checks_in_every_state` and `every_models_cards_pass_the_tree_checks`
run `mxm_plugin_test::tree_checks`'s per-card checks — floor holds, content floor exact, stated
height drawn, nothing outside its leaf — over this editor's structural states: Init, every slot's
four routes in use at full negative depth on Tune, Body, Decay and Level (every route revealed, until
2026-10-07), all four on Tune, Resample on, every LFO synced, the last slot selected, one model
per distinct set of a shaping card's knobs and names (one per noise label until 2026-10-07's
second step), routes left on controls the model does not show, a legacy Off and an unavailable id — the slot cards
at their content floor, so a slot row's stated width is held to what it paints. Tests take
floors from `test_items`/`items_in`, which run `page_items` in a scratch editor context. Review
pictures of every page, light and dark:
`MXM_PICTURES=after cargo test -p mxm-drum-machine --lib tree_pictures -- --ignored`, written to
`target/layout-tree/mxm-drum-machine/after/`.

### What the tests cover

Checks cover identity, the 651 parameter IDs/defaults and their positions, each general control's
range and reading, every grid pair's one route spelling, route slots filling the grid (alone, added,
Off, unused), the route amount's travel against every pair's offer, route adding/removing/offering
in the editor, each model's controls held to what its code reads and a route on a control it does
not show drawn but never offered, presets pairing every host parameter with its own ID, model formatting, Kit/chromatic collision,
shared-channel chord arbitration under permutation, note ownership/tuning/choke including choke
after NoteOff/CC123 and host-order-independent owner events, chromatic bounds, both saturation
policies, stereo fold-down of stored outputs, Mute/Solo precedence, panic, both audio-layout
inventories and slot-numbered port names, output text entry by port name, the kit-wide output row
(its derived reading of all, own and mixed settings, and one balanced gesture that writes only the
outputs that move, from every start, leaving a loaded kit clean), preset exclusion of Output/MIDI channel across
capture/Init/every factory kit/dirty state, no process allocation, the shared crate's unlike-consumer policy fixture, editor paint/fit and all assigned
model-selector paths. The fifty creative kits match their design, store every kit parameter, move
only what their models read, choke closed against open hat and use every model; rendered with the
bank from a fresh engine at 120 BPM, every unmuted key sounds, no two kits sound alike and every
setting is heard (§ [The fifty creative kits](#the-fifty-creative-kits)). Every factory kit and Init sparsely omit unused routes while resolving
them back to Off/zero and leaving non-default Output and MIDI-channel instance settings intact. A
full-layout harness drives the plugin's own `process()` with sixteen mono
auxiliary buffers: every slot reaches exactly its selected port and nothing else, stereo
compatibility folds each to main, Master scales and Mute/Solo silence individual ports, and a live
output edit moves a tail between ports and retires the old one after 2 ms. Every factory kit and Init
also run through `mxm-preset`'s real write functions with non-default Output/MIDI settings and leave
them intact. The DSP suite proves main compatibility, mono Pan bypass, shared-output
summing, all sixteen slots on exactly their own output, a live reassignment of all sixteen, a
transfer at every sample phase, reversal plus latest-wins, and a silent slot — idle, or muted while
its circuit runs — taking a new output before its next hit or unmute. MXM Player opens the recorded
kit's state through the ID mapping and matches its main render bit-for-bit (the fixed pre-D7 state
through its own IDs until 2026-10-07), keeps a slot stored on an individual output audible in stereo compatibility, and
round-trips non-default Output/MIDI channel through CLAP state. Hardware fidelity, real multi-output
DAW restoration, Linux and macOS remain unverified. *Since the split (2026-10-06):* the main render
is matched bit-for-bit on Windows only and within rounding elsewhere, because the recording holds
Windows' bits (the owner, 2026-10-06); CI builds and tests Windows, macOS and Linux on `v*` release
tags or when started by hand, and Linux is checked in WSL before a push (root `AGENTS.md`,
*Verification*).
