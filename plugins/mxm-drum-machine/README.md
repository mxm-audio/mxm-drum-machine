# mxm-drum-machine

A sixteen-slot circuit-modelled drum instrument. In `Kit` mode MIDI notes 36 through 51 address one
retriggerable slot each; a slot can instead claim MIDI channel 1…16 and play chromatically. Pitched
drums — kicks, snares (by their body), toms, congas, bongos, cowbells, claves, rims and the bell —
then play in tune at concert pitch, like any keyboard instrument, tuned to where each one's pitch
comes to rest after its attack; hats, cymbals, claps and other noise voices play around note 60. Each slot selects a machine-specific model from an append-only catalogue.

All 94 admitted catalogue circuits render and passed owner listening on 2026-09-20. Comparative
reference calibration is recorded separately from hardware fidelity, which remains unverified.

Each slot has four modulation routes of its own, edited beside the control each moves rather than
through a global route list. A slot's controls reach the host as `Control 1`…`Control 20`, the same
general scheme as mxm-model-drums; the panel names each as the drum does. Three LFOs are shared
kit-wide. Each has one Sync button: off, Rate is in hertz;
on, the same Rate knob snaps to musical divisions such as 1/16, 1/8 and 1/4T. Per-slot Tune, Decay,
Tone, Attack, Body, Snappy/noise and model-specific controls include
separate Pitch envelope, Pitch decay and Noise decay where the circuit supports them. Every added envelope
control defaults to zero, preserving the
source/reference sound; positive Decay travel extends beyond stock on extensible analogue models.
Each slot also selects main `L+R` or one of sixteen shareable mono individual outputs, named
`Slot 01`…`Slot 16`. The full CLAP layout exposes all seventeen ports; a second stereo-only layout
folds every slot to main for hosts such as MXM Player. Pan applies only on `L+R`.

The Output card's **All slots** row sends every slot to its own output, or every slot back to
`L+R`, in one click. It reads the sixteen Output settings rather than storing a mode, so neither cell
is lit when slots go to different places — two hats sharing one output, say. A slot on an individual
output leaves `L+R`, and a host must pick that output up, or the slot is silent: no host tells a
plugin which of its outputs are actually connected, so the plugin cannot fall back to main on its own.
In Bitwig, choose **Own outputs**, then open the device's chains and click **Add Missing Chains**:
each slot gets its own named mixer strip inside the instrument track. Another track can take a
chain as its input from the instrument track's Chains submenu.

Nine factory audition sets group the catalogue by
source machine. They use one fixed key layout, muting a role when that machine did not provide it:

| MIDI note | Slot role |
|---|---|
| 36–40 | Kick, snare, low drum, mid drum, high drum |
| 41–43 | Second low, mid and high pitched percussion |
| 44–48 | Rim, clap/brush, closed hat, open hat, cymbal/crash |
| 49–51 | Cowbell, clave, auxiliary percussion |

Product-safe preset names map to research references as follows:

| Factory preset | Reference family |
|---|---|
| Bridge 808 | TR-808 |
| Reset 909 | TR-909 |
| Economy 55 | DR-55 |
| Expanded 8000 | CR-8000 |
| Compact 606 | TR-606 |
| Snap 110 | DR-110 |
| Classic 78 | CR-78 |
| Discrete 66 | TR-66 |
| Early 2L | Rhythm Ace FR-2L |

The Model selector uses these same family names in this same order. Family names appear as bold,
non-selectable headings above the individual model rows. Off is no longer a model choice: each slot
row has compact `M` and `S` buttons for Mute and Solo. `S` also selects that slot for editing, and
the selected row uses a clear, stable full-row accent treatment without moving its controls. Model searches survive closing and reopening the menu until
the cross-shaped clear action is used.

The presets contain only sound parameter values. Output and MIDI-channel assignments are instance
settings saved by the host, not kit content, so browsing, loading or initialising a kit never rewires
the DAW or controller. The reusable two-bar **Drum machine family test** beat
lives in MXM Player's saved sequences and addresses these roles, so switching presets changes the
machine rather than the rhythm. The larger fifty-kit creative bank remains separate work.

There is no internal sequencer, sample import or effects rack. Use a DAW or MXM Player for patterns.
The interface follows the MXM design system and does not reproduce any source hardware panel.

```bash
cargo test -p mxm-drum-machine-dsp
cargo test -p mxm-drum-machine
cargo xtask bundle mxm-drum-machine --release
```

GPL-3.0-or-later — see the repository's [`LICENSE`](../../LICENSE). All shipped code and eventual
excitation content must be project-owned.
