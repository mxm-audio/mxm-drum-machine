//! The CLAP parameter surface, on mxm-model-drums' scheme: **the host holds what the panel can show
//! at once** (the owner, 2026-09-30).
//!
//! nice-plug's nested arrays make the IDs `<suffix>_<one-based slot>`: `model_1` through
//! `route4_amount_16`. A slot's sound controls are general, `c01`…`c20`, named `Control k` for the
//! host and by the model on the panel; four route slots replace the 13 × 7 grid of route pairs.
//! 651 parameters in all ([`SLOT_PARAMETERS`] a slot, [`GLOBAL_IDS`] kit-wide).
//!
//! **A mechanical move, the same sound** (the owner, 2026-10-07): each named control of before is one
//! general control now, at its old range, default, unit and smoothing and on its old path into the
//! DSP ([`control`]). The model's plain domain is fixed at 0…255; ID 0 remains legacy silence.
//!
//! **IDs are free to change during pre-alpha** (the owner, 2026-10-07): the permanent-ID freeze
//! applies from the first release, and nothing migrates old IDs ("There are no saved projects - we
//! are in pre alpha", 2026-09-30).

use std::sync::{Arc, LazyLock, RwLock};

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, MODEL_ID_MAX, ModelId, available};
use nice_plug::prelude::*;

use crate::routes::{ROUTES, Routes};

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShapeChoice {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "rampup"]
    #[name = "Ramp up"]
    RampUp,
    #[id = "rampdown"]
    #[name = "Ramp down"]
    RampDown,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "samplehold"]
    #[name = "Sample & hold"]
    SampleHold,
}

impl LfoShapeChoice {
    #[must_use]
    pub const fn dsp(self) -> mxm_drum_machine_dsp::engine::LfoShape {
        use mxm_drum_machine_dsp::engine::LfoShape;
        match self {
            Self::Sine => LfoShape::Sine,
            Self::Triangle => LfoShape::Triangle,
            Self::RampUp => LfoShape::RampUp,
            Self::RampDown => LfoShape::RampDown,
            Self::Square => LfoShape::Square,
            Self::SampleHold => LfoShape::SampleHold,
        }
    }
}

/// Every LFO's tempo sync (`plans/plan-tempo-sync-controls.md`): the collection's LFO ladder, 1/32 to
/// four bars, the top the fastest. The pilot's own fourteen-step table was exactly this span, so every
/// stored rate position keeps its division.
pub const LFO_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

/// The general controls a slot holds, `c01`…`c20` (mxm-model-drums' twenty).
pub const CONTROLS: usize = 20;

/// **What each general control is on this machine**, by its number `k` (the owner, 2026-10-07: a
/// mechanical mapping). Model-drums' common seven come first, so automation keeps its sense across
/// the two instruments: 1 Tune, 2 Decay, 3 Tone, 4 Attack, 5 Velocity, 6 Pitch drop, 7 Pitch decay.
/// Here 5 is **Dynamics**, the velocity curve's exponent on every model (the DSP's `velocity.rs`),
/// and 6 is Pitch envelope; each keeps the panel name it had. 8–11 are the machine's own, in the order
/// they were declared. **12–20 mean nothing on any model**: exact no-ops, never on the panel.
pub mod control {
    /// The DSP's `pitch_semitones`; the panel's *Tune*. Its own range: ±24 semitones.
    pub const TUNE: usize = 1;
    pub const DECAY: usize = 2;
    pub const TONE: usize = 3;
    pub const ATTACK: usize = 4;
    /// Model-drums' *Velocity* place: how a soft hit differs from a hard one.
    pub const DYNAMICS: usize = 5;
    /// Model-drums' *Pitch drop* place: the DSP's `pitch_envelope`.
    pub const PITCH_ENV: usize = 6;
    pub const PITCH_DECAY: usize = 7;
    pub const BODY: usize = 8;
    pub const NOISE: usize = 9;
    pub const NOISE_DECAY: usize = 10;
    pub const CHARACTER: usize = 11;
    /// Controls `1..=USED` mean something; the rest are unused on every model.
    pub const USED: usize = 11;
}

/// The general controls, nested so their IDs are `c01`…`c20` inside a slot.
#[derive(Params)]
pub struct ControlParams {
    #[id = "c01"]
    pub c01: FloatParam,
    #[id = "c02"]
    pub c02: FloatParam,
    #[id = "c03"]
    pub c03: FloatParam,
    #[id = "c04"]
    pub c04: FloatParam,
    #[id = "c05"]
    pub c05: FloatParam,
    /// Additive: zero preserves the calibrated circuit excursion.
    #[id = "c06"]
    pub c06: FloatParam,
    /// Additive: zero preserves the calibrated excursion time.
    #[id = "c07"]
    pub c07: FloatParam,
    #[id = "c08"]
    pub c08: FloatParam,
    #[id = "c09"]
    pub c09: FloatParam,
    /// Separate from Decay wherever the circuit has a distinct wire/noise envelope.
    #[id = "c10"]
    pub c10: FloatParam,
    #[id = "c11"]
    pub c11: FloatParam,
    #[id = "c12"]
    pub c12: FloatParam,
    #[id = "c13"]
    pub c13: FloatParam,
    #[id = "c14"]
    pub c14: FloatParam,
    #[id = "c15"]
    pub c15: FloatParam,
    #[id = "c16"]
    pub c16: FloatParam,
    #[id = "c17"]
    pub c17: FloatParam,
    #[id = "c18"]
    pub c18: FloatParam,
    #[id = "c19"]
    pub c19: FloatParam,
    #[id = "c20"]
    pub c20: FloatParam,
}

impl Default for ControlParams {
    fn default() -> Self {
        let c = |k: usize| deviation(&format!("Control {k}"));
        Self {
            // Tune keeps the old Pitch parameter's range, unit and reading: only its name moved.
            c01: tune("Control 1"),
            c02: c(2),
            c03: c(3),
            c04: c(4),
            c05: c(5),
            c06: c(6),
            c07: c(7),
            c08: c(8),
            c09: c(9),
            c10: c(10),
            c11: c(11),
            c12: c(12),
            c13: c(13),
            c14: c(14),
            c15: c(15),
            c16: c(16),
            c17: c(17),
            c18: c(18),
            c19: c(19),
            c20: c(20),
        }
    }
}

impl ControlParams {
    /// The twenty in order.
    #[must_use]
    pub fn all(&self) -> [&FloatParam; CONTROLS] {
        [
            &self.c01, &self.c02, &self.c03, &self.c04, &self.c05, &self.c06, &self.c07, &self.c08,
            &self.c09, &self.c10, &self.c11, &self.c12, &self.c13, &self.c14, &self.c15, &self.c16,
            &self.c17, &self.c18, &self.c19, &self.c20,
        ]
    }

    /// Control `k`, 1…20 — [`control`] names what it is.
    #[must_use]
    pub fn get(&self, k: usize) -> &FloatParam {
        match k {
            1 => &self.c01,
            2 => &self.c02,
            3 => &self.c03,
            4 => &self.c04,
            5 => &self.c05,
            6 => &self.c06,
            7 => &self.c07,
            8 => &self.c08,
            9 => &self.c09,
            10 => &self.c10,
            11 => &self.c11,
            12 => &self.c12,
            13 => &self.c13,
            14 => &self.c14,
            15 => &self.c15,
            16 => &self.c16,
            17 => &self.c17,
            18 => &self.c18,
            19 => &self.c19,
            20 => &self.c20,
            _ => unreachable!("control {k} is not one of the twenty"),
        }
    }
}

#[derive(Params)]
pub struct SlotParams {
    #[id = "model"]
    pub model: IntParam,
    /// The general controls. Boxed, as the routes are, so building the surface never leans on a
    /// host thread's stack.
    #[nested]
    pub controls: Box<ControlParams>,
    #[id = "level"]
    pub level: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    /// Post-circuit silence that preserves the slot's model and its live machine state.
    #[id = "mute"]
    pub mute: BoolParam,
    /// Post-circuit isolation. Mute wins when both controls are active.
    #[id = "solo"]
    pub solo: BoolParam,
    /// Exclusive choke group: 0 is none, 1…16 cut every other slot in the same group. Nothing
    /// chokes unless it is assigned here — the source machines' own hat pairs are not wired
    /// together behind the user's back, so any two slots can be paired instead.
    #[id = "choke_group"]
    pub choke_group: IntParam,
    /// Instance routing: 0 is main L+R, 1…16 are mono individual outputs.
    #[id = "output"]
    pub output: IntParam,
    /// Instance note mapping: 0 is Kit, 1…16 claim the corresponding chromatic channel.
    #[id = "midi_channel"]
    pub midi_channel: IntParam,
    /// This slot's four route slots. The LFO generators themselves are kit-wide.
    #[nested]
    pub routes: Box<Routes>,
}

impl SlotParams {
    fn new(index: usize) -> Self {
        // The brief's final sixteen-slot Init: every assignment is a completed circuit.
        const INIT_MODELS: [u8; SLOT_COUNT] =
            [1, 17, 2, 18, 3, 20, 7, 22, 12, 23, 13, 15, 16, 14, 26, 27];
        let default_model = INIT_MODELS[index];
        Self {
            model: model_param(default_model),
            controls: Box::default(),
            level: gain("Level", 0.0),
            pan: pan_param(),
            mute: BoolParam::new("Mute", false),
            solo: BoolParam::new("Solo", false),
            choke_group: choke_group_param(),
            output: output_param(),
            midi_channel: midi_channel_param(),
            routes: Box::default(),
        }
    }

    /// The slot's parameters in declaration order, beside [`slot_ids`].
    #[must_use]
    pub fn parameters(&self) -> [&dyn mxm_preset::ErasedParam; SLOT_PARAMETERS] {
        let c = self.controls.all();
        let r = self.routes.all();
        [
            &self.model,
            c[0],
            c[1],
            c[2],
            c[3],
            c[4],
            c[5],
            c[6],
            c[7],
            c[8],
            c[9],
            c[10],
            c[11],
            c[12],
            c[13],
            c[14],
            c[15],
            c[16],
            c[17],
            c[18],
            c[19],
            &self.level,
            &self.pan,
            &self.mute,
            &self.solo,
            &self.choke_group,
            &self.output,
            &self.midi_channel,
            &r[0].source,
            &r[0].target,
            &r[0].amount,
            &r[1].source,
            &r[1].target,
            &r[1].amount,
            &r[2].source,
            &r[2].target,
            &r[2].amount,
            &r[3].source,
            &r[3].target,
            &r[3].amount,
        ]
    }

    #[must_use]
    pub fn model_id(&self) -> ModelId {
        ModelId::new(self.model.value().clamp(0, i32::from(MODEL_ID_MAX)) as u8)
    }
}

#[derive(Params)]
pub struct MxmDrumMachineParams {
    #[nested(array, group = "Slot")]
    pub slots: [SlotParams; SLOT_COUNT],
    #[id = "master"]
    pub master: FloatParam,
    #[id = "lfo1_rate"]
    pub lfo1_rate: FloatParam,
    #[id = "lfo1_shape"]
    pub lfo1_shape: EnumParam<LfoShapeChoice>,
    #[id = "lfo1_sync"]
    pub lfo1_sync: BoolParam,
    #[id = "lfo2_rate"]
    pub lfo2_rate: FloatParam,
    #[id = "lfo2_shape"]
    pub lfo2_shape: EnumParam<LfoShapeChoice>,
    #[id = "lfo2_sync"]
    pub lfo2_sync: BoolParam,
    #[id = "lfo3_rate"]
    pub lfo3_rate: FloatParam,
    #[id = "lfo3_shape"]
    pub lfo3_shape: EnumParam<LfoShapeChoice>,
    #[id = "lfo3_sync"]
    pub lfo3_sync: BoolParam,
    /// Resample: play captured one-shots instead of the circuits (plan §4.7).
    ///
    /// An **instance setting**, so a kit neither engages nor disengages it — see
    /// `preset.rs`'s `is_instance_setting`. It is the only id the feature adds; the sample-domain
    /// readings of Tune and Decay are controls 1 and 2, the slot's own ([`control::TUNE`],
    /// [`control::DECAY`]).
    #[id = "resample"]
    pub resample: BoolParam,
    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

impl Default for MxmDrumMachineParams {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(SlotParams::new),
            master: gain("Master", 0.0),
            lfo1_rate: lfo_rate("LFO 1 rate", 0.7),
            lfo1_shape: EnumParam::new("LFO 1 shape", LfoShapeChoice::Sine),
            lfo1_sync: BoolParam::new("LFO 1 sync", false),
            lfo2_rate: lfo_rate("LFO 2 rate", 1.3),
            lfo2_shape: EnumParam::new("LFO 2 shape", LfoShapeChoice::Triangle),
            lfo2_sync: BoolParam::new("LFO 2 sync", false),
            lfo3_rate: lfo_rate("LFO 3 rate", 4.0),
            lfo3_shape: EnumParam::new("LFO 3 shape", LfoShapeChoice::Sine),
            lfo3_sync: BoolParam::new("LFO 3 sync", false),
            // Off in Init and in every factory kit: a frozen kit must not be what the
            // fifty-kit diversity render measures.
            resample: BoolParam::new("Resample", false),
            preset: RwLock::new(mxm_preset::PresetIdentity::default()),
        }
    }
}

impl MxmDrumMachineParams {
    /// The kit-wide parameters in declaration order, beside [`GLOBAL_IDS`].
    #[must_use]
    pub fn globals(&self) -> [&dyn mxm_preset::ErasedParam; GLOBAL_IDS.len()] {
        [
            &self.master,
            &self.lfo1_rate,
            &self.lfo1_shape,
            &self.lfo1_sync,
            &self.lfo2_rate,
            &self.lfo2_shape,
            &self.lfo2_sync,
            &self.lfo3_rate,
            &self.lfo3_shape,
            &self.lfo3_sync,
            &self.resample,
        ]
    }
}

/// Parameters a slot holds: the model, twenty controls, seven slot settings, four routes of three.
pub const SLOT_PARAMETERS: usize = 1 + CONTROLS + 7 + 3 * ROUTES;

/// Every slot's IDs, in declaration order: what presets write and the tests hold. Made once and kept
/// for the process, because presets take `&'static str`.
static SLOT_IDS: LazyLock<Vec<[&'static str; SLOT_PARAMETERS]>> = LazyLock::new(|| {
    (1..=SLOT_COUNT)
        .map(|n| {
            let mut ids = vec![format!("model_{n}")];
            ids.extend((1..=CONTROLS).map(|k| format!("c{k:02}_{n}")));
            for suffix in [
                "level",
                "pan",
                "mute",
                "solo",
                "choke_group",
                "output",
                "midi_channel",
            ] {
                ids.push(format!("{suffix}_{n}"));
            }
            for r in 1..=ROUTES {
                for field in ["source", "target", "amount"] {
                    ids.push(format!("route{r}_{field}_{n}"));
                }
            }
            let ids: Vec<&'static str> = ids
                .into_iter()
                .map(|id| &*Box::leak(id.into_boxed_str()))
                .collect();
            ids.try_into().expect("a slot's parameter count")
        })
        .collect()
});

/// Where each of a slot's parameters sits in [`slot_ids`] and [`SlotParams::parameters`].
pub mod at {
    pub const MODEL: usize = 0;
    pub const LEVEL: usize = 21;
    pub const PAN: usize = 22;
    pub const MUTE: usize = 23;
    pub const SOLO: usize = 24;
    pub const CHOKE_GROUP: usize = 25;
    pub const OUTPUT: usize = 26;
    pub const MIDI_CHANNEL: usize = 27;

    /// Control `k`, 1…20.
    #[must_use]
    pub const fn control(k: usize) -> usize {
        k
    }

    /// Route `r`'s (0-based) source, target and amount.
    #[must_use]
    pub const fn source(r: usize) -> usize {
        28 + 3 * r
    }

    #[must_use]
    pub const fn target(r: usize) -> usize {
        29 + 3 * r
    }

    #[must_use]
    pub const fn amount(r: usize) -> usize {
        30 + 3 * r
    }
}

/// Slot `slot`'s (0-based) IDs, in declaration order.
#[must_use]
pub fn slot_ids(slot: usize) -> &'static [&'static str; SLOT_PARAMETERS] {
    &SLOT_IDS[slot]
}

/// The slot and the route (both 0-based) whose source, target or amount `id` is; `None` for any other
/// parameter.
#[must_use]
pub fn route_of(id: &str) -> Option<(usize, usize)> {
    if !id.starts_with("route") {
        return None;
    }
    (0..SLOT_COUNT).find_map(|slot| {
        let ids = slot_ids(slot);
        (0..ROUTES)
            .find(|&r| [ids[at::source(r)], ids[at::target(r)], ids[at::amount(r)]].contains(&id))
            .map(|r| (slot, r))
    })
}

/// The kit-wide IDs, in declaration order.
pub const GLOBAL_IDS: [&str; 11] = [
    "master",
    "lfo1_rate",
    "lfo1_shape",
    "lfo1_sync",
    "lfo2_rate",
    "lfo2_shape",
    "lfo2_sync",
    "lfo3_rate",
    "lfo3_shape",
    "lfo3_sync",
    "resample",
];

fn lfo_rate_range() -> FloatRange {
    FloatRange::Skewed {
        min: 0.05,
        max: 20.0,
        factor: FloatRange::skew_factor(-1.4),
    }
}

fn lfo_rate(name: &str, default: f32) -> FloatParam {
    let free_parser = formatters::s2v_f32_hz_then_khz();
    FloatParam::new(name, default, lfo_rate_range())
        .with_smoother(SmoothingStyle::Logarithmic(15.0))
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_rounded(2))
        .with_string_to_value(Arc::new(move |text| {
            // A typed division selects the position that picks it on the LFO ladder.
            match mxm_tempo::Division::parse(text).filter(|d| LFO_SYNC.span.contains(*d)) {
                Some(division) => Some(lfo_rate_range().unnormalize(LFO_SYNC.position(division))),
                None => free_parser(text),
            }
        }))
}

fn model_param(default: u8) -> IntParam {
    IntParam::new(
        "Model",
        i32::from(default),
        IntRange::Linear {
            min: 0,
            max: i32::from(MODEL_ID_MAX),
        },
    )
    .with_value_to_string(Arc::new(|plain| {
        let id = ModelId::new(plain.clamp(0, i32::from(MODEL_ID_MAX)) as u8);
        available(id).map_or_else(
            || {
                if id == ModelId::OFF {
                    "Legacy Off".to_owned()
                } else {
                    format!("Unavailable {}", id.raw())
                }
            },
            |spec| spec.label.to_owned(),
        )
    }))
    .with_string_to_value(Arc::new(|text| {
        let text = text.trim();
        if let Some(spec) = AVAILABLE_MODELS
            .iter()
            .find(|spec| spec.label.eq_ignore_ascii_case(text))
        {
            return Some(i32::from(spec.id.raw()));
        }
        if text.eq_ignore_ascii_case("Legacy Off") {
            return Some(0);
        }
        text.strip_prefix("Unavailable ")
            .unwrap_or(text)
            .parse::<u8>()
            .ok()
            .map(i32::from)
    }))
}

/// Control 1, Tune: the pitch law's deviation in semitones, ±24 — the old `pitch` parameter's
/// range, smoothing and reading, under its general name.
fn tune(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(15.0))
    .with_unit(" st")
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn pan_param() -> FloatParam {
    FloatParam::new(
        "Pan",
        0.0,
        FloatRange::Linear {
            min: -1.0,
            max: 1.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(15.0))
    .with_value_to_string(Arc::new(|value| {
        if value.abs() < 0.005 {
            "Centre".to_owned()
        } else if value < 0.0 {
            format!("L {:.0}%", -100.0 * value)
        } else {
            format!("R {:.0}%", 100.0 * value)
        }
    }))
    .with_string_to_value(Arc::new(|text| {
        let text = text.trim().to_ascii_lowercase();
        if text == "centre" || text == "center" || text == "c" {
            return Some(0.0);
        }
        let (number, sign) = if let Some(number) = text.strip_prefix('l') {
            (number, -1.0)
        } else if let Some(number) = text.strip_prefix('r') {
            (number, 1.0)
        } else {
            (text.as_str(), 1.0)
        };
        number
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| sign * value / 100.0)
    }))
}

fn output_param() -> IntParam {
    IntParam::new("Output", 0, IntRange::Linear { min: 0, max: 16 })
        .with_value_to_string(Arc::new(|value| {
            if value == 0 {
                "L+R".to_owned()
            } else {
                value.to_string()
            }
        }))
        .with_string_to_value(Arc::new(|text| {
            let text = text.trim();
            if text.eq_ignore_ascii_case("L+R") || text.eq_ignore_ascii_case("main") {
                Some(0)
            } else {
                text.strip_prefix("Output ")
                    .or_else(|| text.strip_prefix("Out "))
                    .or_else(|| text.strip_prefix("Slot "))
                    .unwrap_or(text)
                    .parse::<i32>()
                    .ok()
            }
        }))
}

fn choke_group_param() -> IntParam {
    IntParam::new("Choke group", 0, IntRange::Linear { min: 0, max: 16 })
        .with_value_to_string(Arc::new(|value| {
            if value == 0 {
                "Off".to_owned()
            } else {
                value.to_string()
            }
        }))
        .with_string_to_value(Arc::new(|text| {
            let text = text.trim();
            if text.eq_ignore_ascii_case("off") || text.eq_ignore_ascii_case("none") {
                Some(0)
            } else {
                text.strip_prefix("Group ")
                    .or_else(|| text.strip_prefix("Choke "))
                    .unwrap_or(text)
                    .parse::<i32>()
                    .ok()
            }
        }))
}

fn midi_channel_param() -> IntParam {
    IntParam::new("MIDI channel", 0, IntRange::Linear { min: 0, max: 16 })
        .with_value_to_string(Arc::new(|value| {
            if value == 0 {
                "Kit".to_owned()
            } else {
                format!("Ch {value}")
            }
        }))
        .with_string_to_value(Arc::new(|text| {
            let text = text.trim();
            if text.eq_ignore_ascii_case("Kit") {
                Some(0)
            } else {
                text.strip_prefix("Ch ").unwrap_or(text).parse::<i32>().ok()
            }
        }))
}

/// A general control: bipolar, zero the model's reference, read as a percentage.
fn deviation(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -1.0,
            max: 1.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(15.0))
    .with_value_to_string(formatters::v2s_f32_percentage(0))
    .with_string_to_value(formatters::s2v_f32_percentage())
}

fn gain(name: &str, default_db: f32) -> FloatParam {
    FloatParam::new(
        name,
        util::db_to_gain(default_db),
        FloatRange::Skewed {
            min: util::db_to_gain(-60.0),
            max: util::db_to_gain(6.0),
            factor: FloatRange::gain_skew_factor(-60.0, 6.0),
        },
    )
    .with_smoother(SmoothingStyle::Logarithmic(15.0))
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
    .with_string_to_value(formatters::s2v_f32_gain_to_db())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::SourceChoice;
    use nice_plug::params::Param;

    #[test]
    fn init_fills_only_completed_rows_of_the_final_kit() {
        let params = MxmDrumMachineParams::default();
        let expected = [1, 17, 2, 18, 3, 20, 7, 22, 12, 23, 13, 15, 16, 14, 26, 27];
        for (slot, expected) in params.slots.iter().zip(expected) {
            assert_eq!(slot.model_id(), ModelId::new(expected));
        }
        for slot in &params.slots {
            assert!(!slot.mute.value());
            assert!(!slot.solo.value());
            assert_eq!(slot.output.value(), 0);
            // Nothing chokes unless the user assigns it, so Init ships every slot ungrouped.
            assert_eq!(slot.choke_group.value(), 0, "Init assigned a choke group");
            assert_eq!(slot.midi_channel.value(), 0);
            for amount in slot.controls.all().into_iter().chain([&slot.pan]) {
                assert_eq!(amount.default_plain_value(), 0.0);
            }
            for route in slot.routes.all() {
                assert_eq!(route.source.value(), SourceChoice::Off);
                assert_eq!(route.target.value(), 0);
                assert_eq!(route.amount.default_plain_value(), 0.0);
            }
        }
    }

    /// **651 IDs, every one, unique**: forty a slot and eleven kit-wide (the owner's ruling of
    /// 2026-09-30, model-drums' scheme). The grid it replaced held 3,227.
    #[test]
    fn the_ids_are_complete_unique_and_651() {
        let params = MxmDrumMachineParams::default();
        let ids: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 651);
        let mut want: Vec<&str> = (0..SLOT_COUNT).flat_map(|s| *slot_ids(s)).collect();
        want.extend(GLOBAL_IDS);
        assert_eq!(want.len(), 651);
        for id in &want {
            assert!(ids.iter().any(|i| i == id), "missing {id}");
        }
        for (index, id) in ids.iter().enumerate() {
            assert_eq!(
                ids.iter().position(|other| other == id),
                Some(index),
                "duplicate {id}"
            );
        }
    }

    #[test]
    fn the_positions_name_their_parameters() {
        let ids = slot_ids(4);
        assert_eq!(ids[at::MODEL], "model_5");
        assert_eq!(ids[at::control(1)], "c01_5");
        assert_eq!(ids[at::control(20)], "c20_5");
        assert_eq!(ids[at::LEVEL], "level_5");
        assert_eq!(ids[at::PAN], "pan_5");
        assert_eq!(ids[at::MUTE], "mute_5");
        assert_eq!(ids[at::SOLO], "solo_5");
        assert_eq!(ids[at::CHOKE_GROUP], "choke_group_5");
        assert_eq!(ids[at::OUTPUT], "output_5");
        assert_eq!(ids[at::MIDI_CHANNEL], "midi_channel_5");
        assert_eq!(ids[at::source(3)], "route4_source_5");
        assert_eq!(ids[at::target(3)], "route4_target_5");
        assert_eq!(ids[at::amount(3)], "route4_amount_5");
    }

    /// **The mapping keeps each control as it was** (the owner, 2026-10-07): Tune its ±24 semitones
    /// and its reading, every other control the bipolar percentage it had, each smoothed alike, and
    /// the host names them by number.
    #[test]
    fn each_control_keeps_its_range_reading_and_default() {
        let controls = ControlParams::default();
        for (index, param) in controls.all().into_iter().enumerate() {
            let k = index + 1;
            assert_eq!(param.name(), format!("Control {k}"));
            assert_eq!(param.default_plain_value(), 0.0);
            if k == control::TUNE {
                assert_eq!(
                    (param.preview_plain(0.0), param.preview_plain(1.0)),
                    (-24.0, 24.0)
                );
                assert_eq!(param.unit(), " st");
            } else {
                assert_eq!(
                    (param.preview_plain(0.0), param.preview_plain(1.0)),
                    (-1.0, 1.0)
                );
            }
        }
        for k in 1..=CONTROLS {
            assert!(std::ptr::eq(controls.get(k), controls.all()[k - 1]));
        }
    }

    #[test]
    fn model_text_round_trips_assigned_and_unavailable_ids() {
        let param = model_param(1);
        for id in [0, 1, 2, 95, 255] {
            let normalised = param.preview_normalized(id);
            let text = param.normalized_value_to_string(normalised, true);
            let parsed = param
                .string_to_normalized_value(&text)
                .unwrap_or_else(|| panic!("could not parse {text:?}"));
            assert_eq!(param.preview_plain(parsed), id, "{text}");
        }
    }

    #[test]
    fn synced_rate_text_selects_every_musical_division() {
        let param = lfo_rate("LFO rate", 1.0);
        for &division in LFO_SYNC.span.divisions() {
            let parsed = param
                .string_to_normalized_value(division.label())
                .unwrap_or_else(|| panic!("could not parse {}", division.label()));
            assert_eq!(LFO_SYNC.pick(parsed), division, "{}", division.label());
        }
    }

    #[test]
    fn output_text_accepts_the_port_names_as_well_as_numbers() {
        let param = output_param();
        for (text, output) in [
            ("L+R", 0),
            ("main", 0),
            ("3", 3),
            ("Output 3", 3),
            ("Out 3", 3),
            ("Slot 03", 3),
            ("Slot 16", 16),
        ] {
            let parsed = param
                .string_to_normalized_value(text)
                .unwrap_or_else(|| panic!("could not parse {text:?}"));
            assert_eq!(param.preview_plain(parsed), output, "{text}");
        }
    }
}
