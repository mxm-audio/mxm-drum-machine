//! **The fifty creative factory kits** (plan §7.2): their design, the generator that writes their
//! files, and the checks that hold them.
//!
//! Each kit is a playable sixteen-key kit on the audition kits' role map — 36 Kick, 37 Snare,
//! 38–40 low/mid/high tom, 41–43 low/mid/high percussion, 44 Rim, 45 Clap/brush, 46/47
//! closed/open hat, 48 Cymbal, 49 Cowbell, 50 Clave, 51 auxiliary — so one beat plays across the
//! whole bank. A kit may put another sound on a role (a shaker on the closed-hat key), never a kick
//! on the snare's. It stores models, levels, pans, choke groups, the controls each key moves and
//! any routes; never notes, tempo, outputs or MIDI channels (`is_instance_setting`).
//!
//! **The design is written sparsely**: a key names its model, level and pan, and only the controls
//! it moves, in plain values; everything else is Init. [`generated`] lays it over Init and the
//! ignored [`write_the_creative_kits`] writes the files, complete, so a shipped kit never leans on
//! a default. The runs-by-default [`the_shipped_kits_are_their_designs`] builds every file in
//! memory and compares, changing nothing in the checkout.
//!
//! **A kit only moves what its models read**, by the panel's own table (`editor::controls`, from
//! `ModelId::capabilities`): no control a model hides, none that is only a gain (Level does that),
//! and none that waits on another left at the value that silences it
//! ([`every_key_moves_only_what_its_model_reads`]).
//!
//! **They choke as a real kit would** (the owner, 2026-10-07): every kit's closed hat cuts its open
//! hat, and the choke kits add their own groups. The nine audition kits stay ungrouped.
//!
//! **Every kit sounds, and sounds like itself** ([`every_kit_sounds_and_no_two_sound_alike`]): the
//! whole bank renders from a fresh engine at a fixed tempo, each key on its own output, and no two
//! kits — creative, audition or Init — may share a sound; and every setting a kit makes is heard
//! ([`every_setting_a_kit_makes_is_heard`]).
//!
//! The reasoning behind each kit, the categories and the rulings are in the plugin's `NOTES.md`
//! (§ *The fifty creative kits*).

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, ModelId, available};
use mxm_preset::{Category, Preset, Value};
use mxm_tempo::Division;
use nice_plug::params::Param;
use nice_plug::prelude::{Params, util};

use super::{AUDITION_KITS, FACTORY_FILES};
use crate::editor::controls;
use crate::params::LfoShapeChoice::{self, SampleHold, Sine, Triangle};
use crate::params::control::{
    ATTACK, BODY, CHARACTER, DECAY, DYNAMICS, NOISE, NOISE_DECAY, PITCH_DECAY, PITCH_ENV, TONE,
    TUNE, USED,
};
use crate::params::{LFO_SYNC, MxmDrumMachineParams, at, slot_ids};
use crate::routes::SourceChoice::{self, Lfo1, Lfo2, Lfo3, Random, Velocity, Wheel};
use crate::routes::{ROUTES, RouteTarget};

/// Choke groups, the same in every kit: closed and open hats; a bass voice; a conga's open and
/// muted strokes; a cymbal and the short hit that stops it.
const HATS: u8 = 1;
const BASS: u8 = 2;
const CONGA: u8 = 3;
const CYMBAL: u8 = 4;

/// One key of a kit. Whatever it does not name stays at Init.
#[derive(Clone, Copy)]
struct Key {
    model: u8,
    /// Level, dB.
    db: f32,
    pan: f32,
    /// `(control, plain value)`, by [`crate::params::control`] number.
    controls: &'static [(usize, f32)],
    /// Tune the model's measured rest pitch (`ModelId::reference_pitch_hz`) onto this note, rather
    /// than by a number of semitones: a re-measured rest pitch then shows up as a kit to regenerate.
    note: Option<&'static str>,
    choke: u8,
    mute: bool,
    /// `(source, target, amount)`, route slots 1… in order.
    routes: &'static [(SourceChoice, RouteTarget, f32)],
}

const fn k(model: u8, db: f32, pan: f32, controls: &'static [(usize, f32)]) -> Key {
    Key {
        model,
        db,
        pan,
        controls,
        note: None,
        choke: 0,
        mute: false,
        routes: &[],
    }
}

impl Key {
    const fn note(self, note: &'static str) -> Self {
        Self {
            note: Some(note),
            ..self
        }
    }

    const fn choke(self, group: u8) -> Self {
        Self {
            choke: group,
            ..self
        }
    }

    /// A role this kit's machine never had: silent, its model a fallback from the same family.
    const fn muted(self) -> Self {
        Self { mute: true, ..self }
    }

    const fn routes(self, routes: &'static [(SourceChoice, RouteTarget, f32)]) -> Self {
        Self { routes, ..self }
    }
}

/// A route aimed at control `k`.
const fn to(k: usize) -> RouteTarget {
    RouteTarget::Control(k as u8)
}

/// A kit-wide LFO the kit sets; the others stay at Init.
struct Lfo {
    lfo: usize,
    shape: LfoShapeChoice,
    rate: Rate,
}

enum Rate {
    Free(f32),
    Synced(Division),
}

const fn free(lfo: usize, shape: LfoShapeChoice, hz: f32) -> Lfo {
    Lfo {
        lfo,
        shape,
        rate: Rate::Free(hz),
    }
}

const fn synced(lfo: usize, shape: LfoShapeChoice, division: Division) -> Lfo {
    Lfo {
        lfo,
        shape,
        rate: Rate::Synced(division),
    }
}

struct Kit {
    name: &'static str,
    keys: [Key; SLOT_COUNT],
    lfos: &'static [Lfo],
}

/// **The fifty**, in the browser's order: reference, mixed machine, pitched and resonant,
/// metallic, low-cost, electronic, experimental, choke and shared source, subtle versus extreme.
static KITS: [Kit; 50] = [
    // ---- Reference -------------------------------------------------------------------------------
    // The bridged-T machine at its long-decay, dark knob positions: the low end blooms and the
    // toms sit a tone lower.
    Kit {
        name: "Bridge Boom",
        keys: [
            // panel Decay long, Tone dark
            k(1, 0.0, 0.0, &[(DECAY, 0.4), (TONE, -0.2)]),
            // panel Tone toward the low mode, a little more Snappy
            k(2, -1.0, 0.0, &[(TONE, -0.15), (NOISE, 0.2)]),
            // tom tuning a tone down
            k(3, -6.0, 0.3, &[(TUNE, -2.0)]),
            // tom tuning a tone down
            k(5, -6.0, 0.05, &[(TUNE, -2.0)]),
            // tom tuning a tone down
            k(7, -6.0, -0.25, &[(TUNE, -2.0)]),
            k(4, -7.0, 0.4, &[]),
            k(6, -7.0, -0.15, &[]),
            k(8, -7.0, -0.4, &[]),
            k(9, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            k(15, -10.0, -0.2, &[]).choke(HATS),
            // panel Decay longer
            k(16, -11.0, -0.2, &[(DECAY, 0.25)]).choke(HATS),
            // panel Decay longer, Tone darker
            k(14, -13.0, 0.3, &[(DECAY, 0.3), (TONE, -0.25)]),
            k(13, -9.0, 0.2, &[]),
            k(10, -9.0, -0.3, &[]),
            k(11, -11.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The same machine at its short, bright settings: a clipped kick, a cracking snare and
    // tuned-up toms for fast, dry patterns.
    Kit {
        name: "Bridge Tight",
        keys: [
            // panel Decay short, Tone bright
            k(1, 0.0, 0.0, &[(DECAY, -0.45), (TONE, 0.25)]),
            // Tone toward the upper mode, Snappy up
            k(2, 0.0, 0.0, &[(TONE, 0.35), (NOISE, 0.45)]),
            // tuned up
            k(3, -6.0, 0.3, &[(TUNE, 3.0)]),
            // tuned up
            k(5, -6.0, 0.05, &[(TUNE, 3.0)]),
            // tuned up
            k(7, -6.0, -0.25, &[(TUNE, 3.0)]),
            // tuned up
            k(4, -7.0, 0.4, &[(TUNE, 2.0)]),
            // tuned up
            k(6, -7.0, -0.15, &[(TUNE, 2.0)]),
            // tuned up
            k(8, -7.0, -0.4, &[(TUNE, 2.0)]),
            k(9, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            // shorter
            k(15, -10.0, -0.2, &[(DECAY, -0.3)]).choke(HATS),
            // panel Decay short
            k(16, -10.0, -0.2, &[(DECAY, -0.35)]).choke(HATS),
            // short and bright
            k(14, -12.0, 0.3, &[(DECAY, -0.4), (TONE, 0.35)]),
            k(13, -9.0, 0.2, &[]),
            k(10, -9.0, -0.3, &[]),
            k(11, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The reset-VCO machine set for the club: a long, clicky kick with a slower sweep, a bright
    // tuned-up snare, and its three toms doubled higher as percussion.
    Kit {
        name: "Reset Club",
        keys: [
            // panel Decay, Attack and sweep time (its 'Tune') up
            k(
                17,
                0.0,
                0.0,
                &[(DECAY, 0.25), (ATTACK, 0.35), (PITCH_DECAY, 0.15)],
            ),
            // panel Tune, Tone and Snappy up
            k(18, -1.0, 0.0, &[(TUNE, 1.0), (TONE, 0.25), (NOISE, 0.3)]),
            // panel Tune down, Decay up
            k(19, -6.0, 0.3, &[(TUNE, -1.0), (DECAY, 0.15)]),
            // panel Tune down, Decay up
            k(20, -6.0, 0.05, &[(TUNE, -1.0), (DECAY, 0.15)]),
            // panel Tune down, Decay up
            k(21, -6.0, -0.25, &[(TUNE, -1.0), (DECAY, 0.15)]),
            // same tom tuned up and shortened as percussion
            k(19, -8.0, 0.4, &[(TUNE, 5.0), (DECAY, -0.25)]),
            // same tom tuned up and shortened
            k(20, -8.0, -0.15, &[(TUNE, 5.0), (DECAY, -0.25)]),
            // same tom tuned up and shortened
            k(21, -8.0, -0.4, &[(TUNE, 6.0), (DECAY, -0.3)]),
            k(22, -8.0, -0.05, &[]),
            k(23, -5.0, 0.05, &[]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            // panel Decay a touch shorter
            k(25, -10.0, -0.2, &[(DECAY, -0.15)]).choke(HATS),
            k(26, -12.0, 0.3, &[]),
            // no cowbell on this machine
            k(22, -9.0, 0.2, &[]).muted(),
            // no clave
            k(22, -9.0, -0.3, &[]).muted(),
            k(27, -12.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // A deeper reading of the same machine: a long kick with a slow sweep, a low noisy snare,
    // rolling toms, a crash tuned down and the ride forward.
    Kit {
        name: "Reset Ride",
        keys: [
            // Decay long, sweep time slow
            k(
                17,
                0.0,
                0.0,
                &[(DECAY, 0.45), (ATTACK, 0.1), (PITCH_DECAY, 0.35)],
            ),
            // tuned down, darker, Snappy well up
            k(18, 0.0, 0.0, &[(TUNE, -1.5), (TONE, -0.3), (NOISE, 0.55)]),
            // tuned down, longer
            k(19, -6.0, 0.3, &[(TUNE, -3.0), (DECAY, 0.35)]),
            // tuned down, longer
            k(20, -6.0, 0.05, &[(TUNE, -3.0), (DECAY, 0.35)]),
            // tuned down, longer
            k(21, -6.0, -0.25, &[(TUNE, -3.0), (DECAY, 0.35)]),
            // no second percussion row
            k(19, -7.0, 0.4, &[]).muted(),
            k(20, -7.0, -0.15, &[]).muted(),
            k(21, -7.0, -0.4, &[]).muted(),
            k(22, -8.0, -0.05, &[]),
            k(23, -5.0, 0.05, &[]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            // panel Decay longer
            k(25, -10.0, -0.2, &[(DECAY, 0.25)]).choke(HATS),
            // Tune down: slower clock, longer and darker
            k(26, -12.0, 0.3, &[(TUNE, -3.0)]),
            // no cowbell
            k(22, -9.0, 0.2, &[]).muted(),
            // no clave
            k(22, -9.0, -0.3, &[]).muted(),
            // the ride forward
            k(27, -9.0, 0.3, &[(TUNE, 1.0)]),
        ],
        lfos: &[],
    },
    // The dual-resonator studio machine whole: round kick, its almost pure-tone snare, three toms
    // and three congas, and a second half-open hat from the same metal bank.
    Kit {
        name: "Expanded Studio",
        keys: [
            // Decay a little longer
            k(32, 0.0, 0.0, &[(DECAY, 0.2)]),
            // its almost pure-tone snare as it is
            k(33, 0.0, 0.0, &[]),
            k(34, -6.0, 0.3, &[]),
            k(35, -6.0, 0.05, &[]),
            k(36, -6.0, -0.25, &[]),
            k(37, -7.0, 0.4, &[]),
            k(38, -7.0, -0.15, &[]),
            k(39, -7.0, -0.4, &[]),
            k(40, -8.0, -0.05, &[]),
            k(44, -5.0, 0.05, &[]),
            k(42, -10.0, -0.2, &[]).choke(HATS),
            k(43, -10.0, -0.2, &[]).choke(HATS),
            k(41, -12.0, 0.3, &[]),
            k(46, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            // same bank, longer: a half-open hat
            k(42, -11.0, 0.35, &[(DECAY, 0.45)]).choke(HATS),
        ],
        lfos: &[],
    },
    // The preset rhythm box of late-seventies pop: soft kick, ticking hat, bongos and congas,
    // bell, guiro and a tambourine on the backbeat.
    Kit {
        name: "Classic Parlour",
        keys: [
            k(60, 0.0, 0.0, &[]),
            k(61, 0.0, 0.0, &[]),
            // a little lower, as the low drum
            k(69, -6.0, 0.3, &[(TUNE, -2.0)]),
            k(68, -6.0, 0.05, &[]),
            k(67, -6.0, -0.25, &[]),
            k(73, -10.0, 0.4, &[]),
            k(72, -9.0, -0.15, &[]),
            k(71, -10.0, -0.4, &[]),
            k(62, -8.0, -0.05, &[]),
            // the tambourine shortened, on the backbeat
            k(71, -7.0, 0.05, &[(DECAY, -0.35)]),
            k(63, -10.0, -0.2, &[]).choke(HATS),
            // the same hat held longer
            k(63, -10.0, -0.2, &[(DECAY, 0.55)]).choke(HATS),
            k(64, -12.0, 0.3, &[]),
            k(70, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            k(65, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The all-discrete organ-top box: woody kick, bongo-bodied snare, congas and bongos on both
    // rows (lower on the tom keys), cowbell and clave.
    Kit {
        name: "Discrete Lounge",
        keys: [
            k(74, 0.0, 0.0, &[]),
            k(81, 0.0, 0.0, &[]),
            // lower, as the low drum
            k(75, -6.0, 0.3, &[(TUNE, -4.0)]),
            // lower
            k(76, -6.0, 0.05, &[(TUNE, -3.0)]),
            // lower
            k(77, -6.0, -0.25, &[(TUNE, -3.0)]),
            k(75, -7.0, 0.4, &[]),
            k(76, -7.0, -0.15, &[]),
            k(77, -7.0, -0.4, &[]),
            k(79, -8.0, -0.05, &[]),
            // no clap on this machine
            k(79, -5.0, 0.05, &[]).muted(),
            k(82, -10.0, -0.2, &[]).choke(HATS),
            // held longer as the open hat
            k(82, -10.0, -0.2, &[(DECAY, 0.5)]).choke(HATS),
            k(84, -12.0, 0.3, &[]),
            k(78, -9.0, 0.2, &[]),
            k(80, -9.0, -0.3, &[]),
            k(83, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The earliest transistor box: a thumpy kick, soft snare, congas and bongo, brush, and its
    // single noise cymbal cut to three lengths for closed, open and crash.
    Kit {
        name: "Early Organ-Top",
        keys: [
            k(85, 0.0, 0.0, &[]),
            k(91, 0.0, 0.0, &[]),
            k(86, -6.0, 0.3, &[]),
            k(87, -6.0, 0.05, &[]),
            k(88, -6.0, -0.25, &[]),
            // no second row on this machine
            k(86, -7.0, 0.4, &[]).muted(),
            k(87, -7.0, -0.15, &[]).muted(),
            k(88, -7.0, -0.4, &[]).muted(),
            // the clave lower and drier, as a rim
            k(90, -8.0, -0.05, &[(TUNE, -5.0), (DECAY, -0.3)]),
            k(94, -5.0, 0.05, &[]),
            // the cymbal cut short as a closed hat
            k(92, -10.0, -0.2, &[(DECAY, -0.85), (TONE, 0.3)]).choke(HATS),
            // the cymbal half-length as an open hat
            k(92, -10.0, -0.2, &[(DECAY, -0.4)]).choke(HATS),
            k(92, -12.0, 0.3, &[]),
            k(89, -9.0, 0.2, &[]),
            k(90, -9.0, -0.3, &[]),
            k(93, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The small battery box at full stretch: its two toms tuned into three tom pitches and three
    // higher percussion pitches, and a half-open hat beside its tempo-coupled open hat.
    Kit {
        name: "Compact Battery",
        keys: [
            k(47, 0.0, 0.0, &[]),
            k(48, 0.0, 0.0, &[]),
            // a little lower
            k(49, -6.0, 0.3, &[(TUNE, -2.0)]),
            // the low tom up a third
            k(49, -6.0, 0.05, &[(TUNE, 2.5)]),
            k(50, -6.0, -0.25, &[]),
            // tuned up and shorter as percussion
            k(49, -7.0, 0.4, &[(TUNE, 7.0), (DECAY, -0.3)]),
            // tuned up and shorter
            k(50, -7.0, -0.15, &[(TUNE, 5.0), (DECAY, -0.3)]),
            // tuned up and shorter
            k(50, -7.0, -0.4, &[(TUNE, 10.0), (DECAY, -0.4)]),
            // no rim on this machine
            k(48, -8.0, -0.05, &[]).muted(),
            // no clap
            k(48, -5.0, 0.05, &[]).muted(),
            k(52, -10.0, -0.2, &[]).choke(HATS),
            k(53, -10.0, -0.2, &[]).choke(HATS),
            k(51, -12.0, 0.3, &[]),
            // no cowbell
            k(51, -9.0, 0.2, &[]).muted(),
            // no clave
            k(50, -9.0, -0.3, &[]).muted(),
            // half-open: shorter
            k(53, -11.0, 0.35, &[(DECAY, -0.45)]).choke(HATS),
        ],
        lfos: &[],
    },
    // The pocket box: its whack kick tuned up into three toms, its own clap, hats and cymbal, and
    // a half-open hat.
    Kit {
        name: "Snap Pocket",
        keys: [
            k(54, 0.0, 0.0, &[]),
            k(55, 0.0, 0.0, &[]),
            // the kick a fifth up as a low tom
            k(54, -6.0, 0.3, &[(TUNE, 7.0), (DECAY, -0.2)]),
            // up a minor seventh
            k(54, -6.0, 0.05, &[(TUNE, 10.0), (DECAY, -0.25)]),
            // up a ninth
            k(54, -6.0, -0.25, &[(TUNE, 14.0), (DECAY, -0.3)]),
            // no percussion row
            k(54, -7.0, 0.4, &[]).muted(),
            k(54, -7.0, -0.15, &[]).muted(),
            k(54, -7.0, -0.4, &[]).muted(),
            // no rim
            k(55, -8.0, -0.05, &[]).muted(),
            k(59, -5.0, 0.05, &[]),
            k(57, -10.0, -0.2, &[]).choke(HATS),
            k(58, -10.0, -0.2, &[]).choke(HATS),
            k(56, -12.0, 0.3, &[]),
            // no cowbell
            k(56, -9.0, 0.2, &[]).muted(),
            // no clave
            k(55, -9.0, -0.3, &[]).muted(),
            // half-open: shorter
            k(58, -11.0, 0.35, &[(DECAY, -0.5)]).choke(HATS),
        ],
        lfos: &[],
    },
    // ---- Mixed machine ---------------------------------------------------------------------------
    // The hybrid everybody builds by hand: the bridged-T kick and toms under the reset machine's
    // snare, clap, sampled hats and cymbals.
    Kit {
        name: "Heavy Bottom, Bright Top",
        keys: [
            // a little longer and darker
            k(1, 0.0, 0.0, &[(DECAY, 0.25), (TONE, -0.1)]),
            // brighter, more Snappy
            k(18, 0.0, 0.0, &[(TONE, 0.15), (NOISE, 0.25)]),
            k(3, -6.0, 0.3, &[]),
            k(5, -6.0, 0.05, &[]),
            k(7, -6.0, -0.25, &[]),
            k(37, -7.0, 0.4, &[]),
            k(38, -7.0, -0.15, &[]),
            k(39, -7.0, -0.4, &[]),
            k(22, -8.0, -0.05, &[]),
            k(23, -5.0, 0.05, &[]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            k(25, -10.0, -0.2, &[]).choke(HATS),
            k(26, -12.0, 0.3, &[]),
            k(13, -9.0, 0.2, &[]),
            k(10, -9.0, -0.3, &[]),
            k(27, -12.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The other hybrid: the reset kick's click and sweep under the bridged-T snare, clap and
    // six-square metal, with the studio machine's diode toms.
    Kit {
        name: "Reset Low, Bridge High",
        keys: [
            // a little longer, more attack
            k(17, 0.0, 0.0, &[(DECAY, 0.1), (ATTACK, 0.2)]),
            // upper mode and Snappy up
            k(2, 0.0, 0.0, &[(TONE, 0.2), (NOISE, 0.35)]),
            // a semitone low
            k(34, -6.0, 0.3, &[(TUNE, -1.0)]),
            k(35, -6.0, 0.05, &[]),
            k(36, -6.0, -0.25, &[]),
            // a semitone low
            k(4, -7.0, 0.4, &[(TUNE, -1.0)]),
            // a semitone low
            k(6, -7.0, -0.15, &[(TUNE, -1.0)]),
            // a semitone low
            k(8, -7.0, -0.4, &[(TUNE, -1.0)]),
            k(9, -8.0, -0.05, &[]),
            // a slightly bigger room
            k(12, -5.0, 0.05, &[(DECAY, 0.15)]),
            // tighter
            k(15, -10.0, -0.2, &[(DECAY, -0.2)]).choke(HATS),
            // longer
            k(16, -10.0, -0.2, &[(DECAY, 0.2)]).choke(HATS),
            // brighter
            k(14, -12.0, 0.3, &[(TONE, 0.2)]),
            k(46, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            k(11, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Three preset boxes in one: the late-seventies kick and rim, the discrete snare, early congas
    // and bongos, a wire brush on the backbeat.
    Kit {
        name: "Rhythm Box Mixtape",
        keys: [
            k(60, 0.0, 0.0, &[]),
            // a little more wire
            k(81, 0.0, 0.0, &[(NOISE, 0.2)]),
            // lower, as the low drum
            k(69, -6.0, 0.3, &[(TUNE, -3.0)]),
            k(75, -6.0, 0.05, &[]),
            // a tone lower
            k(88, -6.0, -0.25, &[(TUNE, -2.0)]),
            k(86, -7.0, 0.4, &[]),
            k(87, -7.0, -0.15, &[]),
            k(67, -7.0, -0.4, &[]),
            k(62, -8.0, -0.05, &[]),
            k(94, -5.0, 0.05, &[]),
            k(63, -10.0, -0.2, &[]).choke(HATS),
            // held longer as the open hat
            k(82, -10.0, -0.2, &[(DECAY, 0.5)]).choke(HATS),
            k(92, -12.0, 0.3, &[]),
            k(89, -9.0, 0.2, &[]),
            k(80, -9.0, -0.3, &[]),
            k(65, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Nine machines in sixteen keys, no two neighbours from the same box: the pool in one kit.
    Kit {
        name: "One of Each",
        keys: [
            k(74, 0.0, 0.0, &[]),
            k(55, 0.0, 0.0, &[]),
            k(34, -6.0, 0.3, &[]),
            // up a minor third, between its neighbours
            k(49, -6.0, 0.05, &[(TUNE, 3.0)]),
            k(7, -6.0, -0.25, &[]),
            k(86, -7.0, 0.4, &[]),
            k(38, -7.0, -0.15, &[]),
            k(67, -7.0, -0.4, &[]),
            k(30, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            k(24, -10.0, -0.2, &[]).choke(HATS),
            k(53, -10.0, -0.2, &[]).choke(HATS),
            k(41, -12.0, 0.3, &[]),
            k(78, -9.0, 0.2, &[]),
            k(90, -9.0, -0.3, &[]),
            k(71, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The two bridged-T machines side by side: the studio machine's dual-low kick, toms and congas
    // woven through the original bridged-T snare, toms, metal and maraca.
    Kit {
        name: "Bridged Pair",
        keys: [
            // a little longer
            k(32, 0.0, 0.0, &[(DECAY, 0.15)]),
            // a touch brighter, more Snappy
            k(2, 0.0, 0.0, &[(TONE, 0.1), (NOISE, 0.25)]),
            // a semitone low
            k(3, -6.0, 0.3, &[(TUNE, -1.0)]),
            k(35, -6.0, 0.05, &[]),
            // a semitone up
            k(7, -6.0, -0.25, &[(TUNE, 1.0)]),
            // a semitone low
            k(37, -7.0, 0.4, &[(TUNE, -1.0)]),
            k(6, -7.0, -0.15, &[]),
            // a semitone up
            k(39, -7.0, -0.4, &[(TUNE, 1.0)]),
            k(9, -8.0, -0.05, &[]),
            // a bigger room
            k(44, -5.0, 0.05, &[(DECAY, 0.2)]),
            // tighter
            k(42, -10.0, -0.2, &[(DECAY, -0.2)]).choke(HATS),
            k(16, -10.0, -0.2, &[]).choke(HATS),
            // longer
            k(14, -12.0, 0.3, &[(DECAY, 0.2)]),
            k(13, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            k(11, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Six-bit sampled hats, crash and ride over warm transistor drums and hand percussion.
    Kit {
        name: "Lo-Fi Tops, Wooden Floor",
        keys: [
            // a little longer
            k(60, 0.0, 0.0, &[(DECAY, 0.2)]),
            k(91, 0.0, 0.0, &[]),
            // lower, as the low drum
            k(86, -6.0, 0.3, &[(TUNE, -2.0)]),
            // lower
            k(87, -6.0, 0.05, &[(TUNE, -2.0)]),
            k(88, -6.0, -0.25, &[]),
            k(75, -7.0, 0.4, &[]),
            k(76, -7.0, -0.15, &[]),
            k(77, -7.0, -0.4, &[]),
            k(79, -8.0, -0.05, &[]),
            k(23, -5.0, 0.05, &[]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            k(25, -10.0, -0.2, &[]).choke(HATS),
            k(26, -12.0, 0.3, &[]),
            k(70, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            k(27, -12.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // A percussion section from four machines: congas and bongos across six keys, shakers on the
    // hat keys, guiro, and velocity that darkens soft hits.
    Kit {
        name: "Hand Percussion Machines",
        keys: [
            // shorter, out of the percussion's way
            k(74, -2.0, 0.0, &[(DECAY, -0.2)]),
            k(81, 0.0, 0.0, &[]),
            k(37, -6.0, 0.3, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(69, -6.0, 0.05, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(38, -6.0, -0.25, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(68, -6.0, 0.4, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(67, -6.0, -0.15, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(77, -6.0, -0.4, &[(DYNAMICS, -0.3)]).routes(&[(Velocity, to(TONE), 0.4)]),
            k(40, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            // a shaker on the closed-hat key
            k(65, -10.0, -0.2, &[]).choke(HATS),
            // a longer shake on the open-hat key
            k(83, -10.0, -0.2, &[(DECAY, 0.4)]).choke(HATS),
            k(64, -12.0, 0.3, &[]),
            k(70, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            k(72, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Compact boxes up front: the whack kick, a high crisp reset snare, small toms and the pocket
    // box's mixed-source hats, with studio congas tuned up.
    Kit {
        name: "Snappy Hybrid",
        keys: [
            // a little longer
            k(54, 0.0, 0.0, &[(DECAY, 0.15)]),
            // tuned up two semitones, more Snappy
            k(18, 0.0, 0.0, &[(TUNE, 2.0), (NOISE, 0.4)]),
            k(49, -6.0, 0.3, &[]),
            // a minor third down, between the toms
            k(50, -6.0, 0.05, &[(TUNE, -3.0)]),
            k(50, -6.0, -0.25, &[]),
            // tuned up a tone
            k(37, -7.0, 0.4, &[(TUNE, 2.0)]),
            // tuned up a tone
            k(38, -7.0, -0.15, &[(TUNE, 2.0)]),
            // tuned up a tone
            k(39, -7.0, -0.4, &[(TUNE, 2.0)]),
            k(22, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            k(57, -10.0, -0.2, &[]).choke(HATS),
            k(58, -10.0, -0.2, &[]).choke(HATS),
            k(51, -12.0, 0.3, &[]),
            k(46, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            k(83, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // ---- Pitched / resonant ----------------------------------------------------------------------
    // Toms and congas tuned to F major pentatonic, with less pitch fall, so tom fills play a tune;
    // kick, snare, rim, cowbell and clave sit in the same key.
    Kit {
        name: "Pentatonic Toms",
        keys: [
            // tuned to G1, a little longer
            k(1, 0.0, 0.0, &[(DECAY, 0.15)]).note("G1"),
            // body tuned to F3
            k(2, 0.0, 0.0, &[]).note("F3"),
            // F2; less fall and a quicker settle so it reads as a note
            k(
                3,
                -6.0,
                0.3,
                &[(DECAY, 0.2), (PITCH_ENV, -0.4), (PITCH_DECAY, -0.2)],
            )
            .note("F2"),
            // C3, same shaping
            k(
                5,
                -6.0,
                0.05,
                &[(DECAY, 0.2), (PITCH_ENV, -0.4), (PITCH_DECAY, -0.2)],
            )
            .note("C3"),
            // F3, same shaping
            k(
                7,
                -6.0,
                -0.25,
                &[(DECAY, 0.2), (PITCH_ENV, -0.4), (PITCH_DECAY, -0.2)],
            )
            .note("F3"),
            // G3, less fall
            k(4, -7.0, 0.4, &[(DECAY, 0.15), (PITCH_ENV, -0.3)]).note("G3"),
            // C4, less fall
            k(6, -7.0, -0.15, &[(DECAY, 0.15), (PITCH_ENV, -0.3)]).note("C4"),
            // G4, less fall
            k(8, -7.0, -0.4, &[(DECAY, 0.15), (PITCH_ENV, -0.3)]).note("G4"),
            // A4
            k(9, -8.0, -0.05, &[]).note("A4"),
            k(12, -5.0, 0.05, &[]),
            k(15, -10.0, -0.2, &[]).choke(HATS),
            k(16, -10.0, -0.2, &[]).choke(HATS),
            k(14, -12.0, 0.3, &[]),
            // G5, longer ring (leaves the shared bank)
            k(13, -9.0, 0.2, &[(DECAY, 0.2)]).note("G5"),
            // D7
            k(10, -9.0, -0.3, &[]).note("D7"),
            k(11, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The studio machine's diode toms and congas tuned to G major pentatonic, ringing long through
    // their own pink-noise reverb loop.
    Kit {
        name: "Diode Choir",
        keys: [
            // B1
            k(32, 0.0, 0.0, &[]).note("B1"),
            // A3
            k(33, 0.0, 0.0, &[]).note("A3"),
            // G2; long, a longer and slightly louder reverb loop, gentler sweep
            k(
                34,
                -6.0,
                0.3,
                &[
                    (DECAY, 0.45),
                    (NOISE, 0.5),
                    (NOISE_DECAY, 0.4),
                    (PITCH_ENV, -0.2),
                ],
            )
            .note("G2"),
            // D3, same
            k(
                35,
                -6.0,
                0.05,
                &[
                    (DECAY, 0.45),
                    (NOISE, 0.5),
                    (NOISE_DECAY, 0.4),
                    (PITCH_ENV, -0.2),
                ],
            )
            .note("D3"),
            // G3, same
            k(
                36,
                -6.0,
                -0.25,
                &[
                    (DECAY, 0.45),
                    (NOISE, 0.5),
                    (NOISE_DECAY, 0.4),
                    (PITCH_ENV, -0.2),
                ],
            )
            .note("G3"),
            // D4, long
            k(37, -7.0, 0.4, &[(DECAY, 0.35)]).note("D4"),
            // A4, long
            k(38, -7.0, -0.15, &[(DECAY, 0.35)]).note("A4"),
            // E5, long
            k(39, -7.0, -0.4, &[(DECAY, 0.35)]).note("E5"),
            // E6
            k(40, -8.0, -0.05, &[]).note("E6"),
            k(44, -5.0, 0.05, &[]),
            k(42, -10.0, -0.2, &[]).choke(HATS),
            k(43, -10.0, -0.2, &[]).choke(HATS),
            k(41, -12.0, 0.3, &[]),
            // A5
            k(46, -9.0, 0.2, &[]).note("A5"),
            // D7
            k(45, -9.0, -0.3, &[]).note("D7"),
            // a second cowbell on G5, longer
            k(46, -10.0, 0.35, &[(DECAY, 0.3)]).note("G5"),
        ],
        lfos: &[],
    },
    // Disco and electro tom sweeps: deep, slow pitch falls on two machines' toms, the reset toms
    // rounded toward their triangle.
    Kit {
        name: "Syn-Tom Sweeps",
        keys: [
            // a deeper sweep
            k(17, 0.0, 0.0, &[(PITCH_ENV, 0.3), (DECAY, 0.15)]),
            k(33, 0.0, 0.0, &[]),
            // low; four-fold sweep, slower, more body, less noise tail
            k(
                3,
                -6.0,
                0.3,
                &[
                    (TUNE, -3.0),
                    (DECAY, 0.4),
                    (PITCH_ENV, 0.8),
                    (PITCH_DECAY, 0.5),
                    (BODY, 0.4),
                    (NOISE, -0.5),
                ],
            ),
            // same sweep shaping
            k(
                5,
                -6.0,
                0.05,
                &[
                    (TUNE, -3.0),
                    (DECAY, 0.4),
                    (PITCH_ENV, 0.8),
                    (PITCH_DECAY, 0.5),
                    (BODY, 0.4),
                ],
            ),
            // same sweep shaping
            k(
                7,
                -6.0,
                -0.25,
                &[
                    (TUNE, -3.0),
                    (DECAY, 0.4),
                    (PITCH_ENV, 0.8),
                    (PITCH_DECAY, 0.5),
                    (BODY, 0.4),
                ],
            ),
            // deep slow sweep, softer clip, less noise
            k(
                19,
                -7.0,
                0.4,
                &[
                    (DECAY, 0.3),
                    (PITCH_ENV, 0.7),
                    (PITCH_DECAY, 0.6),
                    (NOISE, -0.6),
                    (CHARACTER, -0.4),
                ],
            ),
            // same
            k(
                20,
                -7.0,
                -0.15,
                &[
                    (DECAY, 0.3),
                    (PITCH_ENV, 0.7),
                    (PITCH_DECAY, 0.6),
                    (NOISE, -0.6),
                    (CHARACTER, -0.4),
                ],
            ),
            // same
            k(
                21,
                -7.0,
                -0.4,
                &[
                    (DECAY, 0.3),
                    (PITCH_ENV, 0.7),
                    (PITCH_DECAY, 0.6),
                    (NOISE, -0.6),
                    (CHARACTER, -0.4),
                ],
            ),
            k(40, -8.0, -0.05, &[]),
            k(59, -5.0, 0.05, &[]),
            k(52, -10.0, -0.2, &[]).choke(HATS),
            k(53, -10.0, -0.2, &[]).choke(HATS),
            k(56, -12.0, 0.3, &[]),
            k(78, -9.0, 0.2, &[]),
            k(80, -9.0, -0.3, &[]),
            k(27, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Claves, rims, bongos and a struck rim tuned to C major pentatonic and left to ring: tom and
    // percussion keys play a wooden scale.
    Kit {
        name: "Woodshop Marimba",
        keys: [
            // G1
            k(85, 0.0, 0.0, &[]).note("G1"),
            // C4
            k(81, 0.0, 0.0, &[]).note("C4"),
            // A3, left to ring
            k(22, -6.0, 0.3, &[(DECAY, 0.5)]),
            // E4; longer, rounder
            k(90, -6.0, 0.05, &[(DECAY, 0.4), (BODY, 0.3)]).note("E4"),
            // G4; longer, rounder
            k(68, -6.0, -0.25, &[(DECAY, 0.3), (BODY, 0.3)]).note("G4"),
            // C5; longer, rounder
            k(88, -7.0, 0.4, &[(DECAY, 0.3), (BODY, 0.3)]).note("C5"),
            // D5; longer, rounder
            k(77, -7.0, -0.15, &[(DECAY, 0.3), (BODY, 0.3)]).note("D5"),
            // E5; longer
            k(67, -7.0, -0.4, &[(DECAY, 0.3)]).note("E5"),
            // E6
            k(62, -8.0, -0.05, &[]).note("E6"),
            k(94, -5.0, 0.05, &[]),
            // a shaker on the closed-hat key
            k(65, -10.0, -0.2, &[]).choke(HATS),
            // a longer shake
            k(83, -10.0, -0.2, &[(DECAY, 0.4)]).choke(HATS),
            // shorter
            k(92, -12.0, 0.3, &[(DECAY, -0.3)]),
            // D7, a high block
            k(80, -9.0, 0.2, &[]).note("D7"),
            // E7
            k(66, -9.0, -0.3, &[]).note("E7"),
            // C7
            k(45, -10.0, 0.35, &[]).note("C7"),
        ],
        lfos: &[],
    },
    // Congas and bongos you can bend: the mod wheel raises every hand drum by up to two semitones,
    // and soft hits sit a little flat, as a real skin does.
    Kit {
        name: "Talking Congas",
        keys: [
            k(74, 0.0, 0.0, &[]),
            k(91, 0.0, 0.0, &[]),
            // deeper, with a little more pitch drop, settling sooner
            k(
                69,
                -6.0,
                0.3,
                &[(TUNE, -4.0), (PITCH_ENV, 0.25), (PITCH_DECAY, -0.4)],
            )
            .routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(75, -6.0, 0.05, &[]).routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(76, -6.0, -0.25, &[]).routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(86, -7.0, 0.4, &[]).routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(87, -7.0, -0.15, &[]).routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(88, -7.0, -0.4, &[]).routes(&[(Wheel, to(TUNE), 0.17), (Velocity, to(TUNE), 0.04)]),
            k(79, -8.0, -0.05, &[]),
            k(44, -5.0, 0.05, &[]),
            k(82, -10.0, -0.2, &[]).choke(HATS),
            // held longer as the open hat
            k(63, -10.0, -0.2, &[(DECAY, 0.5)]).choke(HATS),
            k(84, -12.0, 0.3, &[]),
            k(89, -9.0, 0.2, &[]),
            k(90, -9.0, -0.3, &[]),
            k(72, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // ---- Metallic --------------------------------------------------------------------------------
    // The bridged-T machine's six-square metal pushed into its saturating VCAs: cowbells an octave
    // down on the tom keys, a cymbal tuned to a gong, everything driven.
    Kit {
        name: "Six-Square Foundry",
        keys: [
            // a little grit
            k(1, 0.0, 0.0, &[(CHARACTER, 0.35), (DECAY, 0.1)]),
            // upper mode forward, more Snappy
            k(2, 0.0, 0.0, &[(TONE, 0.6), (NOISE, 0.2)]),
            // cowbell an octave down, ringing, driven
            k(
                13,
                -6.0,
                0.3,
                &[(TUNE, -12.0), (DECAY, 0.45), (CHARACTER, 0.3)],
            ),
            // a fifth down
            k(
                13,
                -6.0,
                0.05,
                &[(TUNE, -7.0), (DECAY, 0.4), (CHARACTER, 0.3)],
            ),
            // a minor third down
            k(
                13,
                -6.0,
                -0.25,
                &[(TUNE, -3.0), (DECAY, 0.35), (CHARACTER, 0.3)],
            ),
            // cymbal an octave down, shorter: a gong
            k(
                14,
                -7.0,
                0.4,
                &[(TUNE, -12.0), (DECAY, -0.3), (CHARACTER, 0.4)],
            ),
            // open hat a fifth down
            k(
                16,
                -7.0,
                -0.15,
                &[(TUNE, -7.0), (DECAY, -0.2), (CHARACTER, 0.3)],
            ),
            // closed hat a fourth up
            k(15, -7.0, -0.4, &[(TUNE, 5.0), (CHARACTER, 0.3)]),
            // harder clip
            k(9, -8.0, -0.05, &[(CHARACTER, -0.5)]),
            // narrower, ringing tail
            k(12, -5.0, 0.05, &[(CHARACTER, 0.6)]),
            // driven
            k(15, -10.0, -0.2, &[(CHARACTER, 0.5)]).choke(HATS),
            // driven
            k(16, -10.0, -0.2, &[(CHARACTER, 0.4)]).choke(HATS),
            // longer, driven
            k(14, -12.0, 0.3, &[(DECAY, 0.35), (CHARACTER, 0.35)]),
            // untouched: it reads the same bank as the hats and cymbal
            k(13, -9.0, 0.2, &[]),
            // harder clip
            k(10, -9.0, -0.3, &[(CHARACTER, -0.4)]),
            // brighter
            k(11, -10.0, 0.35, &[(TONE, 0.4)]),
        ],
        lfos: &[],
    },
    // Five cowbells and the triple bell tuned to an A minor seventh across the tom and percussion
    // keys: fills ring out as a chord.
    Kit {
        name: "Cowbell Chord",
        keys: [
            // A1, a little longer
            k(1, 0.0, 0.0, &[(DECAY, 0.2)]).note("A1"),
            // a little brighter
            k(48, 0.0, 0.0, &[(TONE, 0.2)]),
            // A4; ringing, a little drive
            k(89, -6.0, 0.3, &[(DECAY, 0.35), (CHARACTER, 0.2)]).note("A4"),
            // C5; ringing, a little drive
            k(70, -6.0, 0.05, &[(DECAY, 0.35), (CHARACTER, 0.2)]).note("C5"),
            // E5; ringing, a little drive
            k(13, -6.0, -0.25, &[(DECAY, 0.35), (CHARACTER, 0.2)]).note("E5"),
            // G5; ringing, fuller
            k(
                78,
                -7.0,
                0.4,
                &[(DECAY, 0.35), (BODY, 0.3), (CHARACTER, 0.2)],
            )
            .note("G5"),
            // A5; ringing
            k(46, -7.0, -0.15, &[(DECAY, 0.35), (CHARACTER, 0.2)]).note("A5"),
            // C8; longer
            k(73, -7.0, -0.4, &[(DECAY, 0.3)]).note("C8"),
            // E6
            k(40, -8.0, -0.05, &[]).note("E6"),
            k(59, -5.0, 0.05, &[]),
            // a little drive
            k(52, -10.0, -0.2, &[(CHARACTER, 0.3)]).choke(HATS),
            // a little drive
            k(53, -10.0, -0.2, &[(CHARACTER, 0.3)]).choke(HATS),
            k(51, -12.0, 0.3, &[]),
            k(13, -9.0, 0.2, &[]),
            // E7
            k(10, -9.0, -0.3, &[]).note("E7"),
            k(72, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // The six-bit sampled metal bit-crushed and driven, the reset toms clipped toward square,
    // clanging metal on the percussion keys.
    Kit {
        name: "Rust and Chrome",
        keys: [
            // pushed harder into its output stage
            k(17, 0.0, 0.0, &[(TONE, 0.4)]),
            // brighter, more Snappy
            k(18, 0.0, 0.0, &[(TONE, 0.5), (NOISE, 0.3)]),
            // harder clip, a tone down
            k(19, -6.0, 0.3, &[(TUNE, -2.0), (CHARACTER, 0.7)]),
            // harder clip, a tone down
            k(20, -6.0, 0.05, &[(TUNE, -2.0), (CHARACTER, 0.7)]),
            // harder clip, a tone down
            k(21, -6.0, -0.25, &[(TUNE, -2.0), (CHARACTER, 0.7)]),
            // studio cymbal an octave down, short: a clang
            k(41, -7.0, 0.4, &[(TUNE, -12.0), (DECAY, -0.5)]),
            // cowbell a fourth down, driven
            k(46, -7.0, -0.15, &[(TUNE, -5.0), (CHARACTER, 0.5)]),
            // cowbell a fourth up, driven
            k(13, -7.0, -0.4, &[(TUNE, 5.0), (CHARACTER, 0.6)]),
            // more drive
            k(22, -8.0, -0.05, &[(CHARACTER, 0.6)]),
            // narrower, ringing band
            k(23, -5.0, 0.05, &[(CHARACTER, -0.5)]),
            // crushed, harder transient
            k(24, -10.0, -0.2, &[(CHARACTER, -0.5), (ATTACK, -0.4)]).choke(HATS),
            // crushed, slower and longer
            k(25, -10.0, -0.2, &[(CHARACTER, -0.6), (TUNE, -3.0)]).choke(HATS),
            // a fifth down, driven
            k(26, -12.0, 0.3, &[(TUNE, -7.0), (CHARACTER, 0.5)]),
            // driven
            k(70, -9.0, 0.2, &[(CHARACTER, 0.6)]),
            // harder clip
            k(10, -9.0, -0.3, &[(CHARACTER, -0.6)]),
            // up a minor third, crushed hard
            k(27, -10.0, 0.35, &[(TUNE, 3.0), (CHARACTER, -0.8)]),
        ],
        lfos: &[],
    },
    // Hats from three different metal banks ticking against each other, two of them pitched down
    // into percussion, over small, dry drums.
    Kit {
        name: "Clockwork Hats",
        keys: [
            // shorter
            k(60, 0.0, 0.0, &[(DECAY, -0.3)]),
            // more wire
            k(48, 0.0, 0.0, &[(NOISE, 0.3)]),
            // a fourth down
            k(50, -6.0, 0.3, &[(TUNE, -5.0)]),
            // a tone down
            k(50, -6.0, 0.05, &[(TUNE, -2.0)]),
            // a tone up
            k(50, -6.0, -0.25, &[(TUNE, 2.0)]),
            // closed hat a fifth down and longer: a tick-tock
            k(57, -7.0, 0.4, &[(TUNE, -7.0), (DECAY, 0.3)]),
            // a fourth down, longer
            k(52, -7.0, -0.15, &[(TUNE, -5.0), (DECAY, 0.2)]),
            // a major third up
            k(42, -7.0, -0.4, &[(TUNE, 4.0)]),
            k(62, -8.0, -0.05, &[]),
            k(44, -5.0, 0.05, &[]),
            k(57, -10.0, -0.2, &[]).choke(HATS),
            k(58, -10.0, -0.2, &[]).choke(HATS),
            k(56, -12.0, 0.3, &[]),
            // a little drive
            k(46, -9.0, 0.2, &[(CHARACTER, 0.3)]),
            k(45, -9.0, -0.3, &[]),
            // shorter, brighter
            k(53, -11.0, 0.35, &[(DECAY, -0.3), (TONE, 0.3)]).choke(HATS),
        ],
        lfos: &[],
    },
    // The organ-top boxes' noise-and-inductor cymbals as texture: three splashes on the percussion
    // keys, saturated hats, a long brush and a tambourine.
    Kit {
        name: "Noise Cymbal Wash",
        keys: [
            k(60, 0.0, 0.0, &[]),
            // a little more wire, a longer wire tail
            k(61, 0.0, 0.0, &[(NOISE, 0.5), (NOISE_DECAY, 0.4)]),
            // a tone lower
            k(86, -6.0, 0.3, &[(TUNE, -2.0)]),
            // a tone lower
            k(87, -6.0, 0.05, &[(TUNE, -2.0)]),
            k(88, -6.0, -0.25, &[]),
            // darker, shorter: a dark splash
            k(64, -7.0, 0.4, &[(TONE, -0.5), (DECAY, -0.3)]),
            // shorter and brighter
            k(84, -7.0, -0.15, &[(DECAY, -0.5), (TONE, 0.3)]),
            // short and bright
            k(92, -7.0, -0.4, &[(DECAY, -0.6), (TONE, 0.4)]),
            k(79, -8.0, -0.05, &[]),
            // a long swish
            k(94, -5.0, 0.05, &[(DECAY, 0.4), (NOISE_DECAY, 0.3)]),
            // saturated
            k(63, -10.0, -0.2, &[(CHARACTER, 0.4)]).choke(HATS),
            // held longer, saturated
            k(82, -10.0, -0.2, &[(DECAY, 0.6), (CHARACTER, 0.3)]).choke(HATS),
            // longer, saturated
            k(64, -12.0, 0.3, &[(DECAY, 0.4), (CHARACTER, 0.3)]),
            k(71, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            k(93, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // ---- Low-cost --------------------------------------------------------------------------------
    // The cheapest box's four circuits stretched over all sixteen keys: kick tuned into toms, rim
    // tuned into percussion, cowbell and clave, its one hat at four lengths.
    Kit {
        name: "Four-Voice Economy",
        keys: [
            k(28, 0.0, 0.0, &[]),
            k(29, 0.0, 0.0, &[]),
            // a fifth up, shorter: a low tom
            k(28, -6.0, 0.3, &[(TUNE, 7.0), (DECAY, -0.2)]),
            // a minor seventh up
            k(28, -6.0, 0.05, &[(TUNE, 10.0), (DECAY, -0.25)]),
            // a ninth up
            k(28, -6.0, -0.25, &[(TUNE, 14.0), (DECAY, -0.3)]),
            // an octave down, longer: a woody low drum
            k(30, -7.0, 0.4, &[(TUNE, -12.0), (DECAY, 0.4)]),
            // a fifth down, longer
            k(30, -7.0, -0.15, &[(TUNE, -7.0), (DECAY, 0.3)]),
            // a minor third down
            k(30, -7.0, -0.4, &[(TUNE, -3.0), (DECAY, 0.2)]),
            k(30, -8.0, -0.05, &[]),
            // thinner, brighter, more and longer wires: a noisier second snare for the clap key
            k(
                29,
                -5.0,
                0.05,
                &[(NOISE, 0.6), (BODY, -0.6), (TONE, 0.3), (NOISE_DECAY, 0.3)],
            ),
            k(31, -10.0, -0.2, &[]).choke(HATS),
            // held longer as the open hat
            k(31, -10.0, -0.2, &[(DECAY, 0.6)]).choke(HATS),
            // at its longest, darker: a wash
            k(31, -12.0, 0.3, &[(DECAY, 1.0), (TONE, -0.3)]),
            // a minor third up, ringing: a knock
            k(30, -9.0, 0.2, &[(TUNE, 3.0), (DECAY, 0.5)]),
            // a fifth up: a clave
            k(30, -9.0, -0.3, &[(TUNE, 7.0), (DECAY, -0.2)]),
            // short and bright: a tick
            k(31, -10.0, 0.35, &[(DECAY, -0.5), (TONE, 0.4)]),
        ],
        lfos: &[],
    },
    // The two little battery boxes together: one's kick, toms, closed hat and cymbal, the other's
    // snare, clap and open hat, with the whack kick tuned into blips.
    Kit {
        name: "Pocket Pair",
        keys: [
            // shorter, brighter
            k(47, 0.0, 0.0, &[(DECAY, -0.2), (TONE, 0.2)]),
            k(55, 0.0, 0.0, &[]),
            k(49, -6.0, 0.3, &[]),
            // up a minor third
            k(49, -6.0, 0.05, &[(TUNE, 3.0)]),
            k(50, -6.0, -0.25, &[]),
            // an octave up, short: a blip
            k(54, -7.0, 0.4, &[(TUNE, 12.0), (DECAY, -0.4)]),
            // higher, shorter
            k(54, -7.0, -0.15, &[(TUNE, 16.0), (DECAY, -0.5)]),
            // higher still
            k(54, -7.0, -0.4, &[(TUNE, 19.0), (DECAY, -0.55)]),
            // tuned up, fewer wires, short: a rim click
            k(
                48,
                -8.0,
                -0.05,
                &[(TUNE, 5.0), (NOISE, -0.6), (DECAY, -0.5)],
            ),
            k(59, -5.0, 0.05, &[]),
            k(52, -10.0, -0.2, &[]).choke(HATS),
            k(58, -10.0, -0.2, &[]).choke(HATS),
            k(56, -12.0, 0.3, &[]),
            // a fourth down, short, a little drive: a metal knock
            k(
                51,
                -9.0,
                0.2,
                &[(TUNE, -5.0), (DECAY, -0.5), (CHARACTER, 0.3)],
            ),
            // an octave up, fewer wires, short: a snapped clave
            k(
                55,
                -9.0,
                -0.3,
                &[(TUNE, 12.0), (NOISE, -0.7), (DECAY, -0.6)],
            ),
            // shorter
            k(53, -11.0, 0.35, &[(DECAY, -0.4)]).choke(HATS),
        ],
        lfos: &[],
    },
    // Cheap boxes overdriven and dulled, like a demo bounced through a worn cassette deck.
    Kit {
        name: "Thrift Store Drive",
        keys: [
            // saturated, darker
            k(54, 0.0, 0.0, &[(CHARACTER, 0.6), (TONE, -0.3)]),
            // darker, more wire
            k(29, 0.0, 0.0, &[(TONE, -0.2), (NOISE, 0.3)]),
            // a tone down, fuller
            k(49, -6.0, 0.3, &[(TUNE, -2.0), (BODY, 0.3)]),
            // a tone down, fuller
            k(50, -6.0, 0.05, &[(TUNE, -2.0), (BODY, 0.3)]),
            // a tone up, fuller
            k(50, -6.0, -0.25, &[(TUNE, 2.0), (BODY, 0.3)]),
            // a semitone down, fuller
            k(86, -7.0, 0.4, &[(TUNE, -1.0), (BODY, 0.2)]),
            // a tone down
            k(88, -7.0, -0.15, &[(TUNE, -2.0)]),
            k(77, -7.0, -0.4, &[]),
            // darker
            k(30, -8.0, -0.05, &[(TONE, -0.3)]),
            // saturated, darker
            k(59, -5.0, 0.05, &[(CHARACTER, 0.5), (TONE, -0.3)]),
            // saturated, darker
            k(31, -10.0, -0.2, &[(CHARACTER, 0.6), (TONE, -0.3)]).choke(HATS),
            // saturated, darker
            k(58, -10.0, -0.2, &[(CHARACTER, 0.5), (TONE, -0.2)]).choke(HATS),
            // saturated, darker
            k(56, -12.0, 0.3, &[(CHARACTER, 0.5), (TONE, -0.3)]),
            // saturated
            k(89, -9.0, 0.2, &[(CHARACTER, 0.5)]),
            k(90, -9.0, -0.3, &[]),
            // darker
            k(93, -10.0, 0.35, &[(TONE, -0.3)]),
        ],
        lfos: &[],
    },
    // Lo-fi digital: the sampled hats, crash and ride crushed to a few bits over the economy and
    // battery boxes' drums.
    Kit {
        name: "Six-Bit Budget",
        keys: [
            // longer, its ring driven a little
            k(28, 0.0, 0.0, &[(DECAY, 0.2), (BODY, 0.2)]),
            // more wire
            k(48, 0.0, 0.0, &[(NOISE, 0.3)]),
            // a minor third down
            k(49, -6.0, 0.3, &[(TUNE, -3.0)]),
            k(49, -6.0, 0.05, &[]),
            k(50, -6.0, -0.25, &[]),
            // a minor seventh down, longer
            k(30, -7.0, 0.4, &[(TUNE, -10.0), (DECAY, 0.4)]),
            // an octave down, longer
            k(62, -7.0, -0.15, &[(TUNE, -12.0), (DECAY, 0.3)]),
            // a minor seventh down, longer
            k(79, -7.0, -0.4, &[(TUNE, -10.0), (DECAY, 0.2)]),
            k(30, -8.0, -0.05, &[]),
            k(44, -5.0, 0.05, &[]),
            // crushed, punchier
            k(24, -10.0, -0.2, &[(CHARACTER, -0.7), (ATTACK, -0.3)]).choke(HATS),
            // crushed, shorter
            k(25, -10.0, -0.2, &[(CHARACTER, -0.6), (DECAY, -0.2)]).choke(HATS),
            // crushed, a tone down
            k(26, -12.0, 0.3, &[(CHARACTER, -0.8), (TUNE, -2.0)]),
            // a little drive
            k(70, -9.0, 0.2, &[(CHARACTER, 0.3)]),
            k(66, -9.0, -0.3, &[]),
            // crushed hard, a tone up
            k(27, -10.0, 0.35, &[(CHARACTER, -0.9), (TUNE, 2.0)]),
        ],
        lfos: &[],
    },
    // Everything small and high: the battery box and friends tuned up, short, like a toy drum set.
    Kit {
        name: "Toy Box",
        keys: [
            // a fifth up, short
            k(47, 0.0, 0.0, &[(TUNE, 7.0), (DECAY, -0.4)]),
            // a fifth up, shorter
            k(48, 0.0, 0.0, &[(TUNE, 7.0), (DECAY, -0.3)]),
            // an octave up
            k(49, -6.0, 0.3, &[(TUNE, 12.0)]),
            // up an octave and a minor third
            k(49, -6.0, 0.05, &[(TUNE, 15.0)]),
            // an octave and a tone up
            k(50, -6.0, -0.25, &[(TUNE, 14.0)]),
            // a fifth up
            k(68, -7.0, 0.4, &[(TUNE, 7.0)]),
            // a fifth up
            k(67, -7.0, -0.15, &[(TUNE, 7.0)]),
            // an octave up
            k(76, -7.0, -0.4, &[(TUNE, 12.0)]),
            // a fourth up
            k(62, -8.0, -0.05, &[(TUNE, 5.0)]),
            // brighter, shorter
            k(59, -5.0, 0.05, &[(TONE, 0.4), (DECAY, -0.3)]),
            // a fourth up
            k(52, -10.0, -0.2, &[(TUNE, 5.0)]).choke(HATS),
            // a fourth up
            k(53, -10.0, -0.2, &[(TUNE, 5.0)]).choke(HATS),
            // a fifth up, shorter
            k(51, -12.0, 0.3, &[(TUNE, 7.0), (DECAY, -0.4)]),
            // a fifth up
            k(70, -9.0, 0.2, &[(TUNE, 7.0)]),
            // an octave up
            k(90, -9.0, -0.3, &[(TUNE, 12.0)]),
            // brighter
            k(65, -10.0, 0.35, &[(TONE, 0.3)]),
        ],
        lfos: &[],
    },
    // ---- Electronic ------------------------------------------------------------------------------
    // Four-to-the-floor techno: a long, clicky reset kick, dark toms, gritty percussion, and an
    // open hat and ride whose brightness breathes with the bar.
    Kit {
        name: "Warehouse Pulse",
        keys: [
            // long, clicky, quicker sweep, even velocity
            k(
                17,
                0.0,
                0.0,
                &[
                    (DECAY, 0.3),
                    (ATTACK, 0.5),
                    (PITCH_DECAY, -0.2),
                    (TONE, 0.15),
                    (DYNAMICS, 0.3),
                ],
            ),
            // up a tone, bright, more Snappy
            k(18, 0.0, 0.0, &[(TUNE, 2.0), (TONE, 0.3), (NOISE, 0.35)]),
            // a fourth down, longer
            k(19, -6.0, 0.3, &[(TUNE, -5.0), (DECAY, 0.2)]),
            // a fourth down, longer
            k(20, -6.0, 0.05, &[(TUNE, -5.0), (DECAY, 0.2)]),
            // a fourth down, longer
            k(21, -6.0, -0.25, &[(TUNE, -5.0), (DECAY, 0.2)]),
            // a minor seventh up, short, gritty
            k(
                54,
                -7.0,
                0.4,
                &[(TUNE, 10.0), (DECAY, -0.35), (CHARACTER, 0.4)],
            ),
            // a fifth up, short
            k(50, -7.0, -0.15, &[(TUNE, 7.0), (DECAY, -0.3)]),
            // an octave down, longer: a low block
            k(45, -7.0, -0.4, &[(TUNE, -12.0), (DECAY, 0.3)]),
            // a little drive
            k(22, -8.0, -0.05, &[(CHARACTER, 0.3)]),
            // a bigger room
            k(23, -5.0, 0.05, &[(DECAY, 0.2)]),
            // even rolls
            k(24, -9.0, -0.2, &[(DYNAMICS, 0.4)]).choke(HATS),
            // shorter; Tone follows a one-bar triangle
            k(25, -10.0, -0.2, &[(DECAY, -0.2)])
                .choke(HATS)
                .routes(&[(Lfo1, to(TONE), 0.3)]),
            // a tone down
            k(26, -12.0, 0.3, &[(TUNE, -2.0)]),
            // a minor third down
            k(46, -9.0, 0.2, &[(TUNE, -3.0)]),
            k(45, -9.0, -0.3, &[]),
            // Tone on a two-bar sine
            k(27, -11.0, 0.35, &[]).routes(&[(Lfo2, to(TONE), 0.2)]),
        ],
        lfos: &[
            synced(1, Triangle, Division::Whole),
            synced(2, Sine, Division::TwoBars),
        ],
    },
    // Early-eighties electro: a short, gritty bridged-T kick and snappy snare, cowbell and clave
    // up front, the studio machine's hats and clap.
    Kit {
        name: "Electro Breaks",
        keys: [
            // a semitone up, a little shorter, gritty
            k(
                1,
                0.0,
                0.0,
                &[(TUNE, 1.0), (DECAY, -0.1), (CHARACTER, 0.25)],
            ),
            // upper mode, Snappy up
            k(2, 0.0, 0.0, &[(TONE, 0.3), (NOISE, 0.4)]),
            // a semitone down
            k(49, -6.0, 0.3, &[(TUNE, -1.0)]),
            // a major third down
            k(50, -6.0, 0.05, &[(TUNE, -4.0)]),
            k(50, -6.0, -0.25, &[]),
            // a semitone up
            k(4, -7.0, 0.4, &[(TUNE, 1.0)]),
            // a semitone up
            k(6, -7.0, -0.15, &[(TUNE, 1.0)]),
            // a semitone up
            k(8, -7.0, -0.4, &[(TUNE, 1.0)]),
            k(9, -8.0, -0.05, &[]),
            k(44, -5.0, 0.05, &[]),
            k(42, -10.0, -0.2, &[]).choke(HATS),
            k(43, -10.0, -0.2, &[]).choke(HATS),
            k(41, -12.0, 0.3, &[]),
            k(13, -7.0, 0.2, &[]),
            k(10, -7.0, -0.3, &[]),
            k(11, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // Microhouse and minimal: everything short and clicky, rims and claves tuned down into tiny
    // toms, even velocities.
    Kit {
        name: "Minimal Clicks",
        keys: [
            // very short, bright, even
            k(47, 0.0, 0.0, &[(DECAY, -0.6), (TONE, 0.4), (DYNAMICS, 0.4)]),
            // short, bright
            k(55, 0.0, 0.0, &[(DECAY, -0.5), (TONE, 0.3)]),
            // rim more than an octave down, longer: a click tom
            k(62, -6.0, 0.3, &[(TUNE, -14.0), (DECAY, 0.3)]),
            // a minor seventh down
            k(79, -6.0, 0.05, &[(TUNE, -10.0)]),
            // a fifth down
            k(30, -6.0, -0.25, &[(TUNE, -7.0)]),
            // an octave down
            k(80, -7.0, 0.4, &[(TUNE, -12.0)]),
            // a fifth down
            k(66, -7.0, -0.15, &[(TUNE, -7.0)]),
            // a fourth down, a little drive
            k(45, -7.0, -0.4, &[(TUNE, -5.0), (CHARACTER, 0.2)]),
            // shorter
            k(22, -8.0, -0.05, &[(DECAY, -0.3)]),
            // short, brighter
            k(59, -5.0, 0.05, &[(DECAY, -0.5), (TONE, 0.3)]),
            // short, bright, even
            k(
                52,
                -10.0,
                -0.2,
                &[(DECAY, -0.5), (TONE, 0.3), (DYNAMICS, 0.4)],
            )
            .choke(HATS),
            // short
            k(58, -10.0, -0.2, &[(DECAY, -0.6)]).choke(HATS),
            // short, bright
            k(56, -12.0, 0.3, &[(DECAY, -0.6), (TONE, 0.3)]),
            // a fifth down, short
            k(78, -9.0, 0.2, &[(TUNE, -7.0), (DECAY, -0.4)]),
            // a fifth up
            k(90, -9.0, -0.3, &[(TUNE, 7.0)]),
            // short
            k(83, -10.0, 0.35, &[(DECAY, -0.5)]),
        ],
        lfos: &[],
    },
    // A long, slightly saturated bridged-T kick to carry the bass, a crisp reset snare, tight hats
    // for rolls, rim and clap.
    Kit {
        name: "Trap Boom",
        keys: [
            // very long, a semitone down, slower sweep, a little grit
            k(
                1,
                0.0,
                0.0,
                &[
                    (DECAY, 0.7),
                    (TUNE, -1.0),
                    (PITCH_DECAY, 0.2),
                    (CHARACTER, 0.15),
                ],
            ),
            // up a tone, bright, Snappy well up
            k(18, 0.0, 0.0, &[(TUNE, 2.0), (NOISE, 0.5), (TONE, 0.3)]),
            // a tone down
            k(3, -6.0, 0.3, &[(TUNE, -2.0)]),
            k(5, -6.0, 0.05, &[]),
            // a semitone up
            k(7, -6.0, -0.25, &[(TUNE, 1.0)]),
            // a tone down
            k(4, -7.0, 0.4, &[(TUNE, -2.0)]),
            k(38, -7.0, -0.15, &[]),
            k(77, -7.0, -0.4, &[]),
            k(9, -8.0, -0.05, &[]),
            // a little shorter
            k(12, -5.0, 0.05, &[(DECAY, -0.2)]),
            // tight, even for rolls
            k(15, -10.0, -0.2, &[(DECAY, -0.4), (DYNAMICS, 0.4)]).choke(HATS),
            // shorter
            k(16, -10.0, -0.2, &[(DECAY, -0.2)]).choke(HATS),
            k(14, -12.0, 0.3, &[]),
            k(13, -9.0, 0.2, &[]),
            k(10, -9.0, -0.3, &[]),
            // even
            k(11, -10.0, 0.35, &[(DYNAMICS, 0.3)]),
        ],
        lfos: &[],
    },
    // Classic house: the reset kick and clap with the late-seventies box's congas, bongos,
    // cowbell, clave and a tambourine on the off-beats.
    Kit {
        name: "House Shuffle",
        keys: [
            // a little longer, more attack
            k(17, 0.0, 0.0, &[(DECAY, 0.1), (ATTACK, 0.2)]),
            // darker, a little more Snappy
            k(18, 0.0, 0.0, &[(TONE, -0.2), (NOISE, 0.2)]),
            // a tone down
            k(69, -6.0, 0.3, &[(TUNE, -2.0)]),
            k(68, -6.0, 0.05, &[]),
            k(67, -6.0, -0.25, &[]),
            // a minor third up, tighter
            k(37, -7.0, 0.4, &[(TUNE, 3.0), (DECAY, -0.2)]),
            // a minor third up, tighter
            k(38, -7.0, -0.15, &[(TUNE, 3.0), (DECAY, -0.2)]),
            // a minor third up, tighter
            k(39, -7.0, -0.4, &[(TUNE, 3.0), (DECAY, -0.2)]),
            k(40, -8.0, -0.05, &[]),
            // a bigger room
            k(23, -5.0, 0.05, &[(DECAY, 0.15)]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            // longer
            k(25, -10.0, -0.2, &[(DECAY, 0.15)]).choke(HATS),
            // the ride on the cymbal key
            k(27, -11.0, 0.3, &[]),
            k(70, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            // shorter
            k(71, -10.0, 0.35, &[(DECAY, -0.2)]),
        ],
        lfos: &[],
    },
    // Mid-eighties production: an enormous snare tail, toms with a wide reverb loop and a little
    // added sweep, a big clap room.
    Kit {
        name: "Big Snare Eighties",
        keys: [
            // a little longer, brighter
            k(32, 0.0, 0.0, &[(DECAY, 0.1), (TONE, 0.3)]),
            // twice the wires, a long wire tail, a longer body, a touch darker
            k(
                18,
                0.0,
                0.0,
                &[(NOISE, 0.7), (NOISE_DECAY, 0.6), (DECAY, 0.2), (TONE, -0.1)],
            ),
            // long, a louder and longer reverb loop, a deeper sweep
            k(
                34,
                -6.0,
                0.3,
                &[
                    (DECAY, 0.3),
                    (NOISE, 0.6),
                    (NOISE_DECAY, 0.6),
                    (PITCH_ENV, 0.3),
                ],
            ),
            // same
            k(
                35,
                -6.0,
                0.05,
                &[
                    (DECAY, 0.3),
                    (NOISE, 0.6),
                    (NOISE_DECAY, 0.6),
                    (PITCH_ENV, 0.3),
                ],
            ),
            // same
            k(
                36,
                -6.0,
                -0.25,
                &[
                    (DECAY, 0.3),
                    (NOISE, 0.6),
                    (NOISE_DECAY, 0.6),
                    (PITCH_ENV, 0.3),
                ],
            ),
            // a bigger pitch snap
            k(4, -7.0, 0.4, &[(PITCH_ENV, 0.3)]),
            // same
            k(6, -7.0, -0.15, &[(PITCH_ENV, 0.3)]),
            // same
            k(8, -7.0, -0.4, &[(PITCH_ENV, 0.3)]),
            // a little drive
            k(40, -8.0, -0.05, &[(CHARACTER, 0.3)]),
            // a big room, longer bursts
            k(23, -5.0, 0.05, &[(DECAY, 0.45), (NOISE_DECAY, 0.3)]),
            k(24, -9.0, -0.2, &[]).choke(HATS),
            k(25, -10.0, -0.2, &[]).choke(HATS),
            k(26, -12.0, 0.3, &[]),
            k(46, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            k(65, -10.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // ---- Experimental ----------------------------------------------------------------------------
    // Drums that rise into their note: on the circuits with no pitch drop of their own, Pitch
    // sweep below its centre bends upward into the note; random pitch on the small percussion.
    Kit {
        name: "Rising Sweeps",
        keys: [
            // rises from below into its note, quickly
            k(74, 0.0, 0.0, &[(PITCH_ENV, -0.6), (PITCH_DECAY, -0.3)]),
            // brighter
            k(61, 0.0, 0.0, &[(TONE, 0.2)]),
            // rises into its note, longer
            k(
                75,
                -6.0,
                0.3,
                &[(PITCH_ENV, -0.8), (PITCH_DECAY, -0.2), (DECAY, 0.3)],
            ),
            // rises
            k(68, -6.0, 0.05, &[(PITCH_ENV, -0.8)]),
            // rises
            k(67, -6.0, -0.25, &[(PITCH_ENV, -0.8)]),
            // rises, a minor third down
            k(86, -7.0, 0.4, &[(PITCH_ENV, -0.7), (TUNE, -3.0)]),
            // rises
            k(87, -7.0, -0.15, &[(PITCH_ENV, -0.7)]),
            // rises
            k(88, -7.0, -0.4, &[(PITCH_ENV, -0.7)]),
            // a random pitch each hit
            k(79, -8.0, -0.05, &[]).routes(&[(Random, to(TUNE), 0.3)]),
            k(59, -5.0, 0.05, &[]),
            k(63, -10.0, -0.2, &[]).choke(HATS),
            // held longer
            k(82, -10.0, -0.2, &[(DECAY, 0.4)]).choke(HATS),
            k(84, -12.0, 0.3, &[]),
            // random pitch, wider
            k(89, -9.0, 0.2, &[]).routes(&[(Random, to(TUNE), 0.5)]),
            // random pitch, wider
            k(90, -9.0, -0.3, &[]).routes(&[(Random, to(TUNE), 0.5)]),
            // scraped slower; a random place in the stereo field
            k(72, -10.0, 0.35, &[(TUNE, -5.0)]).routes(&[(Random, RouteTarget::Pan, 0.6)]),
        ],
        lfos: &[],
    },
    // Character at the top wherever a circuit has it: every saturator, clipper and VCA driven
    // hard.
    Kit {
        name: "Overdriven Wreck",
        keys: [
            // full drive, more body
            k(54, 0.0, 0.0, &[(CHARACTER, 1.0), (BODY, 0.5)]),
            // more wire, more body (no drive on this snare)
            k(55, 0.0, 0.0, &[(NOISE, 0.6), (BODY, 0.3)]),
            // squared off
            k(19, -6.0, 0.3, &[(CHARACTER, 1.0), (DECAY, 0.2)]),
            // squared off
            k(20, -6.0, 0.05, &[(CHARACTER, 1.0)]),
            // squared off
            k(21, -6.0, -0.25, &[(CHARACTER, 1.0)]),
            // full drive, a fifth down
            k(40, -7.0, 0.4, &[(CHARACTER, 1.0), (TUNE, -7.0)]),
            // full drive, an octave down
            k(73, -7.0, -0.15, &[(CHARACTER, 1.0), (TUNE, -12.0)]),
            // heavy drive
            k(78, -7.0, -0.4, &[(CHARACTER, 0.9)]),
            // full drive
            k(22, -8.0, -0.05, &[(CHARACTER, 1.0)]),
            // full drive
            k(44, -5.0, 0.05, &[(CHARACTER, 1.0)]),
            // full drive
            k(57, -10.0, -0.2, &[(CHARACTER, 1.0)]).choke(HATS),
            // full drive
            k(58, -10.0, -0.2, &[(CHARACTER, 1.0)]).choke(HATS),
            // driven, a fourth down
            k(27, -12.0, 0.3, &[(CHARACTER, 1.0), (TUNE, -5.0)]),
            // full drive
            k(89, -9.0, 0.2, &[(CHARACTER, 1.0)]),
            // full drive
            k(45, -9.0, -0.3, &[(CHARACTER, 1.0)]),
            // full drive
            k(71, -10.0, 0.35, &[(CHARACTER, 1.0)]),
        ],
        lfos: &[],
    },
    // A kit that moves on its own: toms bend against each other on a half-note triangle, the
    // percussion jumps around the stereo field in sixteenths, hats open and darken over two bars.
    Kit {
        name: "LFO Drift",
        keys: [
            // longer
            k(85, 0.0, 0.0, &[(DECAY, 0.3)]),
            k(33, 0.0, 0.0, &[]),
            // Tune rides LFO 1
            k(34, -6.0, 0.3, &[]).routes(&[(Lfo1, to(TUNE), 0.25)]),
            // Tune rides LFO 1, the other way
            k(35, -6.0, 0.05, &[]).routes(&[(Lfo1, to(TUNE), -0.25)]),
            // Tune on the slow sine
            k(36, -6.0, -0.25, &[]).routes(&[(Lfo2, to(TUNE), 0.15)]),
            // Pan jumps on sixteenths
            k(75, -7.0, 0.4, &[]).routes(&[(Lfo3, RouteTarget::Pan, 0.7)]),
            // Pan jumps, opposite
            k(76, -7.0, -0.15, &[]).routes(&[(Lfo3, RouteTarget::Pan, -0.7)]),
            // Tune steps on sixteenths
            k(77, -7.0, -0.4, &[]).routes(&[(Lfo3, to(TUNE), 0.1)]),
            k(40, -8.0, -0.05, &[]),
            // Tone on the slow sine
            k(44, -5.0, 0.05, &[]).routes(&[(Lfo2, to(TONE), 0.4)]),
            // Tone on the half-note triangle
            k(42, -10.0, -0.2, &[])
                .choke(HATS)
                .routes(&[(Lfo1, to(TONE), 0.5)]),
            // Decay on the slow sine
            k(43, -10.0, -0.2, &[])
                .choke(HATS)
                .routes(&[(Lfo2, to(DECAY), 0.4)]),
            // Tone against the hats
            k(41, -12.0, 0.3, &[]).routes(&[(Lfo2, to(TONE), -0.3)]),
            k(46, -9.0, 0.2, &[]),
            k(45, -9.0, -0.3, &[]),
            // a random brightness each hit
            k(65, -10.0, 0.35, &[]).routes(&[(Random, to(TONE), 0.5)]),
        ],
        lfos: &[
            synced(1, Triangle, Division::Half),
            synced(2, Sine, Division::TwoBars),
            synced(3, SampleHold, Division::Sixteenth),
        ],
    },
    // Everything slowed and lowered: very long decays an octave down, cymbal and shaker drifting
    // across the stereo field on a slow free LFO.
    Kit {
        name: "Glacial",
        keys: [
            // longest decay, a fourth down, dark
            k(1, 0.0, 0.0, &[(DECAY, 1.0), (TUNE, -5.0), (TONE, -0.4)]),
            // a fourth down, long body and wires
            k(
                2,
                0.0,
                0.0,
                &[(TUNE, -5.0), (DECAY, 0.8), (NOISE_DECAY, 0.8), (NOISE, 0.3)],
            ),
            // a fifth down, longest, deep slow sweep
            k(
                3,
                -6.0,
                0.3,
                &[
                    (TUNE, -7.0),
                    (DECAY, 0.9),
                    (PITCH_ENV, 0.5),
                    (PITCH_DECAY, 0.8),
                ],
            ),
            // a fifth down, longest
            k(5, -6.0, 0.05, &[(TUNE, -7.0), (DECAY, 0.9)]),
            // a fifth down, longest
            k(7, -6.0, -0.25, &[(TUNE, -7.0), (DECAY, 0.9)]),
            // an octave down, long
            k(37, -7.0, 0.4, &[(TUNE, -12.0), (DECAY, 0.8)]),
            // an octave down, long
            k(38, -7.0, -0.15, &[(TUNE, -12.0), (DECAY, 0.8)]),
            // an octave down, long
            k(39, -7.0, -0.4, &[(TUNE, -12.0), (DECAY, 0.8)]),
            // an octave down, long
            k(9, -8.0, -0.05, &[(TUNE, -12.0), (DECAY, 0.8)]),
            // a huge room
            k(12, -5.0, 0.05, &[(DECAY, 0.9), (NOISE_DECAY, 0.6)]),
            // an octave down, longer
            k(15, -10.0, -0.2, &[(TUNE, -12.0), (DECAY, 0.6)]).choke(HATS),
            // an octave down, longest
            k(16, -10.0, -0.2, &[(TUNE, -12.0), (DECAY, 0.9)]).choke(HATS),
            // an octave down, longest; drifts left-right
            k(14, -12.0, 0.3, &[(TUNE, -12.0), (DECAY, 1.0)]).routes(&[(
                Lfo1,
                RouteTarget::Pan,
                0.6,
            )]),
            // an octave down, long
            k(13, -9.0, 0.2, &[(TUNE, -12.0), (DECAY, 0.9)]),
            // two octaves down, long: a low bell
            k(10, -9.0, -0.3, &[(TUNE, -24.0), (DECAY, 0.9)]),
            // longest, dark; drifts right-left
            k(11, -10.0, 0.35, &[(DECAY, 1.0), (TONE, -0.5)]).routes(&[(
                Lfo1,
                RouteTarget::Pan,
                -0.6,
            )]),
        ],
        lfos: &[free(1, Sine, 0.1)],
    },
    // ---- Choke / shared source -------------------------------------------------------------------
    // One six-square metal bank playing six metal voices at zero Tune: tight, half-open and open
    // hats in one choke group, and a short cymbal that cuts the long one.
    Kit {
        name: "Hat Ladder",
        keys: [
            // a little longer
            k(17, 0.0, 0.0, &[(DECAY, 0.2)]),
            // more wire
            k(48, 0.0, 0.0, &[(NOISE, 0.3)]),
            k(49, -6.0, 0.3, &[]),
            // up a minor third
            k(49, -6.0, 0.05, &[(TUNE, 3.0)]),
            k(50, -6.0, -0.25, &[]),
            k(69, -7.0, 0.4, &[]),
            k(68, -7.0, -0.15, &[]),
            k(67, -7.0, -0.4, &[]),
            k(79, -8.0, -0.05, &[]),
            k(59, -5.0, 0.05, &[]),
            // tight
            k(15, -10.0, -0.2, &[(DECAY, -0.35)]).choke(HATS),
            k(16, -10.0, -0.2, &[]).choke(HATS),
            // the long cymbal
            k(14, -12.0, 0.3, &[]).choke(CYMBAL),
            // same bank, untouched
            k(13, -9.0, 0.2, &[]),
            // the same cymbal at its shortest, brighter: hit it to stop the long one
            k(14, -9.0, -0.3, &[(DECAY, -1.0), (TONE, 0.3)]).choke(CYMBAL),
            // half-open
            k(16, -11.0, 0.35, &[(DECAY, -0.55)]).choke(HATS),
        ],
        lfos: &[],
    },
    // Four choke groups doing four jobs: kick and three tuned kick-toms cut each other (a
    // monophonic bass line), one machine's closed hat cuts another's open hat, an open conga is
    // stopped by its muted slap, and a crash by its own short grab.
    Kit {
        name: "Cross-Cut",
        keys: [
            // long, on G1; one bass voice with the tom keys
            k(1, 0.0, 0.0, &[(DECAY, 0.55)]).choke(BASS),
            // more Snappy
            k(18, 0.0, 0.0, &[(NOISE, 0.3)]),
            // C2, long; same bass voice, centred
            k(1, -3.0, 0.0, &[(DECAY, 0.55)]).note("C2").choke(BASS),
            // D2, long; same bass voice, centred
            k(1, -3.0, 0.0, &[(DECAY, 0.5)]).note("D2").choke(BASS),
            // F2, long; same bass voice, centred
            k(1, -3.0, 0.0, &[(DECAY, 0.45)]).note("F2").choke(BASS),
            // open and longer
            k(6, -7.0, 0.25, &[(DECAY, 0.2)]).choke(CONGA),
            // the same conga muted: short slap that stops the open one
            k(6, -7.0, 0.25, &[(DECAY, -0.8)]).choke(CONGA),
            k(8, -7.0, -0.4, &[]),
            k(9, -8.0, -0.05, &[]),
            k(12, -5.0, 0.05, &[]),
            // closed hat from one machine
            k(24, -9.0, -0.2, &[]).choke(HATS),
            // open hat from another
            k(16, -10.0, -0.2, &[]).choke(HATS),
            // the crash
            k(26, -12.0, 0.3, &[]).choke(CYMBAL),
            k(13, -9.0, 0.2, &[]),
            k(10, -9.0, -0.3, &[]),
            // the crash at its shortest: a grab
            k(26, -14.0, 0.35, &[(DECAY, -1.0)]).choke(CYMBAL),
        ],
        lfos: &[],
    },
    // The reset machine's analogue voices with their noise brought forward: kick click, snare,
    // three toms and clap all read one machine noise source, beside three bridged-T toms on that
    // machine's pink-noise bus and its maraca on the white-noise bus.
    Kit {
        name: "Shared Noise Section",
        keys: [
            // noise click forward, longer
            k(17, 0.0, 0.0, &[(NOISE, 0.6), (NOISE_DECAY, 0.3)]),
            // more Snappy, longer
            k(18, 0.0, 0.0, &[(NOISE, 0.5), (NOISE_DECAY, 0.2)]),
            // noise attack forward, longer
            k(19, -6.0, 0.3, &[(NOISE, 0.6), (NOISE_DECAY, 0.4)]),
            // same
            k(20, -6.0, 0.05, &[(NOISE, 0.6), (NOISE_DECAY, 0.4)]),
            // same
            k(21, -6.0, -0.25, &[(NOISE, 0.6), (NOISE_DECAY, 0.4)]),
            // a minor third up, noise tail forward
            k(
                3,
                -7.0,
                0.4,
                &[(TUNE, 3.0), (NOISE, 0.6), (NOISE_DECAY, 0.5)],
            ),
            // a fifth up, same tail
            k(
                3,
                -7.0,
                -0.15,
                &[(TUNE, 7.0), (NOISE, 0.6), (NOISE_DECAY, 0.5)],
            ),
            // a minor seventh up, same tail
            k(
                3,
                -7.0,
                -0.4,
                &[(TUNE, 10.0), (NOISE, 0.6), (NOISE_DECAY, 0.5)],
            ),
            k(22, -8.0, -0.05, &[]),
            // longer claps, forward
            k(23, -4.0, 0.05, &[(NOISE_DECAY, 0.3)]),
            k(24, -10.0, -0.2, &[]).choke(HATS),
            k(25, -10.0, -0.2, &[]).choke(HATS),
            // a tone down
            k(26, -12.0, 0.3, &[(TUNE, -2.0)]),
            k(70, -9.0, 0.2, &[]),
            k(66, -9.0, -0.3, &[]),
            // forward
            k(11, -8.0, 0.35, &[]),
        ],
        lfos: &[],
    },
    // ---- Subtle vs extreme -----------------------------------------------------------------------
    // Subtle: six machines in one kit, each nudged a few percent from reference: a hair longer,
    // brighter, tighter. Compare with Hybrid Unhinged.
    Kit {
        name: "Gentle Hybrid",
        keys: [
            // a hair longer, a touch more attack
            k(17, 0.0, 0.0, &[(DECAY, 0.1), (ATTACK, 0.1)]),
            // a touch brighter, more Snappy
            k(2, 0.0, 0.0, &[(TONE, 0.08), (NOISE, 0.1)]),
            // half a semitone down, a hair longer
            k(34, -6.0, 0.3, &[(TUNE, -0.5), (DECAY, 0.1)]),
            // same
            k(35, -6.0, 0.05, &[(TUNE, -0.5), (DECAY, 0.1)]),
            // same
            k(36, -6.0, -0.25, &[(TUNE, -0.5), (DECAY, 0.1)]),
            // a third of a semitone up
            k(75, -7.0, 0.4, &[(TUNE, 0.3)]),
            // same
            k(76, -7.0, -0.15, &[(TUNE, 0.3)]),
            // same
            k(77, -7.0, -0.4, &[(TUNE, 0.3)]),
            // a touch cleaner
            k(9, -8.0, -0.05, &[(CHARACTER, 0.1)]),
            // a hair longer
            k(44, -5.0, 0.05, &[(DECAY, 0.1)]),
            // a hair tighter
            k(57, -10.0, -0.2, &[(DECAY, -0.1)]).choke(HATS),
            // a hair longer
            k(58, -10.0, -0.2, &[(DECAY, 0.1)]).choke(HATS),
            // a touch darker
            k(41, -12.0, 0.3, &[(TONE, -0.1)]),
            // a hair longer
            k(70, -9.0, 0.2, &[(DECAY, 0.1)]),
            // a hair longer
            k(66, -9.0, -0.3, &[(DECAY, 0.05)]),
            // a touch brighter
            k(93, -10.0, 0.35, &[(TONE, 0.1)]),
        ],
        lfos: &[],
    },
    // Extreme: the Gentle Hybrid circuits pushed to the ends of their travel: octaves, the longest
    // and shortest decays, inverted sweeps, full drive.
    Kit {
        name: "Hybrid Unhinged",
        keys: [
            // longest, maximum click, deep slow sweep, more body
            k(
                17,
                0.0,
                0.0,
                &[
                    (DECAY, 0.9),
                    (ATTACK, 1.0),
                    (PITCH_DECAY, 0.8),
                    (PITCH_ENV, 0.8),
                    (BODY, 0.6),
                ],
            ),
            // an octave up, brightest, loudest and longest wires
            k(
                2,
                0.0,
                0.0,
                &[(TUNE, 12.0), (TONE, 1.0), (NOISE, 1.0), (NOISE_DECAY, 1.0)],
            ),
            // an octave down, everything long and deep
            k(
                34,
                -6.0,
                0.3,
                &[
                    (TUNE, -12.0),
                    (DECAY, 0.9),
                    (PITCH_ENV, 1.0),
                    (PITCH_DECAY, 1.0),
                    (NOISE, 1.0),
                    (NOISE_DECAY, 1.0),
                ],
            ),
            // a fifth down, deep sweep, long
            k(
                35,
                -6.0,
                0.05,
                &[
                    (TUNE, -7.0),
                    (DECAY, 0.9),
                    (PITCH_ENV, 1.0),
                    (PITCH_DECAY, 0.8),
                ],
            ),
            // an octave up, shortest
            k(36, -6.0, -0.25, &[(TUNE, 12.0), (DECAY, -0.9)]),
            // two octaves down, long, sweeping down from far above
            k(
                75,
                -7.0,
                0.4,
                &[
                    (TUNE, -24.0),
                    (DECAY, 0.8),
                    (PITCH_ENV, 1.0),
                    (PITCH_DECAY, 0.5),
                ],
            ),
            // an octave and a fifth up, shortest
            k(76, -7.0, -0.15, &[(TUNE, 19.0), (DECAY, -1.0)]),
            // two octaves up, rising into its note
            k(77, -7.0, -0.4, &[(TUNE, 24.0), (PITCH_ENV, -1.0)]),
            // an octave down, hardest clip
            k(9, -8.0, -0.05, &[(TUNE, -12.0), (CHARACTER, -1.0)]),
            // full drive, longest
            k(
                44,
                -5.0,
                0.05,
                &[(CHARACTER, 1.0), (DECAY, 1.0), (NOISE_DECAY, 1.0)],
            ),
            // an octave down, full drive, most noise
            k(
                57,
                -10.0,
                -0.2,
                &[(TUNE, -12.0), (CHARACTER, 1.0), (NOISE, 1.0)],
            )
            .choke(HATS),
            // an octave up, longest, full drive
            k(
                58,
                -10.0,
                -0.2,
                &[(TUNE, 12.0), (DECAY, 1.0), (CHARACTER, 1.0)],
            )
            .choke(HATS),
            // two octaves down, longest, full drive
            k(
                41,
                -12.0,
                0.3,
                &[(TUNE, -24.0), (DECAY, 1.0), (CHARACTER, 1.0)],
            ),
            // an octave down, longest, full drive
            k(
                70,
                -9.0,
                0.2,
                &[(TUNE, -12.0), (DECAY, 1.0), (CHARACTER, 1.0)],
            ),
            // two octaves down, longest
            k(66, -9.0, -0.3, &[(TUNE, -24.0), (DECAY, 1.0)]),
            // longest, darkest
            k(93, -10.0, 0.35, &[(DECAY, 1.0), (TONE, -1.0)]),
        ],
        lfos: &[],
    },
    // Subtle: the organ-top boxes cleaned up a few percent: snare wires a touch longer, drums a
    // hair tighter, the bell-like voices a touch brighter. Compare with Rhythm Box Meltdown.
    Kit {
        name: "Rhythm Box Polish",
        keys: [
            // a hair tighter
            k(85, 0.0, 0.0, &[(DECAY, -0.1)]),
            // wires a touch longer
            k(61, 0.0, 0.0, &[(NOISE_DECAY, 0.1), (NOISE, 0.05)]),
            // a hair tighter
            k(86, -6.0, 0.3, &[(DECAY, -0.08)]),
            // same
            k(87, -6.0, 0.05, &[(DECAY, -0.08)]),
            // same
            k(88, -6.0, -0.25, &[(DECAY, -0.08)]),
            // a quarter-tone down
            k(69, -7.0, 0.4, &[(TUNE, -0.5)]),
            // a touch brighter
            k(68, -7.0, -0.15, &[(TONE, 0.1)]),
            // a touch brighter
            k(67, -7.0, -0.4, &[(TONE, 0.1)]),
            // a touch fuller
            k(79, -8.0, -0.05, &[(BODY, 0.1)]),
            // a hair longer
            k(94, -5.0, 0.05, &[(DECAY, 0.1)]),
            // a touch brighter
            k(63, -10.0, -0.2, &[(TONE, 0.1)]).choke(HATS),
            // a hair longer
            k(82, -10.0, -0.2, &[(DECAY, 0.15)]).choke(HATS),
            // a touch darker
            k(84, -12.0, 0.3, &[(TONE, -0.1)]),
            // a touch brighter
            k(89, -9.0, 0.2, &[(TONE, 0.1)]),
            // a hair longer
            k(90, -9.0, -0.3, &[(DECAY, 0.1)]),
            // a hair shorter
            k(71, -10.0, 0.35, &[(DECAY, -0.1)]),
        ],
        lfos: &[],
    },
    // Extreme: the same organ-top circuits melted: drums rising from two octaves below, bongos
    // squeaking two octaves up, cymbals stretched to the end of their decay.
    Kit {
        name: "Rhythm Box Meltdown",
        keys: [
            // an octave down, longest, a deep, slow drop
            k(
                85,
                0.0,
                0.0,
                &[
                    (TUNE, -12.0),
                    (DECAY, 1.0),
                    (PITCH_ENV, 1.0),
                    (PITCH_DECAY, 0.7),
                ],
            ),
            // an octave up, more and far longer wires
            k(
                61,
                0.0,
                0.0,
                &[(TUNE, 12.0), (NOISE, 1.0), (NOISE_DECAY, 1.0)],
            ),
            // rises from below, longest
            k(86, -6.0, 0.3, &[(PITCH_ENV, -1.0), (DECAY, 0.9)]),
            // an octave down, rises
            k(87, -6.0, 0.05, &[(TUNE, -12.0), (PITCH_ENV, -1.0)]),
            // two octaves up, shortest
            k(88, -6.0, -0.25, &[(TUNE, 24.0), (DECAY, -1.0)]),
            // an octave down, longest, deep fall
            k(
                69,
                -7.0,
                0.4,
                &[
                    (TUNE, -12.0),
                    (DECAY, 1.0),
                    (PITCH_ENV, 0.9),
                    (PITCH_DECAY, 0.9),
                ],
            ),
            // two octaves up
            k(68, -7.0, -0.15, &[(TUNE, 24.0)]),
            // an octave and a fifth up, shortest
            k(67, -7.0, -0.4, &[(TUNE, 19.0), (DECAY, -1.0)]),
            // two octaves down, longest
            k(79, -8.0, -0.05, &[(TUNE, -24.0), (DECAY, 1.0)]),
            // longest, full drive
            k(
                94,
                -5.0,
                0.05,
                &[(DECAY, 1.0), (NOISE_DECAY, 1.0), (CHARACTER, 1.0)],
            ),
            // longest, full drive
            k(63, -10.0, -0.2, &[(DECAY, 1.0), (CHARACTER, 1.0)]).choke(HATS),
            // shortest, brightest
            k(82, -10.0, -0.2, &[(DECAY, -1.0), (TONE, 1.0)]).choke(HATS),
            // longest, darkest
            k(84, -12.0, 0.3, &[(DECAY, 1.0), (TONE, -1.0)]),
            // two octaves down, longest, full drive
            k(
                89,
                -9.0,
                0.2,
                &[(TUNE, -24.0), (DECAY, 1.0), (CHARACTER, 1.0)],
            ),
            // two octaves up
            k(90, -9.0, -0.3, &[(TUNE, 24.0)]),
            // longest, darkest, full drive
            k(
                71,
                -10.0,
                0.35,
                &[(DECAY, 1.0), (TONE, -1.0), (CHARACTER, 1.0)],
            ),
        ],
        lfos: &[],
    },
];

/// A kit's file name: its name in lower case, each run of anything else one hyphen.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The equal-tempered frequency of `note` (`"C2"`, `"F#3"`, `"Bb4"`), A4 = 440 Hz.
fn note_hz(note: &str) -> f64 {
    let (name, octave) = note.split_at(note.len() - 1);
    let semitone = match name {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        _ => panic!("{note:?} is not a note"),
    };
    let octave: i32 = octave.parse().expect("a one-digit octave");
    440.0 * 2.0_f64.powf(f64::from(12 * (octave + 1) + semitone - 69) / 12.0)
}

/// The Tune, to a hundredth of a semitone, that puts `model`'s measured rest pitch on `note`.
fn tune_to(model: u8, note: &str) -> f32 {
    let rest = ModelId::new(model)
        .reference_pitch_hz()
        .unwrap_or_else(|| panic!("model {model} has no note to tune"));
    let semitones = 12.0 * (note_hz(note) / f64::from(rest)).log2();
    ((semitones * 100.0).round() / 100.0) as f32
}

/// A key's controls in plain values, its note resolved to a Tune.
fn plain_controls(key: &Key) -> Vec<(usize, f32)> {
    let mut controls = key.controls.to_vec();
    if let Some(note) = key.note {
        controls.push((TUNE, tune_to(key.model, note)));
    }
    controls
}

/// **`kit` laid over Init, as its file holds it**: every parameter a kit stores, the kit's values in
/// place of Init's, each `text` the parameter's own reading. Unused routes stay out, as capture
/// leaves them (`Instrument::omit_captured_parameter`).
fn generated(params: &MxmDrumMachineParams, kit: &Kit) -> Preset {
    let mut preset = Preset::capture("", Category::Percussion, params);
    preset.name = kit.name.to_owned();
    preset.category = Category::Percussion;
    let mut put = |id: &str, param: &dyn mxm_preset::ErasedParam, v: f32| {
        preset.params.insert(
            id.to_owned(),
            Value {
                v,
                text: param.format(v),
            },
        );
    };
    for (slot, key) in kit.keys.iter().enumerate() {
        let (p, ids) = (&params.slots[slot], slot_ids(slot));
        put(
            ids[at::MODEL],
            &p.model,
            p.model.preview_normalized(i32::from(key.model)),
        );
        put(
            ids[at::LEVEL],
            &p.level,
            p.level.preview_normalized(util::db_to_gain(key.db)),
        );
        put(ids[at::PAN], &p.pan, p.pan.preview_normalized(key.pan));
        put(ids[at::MUTE], &p.mute, p.mute.preview_normalized(key.mute));
        put(
            ids[at::CHOKE_GROUP],
            &p.choke_group,
            p.choke_group.preview_normalized(i32::from(key.choke)),
        );
        for (k, value) in plain_controls(key) {
            let control = p.controls.get(k);
            put(
                ids[at::control(k)],
                control,
                control.preview_normalized(value),
            );
        }
        for (r, &(source, target, amount)) in key.routes.iter().enumerate() {
            let route = p.routes.all()[r];
            put(
                ids[at::source(r)],
                &route.source,
                route.source.preview_normalized(source),
            );
            put(
                ids[at::target(r)],
                &route.target,
                route.target.preview_normalized(target.index() as i32),
            );
            put(
                ids[at::amount(r)],
                &route.amount,
                route.amount.preview_normalized(amount),
            );
        }
    }
    for lfo in kit.lfos {
        let (rate, shape, sync) = match lfo.lfo {
            1 => (&params.lfo1_rate, &params.lfo1_shape, &params.lfo1_sync),
            2 => (&params.lfo2_rate, &params.lfo2_shape, &params.lfo2_sync),
            3 => (&params.lfo3_rate, &params.lfo3_shape, &params.lfo3_sync),
            n => panic!("there is no LFO {n}"),
        };
        // A synced Rate stores the position on the ladder that picks its division.
        let (position, synced) = match lfo.rate {
            Rate::Free(hz) => (rate.preview_normalized(hz), false),
            Rate::Synced(division) => (LFO_SYNC.position(division), true),
        };
        let n = lfo.lfo;
        put(&format!("lfo{n}_rate"), rate, position);
        put(
            &format!("lfo{n}_shape"),
            shape,
            shape.preview_normalized(lfo.shape),
        );
        put(
            &format!("lfo{n}_sync"),
            sync,
            sync.preview_normalized(synced),
        );
    }
    preset
}

/// Writes the fifty files beside the audition kits. The only thing that writes them: the design
/// above is the readable statement of each kit, and the JSON is its output.
///
/// `cargo test -p mxm-drum-machine --lib write_the_creative_kits -- --ignored --nocapture`
#[test]
#[ignore = "writes the creative kits' preset files"]
fn write_the_creative_kits() {
    let params = MxmDrumMachineParams::default();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
    for kit in &KITS {
        let path = dir.join(format!("{}.json", slug(kit.name)));
        std::fs::write(&path, generated(&params, kit).to_json()).expect("write a kit");
        eprintln!("wrote {}", path.display());
    }
}

/// **The files are their design**, built in memory and compared, changing nothing in the checkout.
/// The generator is ignored, so nothing else says a file has fallen behind the table above — or
/// behind a retuned default, a renamed reading or a re-measured rest pitch.
#[test]
fn the_shipped_kits_are_their_designs() {
    let params = MxmDrumMachineParams::default();
    let shipped = &FACTORY_FILES[AUDITION_KITS..];
    assert_eq!(
        shipped.len(),
        KITS.len(),
        "every kit is listed after the nine"
    );
    for (&(name, text), kit) in shipped.iter().zip(&KITS) {
        assert_eq!(name, kit.name, "the list follows the design's order");
        let preset = Preset::parse(text, crate::CLAP_ID).expect("a kit parses");
        assert!(
            preset == generated(&params, kit),
            "{name} is not its design: run write_the_creative_kits"
        );
    }
}

/// Every file in `presets/` is a factory kit the plugin compiles in, and every kit has its file:
/// a stale or misnamed file cannot sit there unshipped.
#[test]
fn every_preset_file_is_a_listed_kit() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .expect("the presets folder")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    let mut listed: Vec<String> = FACTORY_FILES.iter().map(|(name, _)| slug(name)).collect();
    files.sort();
    listed.sort();
    assert_eq!(files, listed);
}

/// **A shipped kit never leans on a default** (plan §7.2: a default retune must not collapse a
/// kit's sparse overrides). The design is sparse; the file is not: it holds every parameter a kit
/// stores — all but the instance settings and the routes it does not use — so a later change to a
/// default cannot reach a kit already shipped, and the comparison above fails until the kit is
/// regenerated on purpose. [`every_setting_a_kit_makes_is_heard`] is the audible half.
#[test]
fn a_shipped_kit_never_leans_on_a_default() {
    use mxm_preset::Instrument;
    let params = MxmDrumMachineParams::default();
    let stored: Vec<&str> = params
        .parameters()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !params.is_instance_setting(id) && crate::params::route_of(id).is_none())
        .collect();
    for &(name, text) in &FACTORY_FILES[AUDITION_KITS..] {
        let preset = Preset::parse(text, crate::CLAP_ID).expect("a kit parses");
        for id in &stored {
            assert!(
                preset.params.contains_key(*id),
                "{name} leaves {id} to its default"
            );
        }
    }
}

/// **A kit moves only what its models read**, by the panel's own table: no control a model hides,
/// none that is only a gain (Level does that), and none that waits on another — *Only while Pitch
/// sweep is off its centre* — left where it waits for nothing. Routes aim only at what the model
/// reads; every value is in its range.
#[test]
fn every_key_moves_only_what_its_model_reads() {
    let mut problems = Vec::new();
    for kit in &KITS {
        for (slot, key) in kit.keys.iter().enumerate() {
            let model = ModelId::new(key.model);
            let here = format!("{} key {} (model {})", kit.name, 36 + slot, key.model);
            if available(model).is_none() {
                problems.push(format!("{here}: not an available model"));
                continue;
            }
            let controls = plain_controls(key);
            let value = |k: usize| {
                controls
                    .iter()
                    .find(|(c, _)| *c == k)
                    .map_or(0.0, |(_, v)| *v)
            };
            for &(k, v) in key.controls {
                if v == 0.0 {
                    problems.push(format!("{here}: control {k} written at zero"));
                }
            }
            for &(k, v) in &controls {
                let reach = if k == TUNE { 24.0 } else { 1.0 };
                if !(-reach..=reach).contains(&v) {
                    problems.push(format!("{here}: control {k} at {v} is out of range"));
                }
                let Some(face) = controls::face(model, k) else {
                    problems.push(format!("{here}: its model does not read control {k}"));
                    continue;
                };
                if face.name == "Gain" {
                    problems.push(format!("{here}: control {k} is only a gain here"));
                }
                if let Some(rest) = face.help.split("Only while ").nth(1) {
                    let (gate, condition) = rest.split_once(" is ").expect("a waited-on control");
                    let g = (1..=USED)
                        .find(|&j| controls::face(model, j).is_some_and(|f| f.name == gate))
                        .unwrap_or_else(|| panic!("{here}: {gate} is not one of its controls"));
                    let open = if condition.starts_with("above its bottom") {
                        value(g) > -1.0
                    } else if condition.starts_with("off its centre") {
                        value(g) != 0.0
                    } else {
                        panic!("{here}: an unread condition, {condition:?}")
                    };
                    if !open {
                        problems.push(format!("{here}: {} waits on {gate}", face.name));
                    }
                }
            }
            if key.routes.len() > ROUTES {
                problems.push(format!("{here}: more than {ROUTES} routes"));
            }
            for &(_, target, amount) in key.routes {
                if let RouteTarget::Control(k) = target
                    && controls::face(model, usize::from(k)).is_none()
                {
                    problems.push(format!("{here}: a route to control {k}, which it hides"));
                }
                if amount == 0.0 || !(-1.0..=1.0).contains(&amount) {
                    problems.push(format!("{here}: a route amount of {amount}"));
                }
            }
            if !(-60.0..=6.0).contains(&key.db) || !(-1.0..=1.0).contains(&key.pan) {
                problems.push(format!("{here}: level or pan out of range"));
            }
            if key.choke > 16 {
                problems.push(format!("{here}: no choke group {}", key.choke));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The bank uses the whole pool: every available model sounds in some kit.
#[test]
fn every_model_sounds_in_some_kit() {
    for spec in &AVAILABLE_MODELS {
        assert!(
            KITS.iter().any(|kit| kit
                .keys
                .iter()
                .any(|key| !key.mute && key.model == spec.id.raw())),
            "{} sounds in no kit",
            spec.label
        );
    }
}

/// **The owner's ruling, 2026-10-07: the fifty choke as a real kit would.** Every kit's closed hat
/// (key 46) cuts its open hat (key 47). The nine audition kits stay ungrouped
/// (`super::tests::the_audition_kits_ship_ungrouped`).
#[test]
fn every_kit_chokes_its_open_hat_with_its_closed_hat() {
    for kit in &KITS {
        let (closed, open) = (kit.keys[10], kit.keys[11]);
        assert!(
            closed.choke != 0 && closed.choke == open.choke,
            "{}: closed and open hat in no common group",
            kit.name
        );
    }
}

/// Factory names are original words (the collection's naming rule): no maker or machine names, no
/// machine numbers — the audition kits keep theirs by the owner's request — and each its own.
#[test]
fn the_names_are_original_and_their_own() {
    const FORBIDDEN: [&str; 12] = [
        "roland",
        "korg",
        "boss",
        "maestro",
        "rhythm ace",
        "rhythm composer",
        "compurhythm",
        "ace tone",
        "tr-",
        "cr-",
        "dr-",
        "fr-",
    ];
    let audition: Vec<&str> = FACTORY_FILES[..AUDITION_KITS]
        .iter()
        .map(|(name, _)| *name)
        .collect();
    for (i, kit) in KITS.iter().enumerate() {
        let lower = kit.name.to_ascii_lowercase();
        assert!(
            !lower.chars().any(|c| c.is_ascii_digit()),
            "{}: a number",
            kit.name
        );
        for word in FORBIDDEN {
            assert!(!lower.contains(word), "{}: {word:?}", kit.name);
        }
        assert!(!audition.contains(&kit.name), "{}", kit.name);
        assert!(
            KITS[..i]
                .iter()
                .all(|other| slug(other.name) != slug(kit.name)),
            "{} twice",
            kit.name
        );
    }
}

// ---- The bank, rendered ----------------------------------------------------------------------

/// The bank's render: one hit of every key at once, at velocity 0.8, from a freshly activated
/// plugin, each key on an output of its own, at a fixed tempo (synced LFOs and the tempo-coupled
/// open hat follow it). `CALLS` blocks of `FRAMES` at 48 kHz, 0.21 s: every attack and the start of
/// every tail, and no more — a long-ringing kit is cut there, which is what keeps the bank cheap.
const CALLS: usize = 10;
const TEMPO: f64 = 120.0;
/// A key's print, in four windows (0–10, 10–40, 40–100 and 100–208 ms): its energy in three bands
/// (below 200 Hz, 200 Hz–2 kHz, above) in dB against the key's own energy, floored at −60 dB, and
/// its RMS frequency in dB (`20 log10` of hertz: two semitones read 1 dB). Against its own energy,
/// so a print is the sound and not the mix: a level alone moves none of it.
const WINDOWS: [usize; 5] = [0, 480, 1_920, 4_800, CALLS * crate::full_layout::FRAMES];
const BANDS: usize = 3;
const PER_WINDOW: usize = BANDS + 1;
const CELLS: usize = (WINDOWS.len() - 1) * PER_WINDOW;
const FLOOR_DB: f32 = -60.0;

/// What one kit sounds like.
struct Print {
    name: String,
    /// Each key's peak, dBFS.
    peak: [f32; SLOT_COUNT],
    /// Each key's cells; a silent key's are all at the floor.
    cells: [[f32; CELLS]; SLOT_COUNT],
}

/// Writes `preset` into `params` as applying it does: every value it resolves to.
fn apply(params: &MxmDrumMachineParams, preset: &Preset) {
    let (writes, problems) = preset.resolve(params);
    assert!(problems.is_empty(), "{}: {problems:?}", preset.name);
    let map = params.param_map();
    for (id, _, value) in writes {
        let (_, ptr, _) = map.iter().find(|(m, _, _)| m == id).expect("a parameter");
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { ptr._internal_set_normalized_value(value) };
    }
}

/// Each key's output, the bank's render of `preset`.
fn render(preset: &Preset) -> Vec<Vec<f32>> {
    use crate::full_layout::{activated, port_for, render_at};
    let mut plugin = activated(0, |params| apply(params, preset));
    let notes: Vec<u8> = (36..36 + SLOT_COUNT as u8).collect();
    let mut keys = vec![Vec::new(); SLOT_COUNT];
    for call in 0..CALLS {
        let struck: &[u8] = if call == 0 { &notes } else { &[] };
        let rendered = render_at(&mut plugin, struck, Some(TEMPO));
        for (slot, key) in keys.iter_mut().enumerate() {
            key.extend_from_slice(&rendered.individual[port_for(slot)]);
        }
    }
    keys
}

fn fingerprint(name: &str, keys: &[Vec<f32>]) -> Print {
    use crate::full_layout::SAMPLE_RATE;
    let pole = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz / SAMPLE_RATE).exp();
    let (low_pole, high_pole) = (pole(200.0), pole(2_000.0));
    let mut peak = [0.0; SLOT_COUNT];
    let mut cells = [[FLOOR_DB; CELLS]; SLOT_COUNT];
    for (slot, samples) in keys.iter().enumerate() {
        peak[slot] = 20.0
            * samples
                .iter()
                .fold(1e-30_f32, |m, s| m.max(s.abs()))
                .log10();
        let total: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        if total == 0.0 {
            continue;
        }
        let (mut low, mut below, mut last) = (0.0_f32, 0.0_f32, 0.0_f32);
        // Per window: the three bands' energy, the whole key's, then its first difference's.
        let mut energy = [[0.0_f64; PER_WINDOW + 1]; WINDOWS.len() - 1];
        for (i, &x) in samples.iter().enumerate() {
            low += low_pole * (x - low);
            below += high_pole * (x - below);
            let w = WINDOWS.windows(2).position(|w| i < w[1]).expect("a window");
            for (b, band) in [low, below - low, x - below, x, x - last]
                .into_iter()
                .enumerate()
            {
                energy[w][b] += f64::from(band) * f64::from(band);
            }
            last = x;
        }
        for (w, e) in energy.iter().enumerate() {
            let cell = &mut cells[slot][w * PER_WINDOW..(w + 1) * PER_WINDOW];
            for b in 0..BANDS {
                cell[b] = (10.0 * (e[b] / total).max(1e-30).log10() as f32).max(FLOOR_DB);
            }
            // A sinusoid at f has a first difference 2 sin(πf/fs) its own size; a window the key
            // has all but left reads no frequency.
            cell[BANDS] = if 10.0 * (e[BANDS] / total).log10() > f64::from(FLOOR_DB) {
                let ratio = ((e[BANDS + 1] / e[BANDS]).sqrt() / 2.0).min(1.0);
                let hz = f64::from(SAMPLE_RATE) / std::f64::consts::PI * ratio.asin();
                20.0 * hz.max(1.0).log10() as f32
            } else {
                0.0
            };
        }
    }
    Print {
        name: name.to_owned(),
        peak,
        cells,
    }
}

/// How far apart two keys sound: the mean of their cells' differences, dB.
fn key_distance(a: &[f32; CELLS], b: &[f32; CELLS]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / CELLS as f32
}

/// How far apart two kits sound: their keys' distances, averaged.
fn distance(a: &Print, b: &Print) -> f32 {
    a.cells
        .iter()
        .zip(&b.cells)
        .map(|(x, y)| key_distance(x, y))
        .sum::<f32>()
        / SLOT_COUNT as f32
}

/// `kit` with no settings of its own: every key's controls and routes at Init and the LFOs too;
/// its models, levels, pans, mutes and choke groups as they are.
fn unset(kit: &Kit) -> Kit {
    Kit {
        name: kit.name,
        keys: kit.keys.map(|key| Key {
            controls: &[],
            note: None,
            routes: &[],
            ..key
        }),
        lfos: &[],
    }
}

/// Each key's largest sample difference between two renders, dB against the first one's peak.
fn moved(kit: &[Vec<f32>], unset: &[Vec<f32>]) -> [f32; SLOT_COUNT] {
    std::array::from_fn(|slot| {
        let (a, b) = (&kit[slot], &unset[slot]);
        let peak = a.iter().fold(1e-30_f32, |m, s| m.max(s.abs()));
        let most = a
            .iter()
            .zip(b)
            .fold(1e-30_f32, |m, (x, y)| m.max((x - y).abs()));
        20.0 * (most / peak).log10()
    })
}

/// Every render the checks below read, made once for all of them and spread over the machine's
/// threads: Init, the nine, the fifty — each also unset — and Init again.
struct Bank {
    /// Init, the audition kits, then the creative kits, in the browser's order.
    kits: Vec<Print>,
    /// For each creative kit, how far each key moves from itself unset ([`moved`]).
    moved: Vec<[f32; SLOT_COUNT]>,
    init_again: Print,
}

fn bank() -> &'static Bank {
    static BANK: std::sync::OnceLock<Bank> = std::sync::OnceLock::new();
    BANK.get_or_init(|| {
        let params = MxmDrumMachineParams::default();
        let mut presets = mxm_preset::factory(&params);
        assert_eq!(presets.len(), 1 + AUDITION_KITS + KITS.len());
        presets.push(presets[0].clone());
        // A job is a preset, and for a creative kit its unset twin too.
        let jobs: Vec<(Preset, Option<Preset>)> = presets
            .into_iter()
            .enumerate()
            .map(|(i, preset)| {
                let twin = i
                    .checked_sub(1 + AUDITION_KITS)
                    .and_then(|k| KITS.get(k))
                    .map(|kit| generated(&params, &unset(kit)));
                (preset, twin)
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let chunk = jobs.len().div_ceil(threads);
        let mut done: Vec<(Print, Option<[f32; SLOT_COUNT]>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = jobs
                .chunks(chunk)
                .map(|chunk| {
                    scope.spawn(move || {
                        let job = |(preset, twin): &(Preset, Option<Preset>)| {
                            let keys = render(preset);
                            let twin = twin.as_ref().map(|twin| moved(&keys, &render(twin)));
                            (fingerprint(&preset.name, &keys), twin)
                        };
                        chunk.iter().map(job).collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().expect("a render"))
                .collect()
        });
        let init_again = done.pop().expect("Init again").0;
        let moved = done.iter().filter_map(|(_, moved)| *moved).collect();
        Bank {
            kits: done.into_iter().map(|(print, _)| print).collect(),
            moved,
            init_again,
        }
    })
}

/// Two kits closer than this, in [`distance`], sound alike: one key of sixteen moved by 3.2 dB of
/// its print, and no more. About half the bank's closest pair as `print_the_kit_distances` measured
/// it (NOTES.md records which); two kits that differ only in level or pan read zero.
const ALIKE_DB: f32 = 0.2;
/// A setting that moves its key's output by less than this, against the key's peak, is not heard.
const UNHEARD_DB: f32 = -60.0;
/// The quietest a sounding key may peak in the render, dBFS.
const SOUNDING_DBFS: f32 = -40.0;

/// **Every kit sounds, and sounds like itself.** Every key a kit leaves unmuted sounds and every
/// muted one is silent; no two kits of the bank — Init, the nine and the fifty — are closer than
/// [`ALIKE_DB`]; and the render is a function of the kit alone (Init twice is one print).
#[test]
fn every_kit_sounds_and_no_two_sound_alike() {
    let bank = bank();
    let params = MxmDrumMachineParams::default();
    let presets = mxm_preset::factory(&params);
    for (preset, print) in presets.iter().zip(&bank.kits) {
        for (slot, &peak) in print.peak.iter().enumerate() {
            let muted = preset
                .params
                .get(slot_ids(slot)[at::MUTE])
                .is_some_and(|value| value.v > 0.5);
            if muted {
                assert!(
                    peak < -300.0,
                    "{} key {} is muted but sounds",
                    print.name,
                    36 + slot
                );
            } else {
                assert!(
                    peak > SOUNDING_DBFS,
                    "{} key {} peaks at {peak:.1} dBFS",
                    print.name,
                    36 + slot
                );
            }
        }
    }
    assert_eq!(
        bank.init_again.cells, bank.kits[0].cells,
        "a fresh engine renders alike"
    );
    let mut alike = Vec::new();
    for (i, a) in bank.kits.iter().enumerate() {
        for b in &bank.kits[i + 1..] {
            let d = distance(a, b);
            if d < ALIKE_DB {
                alike.push(format!("{} and {}: {d:.2} dB", a.name, b.name));
            }
        }
    }
    assert!(
        alike.is_empty(),
        "kits that sound alike:\n{}",
        alike.join("\n")
    );
}

/// **Every setting a kit makes is heard** (plan §7.2: a default retune must not collapse a kit's
/// sparse overrides). Each key that moves a control sounds apart from the same key with its
/// controls at Init — same model, level and pan — by more than [`UNHEARD_DB`]. A setting that had
/// collapsed into its default, dropped by a generator or met by a retuned default, would not.
/// [`a_shipped_kit_never_leans_on_a_default`] is the other half.
#[test]
fn every_setting_a_kit_makes_is_heard() {
    let bank = bank();
    let mut unheard = Vec::new();
    for (kit, moved) in KITS.iter().zip(&bank.moved) {
        for (slot, key) in kit.keys.iter().enumerate() {
            let sets = !key.controls.is_empty() || key.note.is_some();
            if sets && !key.mute && moved[slot] < UNHEARD_DB {
                unheard.push(format!(
                    "{} key {}: {:.1} dB",
                    kit.name,
                    36 + slot,
                    moved[slot]
                ));
            }
        }
    }
    assert!(
        unheard.is_empty(),
        "settings nobody hears:\n{}",
        unheard.join("\n")
    );
}

/// Prints what [`ALIKE_DB`] and [`UNHEARD_DB`] were chosen from: the closest pairs, key by key,
/// the faintest settings, and the quietest sounding key.
///
/// `cargo test -p mxm-drum-machine --lib print_the_kit_distances -- --ignored --nocapture`
#[test]
#[ignore = "a measurement tool, not a check"]
fn print_the_kit_distances() {
    let bank = bank();
    let mut pairs = Vec::new();
    for (i, a) in bank.kits.iter().enumerate() {
        for b in &bank.kits[i + 1..] {
            pairs.push((distance(a, b), a, b));
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (d, a, b) in pairs.iter().take(12) {
        let keys: Vec<String> = a
            .cells
            .iter()
            .zip(&b.cells)
            .map(|(x, y)| format!("{:.1}", key_distance(x, y)))
            .collect();
        println!("{d:6.2} dB  {} / {}: {}", a.name, b.name, keys.join(" "));
    }
    let mut faint = Vec::new();
    for (kit, moved) in KITS.iter().zip(&bank.moved) {
        for (slot, key) in kit.keys.iter().enumerate() {
            if (!key.controls.is_empty() || key.note.is_some()) && !key.mute {
                faint.push((moved[slot], kit.name, 36 + slot));
            }
        }
    }
    faint.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (d, name, key) in faint.iter().take(8) {
        println!("{d:6.1} dB  {name} key {key}: its settings against Init's");
    }
    let mut quietest = (0.0_f32, "");
    for print in &bank.kits {
        for &peak in print.peak.iter().filter(|p| **p > -300.0) {
            if peak < quietest.0 {
                quietest = (peak, print.name.as_str());
            }
        }
    }
    println!(
        "{:6.1} dBFS  the quietest sounding key ({})",
        quietest.0, quietest.1
    );
}
