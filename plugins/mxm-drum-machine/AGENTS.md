# AGENTS.md — plugins/mxm-drum-machine

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug CLAP shell and editor for `mxm-drum-machine`: sixteen MIDI-addressed slots selecting
from an append-only pool of machine-specific drum circuits. All 94 admitted models render in the
permanent framework and are owner listening-approved as of 2026-09-20. New circuits may append only
as complete, evidence-backed and individually auditable rows.

# Ownership

Owns `Cargo.toml`, `LICENSE`, `README.md`, `control-map.json`, `src/` and eventual `presets/`. It owns
permanent CLAP/parameter identity, parameter smoothing, host event translation, telemetry and editor.
DSP and model behavior belong to [`../../crates/mxm-drum-machine-dsp/AGENTS.md`](../../crates/mxm-drum-machine-dsp/AGENTS.md).

# Local Contracts

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

Deep bridge kick is the schematic-derived, fidelity-unverified TR-808 bass-drum function documented
by `research:instruments/analogue-drum-machines.md` §3.2. Its public label does not use the maker or
model name.

**Quick repeated strikes continue from live resonator and output-filter state.** This is the hardware
wart that prevents sample-like machine-gun identity; never reset phase/state on NoteOn.

**Its routes are the collection's modulation standard** (`plans/plan-modulation-standard.md`):
every amount is the shared route parameter, `mxm_modulation_params::reading::amount_param`, and the
`level` routes read **Amplitude** in percent — the standard factor, `+100 %` doubling the level —
where they read decibels. Their permanent ids stay `route_level_*`.
`every_route_parameter_says_what_the_dsp_does` holds every pair's travel and reading to
`mxm_drum_machine_dsp::conformance` (`mxm_plugin_test::routing_checks`).

**A hit is its own velocity** (owner, 2026-09-20). The machines' common and global accent buses are
not modelled and there is no accent control; one slot's level never depends on another slot's, and
`Dynamics` is the only control over the response. The DSP crate's AGENTS.md carries the ruling and
its reason. What survives is *collision* reduction, which is a different thing: several notes landing
on one slot at one sample are reduced to a single strike before any trigger, so host event order
cannot change the sound — but the value that sounds is the **winning owner's own** strike
(`ArbitrationResult::owner_strike`), not the combined one. One note owns the hit, so the hit is that
note's velocity: a chord on a chromatic slot strikes at the velocity of the note that also set its
pitch, never at a losing note nobody can hear.

Pitch moves resonator poles; Decay changes feedback-equivalent T60; Attack changes strike/pitch-jump;
Tone changes the passive output low-pass; Body changes sigh/leakage; Character changes local
headroom; Dynamics bends the velocity curve, as on every model (`crates/mxm-drum-machine-dsp`,
`velocity.rs`). Noise is visibly unavailable and an exact DSP no-op.
Its reference follows the 2026-09-19 comparison fit to an acquired recording:
- a trigger feed-through spike;
- a held pitch lift;
- a 48.8 Hz rest pitch.

`crates/mxm-drum-machine-dsp/AGENTS.md` owns the calibration. Fidelity remains hardware-unverified.

Twin-mode snare is the matching schematic-derived snare function from
`research:instruments/analogue-drum-machines.md` §3.3: two live-state struck bridged-T modes at the
comparison recording's 168.3 Hz and 331.9 Hz, in parallel with a wire path. Every simultaneous instance reads the same
fixed-seed machine-family white-noise sample, then applies its own filter and envelope; private
lookalike noise generators are forbidden. Noise maps the Snappy amount, Tone balances the two body
modes, and Character is visibly unavailable and an exact no-op. Fidelity remains hardware-unverified.

The six falling drums take their rest pitches, decays and diode-control pitch excursions from the
comparison fit. All three toms read the machine-shared coloured-noise tail, as all three recorded toms
carry it; only Low falling tom exposes it on Noise, and the congas carry none.
- Layered short rim drives 1761 Hz and 472 Hz modes through an asymmetric clipper and a 9 ms
  closing switch. Its trigger reaches the output 0.45 ms before the strike.
- Pure high clave rings at 2497 Hz until a 33 ms switch.

Noise and Character availability follows the brief exactly. Hardware fidelity remains unverified.

Both read the same machine-shared white-noise bus:
- Bright short maraca high-passes it behind a charging envelope, an 18.5 ms gate and a VCA threshold.
- Triple pulse clap runs it through a burst band, with bursts at 0/8/19.5/29.5 ms, and a separate
  reverb tail of about 2.1 s.

Pitch/Body and Pitch/Body/Character are respectively disabled exact no-ops.

The metal batch owns one continuously advancing six-square bank.
- **The bank:** band-limited squares at this unit's fitted frequencies and duties, not the service
  nominals (crate `AGENTS.md`).
- **Cowbell:** reads oscillators 5/6.
- **Cymbal:** reads its bands through three jointly fitted decay paths.
- **Hats:** both share one 7.1 kHz band, behind a one-polarity closed VCA and a soft-saturating open
  one. Neither articulation chokes the other unless their slots share a user-assigned choke group.
  Zero Pitch reads the shared frame, while nonzero Pitch uses a persistent private slot bank and
  rejoins shared phase at zero. Its render was accepted with the complete catalogue on 2026-09-20.

## Permanent parameter and routing surface

All sixteen slots carry Model, Pitch, Pitch envelope, Pitch decay, Decay, Attack, Tone, Body, Noise,
Noise decay, Character, Dynamics, Level, Pan, Mute and Solo even when the selected model cannot use a
shaping axis. The additive IDs are `pitch_env_1`…`pitch_env_16`,
`pitch_decay_1`…`pitch_decay_16` and `noise_decay_1`…`noise_decay_16`; every default is zero and
preserves the source/reference sound. They are directly automatable slot parameters and each of the
thirteen continuous slot controls is a routing target. The nice-plug nested-array
IDs are `<suffix>_<slot>` with one-based slots, including additive `mute_1`…`mute_16`,
`solo_1`…`solo_16`, `output_1`…`output_16`, `choke_group_1`…`choke_group_16` and
`midi_channel_1`…`midi_channel_16`. Output is stepped `L+R`, `1`…`16`; MIDI channel is stepped
`Kit`, `Ch 1`…`Ch 16`; Choke group is stepped `Off`, `1`…`16`.

**Choke is an assignment, not hardware wiring** (owner, 2026-09-20). Slots sharing a choke group cut
each other through the DSP's bounded de-click, whatever models they are — an 808 open hat closed by a
909 closed hat, or a long kick closed by a short one. The machines' hardwired closed/open hat pairs
are gone, nothing chokes unless the user assigns it, and every factory kit and Init ships at `Off`.
Unlike Output and MIDI channel, Choke group is a **kit** setting: which slots cut each other is sound
design, so preset capture, apply and Init all carry it. The two new families are
instance settings: host/project state stores them, while preset capture/apply, Init, completeness,
identity baseline and dirty comparison exclude them through `mxm-preset::Instrument::is_instance_setting`.
`master` is global. Changing Model never changes the host parameter inventory.
Mute and Solo are post-circuit: muted/non-soloed circuits keep running and preserve machine-shared
interaction; when any Solo is on only unmuted soloed slots sound, so Mute wins.

Model parameters store the fixed integer domain 0…255. The editor maps the available catalogue onto
the shared grouped `mxm-ui` caret selector, so arrows/search visit implemented models rather than
reserved or unavailable IDs. Host automation of an unavailable ID displays `Unavailable N` and
renders silence. Group headings are display-only and cannot shift a stored index.

The permanent globals are `master` plus `lfo{1,2,3}_{rate,shape,sync}`. There are exactly three LFO
generators for the kit, never three per slot. Each `sync` is one on/off button beside its Rate knob.
Off, Rate is hertz; on, the same knob snaps across 4 bars through 1/32 and reads the selected musical
division. Missing or invalid host tempo falls back to the continuously advanced free Rate. The three
pre-release `lfo{1,2,3}_division` ids are retired: a separate Division parameter was the clunky two-
control design the owner rejected on 2026-09-20, and none may be reused. The routing Cartesian
product is per slot:
`route_<target>_<source>_on_<slot>` and `route_<target>_<source>_amount_<slot>`, for thirteen
continuous targets and seven sources. Every route starts absent at zero. Presence alone controls
whether the assignment exists and whether its row is shown; amount zero is valid and never removes
it. Realtime compaction is narrower than assignment: an assigned route at settled zero stays in the
interface and preset topology but leaves the per-sample DSP list until its smoothed amount moves
again. Removing one route leaves its dormant amount intact. The per-slot route trees and their
thirteen target groups are boxed, and the production shell boxes its DSP engine, so neither
constructing the 3,226-parameter surface nor moving the plugin through the CLAP wrapper can overflow
an ordinary host or validator thread's stack.

The owner's pre-release correction on 2026-09-20 retired all 108 unsuffixed global route IDs — the
old nine-target/six-source `route_<target>_<source>_{on,amount}` grid — rather than reusing them for
one slot. They remain permanently retired. A state written before this correction restores its sound
parameters but cannot carry the old global modulation topology into one arbitrary drum.

## Interface

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
no standalone Routes card or target picker: every continuous slot control's
`mxm-modulation-params` stack is in the card that owns the control, bound to the selected slot only.
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

The permanent host name remains Pitch, while the musician card labels it Tune; snare Noise is
labelled Snappy. Pitch envelope and Pitch decay separate excursion depth from time, and Noise decay is
available only for distinct wire/noise envelopes. Decay's positive half reaches bounded extended
ranges while zero remains source/reference. Unsupported controls remain visible and disabled; do not
explain their implementation contract in the editor. All visible help and tooltips use musician-facing
language. Parameter identity, exact-no-op guarantees, evidence status and other engineering prose stay
in this document and the brief, never in editor copy.

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

**Every card body is a `mxm_ui::tree`** (`editor::card`), built each frame from the parameters and
the selected slot, measured for its floor and height and drawn leaf by leaf through the bindings
(`editor::paint`, `paging::editor::show`). **Floors are computed**, never typed: `page_items`
takes each from its card's tree, so a floor follows the selected slot's model — the Model
selector's widest option, *Noise* or *Snappy* — and every route stack at its widest reading. **The
model's description is the Model selector's hover text, not a line on the card** (the owner,
2026-09-27: no help text on the panel, and hover text written for the player), in the sound's words
— *A bright open hi-hat with a long ring* — never the circuit's. Each card is exactly as wide as its floor: its ceiling is its floor
(`plans/plan-editor-standard.md` A1). **The one declared number is the model button's own minimum,
`SLOT_MODEL_MIN`**: the button truncates, and the owner's judgement of how wide a slot's name must be
to read (2026-09-18, 290 points of card) is held since R2 on the control it protects rather than as
a card minimum (A2). What the editor states for what it draws itself: a slot row is `MIN_TARGET`
tall and at its narrowest its frame's margins, the number's `SLOT_NUMBER_WIDTH`, the model button at
`SLOT_MODEL_MIN`, Mute and Solo and the row's spacing (`slot_row_min_width`). **Painted names:** the
LFO knobs read *Rate 1*–*3* (the card says *LFOs*), and the Tune knob's stack reads *Tune*, as its
knob does (`panel_name`); the controls' names are sentence case (*Pitch envelope*, *Pitch decay*,
*Noise decay*), the routes' host names with them; the mechanism display is `MECHANISM_HEIGHT` tall and fills
the Model card; every knob stands in the collection's knob row or column
(`mxm_ui::control::knob_column`), the LFO rate's widest reading the longest of its hertz reading and
every musical division.

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
or controller. Routing is sparse in preset files: an unassigned pair stores neither presence nor its
dormant amount; an assigned zero route stores presence alone; an assigned nonzero route stores both.
Resolving every omission writes the parameter default, so a kit load clears unrelated live routes.
These are audition mixes, not sixteen solo levels: the DSP first places every model's isolated
zero-deviation reference hit on its documented common peak plane, then Kick and Snare stay at 0 dB;
slots 3–8 and Clap sit at −6 dB; Rim, Cowbell and Clave at −8 dB; both hats and auxiliary percussion
at −10 dB; and Cymbal/crash at −12 dB. A missing machine role is muted, never filled by moving another role. Its retained fallback model
belongs to the same family and does not move onto the missing role audibly. The tests prove all 94
models appear exactly once across the nine authored assignments and every file matches this map. The eventual
fifty-kit creative bank remains a separate content requirement.

## Resample is an instance setting, and the capture happens at activation

`resample` (plan §4.7, D9) plays each slot from a recording of itself instead of its circuit. It is
**one additive id**, taking the surface from 3,226 to 3,227, and it is declared an **instance
setting** beside `output_*` and `midi_channel_*`: host and project state remember it, while preset
capture, apply, Init, completeness, the identity baseline and dirty comparison all exclude it. So
**no kit engages or disengages the mode** — loading a kit while frozen changes the kit and leaves
the mode alone, and the factory bank keeps measuring kits rather than the capture path. It is off
in Init and in every factory kit.

The sample-domain Pitch and Decay of §4.7 add **no** ids: they are the per-slot axes that already
exist, read in the captured domain by `mxm-drum-machine-dsp`'s reader.

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
`crates/ui/AGENTS.md`'s "reserve the widest form" is what an under-reserved bar control breaks.

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
`vendor/nice-plug/src/event_loop/background_thread.rs` and it is not about how long a task runs:

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
`crates/mxm-audio-file`. **This is the collection's first plugin to ship that crate** (owner,
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

`host-tests/tests/fixtures/mxm-drum-machine-pre-d7/` is project-generated D7 compatibility evidence:
a path-remapped Windows x86_64 bundle rebuilt from the fixed pre-D7 source commit, plus the original
pre-change non-default CLAP state and stereo float render and their manifest. Routine tests load the
state into the current bundle and compare against a current render, re-captured only for a deliberate
change to the slot's model sound (first on 2026-09-19); the retained old bundle and original render
are for manual release/DAW diagnosis.

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
height drawn, nothing outside its leaf — over this editor's structural states: Init, every route
revealed at full negative depth, Resample on, every LFO synced, the last slot selected, one model
per noise label, a legacy Off and an unavailable id — the slot cards
at their content floor, so a slot row's stated width is held to what it paints. Tests take
floors from `test_items`/`items_in`, which run `page_items` in a scratch editor context. Review
pictures of every page, light and dark:
`MXM_PICTURES=after cargo test -p mxm-drum-machine --lib tree_pictures -- --ignored`, written to
`target/layout-tree/mxm-drum-machine/after/`.

Checks cover permanent identity, parameter IDs/defaults, model formatting, Kit/chromatic collision,
shared-channel chord arbitration under permutation, note ownership/tuning/choke including choke
after NoteOff/CC123 and host-order-independent owner events, chromatic bounds, both saturation
policies, stereo fold-down of stored outputs, Mute/Solo precedence, panic, both audio-layout
inventories and slot-numbered port names, output text entry by port name, the kit-wide output row
(its derived reading of all, own and mixed settings, and one balanced gesture that writes only the
outputs that move, from every start, leaving a loaded kit clean), preset exclusion of Output/MIDI channel across
capture/Init/every factory kit/dirty state, no process allocation, the shared crate's unlike-consumer policy fixture, editor paint/fit and all assigned
model-selector paths. Every factory kit and Init sparsely omit absent route pairs while resolving
them back to absent/zero and leaving non-default Output and MIDI-channel instance settings intact. A
full-layout harness drives the plugin's own `process()` with sixteen mono
auxiliary buffers: every slot reaches exactly its selected port and nothing else, stereo
compatibility folds each to main, Master scales and Mute/Solo silence individual ports, and a live
output edit moves a tail between ports and retires the old one after 2 ms. Every factory kit and Init
also run through `mxm-preset`'s real write functions with non-default Output/MIDI settings and leave
them intact. The DSP suite proves main compatibility, mono Pan bypass, shared-output
summing, all sixteen slots on exactly their own output, a live reassignment of all sixteen, a
transfer at every sample phase, reversal plus latest-wins, and a silent slot — idle, or muted while
its circuit runs — taking a new output before its next hit or unmute. MXM Player reopens the fixed pre-D7 state and matches its main render
bit-for-bit, keeps a slot stored on an individual output audible in stereo compatibility, and
round-trips non-default Output/MIDI channel through CLAP state. Hardware fidelity, real multi-output
DAW restoration, Linux and macOS remain unverified.

# Child DOX Index

No child AGENTS.md files. This document covers the plugin.
