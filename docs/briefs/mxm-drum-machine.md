# mxm-drum-machine — UI and catalogue brief

Required by mxm-kit's `docs/MXM_DESIGN_SYSTEM.md` §14 and written before editor or DSP implementation. Product
architecture: `plans/plan-mxm-drum-machine.md` revision 33. Hardware evidence:
`research:instruments/analogue-drum-machines.md`.

**Plugin:** `mxm-drum-machine`; permanent CLAP id `dk.mxm.mxm-drum-machine`. Stereo instrument
output, no audio input. Sixteen retriggerable slots on MIDI notes 36–51, each selecting one model
from the append-only catalogue in Appendix A.

**Evidence standing:** acquired comparison recordings have guided calibration, but none authenticates
the required hardware identity and chain, so every admitted model remains **fidelity unverified**.
The TR-909 PCM entries use three owner-selected NMF-12 sources: one shared by both hats, plus crash
and ride. Their source selection, offline method and fixed representation are recorded by the DSP
assets.

---

## 1. Primary sound-design task

**Build one playable sixteen-voice kit from real machine-specific circuits without flattening them
into generic drum categories.** The primary gesture is: select a slot, choose a model from the
searchable caret selector, and shape that circuit around its zero/reference detent. A second slot may
choose the same model and becomes a second local drum circuit; slots from one reference family still
share the source and interaction state the hardware shared.

The editor therefore leads with the sixteen-slot inventory, then the selected slot's model and
reference-centred controls. It does not lead with a step row: sequencing belongs to the host.

## 2. Controls reached for most

1. **Model** — the instrument's defining choice. The closed control is the collection caret selector;
   the long menu searches and groups models by voice family.
2. **Tune** — the panel's name for Control 1 (`c01`), the pitch axis: chromatic where the physical family
   permits it through poles, VCO control, metal-bank scale, PCM clock, or an evidenced noise
   clock/filter. It is unavailable where none is honest.
3. **Decay** — the most immediate articulation control, mapped to resonator feedback, VCA/envelope
   time, PCM envelope, or burst tail rather than one generic output envelope. Zero remains the
   source/reference setting; positive travel continues into a bounded extended-decay range.
4. **Tone** — the evidenced filter or spectral balance inside the selected model.
5. **Attack** — strike pulse, click/noise path, burst contour, or metallic attack where present.
6. **Pitch envelope / Pitch decay** — separate depth and time controls wherever a native or honest
   resonator/VCO excursion exists. They never replace base Tune or amplitude Decay.
7. **Noise decay** — independent wire/noise-envelope time where the topology actually separates it
   from the tonal body.

Model, Tune, Decay and Tone are Primary controls on their selected-slot cards. Attack, Pitch drop,
Pitch decay, Body, Noise/Snappy, Noise decay, Character, Soft hits, Level and Pan are Standard or
Compact according to measured card fit. Each slot
row also carries Mute and Solo. **A model shows only the controls its code reads, each under the
model's honest name** (the owner, 2026-10-07; the plugin's `editor::controls` and its `NOTES.md`, *Each
model's controls*); a control is never silently reassigned. *(Until 2026-10-07 an unavailable axis
remained named and visibly unavailable.)*

### Source-control coverage

The common surface is intentionally a superset of the source panels, not a reduction of them:

| Source control/function | Editor mapping |
|---|---|
| Per-voice Level | Level; Soft hits (Dynamics until 2026-10-07) bends the velocity response around reference. The source machines' accent is deliberately not modelled — see §3 |
| 808 bass Tone / Decay | Tone / Decay; Pitch and Pitch envelope are additional circuit-bend controls |
| 808 snare Tone / Snappy | Tone / Noise, locally labelled Snappy; Noise decay extends the distinct wire envelope |
| 808 tom/conga tuning; cymbal Tone/Decay; open-hat Decay | Tune; Tone/Decay; Decay respectively |
| 909 bass Tune / Attack / Decay | Pitch decay / Attack / Decay; Tune is correctly treated as pitch-envelope time, while added base Tune and Pitch envelope expose common modification nodes |
| 909 snare Tune / Tone / Snappy | Tune / Tone / Snappy, with separate pitch- and noise-envelope extension controls |
| 909 tom Tune / Decay | Tune / Decay, with separate Pitch envelope and Pitch decay |
| 909 crash/ride Tune; open-hat Decay | Tune; Decay, preserving PCM clock coupling and finite source length |
| Supporting-machine voice tuning, decay and balance controls | Tune, Decay and Level where the relevant row's capability is honest |

Sequencer-side tempo, pattern, fill, fade and start/stop controls remain outside the voice plugin by
the collection's host-versus-voice rule. No source sound control is omitted merely because its source
panel grouped it differently.

## 3. Signal flow that must be visible

```text
MIDI 36–51 ─► slot 1…16 ─► selected model circuit ─► level / pan / mute / solo ─┐
       │                      ▲                                        │
       │                      │ model-specific creative domains         ├─► stereo sum ─► Master
       │                      │                                        │
       └─► same-offset trigger/choke-group dispatch                     │
                              ▲                                        │
shared per-machine noise / metal / PCM source buses ───────────────────┘

LFO 1 / LFO 2 / LFO 3 (kit-wide) / Wheel / Pressure / Velocity / Random
        └─► each slot's own route topology ─► that slot's supported creative domains
```

Five relationships must be understandable without a manual:

- **Sixteen slots, larger pool.** Slot count is simultaneous availability, not model count.
- **One slot is one retriggerable circuit.** A repeated note strikes or resets existing state; it
  does not stack sample-style copies.
- **Reference is zero.** Every creative axis at zero returns to the catalogue row's source/reference
  law. The additive Pitch envelope, Pitch decay and Noise decay parameters default to zero in Init and
  every factory preset; widening a range must not move the zero-deviation sound or its fixed output
  trim.
- **Extended means beyond zero, never instead of source.** Decay retains the calibrated source
  setting at zero. Positive travel reaches bounded capacitor-switch/modification territory—4.8 s
  T60 for Deep bridge kick, twelve times reference for Reset punch kick, and generally eight times
  reference for extensible envelopes—while fixed PCM source duration remains honest rather than
  being looped or invented.
- **Shared means shared.** A reference metal/noise source is generated once per machine family and
  read by every relevant slot on the same sample.
- **Pitch is not one resampler.** Which mechanism moves a model's pitch — poles, VCO, bank, clock
  or filter — is this brief's and the model notes' record, not the panel's: the Model selector's
  hover text says in the player's words what the sound does (a six-bit hat's tuning changing its
  length), since the owner ruled out help text on the panel (2026-09-27).

**A hit is its own velocity** (owner, 2026-09-20). The source machines resolve simultaneous hits
through an accent mechanism — the 808's common trigger voltage, the 909's programmed accent values,
the CR-78/CR-8000/DR-110 global accent envelopes — and `research:instruments/analogue-drum-machines.md`
§822 records that those boxes have accent rather than continuous key velocity. **None of it is
modelled.** There is no accent control on this interface and there is not going to be one, because
this instrument is played from a keyboard or a DAW rather than from a step sequencer with an accent
button. One slot's level never depends on another's, and Soft hits (`dynamics`) bends that velocity
response around its reference.

Where several notes land on one slot at one sample, they are reduced to a single strike before any
trigger so that host event order cannot change the sound, and the value that sounds is the **winning
owner's own** velocity — never a losing note's.

**Choke is an assignment, not wiring.** Each slot carries a Choke Group; slots sharing one cut each
other through a bounded de-click, whatever models they are. The machines' hardwired closed/open hat
pairs are not modelled either: an open hat can be closed by any other slot, including one from
another machine or a short version of the same drum. Nothing chokes unless a slot is assigned to a
group. Init and the nine audition kits ship ungrouped; the fifty creative kits choke as a real kit
would, every closed hat cutting its open hat, and the choke kits add groups of their own (the owner,
2026-10-07).

## 4. Play view

There is no authored Play tab. Space-derived paging opens on the first **Slots** card in Performance,
where a complete kit can be read and a slot selected. The app bar carries presets, Master and the
output meter (design system §3.1). Host or
MXM Player supplies note input and sequencing.

The two Slots cards are overview/selectors, not fake pads and not a copied step-key row. Each fits
eight rows at the pointer floor (`MIN_TARGET`), with no separate activity-bar row or decorative vertical
padding. The activity meter is a two-point inset at the bottom of the model button, where the row's
selection border cannot cover it. The immutable two-digit slot number sits outside the model button;
MIDI note
numbers are not shown. Each row also shows the current public model label, selected state, bounded
activity and compact square `M`/`S` buttons for Mute/Solo. Their full names remain in tooltips and
accessibility. Clicking the model button selects what the Generator/Tone cards edit; it does not fire
a transient note. Clicking `S` also selects that slot. Selection must be unmistakable across the
whole row—selection fill, a moderate accent outline and a narrow leading rail—not only the shared
button's slight selected-state shift. Text weight and all frame geometry stay constant, so selection
never moves or resizes the controls. Mute and Solo act after the circuit, preserving tails and shared-machine state;
Mute wins over Solo, and any active Solo silences all unmuted non-soloed slots.

## 5. Advanced controls and disclosure

**A model shows only the controls its code reads** (the owner, 2026-10-07: honest controls), each
under the model's own name, with what it does — and what it waits on, where it acts only while
another control is set — in its hover text. A control the model never reads has no knob; its stored
value stays, unread. A route still aimed at one is drawn, marked unused, and never offered more.
*(Until 2026-10-07: "No model sound control is hidden merely because only some models use it. The
fixed selected-slot axis cards show all eleven canonical shaping axes, reducing unavailable ones in
emphasis and stating why.")*

The only permitted disclosure is **Model notes**, inside the Model card: concise, musician-facing
sound mechanism, pitch behaviour and audible shared interactions. Evidence status, parameter
identity, compatibility guarantees and implementation terminology belong in project documentation,
not editor help. The editor is measured with Model notes open. There is no setup drawer, utility
macro layer, model editor, randomizer or mouse-audition command path.

Routing is parameter-local and slot-local. Each continuous control's owning card places its route
rows directly below the control group — the rows of the slot's four route slots aimed at that
control, drawn as mxm-model-drums' are, with a `‹ modulate ›` line that adds a source to a free slot;
changing the selected slot changes which route slots those rows edit. There is no standalone Routes
card, global topology or transient target picker. *(Until 2026-10-07 each control carried the
collection's `mxm-modulation-params` stack over that slot's fixed route pairs.)*

## 6. Categories, cards and grouping

Cards retain stable identities; pages derive from available space.

| Category | Stable cards / responsibility |
|---|---|
| Performance | `slots-01-08` and `slots-09-16`, each with eight compact rows. |
| Modulators | One `lfos` card containing three stacked kit-wide LFO rows. Each row has Rate, one adjacent on/off Sync button (the collection's quarter note) and Shape; Sync makes Rate itself snap to musical divisions, never reveals a separate Division control. |
| Generators | `model` for the selected slot's grouped searchable selector and Model notes; `excitation` for Controls 1, 4 and 5 (Tune, Attack, Soft hits) plus their local route stacks; `body` for Controls 8, 9 and 11 (Body, Noise/Snappy, Character) plus their local route stacks — each as the selected model shows and names it. |
| Tone | `envelopes-tone` for Controls 2, 3, 10, 6 and 7 (Decay, Tone, Noise decay, Pitch drop, Pitch decay), as the model shows them, plus their local route stacks; then `output`, the last card: the selected slot's Level and Pan with their local route stacks, followed by Output, Choke Group and MIDI channel. It is the slot's output stage, not a mix, so it ends the signal chain (owner, 2026-09-18). Directly under the slot's Output sits the one kit-wide **All slots** row, `L+R` or `Own outputs`, which writes all sixteen Output settings at once and lights neither cell when they form neither pattern (plan revision 42). It is here, beside the selector it generalises, because output routing is a rare action that design system §3.1 keeps out of the app bar. The whole kit's Master stays in the app bar, never beside one slot's controls. |

Category order is the collection order. Inside the signal path: slot/trigger context, modulation
sources and routes, generator/model/excitation/body, then decay/tone and output. There are no
Sequencers or Effects cards. A selected slot changes which parameter instances these cards
bind; it does not change card identity or order.

## 7. Identity accent

Use the collection's default teal accent rather than assigning another hardware-associated colour.
The existing theme owns and tests dark `#4CC9D8` / light `#247F91`; this product introduces no raw
colour and no new token. Modulation and status colours remain unchanged.

## 8. Live visualizations

1. **Slot activity.** Each overview row shows a bounded level/activity mark. Selected, sounding and
   selected+sound states differ by border, fill and mark rather than hue alone.
2. **Model sound display.** A compact waveform is rendered from the selected model's real DSP and
   current shaping controls on the UI thread and cached by a 1/64-step parameter signature. Its 96
   peak bins give the first 64 to a linear 80 ms attack/body window, then compress the remainder of a
   two-second render quadratically into 32 tail bins. Parameter edits must visibly affect the trace;
   it must distinguish actual kick, noise, metallic and PCM output rather than reuse decorative
   capability-class curves. It is
   explanatory geometry, never a product picture.
3. **Shared-source/coupling summary.** Model notes explain audible sharing in musician-facing
   language. They do not promise accent or automatic hat choking, neither of which this instrument
   has. No implementation status, code-contract prose, fake
   cable or jack.
4. **Output.** Carries the slot's output selection, its Choke Group and its MIDI channel. Per-slot
   peaks max-combine; the app-bar master meter carries a latched clip. There is
   no hidden limiter, automatic gain control or dynamic normalization. One measured, fixed
   post-circuit trim per model places zero-deviation reference hits on a common peak plane before
   the visible slot Level; it does not react to signal level or rewrite circuit dynamics.

Telemetry is atomics or bounded lock-free state written once per block. No full waveform history,
per-oscillator GUI object or audio-thread lock/allocation is permitted.

## 9. What is removed from the source hardware layouts, and why

| Removed or translated | Why |
|---|---|
| Every source panel's geometry, colours, typography, switches, step keys and case | Design system §2 forbids hardware replicas; this is one original instrument over several machines. |
| Internal sequencers, rhythm patterns, fills, songs, tempo, start/stop and keyboard transpose | The host/player owns sequencing and transpose. Voice-side articulation, including 606 tempo-coupled open-hat decay, remains when it changes sound. |
| Individual hardware output jacks and switching-jack master removal | Translated, not copied: each slot selects main `L+R` or one of sixteen mono software outputs named `Slot 01`…`Slot 16`, and a stereo-compatibility layout keeps MXM Player on `L+R`. A slot on an individual output simply leaves `L+R`; there is no switching-jack behaviour, because a host does not tell a plugin which outputs are patched (measured in Bitwig 6.0, plan revision 42). The Output card's **All slots** row is the software stand-in for patching every jack at once. The card also carries the slot's Choke Group, which is where one sound is assigned to close another. |
| User sample/ROM replacement | Not on the references and would turn this into a sampler. The 909 PCM source is the fixed owner-selected NMF-12 resynthesis after its explicit ruling. |
| General compressor, distortion, EQ, delay and reverb | Not part of this original instrument's voice contract. Circuit clipping/filtering and clap/tom synthetic tails stay inside their specific model. |
| Universal chromatic sample playback | Contradicts the distinct resonator, VCO, metal-bank, PCM and noise pitch laws. |
| Manufacturer/model names in selector labels and factory kits | Collection naming contract: references belong in documentation, not product parameter values or preset names. Appendix A keeps the mapping for maintainers. |

## 10. Quarter-4K fit, minimum size and 200% zoom

The opening size is not fixed by this brief; implementation derives and hugs it with Model notes
open under the 1920 × 1080 physical budget. Planned card floors are measured from the real controls,
including the longest public model label and every route revealed. The two eight-slot cards are a
preferred group, never a minimum-width demand; a narrow editor may show one card per row/page.

The minimum holds one widest card plus gutters, and it is **measured**: the widest card's computed
floor and the workspace's gutter on each side must fit the editor's `MINIMUM`, held by
`the_widest_card_fits_the_pager_viewport_at_the_advertised_minimum`. The app bar at its last compact
step — Resample and Export samples in its `…` menu — is wider and sets it
(`the_app_bar_holds_in_the_minimum_window`). At 200% editor zoom the
physical test window remains fixed; category paging/reflow and indivisible-card scrolling preserve
every model, route and parameter. Unbounded preset/search results may scroll. Ordinary controls on a
fitting page do not.

Verification renders every derived page and selected-slot binding in both themes at 1×/2×, checks
painted labels/control rectangles rather than only card bounds, and operates the actual Model search,
slot selection, every parameter-local route stack, theme and zoom controls. Native DPI/window chrome and real-DAW
resize remain manual §15 gates.

---

# Product surfaces

*Titled "Permanent product surfaces fixed at D0 and extended additively" until 2026-10-07.* **A
recorded deviation** (the owner, 2026-10-07): **parameter IDs are free to change during pre-alpha**;
the permanent-ID freeze applies from the first release. Until then the parameter IDs below are this
version's, not a promise, and nothing migrates an older state's IDs (the owner, 2026-09-30: "There
are no saved projects - we are in pre alpha"). The model catalogue's IDs stay append-only.

## Model selector representation

- Parameter plain-value domain: integer **0…255**, fixed permanently. `0` is legacy Off: it remains
  silent and restorable for compatibility but is not offered for new assignments.
- IDs **1…94** are reserved contiguously in Appendix A. A reservation becomes available only when
  that circuit renders; reservations never move or change meaning. Later research starts at the next
  unused ID rather than filling a historical hole, and the domain never expands.
- Presets and state store nice-plug's normalized representation over the fixed 0…255 domain, so
  making another reserved model available does not remap an existing value.
- The editor gives `mxm-ui::control::selector` only available sounding entries, in ID order, and maps
  the returned list index to its stable ID. Keyboard arrows/Home/End therefore traverse sounding
  models, not legacy Off, reservations or the unassigned tail. Family grouping is display metadata
  and never changes index order.
- If host automation or text input selects a reserved-but-unavailable or unassigned ID, DSP resolves
  it to Off while the host and editor display `Unavailable <id>`. The open menu adds that current
  unavailable value as one temporary row so the selected value remains visible; any available choice
  leaves it. No unavailable ID aliases an existing model.
- Text accepts exact public labels, `Legacy Off`, and decimal IDs. Formatting is idempotent for all
  256 values.
- Tests serialize every available ID, exercise pointer/search/keyboard/direct text, automate
  unavailable 2 and 255, make a fixture ID available, and require every old normalized value and
  label to restore unchanged.

The existing shared selector already searches above 24 options. mxm-kit's `crates/ui` gains only generic group
heading metadata if the real menu proof shows headings fit and filter correctly; the mapped stable
IDs remain plugin-owned data rather than a drum-specific shared widget.

## Source-family audition layout

Nine factory presets group the admitted catalogue by source machine while keeping one reusable MIDI
role map. Notes 36–40 are Kick, Snare and low/mid/high drum; 41–43 are second low/mid/high pitched
percussion; 44–48 are Rim, Clap/brush, closed hat, open hat and Cymbal/crash; 49–51 are Cowbell,
Clave and auxiliary percussion. A family without a role mutes that slot rather than shifting a
different drum onto that note; its retained assignment is a same-family fallback and remains
inaudible. Unique machine percussion may occupy otherwise absent secondary pitched or auxiliary
positions, but the common roles never move. Preset names omit maker names and full model designations, but carry a recognisable numeric token
such as `808` or `909` by owner request; the plugin README documents the exact research mapping.
The Model selector repeats those nine names in the same order as bold, non-selectable group headings,
so presets and individual models share one visible family index. Off is absent; slot Mute owns
intentional silence. The long-list search persists across menu closings for repeated assignment and
has an explicit cross-shaped clear action.

MXM Player's two-bar `Drum machine family test` saved sequence exercises this map. The sequence owns
notes and tempo only, so switching among these presets keeps the rhythm fixed.

## Creative kits

Fifty creative kits (plan §7.2) follow the nine in the preset browser, filed under *Percussion* and
on the same role map, so the same beat plays every one of them. They demonstrate the pool: reference
kits of one machine each, mixed-machine kits, pitched and resonant, metallic, low-cost, electronic
and experimental kits, choke and shared-source kits, and two pairs of subtle against extreme
deviation. A creative kit may put another sound on a role (a shaker on the closed-hat key), never a
kick on the snare's; it moves only controls its models read, and none that is only a gain. Names are
original words without machine numbers. Every closed hat cuts its open hat (the owner, 2026-10-07).
The plugin's `NOTES.md` (*The fifty creative kits*) records the bank and each decision behind it.

## Per-slot parameters

**mxm-model-drums' scheme: the host holds what the panel can show at once** (the owner, 2026-09-30;
this machine moved to it on 2026-10-07). nice-plug's nested arrays make the full IDs `<suffix>_1`
through `<suffix>_16`. **A slot's sound controls are general: the host sees `Slot N Control k`, the
panel the names below.** The move was **mechanical, the same sound** (the owner, 2026-10-07): each
named control of before is one general control, keeping its range, default, unit, smoothing and its
law in the circuit, and every kit renders bit-identically. Per-model redesign can come later.

| Suffix | Host name inside slot N | Kind / zero meaning |
|---|---|---|
| `model` | `Slot N model` | Fixed-domain stepped integer; Init chooses a useful model. |
| `c01` … `c20` | `Slot N Control 1` … `Control 20` | The controls below, each bipolar with zero its reference. |
| `level` | `Slot N level` | Linear gain formatted dB; Init balances the kit. |
| `pan` | `Slot N pan` | Bipolar stereo placement; centre is reference. |
| `mute` | `Slot N mute` | Boolean post-circuit silence; false by default and Mute wins over Solo. |
| `solo` | `Slot N solo` | Boolean post-circuit isolation; if any is true, only unmuted soloed slots sound. |
| `choke_group` | `Slot N choke group` | Stepped `Off`, `1`…`16`. Slots sharing a group cut each other through a bounded de-click, whatever models they are. `Off` by default: nothing chokes unless assigned. A **kit** setting — which slots cut each other is sound design — so presets carry it, unlike `output` and `midi_channel`. |
| `output` | `Slot N output` | `L+R` or `1`…`16`; an instance setting presets never carry. |
| `midi_channel` | `Slot N MIDI channel` | `Kit` or channel `1`…`16`; an instance setting presets never carry. |

**The controls, the same on every model** — mxm-model-drums' common seven first, so automation keeps
its sense across the two instruments. The panel name below is the general one; each model shows only
the controls its code reads, under its own name where the general one would mislead (*Gain*, *Bend*,
*Spread*, …: the plugin's `NOTES.md`, *Each model's controls*). *(Until 2026-10-07 each control's panel
name was the one it had: Dynamics for 5, Pitch envelope for 6.)*

| Control | Panel name | Kind / zero meaning (was) |
|---|---|---|
| 1 | Tune | Pitch-law deviation, ±24 semitones; `0 st` is reference. Unavailable where no honest pitch law exists. (`pitch`) |
| 2 | Decay | Time/feedback deviation; zero is reference and positive travel extends beyond stock where the topology permits. (`decay`) |
| 3 | Tone | Internal spectral/filter deviation. (`tone`) |
| 4 | Attack | Excitation/click/burst deviation. (`attack`) |
| 5 | Soft hits | The velocity curve's exponent; its top brings soft hits up — mxm-model-drums' Velocity place. (`dynamics`) |
| 6 | Pitch drop; *Pitch sweep* where it also rises | Additive depth deviation; zero preserves a native reference sweep or leaves a source-accurate no-sweep circuit unchanged — mxm-model-drums' Pitch drop place. (`pitch_env`) |
| 7 | Pitch decay | Additive pitch-envelope time deviation; zero preserves source/reference timing. (`pitch_decay`) |
| 8 | Body | Tonal-body balance/shape deviation. (`body`) |
| 9 | Noise; *Snappy* on a snare | Noise/snappy contribution deviation. (`noise`) |
| 10 | Noise decay; *Snappy decay* on a snare | Additive time deviation for a distinct wire/noise envelope; unsupported models are exact no-ops. (`noise_decay`) |
| 11 | Character | Model-specific metal/room/nonlinear character deviation. (`character`) |
| 12–20 | — | Unused on every model: exact no-ops, not on the panel. |

An axis a model cannot use remains an ordinary parameter with zero effect for that model, not drawn
on its panel (visible and disabled until 2026-10-07), so model automation never changes the host's
parameter inventory.
Parameter ranges, smoothing and route full scales were measured before D2's first bundle. The
vocabulary of before, with its IDs, is kept in the plugin's `NOTES.md` (*The surface before
2026-10-07*).

## Global and routing IDs

Global parameters are:

- `master`;
- `lfo1_rate`, `lfo1_shape`, `lfo1_sync`, and the matching `lfo2_*` and `lfo3_*`;
- `resample`, an instance setting presets never carry.

**Four route slots per drum** (the owner's principle: the host holds what the panel can show at
once), each three IDs:

```text
route<r>_source_<slot>    Off, LFO 1, LFO 2, LFO 3, Wheel, Pressure, Velocity, Random
route<r>_target_<slot>    Off, Control 1 … Control 20, Level, Pan
route<r>_amount_<slot>    bipolar −1…+1; one full route reaches its target's full route reach
```

with `r` 1…4. A route is in use when neither its source nor its target is Off; switching its source
off keeps its target and amount; two on one source and target add; one aimed at Controls 12–20 does
nothing. Each in-use route reaches the target the named control's grid pair reached (Control 1 Tune,
… as the table above; Level the Amplitude target; Pan the pan), so one route is exactly that pair.
Optional creative routes are all Off and zero in Init. Preset files store routes sparsely: nothing for
a route not in use, its source and target for an in-use one at zero, and all three for one with
depth. Velocity reaches each hit directly (through Soft hits) and is also a route source. Per-note
tuning and channel bend enter model pitch directly, not as removable routes.

**A recorded deviation**, as mxm-model-drums'. The collection's modulation standard
(mxm-kit's `crates/mxm-modulation-params`, `plans/plan-modulation-routing.md`) gives every target a
presence and an amount for every source; this machine's sixteen slots made that 2,912 IDs, and the
whole surface 3,227. Four route slots a drum make it 651 parameters, by the owner's ruling. The rows
stay derived from parameters, so a preset alone decides the panel, as the standard requires; the
plugin-side pair-by-pair reading check (`mxm_plugin_test::routing_checks`) needs a parameter per pair
and does not apply. The DSP's per-slot route graph and its conformance are unchanged: the plugin fills
it from the four slots.

The source configurations and route reaches are:

- `lfo1_rate`: 0.05–20 Hz, default 0.70 Hz; `lfo1_shape`: Sine by default.
- `lfo2_rate`: 0.05–20 Hz, default 1.30 Hz; `lfo2_shape`: Triangle by default.
- `lfo3_rate`: 0.05–20 Hz, default 4.00 Hz; `lfo3_shape`: Sine by default.
- every `sync` defaults off; when on, that LFO's `rate` position selects 4 bars through 1/32,
  including dotted and triplet values. Invalid or absent host tempo falls back to free Rate.
- one full Tune route reaches 12 semitones, and one full Level route is the collection's Amplitude
  factor's whole swing, silence to double (it reached 12 dB before the modulation standard of
  2026-09-26);
- every other full route reaches one complete bipolar target unit (100% of that creative axis or
  full pan travel). Several live routes sum, then the target's own valid range clamps once.

Exactly three LFO values are generated once per sample for the whole kit, never once per slot. Wheel and Pressure are per channel projected into
the slot's sounding channel; Velocity and deterministic bipolar Random are held from that slot's
latest trigger. Route amounts smooth for 15 ms. A route not in use contributes nothing while
retaining its amount. An in-use route at settled zero remains visible but is omitted from the compact
per-sample DSP list; moving it away from zero activates it again. *(Until 2026-10-07 the same held
of the grid's pairs: presence was discrete, and an absent pair retained its dormant amount.)*

## Init assignments

All shaping deviations and optional route amounts begin at zero; every route's source and target is
Off (route presences were absent, until 2026-10-07). LFO
rates/shapes are useful configurations and Sync begins off. Each model's fixed output adaptation reaches the common
reference peak at Level 0 dB; factory presets then use visible Level values for their role hierarchy.
Pan is centred, and Master is open. The final sixteen model defaults are:

| Slot / note | Model ID / label |
|---|---|
| 1 / 36 | 1 — Deep bridge kick |
| 2 / 37 | 17 — Reset punch kick |
| 3 / 38 | 2 — Twin-mode snare |
| 4 / 39 | 18 — Reset twin snare |
| 5 / 40 | 3 — Low falling tom |
| 6 / 41 | 20 — Mid reset triad tom |
| 7 / 42 | 7 — High falling tom |
| 8 / 43 | 22 — Triple-resonator rim |
| 9 / 44 | 12 — Triple pulse clap |
| 10 / 45 | 23 — Four-cell clap |
| 11 / 46 | 13 — Twin-square cowbell |
| 12 / 47 | 15 — Six-square closed hat |
| 13 / 48 | 16 — Six-square open hat |
| 14 / 49 | 14 — Three-path cymbal |
| 15 / 50 | 26 — Six-bit crash |
| 16 / 51 | 27 — Six-bit ride |

All sixteen Init assignments render. IDs 24–27 use the owner-selected NMF-12 resyntheses under plan
§2.3; IDs 24–25 remain available in the catalogue but are not separate Init rows.

---

# Appendix A — reserved model catalogue

Axis codes: **P** Pitch, **D** Decay, **A** Attack, **T** Tone, **B** Body, **N** Noise,
**C** Character, **Y** Dynamics. Every listed axis is a deviation around reference. `—` means no
honest mapping. Pitch laws: **R** resonator poles, **V** reset VCO CV, **M** shared/private metallic
bank, **C** PCM clock, **F** noise clock/filter, **—** unavailable.

Evidence is primary service material unless a cited academic analysis strengthens it. No row is
hardware-measured. “Blocked” means architecture admitted but distributable content missing.

| ID | Public selector label | Reference / articulation | Family / shared state | Pitch | Axes | Evidence | Listening |
|---:|---|---|---|---|---|---|---|
| 0 | Off | Silence | None | — | — | Defined | N/A |
| 1 | Deep bridge kick | TR-808 bass | live bridged-T | R | P D A T B Y | Schematic + DAFx analysis | Owner listening-approved (2026-09-20) |
| 2 | Twin-mode snare | TR-808 snare | two bridged-T + shared white noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 3 | Low falling tom | TR-808 low tom | diode resonator + shared pink tail | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 4 | Low falling conga | TR-808 low conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 5 | Mid falling tom | TR-808 mid tom | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 6 | Mid falling conga | TR-808 mid conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 7 | High falling tom | TR-808 high tom | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 8 | High falling conga | TR-808 high conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 9 | Layered short rim | TR-808 rim | shared rim/clave resonator | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 10 | Pure high clave | TR-808 clave | shared rim/clave resonator | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 11 | Bright short maraca | TR-808 maracas | shared white noise | F | D A T N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 12 | Triple pulse clap | TR-808 clap | shared white noise + comparator bursts | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 13 | Twin-square cowbell | TR-808 cowbell | shared six-square bank oscillators 5/6 | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 14 | Three-path cymbal | TR-808 cymbal | shared six-square bank / three nonlinear paths | M | P D A T C Y | Schematic + ICMC analysis | Owner listening-approved (2026-09-20) |
| 15 | Six-square closed hat | TR-808 closed hat | shared six-square bank | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 16 | Six-square open hat | TR-808 open hat | shared six-square bank | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 17 | Reset punch kick | TR-909 bass | reset VCO + shared hardware noise attack | V | P D A T B N Y | Schematic-derived; tuning chosen | Owner listening-approved (2026-09-20) |
| 18 | Reset twin snare | TR-909 snare | two reset VCOs + shared hardware noise | V | P D A T B N Y | Schematic-derived; tuning chosen | Owner listening-approved (2026-09-20) |
| 19 | Low reset triad tom | TR-909 low tom | three reset VCOs + shared noise attack | V | P D A T B N C Y | Schematic-derived; tuning chosen | Owner listening-approved (2026-09-20) |
| 20 | Mid reset triad tom | TR-909 mid tom | three reset VCOs + shared noise attack | V | P D A T B N C Y | Schematic-derived; tuning chosen | Owner listening-approved (2026-09-20) |
| 21 | High reset triad tom | TR-909 high tom | three reset VCOs + shared noise attack | V | P D A T B N C Y | Schematic-derived; tuning chosen | Owner listening-approved (2026-09-20) |
| 22 | Triple-resonator rim | TR-909 rim | three struck resonators | R | P D A T B C Y | Schematic-derived; envelopes chosen | Owner listening-approved (2026-09-20) |
| 23 | Four-cell clap | TR-909 clap | shared hardware noise + four burst cells | F | D A T N C Y | Schematic-derived; timings chosen | Owner listening-approved (2026-09-20) |
| 24 | Six-bit closed hat | TR-909 closed hat | Shared hat NMF-12 source / measured envelope + light broadband residual / hat restart | C | P D A T C Y | KL coherent-component source; fidelity unverified | Owner listening-approved (2026-09-20) |
| 25 | Six-bit open hat | TR-909 open hat | Shared hat NMF-12 source / long address + envelope / hat restart | C | P D A T C Y | KL coherent-component source; fidelity unverified | Owner listening-approved (2026-09-20) |
| 26 | Six-bit crash | TR-909 crash | Fixed NMF-12 resynthesis / clocked playback | C | P D A T C Y | KL coherent-component resynthesis; fidelity unverified | Owner listening-approved (2026-09-20) |
| 27 | Six-bit ride | TR-909 ride | Fixed NMF-12 resynthesis / clocked playback | C | P D A T C Y | KL coherent-component resynthesis; fidelity unverified | Owner listening-approved (2026-09-20) |
| 28 | Economy 62 kick | DR-55 bass | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 29 | Economy body snare | DR-55 snare | resonator + shared transistor noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 30 | Economy short rim | DR-55 rim | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 31 | Inductor noise hat | DR-55 hat | shared transistor noise + LC filter | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 32 | Dual-low kick | CR-8000 bass | two bridged-T resonators | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 33 | Dual-bridge snare | CR-8000 snare | two resonators + shared noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 34 | Diode low tom | CR-8000 low tom | diode resonator + shared pink tail | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 35 | Diode mid tom | CR-8000 mid tom | diode resonator + shared pink tail | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 36 | Diode high tom | CR-8000 high tom | diode resonator + shared pink tail | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 37 | Diode low conga | CR-8000 low conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 38 | Diode mid conga | CR-8000 mid conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 39 | Diode high conga | CR-8000 high conga | diode resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 40 | Thirty-millisecond rim | CR-8000 rim | short resonator | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 41 | Six-square cymbal | CR-8000 cymbal | own six-square bank | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 42 | Six-square short hat | CR-8000 hat | own six-square bank | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 43 | Six-square long hat | CR-8000 open hat | own six-square bank | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 44 | Saw-noise clap | CR-8000 clap | shared noise + saw modulation | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 45 | Phase-shift clave | CR-8000 clave | RC phase-shift oscillator | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 46 | Split-square cowbell | CR-8000 cowbell | own shared metal-bank pair | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 47 | Compact dual kick | TR-606 bass | dual resonators | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 48 | Compact body snare | TR-606 snare | resonator + attack + shared white noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 49 | Compact low tom | TR-606 low tom | struck resonator + attack | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 50 | Compact high tom | TR-606 high tom | struck resonator + attack | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 51 | Two-band cymbal | TR-606 cymbal | own six-square bank / two bands | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 52 | Resonant closed hat | TR-606 closed hat | own six-square bank / shared hat VCA | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 53 | Tempo-coupled open hat | TR-606 open hat | own six-square bank / tempo decay | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 54 | Damped whack kick | DR-110 bass | damped oscillator + low-passed trigger | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 55 | LFSR snap snare | DR-110 snare | damped body + direct attack + shared 18-stage noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 56 | Mixed-source cymbal | DR-110 cymbal | shared 18-stage noise + four-square bank | M | P D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 57 | Mixed-source closed hat | DR-110 closed hat | shared mixed metal | M | P D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 58 | Mixed-source open hat | DR-110 open hat | shared mixed metal | M | P D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 59 | Timed-burst clap | DR-110 clap | CPU burst schedule + shared noise | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 60 | Classic 62 kick | CR-78 bass | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 61 | Classic 340 snare | CR-78 snare | resonator + shared noise | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 62 | Five-millisecond rim | CR-78 rim | short resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 63 | Sixty-millisecond noise hat | CR-78 hat | shared noise | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 64 | Long noise cymbal | CR-78 cymbal | shared noise/inductor colour | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 65 | Twenty-millisecond maraca | CR-78 maraca | shared noise | F | D A T N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 66 | High 2630 clave | CR-78 clave | high resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 67 | High 600 bongo | CR-78 high bongo | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 68 | Low 400 bongo | CR-78 low bongo | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 69 | Low 208 conga | CR-78 low conga | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 70 | Close-interval cowbell | CR-78 cowbell | shared two-square source | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 71 | Tambourine wash | CR-78 tambourine | shared noise/metal colour | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 72 | Two-rate guiro | CR-78 guiro | two low pulse rates | M | P D A T C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 73 | Triple high bell | CR-78 bell group | three high resonant parts | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 74 | Discrete 62 kick | TR-66 bass | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 75 | Discrete 208 conga | TR-66 high conga | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 76 | Discrete low bongo | TR-66 low bongo | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 77 | Discrete high bongo | TR-66 high bongo | transistor resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 78 | Resonant 830 cowbell | TR-66 cowbell | resonant cowbell section | R | P D A T B C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 79 | Discrete short rim | TR-66 rim | short resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 80 | Resonant 2350 clave | TR-66 clave | high resonator | R | P D A T B Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 81 | Bongo-body snare | TR-66 snare | shared noise + weak low-bongo body | R | P D A T B N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 82 | Forty-millisecond noise hat | TR-66 hat | shared noise | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 83 | Forty-millisecond maraca | TR-66 maraca | shared noise | F | D A T N Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 84 | Four-hundred-millisecond cymbal | TR-66 cymbal | shared noise/inductor colour | F | D A T N C Y | Schematic-derived | Owner listening-approved (2026-09-20) |
| 85 | Early transistor kick | Rhythm Ace FR-2L bass | discrete damped cell | R | P D A T B Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 86 | Early low conga | Rhythm Ace FR-2L low conga | discrete damped cell | R | P D A T B Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 87 | Early high conga | Rhythm Ace FR-2L high conga | discrete damped cell | R | P D A T B Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 88 | Early high bongo | Rhythm Ace FR-2L high bongo | discrete damped cell | R | P D A T B Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 89 | Early cowbell | Rhythm Ace FR-2L cowbell | discrete damped cell | R | P D A T B C Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 90 | Early clave | Rhythm Ace FR-2L clave | discrete damped cell | R | P D A T B Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 91 | Early snare | Rhythm Ace FR-2L snare | shared noise/inductor + body | R | P D A T B N Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 92 | Early cymbal | Rhythm Ace FR-2L cymbal | shared noise/inductor colour | F | D A T N C Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 93 | Early maraca | Rhythm Ace FR-2L maraca | shared noise | F | D A T N Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |
| 94 | Early wire brush | Rhythm Ace FR-2L wirebrush | shared noise/inductor colour | F | D A T N C Y | Schematic-derived; calibration chosen | Owner listening-approved (2026-09-20) |

## Deferred catalogue rows

| Reference | Named articulations | Blocker and admission condition |
|---|---|---|
| Maestro MRK-2 | Bass, low/high tom, bongo, blocks, clave, rim, high/low drum, brush, hi-hat, cymbal, snare | The available package labels the paths but supplies no readable calibration table and the research page deliberately declines exact transfer-function claims. D4 traces each candidate node by node and admits only rows whose topology and independently chosen calibration can be stated honestly. New rows append at ID 95 onward; none is represented by a generic substitute before then. |
| Production revisions | Early TR-808 metal and TR-909 bass/rim/hat/tom-noise changes | Service material establishes revisions but not yet a complete audible model boundary. Admit separate rows only after the changed circuit and a distinguishing render/measurement are recorded. |

---

# Event, coupling and capability contracts

## Same-offset event order

At each sample offset the shell supplies the DSP one complete event group. The group resolves in this
order, independent of host iteration order:

1. CC120 panic clears all prior and pending state; if present, it dominates the group.
2. Targeted `NoteChoke` terminates matching prior hits and cancels their pending expression.
3. Zero-velocity NoteOn/NoteOff release expression ownership without truncating ordinary one-shots.
4. Parameter/model topology for the interval is already coherent before triggers.
5. Positive-velocity NoteOns are collected per slot. Where several land on one slot at one sample
   they are reduced to a single strike before any trigger, so host event order cannot change the
   result, and the value that sounds is the winning owner's own velocity. No value crosses between
   slots: this instrument has no accent.
6. Choke groups are resolved before the newly triggered outputs: a struck slot silences every other slot in its group, and slots struck on the same sample are exempt.
7. New note IDs own subsequent per-note tuning; duplicate ID-less triggers replace that slot's prior
   expression owner because the slot itself is monophonic.

Different sample offsets remain different events. CC123 releases owners but leaves one-shot tails;
CC120 clears everything. Queue overflow retains panic/choke/releases before assertions.

## Shared buses

- `a8_noise`, `a8_metal`, `a8_accent`, `a8_hats`
- `a9_noise`, `a9_pcm_hats`
- `c8_noise`, `c8_metal`, `c8_accent`
- `c6_noise`, `c6_metal`, `c6_hats`
- `d110_noise_metal`, `d110_accent`, `d110_hats`
- `c78_noise`, `c78_accent`; `t66_noise`; `fr2_noise`

The names are internal references, not product labels. At reference, all consumers read one raw
source sample per bus/frame. A nonzero Pitch on a metallic model takes a private bank as the plan
specifies; returning to zero rejoins the shared bank without resetting it.

## 909 PCM source ruling

On 2026-09-18 the owner replaced the Medium all-pass outputs with the listening-selected **NMF-12**
resyntheses. One shared hat source, crash and ride use KL-NMF soft masks to recover twelve coherent
time-domain components, apply gentle constant quadrature rotations and sum them without direct
modified-magnitude inversion. Closed and open hats restart the same source; their address/envelope
paths differ. Closed uses a measured 13,920-frame gain table and the owner-selected light 0.03
source-addressed broadband residual under that envelope; neither is a second PCM source. The three
fixed results are quantised to signed 16-bit PCM and embedded;
`crates/mxm-drum-machine-dsp/assets/reset-909-nmf/README.md` owns the methods, seeds, frame counts,
closed articulation and checksums.

At zero controls the DSP plays those resynthesized sources directly. It does not apply the old
procedural source's six-bit conversion or reconstruction filter a second time. Pitch still changes
the playback clock and duration together; Decay, Attack, Tone and Character remain
reference-bypassed creative deviations. Closed and open hats do not choke automatically; assigning
their slots to one choke group supplies that performance behavior.

---

# Recognisability and sign-off

Before final composition, a wireframe trial asks a musician to:

1. identify that there are sixteen slots but more than sixteen models;
2. change slot 6 to a different snare model and return every shaping control to reference;
3. explain why Pitch changes duration on a PCM cymbal but not through a duration-preserving resampler;
4. identify which two active slots share a metal/noise source, then assign two hats to one choke
   group and identify which hit will cut the other;
5. find Model, Pitch, Decay, Tone and Master within ten seconds each, entering at most one wrong page.

Four of five conceptual answers and all five control finds are the gate. Owner listening separately
judges whether priority models are recognisable. Neither result changes the evidence status: until
hardware is measured, the product remains schematic-derived and fidelity unverified.
