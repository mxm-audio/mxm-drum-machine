//! **Each model's controls: which of the general controls its panel shows, and under what name**
//! (the owner, 2026-10-07: honest controls — only the controls that do something for that model,
//! under the model's honest name).
//!
//! **Decided from the code, not from renders** (the owner, 2026-10-07: *"Can't you read the code
//! anymore? One parameter might not do anything depending on the setting of another parameter."*).
//! A model shows a control when its circuit reads it at all, in any setting of the others; one it
//! never reads, or reads only to discard, has no knob. That is exactly what the DSP declares in
//! `ModelId::capabilities` (its families read nothing else, and its
//! `every_declared_unsupported_axis_is_an_exact_dsp_no_op` holds the other half), so the table is
//! held to it (`each_model_shows_exactly_the_controls_its_code_reads`). Where a control only acts
//! while another is set, its help says so: *Only while Snappy is above its bottom.*
//!
//! Model-drums' common seven keep their places — 1 Tune, 2 Decay, 3 Tone, 4 Attack, 5 the velocity
//! response, 6 the pitch drop, 7 its time — and their names wherever those are honest. A name
//! changes only where the general one would send the player to the wrong part of the sound or the
//! wrong way: *Soft hits* for Dynamics everywhere (its top brings soft hits up, the opposite of
//! "more dynamics"), *Gain* for a control that only changes the level, *Snappy decay* beside
//! *Snappy*. The reasons, model by model, are in the plugin's `NOTES.md`.

use mxm_drum_machine_dsp::model::ModelId;

use crate::params::{CONTROLS, control};

/// One general control as a model's panel shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    /// What its knob and its route rows are called.
    pub name: &'static str,
    /// What it does to this drum, on hover, in the player's words — and what it waits on, if it
    /// only acts while another control is set.
    pub help: &'static str,
}

/// A model's controls 1–11, by number from 1: `None` where the model's code never reads it.
pub type Faces = [Option<Face>; control::USED];

const fn on(name: &'static str, help: &'static str) -> Option<Face> {
    Some(Face { name, help })
}

/// What a control is called where no model shows it: an unavailable model's disabled knobs, and a
/// route still aimed at a control its slot's model does not read.
const GENERAL: [&str; CONTROLS] = [
    "Tune",
    "Decay",
    "Tone",
    "Attack",
    "Soft hits",
    "Pitch drop",
    "Pitch decay",
    "Body",
    "Noise",
    "Noise decay",
    "Character",
    "Control 12",
    "Control 13",
    "Control 14",
    "Control 15",
    "Control 16",
    "Control 17",
    "Control 18",
    "Control 19",
    "Control 20",
];

const GENERAL_HELP: [&str; control::USED] = [
    "Tunes the drum up or down.",
    "Shortens or lengthens the sound.",
    "Darkens or brightens the sound.",
    "Softens or sharpens the start of the sound.",
    "Brings soft hits up towards full ones, or down, away from them.",
    "How far the pitch falls when the drum is struck.",
    "How quickly the falling pitch settles.",
    "Changes the weight of the drum's tonal body.",
    "Balances the noise in the sound.",
    "Shortens or lengthens the noise.",
    "Changes the drum's bite and colour.",
];

/// Control `k`'s general name, 1…20.
#[must_use]
pub fn general_name(k: usize) -> &'static str {
    GENERAL[k - 1]
}

/// Control `k`'s general help, 1…20.
#[must_use]
pub fn general_help(k: usize) -> &'static str {
    GENERAL_HELP
        .get(k - 1)
        .copied()
        .unwrap_or("No drum of this instrument uses this control.")
}

/// Model `model`'s control `k` (1…20), or `None` when its code does not read it — always for 12–20,
/// and for every control of a legacy Off or an unavailable id.
#[must_use]
pub fn face(model: ModelId, k: usize) -> Option<Face> {
    faces(model).get(k.wrapping_sub(1)).copied().flatten()
}

// ---------------------------------------------------------------------------------------------
// The faces most models share.
// ---------------------------------------------------------------------------------------------

const TUNE: Option<Face> = on("Tune", "Tunes the drum up or down.");
const TUNE_DRUM: Option<Face> = on("Tune", "Tunes the drum; the snares keep their pitch.");
const TUNE_METAL: Option<Face> = on(
    "Tune",
    "Moves the metallic ring up or down; it has no single note.",
);
const DECAY: Option<Face> = on("Decay", "How long it rings.");
const DECAY_DRUM: Option<Face> = on(
    "Decay",
    "How long the drum rings; the snares keep their length.",
);
const DECAY_ROOM: Option<Face> = on("Decay", "How long the room tail after the claps lasts.");
const TONE: Option<Face> = on("Tone", "Darkens or brightens the sound.");
const TONE_CLICK: Option<Face> = on("Tone", "Darkens or brightens the click and the overtones.");
const SOFT_HITS: Option<Face> = on(
    "Soft hits",
    "Brings soft hits up towards full ones, or down, away from them. A full hit stays as it is.",
);
const PITCH_DROP: Option<Face> = on(
    "Pitch drop",
    "How far the pitch falls when the drum is struck; at the bottom it stays put.",
);
const PITCH_DECAY: Option<Face> = on(
    "Pitch decay",
    "How quickly the falling pitch settles. Only while Pitch drop is above its bottom.",
);
const PITCH_SWEEP: Option<Face> = on(
    "Pitch sweep",
    "Bends the pitch down after the strike, or up below the centre; at the centre it stays put. Harder hits bend further.",
);
const SWEEP_DECAY: Option<Face> = on(
    "Pitch decay",
    "How quickly the pitch sweep settles; Decay stretches it too. Only while Pitch sweep is off its centre.",
);
const GAIN: Option<Face> = on("Gain", "Only makes it louder or softer, as Level does.");
const BODY_CLICK: Option<Face> = on(
    "Body",
    "How loud the ring is against the click at the start.",
);
const BODY_DRUM: Option<Face> = on("Body", "How loud the drum is against the snares.");
const SNAPPY: Option<Face> = on(
    "Snappy",
    "How loud the snares are against the drum; they never go away entirely.",
);
const SNAPPY_DECAY: Option<Face> = on("Snappy decay", "How long the snares rattle.");
const CLAP_DECAY: Option<Face> = on(
    "Clap decay",
    "How long each clap lasts, from a tight snap to one long smear.",
);
const DRIVE: Option<Face> = on(
    "Drive",
    "Dirtier and fatter above the centre; below it, only quieter.",
);
const DRIVE_METAL: Option<Face> = on(
    "Drive",
    "Grittier and fuller above the centre, cleaner below.",
);

// ---------------------------------------------------------------------------------------------
// Each model, by the family whose code it runs. Order: Tune, Decay, Tone, Attack, Soft hits, Pitch
// drop, Pitch decay, Body, Noise, Noise decay, Character.
// ---------------------------------------------------------------------------------------------

/// 1 Deep bridge kick (`deep_bridge_kick.rs`).
const BRIDGE_KICK: Faces = [
    on("Tune", "Tunes the boom; its swoop follows."),
    on("Decay", "How long the boom rings."),
    on(
        "Tone",
        "Darkens or brightens the click; the boom itself barely changes.",
    ),
    on("Attack", "How loud the beater click is at the start."),
    SOFT_HITS,
    on(
        "Pitch drop",
        "How far the boom swoops down at the start; at the bottom it is a steady note.",
    ),
    on("Pitch decay", "How long the swoop takes."),
    on(
        "Bend",
        "How far the pitch bends down after the click, and how much the boom growls. Only while Pitch drop is above its bottom.",
    ),
    None,
    None,
    on(
        "Drive",
        "Rounds off and fattens the boom above the centre; below it, it stays clean.",
    ),
];

/// 2 Twin-mode snare (`twin_mode_snare.rs`).
const TWIN_SNARE: Faces = [
    TUNE_DRUM,
    DECAY_DRUM,
    on(
        "Tone",
        "Shifts the drum between its low note and its higher ring.",
    ),
    on(
        "Attack",
        "How hard the drum is struck: a louder ring and a firmer thump.",
    ),
    SOFT_HITS,
    None,
    None,
    BODY_DRUM,
    on("Snappy", "How loud the snares are against the drum."),
    on(
        "Snappy decay",
        "How long the snares rattle. Only while Snappy is above its bottom.",
    ),
    None,
];

/// 4–8 the falling congas and the mid and high falling toms (`falling_drum.rs`).
const FALLING: Faces = [
    TUNE,
    DECAY,
    on("Tone", "Darkens or brightens the click at the start."),
    on(
        "Attack",
        "How hard it is struck: louder, with a harder click and a bigger pitch snap.",
    ),
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    on(
        "Body",
        "A little more or less ring, and a bigger or smaller pitch snap at the strike.",
    ),
    None,
    None,
    None,
];

/// 3 Low falling tom: the falling drum with its hiss.
const LOW_FALLING_TOM: Faces = {
    let mut faces = FALLING;
    faces[control::NOISE - 1] = on("Noise", "How loud the hiss under the tom is.");
    faces[control::NOISE_DECAY - 1] = on(
        "Noise decay",
        "How long the hiss lasts. Only while Noise is above its bottom.",
    );
    faces
};

/// 9 Layered short rim (`rim_clave.rs`).
const LAYERED_RIM: Faces = [
    TUNE,
    on("Decay", "How long it rings before it is cut off."),
    TONE,
    on("Attack", "How hard the rim is struck: more or less crack."),
    SOFT_HITS,
    None,
    None,
    on("Body", "How much of the low knock is in the hit."),
    None,
    None,
    on(
        "Clean",
        "Cleaner and more ringing above the centre; harsher and buzzier below.",
    ),
];

/// 10 Pure high clave (`rim_clave.rs`).
const PURE_CLAVE: Faces = [
    on("Tune", "Tunes the clave; very high, it also gets quieter."),
    on("Decay", "How long it rings before it is cut off."),
    on(
        "Tone",
        "Softens or sharpens the click of its start and end.",
    ),
    GAIN,
    SOFT_HITS,
    None,
    None,
    GAIN,
    None,
    None,
    on(
        "Clean",
        "Fuzzier and squashed below the centre; above it, it stays clean.",
    ),
];

/// 11 Bright short maraca (`noise_percussion.rs`).
const MARACA: Faces = [
    None,
    on("Decay", "How long the shake lasts."),
    TONE,
    on("Attack", "How quickly it gets loud."),
    SOFT_HITS,
    None,
    None,
    None,
    GAIN,
    None,
    None,
];

/// 12 Triple pulse clap (`noise_percussion.rs`).
const PULSE_CLAP: Faces = [
    None,
    DECAY_ROOM,
    on("Tone", "Darkens or brightens the claps and the room."),
    on(
        "Clap length",
        "How long each clap lasts, tighter or looser, as Clap decay does over a smaller range.",
    ),
    SOFT_HITS,
    None,
    None,
    None,
    GAIN,
    CLAP_DECAY,
    on(
        "Ring",
        "Narrower and more pitched room above the centre; airier and hissier below.",
    ),
];

/// 13 Twin-square cowbell (`metal_808.rs`).
const SQUARE_COWBELL: Faces = [
    on("Tune", "Tunes the cowbell."),
    on("Decay", "How long it rings after the first clank."),
    on("Tone", "Darker and lower, or brighter and higher."),
    on("Attack", "How long the bright clank at the start lasts."),
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    on("Drive", "Adds a little grit above the centre."),
];

/// 14 Three-path cymbal (`metal_808.rs`).
const THREE_PATH_CYMBAL: Faces = [
    TUNE_METAL,
    DECAY,
    on("Tone", "A darker wash, or a brighter sizzle."),
    on("Sizzle", "How much bright sizzle is on the hit."),
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    DRIVE_METAL,
];

/// 15 and 16, the six-square hats (`metal_808.rs`).
const SQUARE_HAT: Faces = [
    TUNE_METAL,
    on("Decay", "How long it rings: how open it sounds."),
    TONE,
    on("Attack", "How sharp the click at the start is."),
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    DRIVE_METAL,
];

/// 17 Reset punch kick (`reset_vco_909.rs`).
const RESET_KICK: Faces = [
    on("Tune", "Tunes the kick; its pitch drop follows."),
    on("Decay", "How long the kick lasts."),
    on(
        "Weight",
        "How heavy the body is against the click; more also adds a little saturation.",
    ),
    on("Attack", "How loud the click at the start is."),
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    on("Punch", "A deeper pitch drop and a slightly louder body."),
    on(
        "Noise",
        "How much noise is in the click. Only while Attack is above its bottom.",
    ),
    on(
        "Click length",
        "Turns the click from a sharp tick into a soft thump. Only while Attack is above its bottom.",
    ),
    None,
];

/// 18 Reset twin snare (`reset_vco_909.rs`).
const RESET_SNARE: Faces = [
    TUNE_DRUM,
    DECAY_DRUM,
    on(
        "Tone",
        "Shifts the drum between its lower and its upper note.",
    ),
    on(
        "Snappy length",
        "How long the snares rattle, as Snappy decay does over a smaller range. Only while Snappy is above its bottom.",
    ),
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    BODY_DRUM,
    on("Snappy", "How loud the snares are against the drum."),
    on(
        "Snappy decay",
        "How long the snares rattle. Only while Snappy is above its bottom.",
    ),
    None,
];

/// 20 and 21, the mid and high reset triad toms (`reset_vco_909.rs`).
const TRIAD_TOM: Faces = [
    TUNE,
    on("Decay", "How long its main tone rings."),
    TONE,
    on(
        "Attack",
        "How hard the knock at the start is, and how long its hiss lasts.",
    ),
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    GAIN,
    on("Noise", "How loud the hiss is."),
    on(
        "Noise decay",
        "How long the hiss lasts. Only while Noise is above its bottom.",
    ),
    on(
        "Drive",
        "Buzzier and squarer above the centre, rounder below.",
    ),
];

/// 19 Low reset triad tom: the triad tom without a knock.
const LOW_TRIAD_TOM: Faces = {
    let mut faces = TRIAD_TOM;
    faces[control::ATTACK - 1] = on(
        "Attack",
        "On this tom, only how long the faint hiss at the start lasts.",
    );
    faces
};

/// 22 Triple-resonator rim (`analogue_909.rs`).
const TRIPLE_RIM: Faces = [
    TUNE,
    DECAY,
    TONE,
    GAIN,
    SOFT_HITS,
    None,
    None,
    GAIN,
    None,
    None,
    on("Drive", "Harsher above the centre, cleaner below."),
];

/// 23 Four-cell clap (`analogue_909.rs`).
const FOUR_CELL_CLAP: Faces = [
    None,
    DECAY_ROOM,
    on("Tone", "Moves the claps' colour down or up."),
    on("Spread", "How far apart the claps are."),
    SOFT_HITS,
    None,
    None,
    None,
    GAIN,
    CLAP_DECAY,
    on(
        "Air",
        "Airier and softer above the centre; more ringing and pitched below.",
    ),
];

/// 24–27, the six-bit hats and cymbals (`pcm_909.rs`).
const SIX_BIT: Faces = [
    on("Tune", "Tunes it up or down; higher is also shorter."),
    on(
        "Decay",
        "Fades it out sooner, or holds its tail up to its natural end.",
    ),
    TONE,
    on(
        "Soft start",
        "Softens the start above the centre; hardens it below.",
    ),
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    on(
        "Crunch",
        "Saturates it above the centre; grainier and more lo-fi below.",
    ),
];

/// 28 Economy 62 kick (`economy_55.rs`).
const ECONOMY_KICK: Faces = [
    TUNE,
    on(
        "Decay",
        "How long the kick lasts; it stretches the pitch sweep too.",
    ),
    TONE,
    on("Attack", "How hard the thump at the start is."),
    SOFT_HITS,
    PITCH_SWEEP,
    SWEEP_DECAY,
    on("Drive", "Drives the ring harder, flattening it."),
    None,
    None,
    None,
];

/// 29 Economy body snare (`economy_55.rs`).
const ECONOMY_SNARE: Faces = [
    TUNE_DRUM,
    DECAY_DRUM,
    on("Tone", "Brightens or darkens the snares only."),
    GAIN,
    SOFT_HITS,
    None,
    None,
    BODY_DRUM,
    SNAPPY,
    SNAPPY_DECAY,
    None,
];

/// 30 Economy short rim (`economy_55.rs`).
const ECONOMY_RIM: Faces = [
    TUNE,
    DECAY,
    on(
        "Tone",
        "Darkens or brightens it; very dark, it also gets quieter.",
    ),
    GAIN,
    SOFT_HITS,
    None,
    None,
    GAIN,
    None,
    None,
    None,
];

/// 31 Inductor noise hat (`economy_55.rs`).
const INDUCTOR_HAT: Faces = [
    None,
    on("Decay", "How long the hat lasts."),
    on("Tone", "Moves the hat's ringing colour down or up."),
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    GAIN,
    None,
    on(
        "Drive",
        "Louder and saturated above the centre; quieter and clean below.",
    ),
];

// The supporting machines (`legacy.rs`): on every one of them Attack is a level change only.

/// 47 and (with Drive) 32, the two-oscillator kicks, sweeping (`legacy.rs`'s dual voice).
const DUAL_KICK: Faces = [
    TUNE,
    DECAY,
    on(
        "Tone",
        "Darkens or brightens the overtones; the low note barely changes.",
    ),
    GAIN,
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    GAIN,
    None,
    None,
    None,
];

/// 73 Triple high bell and 89 Early cowbell: the dual voice, unswept.
const DUAL_BELL: Faces = [
    TUNE, DECAY, TONE, GAIN, SOFT_HITS, None, None, GAIN, None, None, DRIVE,
];

/// 34–36, the diode toms, with their faint noise.
const DIODE_TOM: Faces = [
    TUNE,
    DECAY,
    TONE_CLICK,
    GAIN,
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    BODY_CLICK,
    on("Noise", "How loud the faint noisy hum inside the drum is."),
    on("Noise decay", "How long that hum lasts."),
    None,
];

/// 37–39, 49, 50, 60, 69, 85 (and 54, with Drive): a resonator with a pitch drop of its own.
const SWEPT_RESONATOR: Faces = [
    TUNE,
    DECAY,
    TONE_CLICK,
    GAIN,
    SOFT_HITS,
    PITCH_DROP,
    PITCH_DECAY,
    BODY_CLICK,
    None,
    None,
    None,
];

/// 67, 68, 74–77, 86–88: a resonator with no pitch movement of its own, which Pitch sweep adds.
const SWEEPABLE_RESONATOR: Faces = [
    TUNE,
    DECAY,
    TONE_CLICK,
    GAIN,
    SOFT_HITS,
    PITCH_SWEEP,
    SWEEP_DECAY,
    BODY_CLICK,
    None,
    None,
    None,
];

/// 62, 66, 79, 80, 90: a resonator whose pitch does not move.
const RESONATOR: Faces = [
    TUNE, DECAY, TONE_CLICK, GAIN, SOFT_HITS, None, None, BODY_CLICK, None, None, None,
];

/// 40, 45, 78: the resonator with Drive.
const DRIVEN_RESONATOR: Faces = {
    let mut faces = RESONATOR;
    faces[control::CHARACTER - 1] = DRIVE;
    faces
};

/// 33, 48, 55, 61, 81, 91: the supporting machines' snares.
const SNARE: Faces = [
    TUNE_DRUM,
    DECAY_DRUM,
    on(
        "Tone",
        "Brightens or darkens the snares and the click; the drum's own note is untouched.",
    ),
    GAIN,
    SOFT_HITS,
    None,
    None,
    BODY_DRUM,
    SNAPPY,
    SNAPPY_DECAY,
    None,
];

/// 41–43, 51–53: metallic hats and cymbals.
const METAL: Faces = [
    TUNE_METAL,
    DECAY,
    on("Tone", "Moves the metallic colour darker or brighter."),
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    DRIVE,
];

/// 53 Tempo-coupled open hat: its length follows the song's tempo.
const TEMPO_HAT: Faces = {
    let mut faces = METAL;
    faces[control::DECAY - 1] = on(
        "Decay",
        "How long it rings; it also follows the song's tempo.",
    );
    faces
};

/// 56–58: metal with hiss mixed in.
const MIXED_METAL: Faces = {
    let mut faces = METAL;
    faces[control::NOISE - 1] = on("Noise", "How much hiss is mixed into the metal.");
    faces
};

/// 46 and 70, the two-note cowbells.
const METAL_COWBELL: Faces = [
    on("Tune", "Tunes the cowbell."),
    DECAY,
    on(
        "Tone",
        "Moves the cowbell's colour; it can change which of its two notes leads.",
    ),
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    DRIVE,
];

/// 63, 64, 71, 92: hiss against a click or a ringing line, which Noise balances.
const NOISE_WASH: Faces = [
    None,
    DECAY,
    on("Tone", "Moves the hiss darker or brighter."),
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    on(
        "Noise",
        "How loud the hiss is against the rest of the sound.",
    ),
    None,
    DRIVE,
];

/// 82, 84: all hiss, so Noise is its level.
const NOISE_ONLY: Faces = {
    let mut faces = NOISE_WASH;
    faces[control::NOISE - 1] = GAIN;
    faces
};

/// 65, 83, 93, the maracas: all hiss, no Drive.
const NOISE_MARACA: Faces = {
    let mut faces = NOISE_ONLY;
    faces[control::CHARACTER - 1] = None;
    faces
};

/// 94 Early wire brush.
const WIRE_BRUSH: Faces = {
    let mut faces = NOISE_ONLY;
    faces[control::NOISE_DECAY - 1] = on(
        "Noise decay",
        "Shortens or lengthens the brush, as Decay does, but leaves its long second fade alone.",
    );
    faces
};

/// 44 and 59, the supporting machines' claps.
const CLAP: Faces = [
    None,
    DECAY_ROOM,
    on("Tone", "Darkens or brightens the claps and the room."),
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    GAIN,
    on("Clap decay", "How long the last clap lasts."),
    DRIVE,
];

/// 72 Two-rate guiro.
const GUIRO: Faces = [
    on(
        "Speed",
        "How fast it is scraped; very fast, it turns into a hiss.",
    ),
    on("Decay", "How long the scrape lasts, and when it speeds up."),
    TONE,
    GAIN,
    SOFT_HITS,
    None,
    None,
    None,
    None,
    None,
    DRIVE,
];

/// A legacy Off or an unavailable id: nothing to read.
const NONE: Faces = [None; control::USED];

/// **Model `model`'s controls**, by the family whose code it runs.
#[must_use]
pub fn faces(model: ModelId) -> &'static Faces {
    match model.raw() {
        1 => &BRIDGE_KICK,
        2 => &TWIN_SNARE,
        3 => &LOW_FALLING_TOM,
        4..=8 => &FALLING,
        9 => &LAYERED_RIM,
        10 => &PURE_CLAVE,
        11 => &MARACA,
        12 => &PULSE_CLAP,
        13 => &SQUARE_COWBELL,
        14 => &THREE_PATH_CYMBAL,
        15 | 16 => &SQUARE_HAT,
        17 => &RESET_KICK,
        18 => &RESET_SNARE,
        19 => &LOW_TRIAD_TOM,
        20 | 21 => &TRIAD_TOM,
        22 => &TRIPLE_RIM,
        23 => &FOUR_CELL_CLAP,
        24..=27 => &SIX_BIT,
        28 => &ECONOMY_KICK,
        29 => &ECONOMY_SNARE,
        30 => &ECONOMY_RIM,
        31 => &INDUCTOR_HAT,
        32 => &DUAL_KICK_DRIVEN,
        47 => &DUAL_KICK,
        33 | 48 | 55 | 61 | 81 | 91 => &SNARE,
        34..=36 => &DIODE_TOM,
        54 => &SWEPT_RESONATOR_DRIVEN,
        37..=39 | 49 | 50 | 60 | 69 | 85 => &SWEPT_RESONATOR,
        67 | 68 | 74..=77 | 86..=88 => &SWEEPABLE_RESONATOR,
        40 | 45 | 78 => &DRIVEN_RESONATOR,
        62 | 66 | 79 | 80 | 90 => &RESONATOR,
        41..=43 | 51 | 52 => &METAL,
        53 => &TEMPO_HAT,
        56..=58 => &MIXED_METAL,
        46 | 70 => &METAL_COWBELL,
        73 => &BELL,
        89 => &DUAL_BELL,
        63 | 64 | 71 | 92 => &NOISE_WASH,
        82 | 84 => &NOISE_ONLY,
        65 | 83 | 93 => &NOISE_MARACA,
        94 => &WIRE_BRUSH,
        44 | 59 => &CLAP,
        72 => &GUIRO,
        _ => &NONE,
    }
}

/// 32 Dual-low kick: the sweeping dual voice with Drive.
const DUAL_KICK_DRIVEN: Faces = {
    let mut faces = DUAL_KICK;
    faces[control::CHARACTER - 1] = DRIVE;
    faces
};

/// 54 Damped whack kick: the swept resonator with Drive.
const SWEPT_RESONATOR_DRIVEN: Faces = {
    let mut faces = SWEPT_RESONATOR;
    faces[control::CHARACTER - 1] = DRIVE;
    faces
};

/// 73 Triple high bell: its partials sit high above Tone's corner, so Tone mostly moves its level.
const BELL: Faces = {
    let mut faces = DUAL_BELL;
    faces[control::TONE - 1] = on(
        "Tone",
        "Darkens or brightens it; on this bell, mostly louder or softer.",
    );
    faces
};

#[cfg(test)]
mod tests {
    use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, Capabilities, ModelId};

    use super::*;

    /// What the DSP declares each model's code reads, by control number.
    fn reads(capabilities: Capabilities, k: usize) -> bool {
        match k {
            control::TUNE => capabilities.pitch,
            control::DECAY => capabilities.decay,
            control::TONE => capabilities.tone,
            control::ATTACK => capabilities.attack,
            control::DYNAMICS => capabilities.dynamics,
            control::PITCH_ENV => capabilities.pitch_envelope,
            control::PITCH_DECAY => capabilities.pitch_decay,
            control::BODY => capabilities.body,
            control::NOISE => capabilities.noise,
            control::NOISE_DECAY => capabilities.noise_decay,
            control::CHARACTER => capabilities.character,
            _ => false,
        }
    }

    /// **A model shows exactly the controls its code reads** (the owner, 2026-10-07): the DSP's
    /// declaration, which its families' code was read against control by control, and whose other
    /// half — an undeclared control is an exact no-op — the DSP's own tests hold. Controls 12–20
    /// and every control of an unavailable id show nowhere.
    #[test]
    fn each_model_shows_exactly_the_controls_its_code_reads() {
        for spec in &AVAILABLE_MODELS {
            let capabilities = spec.id.capabilities();
            for k in 1..=CONTROLS {
                assert_eq!(
                    face(spec.id, k).is_some(),
                    reads(capabilities, k),
                    "{} ({}) control {k}",
                    spec.id.raw(),
                    spec.label
                );
            }
        }
        for id in [0, 95, 200, 255] {
            assert!((1..=CONTROLS).all(|k| face(ModelId::new(id), k).is_none()));
        }
    }

    /// The names a player reads: short, sentence case, and the help a sentence of its own. The
    /// snares keep *Snappy*, and every model's velocity response is *Soft hits*.
    #[test]
    fn every_name_is_short_sentence_case_and_every_help_a_sentence() {
        for spec in &AVAILABLE_MODELS {
            for k in 1..=control::USED {
                let Some(face) = face(spec.id, k) else {
                    continue;
                };
                let mut chars = face.name.chars();
                assert!(chars.next().is_some_and(char::is_uppercase), "{face:?}");
                assert!(chars.all(|c| !c.is_uppercase()), "{face:?}");
                assert!(face.name.len() <= 14, "{face:?}");
                assert!(face.help.ends_with('.'), "{face:?}");
            }
            assert_eq!(
                face(spec.id, control::DYNAMICS).map(|f| f.name),
                Some("Soft hits")
            );
        }
        for snare in [2, 18, 29, 33, 48, 55, 61, 81, 91] {
            assert_eq!(
                face(ModelId::new(snare), control::NOISE).map(|f| f.name),
                Some("Snappy"),
                "{snare}"
            );
        }
    }

    /// Prints the table for the plugin's `NOTES.md`, from this module: each model's names, then
    /// what each distinct control does. `cargo test -p mxm-drum-machine --lib
    /// print_the_control_table -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints the NOTES.md table"]
    fn print_the_control_table() {
        let header: Vec<String> = (1..=control::USED)
            .map(|k| format!("{k} {}", general_name(k)))
            .collect();
        println!("| Model | {} |", header.join(" | "));
        println!("|---|{}", "---|".repeat(control::USED));
        for spec in &AVAILABLE_MODELS {
            let cells: Vec<&str> = (1..=control::USED)
                .map(|k| face(spec.id, k).map_or("—", |f| f.name))
                .collect();
            println!(
                "| {} {} | {} |",
                spec.id.raw(),
                spec.label,
                cells.join(" | ")
            );
        }
        println!();
        let mut seen: Vec<(usize, Face, Vec<u8>)> = Vec::new();
        for spec in &AVAILABLE_MODELS {
            for k in 1..=control::USED {
                if let Some(face) = face(spec.id, k) {
                    match seen.iter_mut().find(|(j, f, _)| *j == k && *f == face) {
                        Some((_, _, ids)) => ids.push(spec.id.raw()),
                        None => seen.push((k, face, vec![spec.id.raw()])),
                    }
                }
            }
        }
        seen.sort_by_key(|(k, _, ids)| (*k, ids[0]));
        println!("| Control | Name | What it does | Models |");
        println!("|---|---|---|---|");
        for (k, face, ids) in seen {
            let ids: Vec<String> = ids.iter().map(u8::to_string).collect();
            println!(
                "| {k} | {} | {} | {} |",
                face.name,
                face.help,
                ids.join(", ")
            );
        }
    }
}
