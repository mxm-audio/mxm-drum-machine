//! Permanent CLAP parameter surface.
//!
//! nice-plug's nested-array convention makes the IDs `<base>_<one-based slot>`: `model_1` through
//! `solo_16`. The model's plain domain is fixed at 0…255; ID 0 remains legacy silence while all 94
//! admitted sounding IDs are assigned.

use std::sync::{Arc, RwLock};

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, MODEL_ID_MAX, ModelId, available};
use nice_plug::prelude::*;

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

#[derive(Params)]
pub struct SlotParams {
    #[id = "model"]
    pub model: IntParam,
    #[id = "pitch"]
    pub pitch: FloatParam,
    /// Additive compatibility surface: zero preserves the calibrated circuit excursion.
    #[id = "pitch_env"]
    pub pitch_env: FloatParam,
    /// Additive compatibility surface: zero preserves the calibrated excursion time.
    #[id = "pitch_decay"]
    pub pitch_decay: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "tone"]
    pub tone: FloatParam,
    #[id = "body"]
    pub body: FloatParam,
    #[id = "noise"]
    pub noise: FloatParam,
    /// Separate from body Decay wherever the circuit has a distinct wire/noise envelope.
    #[id = "noise_decay"]
    pub noise_decay: FloatParam,
    #[id = "character"]
    pub character: FloatParam,
    #[id = "dynamics"]
    pub dynamics: FloatParam,
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
    /// Instance routing: 0 is main L+R, 1…16 are mono individual outputs.
    #[id = "output"]
    pub output: IntParam,
    /// Exclusive choke group: 0 is none, 1…16 cut every other slot in the same group. Nothing
    /// chokes unless it is assigned here — the source machines' own hat pairs are not wired
    /// together behind the user's back, so any two slots can be paired instead.
    #[id = "choke_group"]
    pub choke_group: IntParam,
    /// Instance note mapping: 0 is Kit, 1…16 claim the corresponding chromatic channel.
    #[id = "midi_channel"]
    pub midi_channel: IntParam,
    /// This slot's modulation topology and amounts. The LFO generators themselves are kit-wide.
    #[nested]
    pub routes: Box<crate::routes::Routes>,
}

impl SlotParams {
    fn new(index: usize) -> Self {
        // The brief's final sixteen-slot Init: every assignment is a completed circuit.
        const INIT_MODELS: [u8; SLOT_COUNT] =
            [1, 17, 2, 18, 3, 20, 7, 22, 12, 23, 13, 15, 16, 14, 26, 27];
        let default_model = INIT_MODELS[index];
        Self {
            model: model_param(default_model),
            pitch: FloatParam::new(
                "Pitch",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(15.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            pitch_env: deviation("Pitch envelope"),
            pitch_decay: deviation("Pitch decay"),
            decay: deviation("Decay"),
            attack: deviation("Attack"),
            tone: deviation("Tone"),
            body: deviation("Body"),
            noise: deviation("Noise"),
            noise_decay: deviation("Noise decay"),
            character: deviation("Character"),
            dynamics: deviation("Dynamics"),
            level: gain("Level", 0.0),
            pan: FloatParam::new(
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
            })),
            mute: BoolParam::new("Mute", false),
            solo: BoolParam::new("Solo", false),
            output: output_param(),
            choke_group: choke_group_param(),
            midi_channel: midi_channel_param(),
            routes: Box::new(crate::routes::Routes::default()),
        }
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
    /// readings of Pitch and Decay reuse the per-slot axes that already exist.
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
            for amount in [
                &slot.pitch,
                &slot.pitch_env,
                &slot.pitch_decay,
                &slot.decay,
                &slot.attack,
                &slot.tone,
                &slot.body,
                &slot.noise,
                &slot.noise_decay,
                &slot.character,
                &slot.dynamics,
                &slot.pan,
            ] {
                assert_eq!(amount.default_plain_value(), 0.0);
            }
        }
    }

    #[test]
    fn nested_slot_ids_are_complete_unique_and_stable() {
        let params = MxmDrumMachineParams::default();
        let ids: Vec<_> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), SLOT_COUNT * (19 + 13 * 7 * 2) + 11);
        for slot in 1..=SLOT_COUNT {
            for base in [
                "model",
                "pitch",
                "pitch_env",
                "pitch_decay",
                "decay",
                "attack",
                "tone",
                "body",
                "noise",
                "noise_decay",
                "character",
                "dynamics",
                "level",
                "pan",
                "mute",
                "solo",
                "output",
                "choke_group",
                "midi_channel",
            ] {
                assert!(
                    ids.contains(&format!("{base}_{slot}")),
                    "missing {base}_{slot}"
                );
            }
        }
        for id in [
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
        ] {
            assert!(ids.contains(&id.to_owned()), "missing {id}");
        }
        for retired in ["lfo1_division", "lfo2_division", "lfo3_division"] {
            assert!(!ids.contains(&retired.to_owned()), "retired {retired}");
        }
        for slot in crate::routes::ROUTE_IDS {
            for target in slot {
                for (amount, presence) in target {
                    assert!(ids.contains(&amount.to_owned()), "missing {amount}");
                    assert!(ids.contains(&presence.to_owned()), "missing {presence}");
                }
            }
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
