//! Collection preset integration and the source-family audition sets.
//!
//! Each factory set puts models from exactly one source machine into canonical role slots and mutes
//! absent roles. Preset names use recognisable numeric source tokens without manufacturer names;
//! the exact mapping is documented in this plugin's README and proved against the permanent model-ID
//! ranges below.

use std::sync::RwLock;

use mxm_preset::{Instrument, PresetIdentity};

use crate::params::MxmDrumMachineParams;

pub use mxm_preset::Library;

impl Instrument for MxmDrumMachineParams {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out = Vec::with_capacity(16 * (19 + 13 * 7 * 2) + 11);
        for (slot, params) in self.slots.iter().enumerate() {
            out.extend([
                (
                    crate::editor::MODEL_IDS[slot],
                    &params.model as &dyn mxm_preset::ErasedParam,
                ),
                (crate::editor::PITCH_IDS[slot], &params.pitch),
                (crate::editor::PITCH_ENV_IDS[slot], &params.pitch_env),
                (crate::editor::PITCH_DECAY_IDS[slot], &params.pitch_decay),
                (crate::editor::DECAY_IDS[slot], &params.decay),
                (crate::editor::ATTACK_IDS[slot], &params.attack),
                (crate::editor::TONE_IDS[slot], &params.tone),
                (crate::editor::BODY_IDS[slot], &params.body),
                (crate::editor::NOISE_IDS[slot], &params.noise),
                (crate::editor::NOISE_DECAY_IDS[slot], &params.noise_decay),
                (crate::editor::CHARACTER_IDS[slot], &params.character),
                (crate::editor::DYNAMICS_IDS[slot], &params.dynamics),
                (crate::editor::LEVEL_IDS[slot], &params.level),
                (crate::editor::PAN_IDS[slot], &params.pan),
                (crate::editor::MUTE_IDS[slot], &params.mute),
                (crate::editor::SOLO_IDS[slot], &params.solo),
                (crate::editor::OUTPUT_IDS[slot], &params.output),
                (crate::editor::CHOKE_GROUP_IDS[slot], &params.choke_group),
                (crate::editor::MIDI_CHANNEL_IDS[slot], &params.midi_channel),
            ]);
            out.extend(params.routes.parameters(slot));
        }
        out.extend([
            ("master", &self.master as &dyn mxm_preset::ErasedParam),
            ("lfo1_rate", &self.lfo1_rate),
            ("lfo1_shape", &self.lfo1_shape),
            ("lfo1_sync", &self.lfo1_sync),
            ("lfo2_rate", &self.lfo2_rate),
            ("lfo2_shape", &self.lfo2_shape),
            ("lfo2_sync", &self.lfo2_sync),
            ("lfo3_rate", &self.lfo3_rate),
            ("lfo3_shape", &self.lfo3_shape),
            ("lfo3_sync", &self.lfo3_sync),
            ("resample", &self.resample),
        ]);
        out
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }

    fn is_instance_setting(&self, id: &str) -> bool {
        // Resample joins the routing and channel settings: host and project state remember it,
        // but no kit engages or disengages the mode. Loading a kit while frozen changes the kit
        // and leaves the mode alone, which is what a person expects — and it keeps the factory
        // bank measuring kits rather than the capture path.
        id.starts_with("output_") || id.starts_with("midi_channel_") || id == "resample"
    }

    fn default_missing_legacy_parameter(&self, id: &str) -> bool {
        crate::routes::is_route_parameter(id)
    }

    fn omit_default_parameter(&self, id: &str) -> bool {
        crate::routes::is_route_parameter(id)
    }

    fn omit_captured_parameter(&self, id: &str) -> bool {
        self.slots
            .iter()
            .enumerate()
            .any(|(slot, params)| params.routes.omit_captured_parameter(slot, id))
    }
}

pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Bridge 808", include_str!("../presets/bridge-808.json")),
    ("Reset 909", include_str!("../presets/reset-909.json")),
    ("Economy 55", include_str!("../presets/economy-55.json")),
    (
        "Expanded 8000",
        include_str!("../presets/expanded-8000.json"),
    ),
    ("Compact 606", include_str!("../presets/compact-606.json")),
    ("Snap 110", include_str!("../presets/snap-110.json")),
    ("Classic 78", include_str!("../presets/classic-78.json")),
    ("Discrete 66", include_str!("../presets/discrete-66.json")),
    ("Early 2L", include_str!("../presets/early-2l.json")),
];

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_preset::{Category, Preset, Value, factory};
    use nice_plug::params::Param;
    use nice_plug::prelude::Params;

    struct Design {
        name: &'static str,
        models: &'static [u8],
    }

    // The family presets are audition mixes, not sixteen equally foregrounded solo renders. Kick
    // and snare establish the reference plane; pitched drums sit one step back, short percussion
    // farther back, and sustained high-frequency metal lower again.
    const ROLE_LEVEL_DB: [f32; 16] = [
        0.0, 0.0, -6.0, -6.0, -6.0, -6.0, -6.0, -6.0, -8.0, -6.0, -10.0, -10.0, -12.0, -8.0, -8.0,
        -10.0,
    ];

    const DESIGNS: &[Design] = &[
        Design {
            name: "Bridge 808",
            // Kick, snare, low/mid/high tom, low/mid/high conga, rim, clap, closed/open hat,
            // cymbal, cowbell, clave, maraca.
            models: &[1, 2, 3, 5, 7, 4, 6, 8, 9, 12, 15, 16, 14, 13, 10, 11],
        },
        Design {
            name: "Reset 909",
            models: &[17, 18, 19, 20, 21, 0, 0, 0, 22, 23, 24, 25, 26, 0, 0, 27],
        },
        Design {
            name: "Economy 55",
            models: &[28, 29, 0, 0, 0, 0, 0, 0, 30, 0, 31, 0, 0, 0, 0, 0],
        },
        Design {
            name: "Expanded 8000",
            models: &[
                32, 33, 34, 35, 36, 37, 38, 39, 40, 44, 42, 43, 41, 46, 45, 0,
            ],
        },
        Design {
            name: "Compact 606",
            models: &[47, 48, 49, 0, 50, 0, 0, 0, 0, 0, 52, 53, 51, 0, 0, 0],
        },
        Design {
            name: "Snap 110",
            models: &[54, 55, 0, 0, 0, 0, 0, 0, 0, 59, 57, 58, 56, 0, 0, 0],
        },
        Design {
            name: "Classic 78",
            models: &[60, 61, 69, 68, 67, 73, 72, 71, 62, 0, 63, 0, 64, 70, 66, 65],
        },
        Design {
            name: "Discrete 66",
            models: &[74, 81, 75, 76, 77, 0, 0, 0, 79, 0, 82, 0, 84, 78, 80, 83],
        },
        Design {
            name: "Early 2L",
            models: &[85, 91, 86, 87, 88, 0, 0, 0, 0, 94, 0, 0, 92, 89, 90, 93],
        },
    ];

    fn generated(params: &MxmDrumMachineParams, design: &Design) -> Preset {
        let mut preset = Preset::capture("", Category::Template, params);
        preset.name = design.name.to_owned();
        preset.category = Category::Template;
        let fallback = design
            .models
            .iter()
            .copied()
            .find(|model| *model != 0)
            .expect("every family has at least one model");
        for (slot, parameter) in params.slots.iter().enumerate() {
            let requested = design.models.get(slot).copied().unwrap_or(0);
            let model = i32::from(if requested == 0 { fallback } else { requested });
            let model_value = parameter.model.preview_normalized(model);
            preset.params.insert(
                crate::editor::MODEL_IDS[slot].to_owned(),
                Value {
                    v: model_value,
                    text: (&parameter.model as &dyn mxm_preset::ErasedParam).format(model_value),
                },
            );
            let mute_value = if requested == 0 { 1.0 } else { 0.0 };
            preset.params.insert(
                crate::editor::MUTE_IDS[slot].to_owned(),
                Value {
                    v: mute_value,
                    text: (&parameter.mute as &dyn mxm_preset::ErasedParam).format(mute_value),
                },
            );
            let level_value = parameter
                .level
                .preview_normalized(nice_plug::prelude::util::db_to_gain(ROLE_LEVEL_DB[slot]));
            preset.params.insert(
                crate::editor::LEVEL_IDS[slot].to_owned(),
                Value {
                    v: level_value,
                    text: (&parameter.level as &dyn mxm_preset::ErasedParam).format(level_value),
                },
            );
        }
        preset
    }

    #[test]
    #[ignore = "writes the factory preset files"]
    fn write_the_factory_presets() {
        let params = MxmDrumMachineParams::default();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
        std::fs::create_dir_all(&dir).expect("create preset directory");
        for design in DESIGNS {
            let slug = design.name.to_lowercase().replace(' ', "-");
            let path = dir.join(format!("{slug}.json"));
            std::fs::write(&path, generated(&params, design).to_json()).expect("write preset");
            eprintln!("wrote {}", path.display());
        }
    }

    #[test]
    fn factory_files_match_the_machine_family_designs() {
        let params = MxmDrumMachineParams::default();
        assert_eq!(FACTORY_FILES.len(), DESIGNS.len());
        for design in DESIGNS {
            let (_, text) = FACTORY_FILES
                .iter()
                .find(|(name, _)| *name == design.name)
                .unwrap_or_else(|| panic!("missing {:?}", design.name));
            let shipped = Preset::parse(text, crate::CLAP_ID).expect("factory preset parses");
            assert_eq!(shipped, generated(&params, design), "{:?}", design.name);
        }
    }

    #[test]
    fn factory_mix_keeps_kick_and_snare_above_every_supporting_role() {
        let params = MxmDrumMachineParams::default();
        for design in DESIGNS {
            let preset = generated(&params, design);
            for (slot, expected_db) in ROLE_LEVEL_DB.into_iter().enumerate() {
                let stored = preset
                    .params
                    .get(crate::editor::LEVEL_IDS[slot])
                    .unwrap_or_else(|| panic!("{} is missing level {}", design.name, slot + 1));
                let gain = params.slots[slot].level.preview_plain(stored.v);
                let actual_db = nice_plug::prelude::util::gain_to_db(gain);
                assert!((actual_db - expected_db).abs() < 0.01);
                if slot >= 2 {
                    assert!(
                        actual_db < 0.0,
                        "{} role {} is not subordinate",
                        design.name,
                        slot + 1
                    );
                }
            }
        }
    }

    #[test]
    fn each_machine_family_is_whole_exclusive_and_every_model_appears_once() {
        let mut seen = [false; 95];
        for design in DESIGNS {
            assert_eq!(
                design.models.len(),
                16,
                "{} must define every key",
                design.name
            );
            for &id in design.models {
                assert!(id <= 94, "{} contains ID {id}", design.name);
                if id != 0 {
                    assert!(!seen[usize::from(id)], "model ID {id} appears twice");
                    seen[usize::from(id)] = true;
                }
            }
        }
        assert!(seen[1..].iter().all(|seen| *seen));
    }

    #[test]
    fn preset_order_matches_the_model_selectors_family_order() {
        let mut groups = Vec::new();
        for spec in &mxm_drum_machine_dsp::model::AVAILABLE_MODELS {
            if groups.last().copied() != Some(spec.group) {
                groups.push(spec.group);
            }
        }
        let presets: Vec<_> = FACTORY_FILES.iter().map(|(name, _)| *name).collect();
        assert_eq!(presets, groups);
    }

    #[test]
    fn every_factory_preset_is_complete_categorised_and_listed_after_init() {
        let params = MxmDrumMachineParams::default();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("factory preset parses");
            assert_eq!(preset.name, *name);
            assert_eq!(preset.category, Category::Template);
            assert!(preset.resolve(&params).1.is_empty(), "{name} is incomplete");
        }
        let all = factory(&params);
        assert_eq!(all.len(), FACTORY_FILES.len() + 1);
        assert_eq!(all[0].name, mxm_preset::INIT_NAME);
    }

    #[test]
    fn every_host_parameter_is_declared_once_for_presets() {
        let params = MxmDrumMachineParams::default();
        let mut declared: Vec<_> = params.parameters().into_iter().map(|(id, _)| id).collect();
        let mut host: Vec<_> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        declared.sort_unstable();
        host.sort_unstable();
        assert_eq!(declared, host);
        declared.dedup();
        assert_eq!(declared.len(), 16 * (19 + 13 * 7 * 2) + 11);
    }

    #[test]
    fn route_assignment_and_amount_capture_sparsely_without_conflating_zero() {
        use nice_plug::params::InternalParamMut;

        let params = MxmDrumMachineParams::default();
        let (amount_id, presence_id) = crate::routes::ROUTE_IDS[0]
            [mxm_drum_machine_dsp::routing::target::PITCH]
            [mxm_drum_machine_dsp::routing::source::LFO1];
        let sparse = Preset::capture("Sparse", Category::Template, &params);
        let init = Preset::init(&params);
        assert!(!sparse.params.contains_key(amount_id));
        assert!(!sparse.params.contains_key(presence_id));
        assert!(!init.params.contains_key(amount_id));
        assert!(!init.params.contains_key(presence_id));

        // Assignment at exactly zero remains explicit while its default amount costs no file data.
        unsafe {
            params.slots[0]
                .routes
                .pitch
                .lfo1_on
                ._internal_set_plain_value(true)
        };
        let zero = Preset::capture("Zero", Category::Template, &params);
        assert!(zero.params.contains_key(presence_id));
        assert!(!zero.params.contains_key(amount_id));

        unsafe {
            params.slots[0]
                .routes
                .pitch
                .lfo1
                ._internal_set_plain_value(0.5)
        };
        let active = Preset::capture("Active", Category::Template, &params);
        assert!(active.params.contains_key(presence_id));
        assert!(active.params.contains_key(amount_id));

        // Once unassigned, even a dormant nonzero amount is irrelevant and omitted.
        unsafe {
            params.slots[0]
                .routes
                .pitch
                .lfo1_on
                ._internal_set_plain_value(false)
        };
        let absent = Preset::capture("Absent", Category::Template, &params);
        assert!(!absent.params.contains_key(presence_id));
        assert!(!absent.params.contains_key(amount_id));

        let (writes, problems) = sparse.resolve(&params);
        assert!(problems.is_empty(), "{problems:?}");
        for id in [presence_id, amount_id] {
            let (_, parameter, value) = writes
                .iter()
                .find(|(candidate, _, _)| *candidate == id)
                .unwrap_or_else(|| panic!("missing default write for {id}"));
            assert_eq!(*value, parameter.default_normalised());
        }
    }

    #[test]
    fn a_kit_carries_choke_groups_while_output_routing_stays_with_the_instance() {
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        for (index, slot) in params.slots.iter().enumerate() {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                slot.choke_group
                    ._internal_set_plain_value(index as i32 % 16 + 1);
                slot.output._internal_set_plain_value(5);
                slot.midi_channel._internal_set_plain_value(6);
            }
        }
        let saved = Preset::capture("Kit", Category::Template, &params);
        for slot in 1..=16 {
            // Which slots cut each other is sound design, so a kit owns it.
            assert!(
                saved.params.contains_key(&format!("choke_group_{slot}")),
                "a saved kit dropped choke_group_{slot}"
            );
            // Output routing and channel belong to the project, not the sound.
            assert!(!saved.params.contains_key(&format!("output_{slot}")));
            assert!(!saved.params.contains_key(&format!("midi_channel_{slot}")));
        }
        // And Init puts them all back to Off rather than leaving the instance's assignments.
        let init = Preset::init(&params);
        for slot in 1..=16 {
            let entry = init
                .params
                .get(&format!("choke_group_{slot}"))
                .expect("Init declares every choke group");
            assert_eq!(entry.text, "Off", "Init left slot {slot} grouped");
        }
    }

    #[test]
    fn routing_and_channels_are_instance_settings_no_kit_writes_or_dirties() {
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        let instance: Vec<_> = params
            .parameters()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| params.is_instance_setting(id))
            .collect();
        // The sixteen outputs, the sixteen MIDI channels, and Resample.
        assert_eq!(instance.len(), 2 * 16 + 1);
        for slot in &params.slots {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                slot.output._internal_set_plain_value(7);
                slot.midi_channel._internal_set_plain_value(3);
            }
        }

        let saved = Preset::capture("Kit", Category::Template, &params);
        let init = Preset::init(&params);
        for id in &instance {
            assert!(!saved.params.contains_key(*id), "a saved kit stores {id}");
            assert!(!init.params.contains_key(*id), "Init stores {id}");
        }
        // Stepping through the bank applies each kit, Init first.
        for preset in factory(&params) {
            let (writes, missing) = preset.resolve(&params);
            assert!(missing.is_empty(), "{}: {missing:?}", preset.name);
            assert!(
                writes
                    .iter()
                    .all(|(id, _, _)| !params.is_instance_setting(id)),
                "{} rewires the instance",
                preset.name
            );
        }
        // A file that names one anyway still cannot apply it.
        let mut edited = saved.clone();
        edited.params.insert(
            "output_1".to_owned(),
            Value {
                v: 0.0,
                text: "L+R".to_owned(),
            },
        );
        let (writes, missing) = edited.resolve(&params);
        assert!(missing.is_empty(), "{missing:?}");
        assert!(writes.iter().all(|(id, _, _)| *id != "output_1"));

        mxm_preset::mark_loaded(&params, "Kit", mxm_preset::Origin::Factory);
        // SAFETY: same exclusive test ownership.
        unsafe {
            params.slots[2].output._internal_set_plain_value(0);
            params.slots[2].midi_channel._internal_set_plain_value(0);
        }
        assert!(!mxm_preset::loaded(&params).is_modified());
        // SAFETY: same exclusive test ownership.
        unsafe { params.slots[2].mute._internal_set_plain_value(true) };
        assert!(mxm_preset::loaded(&params).is_modified());
    }

    /// A host that applies each parameter write at once, as a GUI gesture reaches the plugin.
    struct ApplyingHost;

    impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
        fn plugin_api(&self) -> nice_plug::prelude::PluginApi {
            nice_plug::prelude::PluginApi::Clap
        }

        unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}

        unsafe fn raw_set_parameter_normalized(
            &self,
            param: nice_plug::params::internals::ParamPtr,
            normalized: f32,
        ) {
            // SAFETY: the test owns the parameters and no other thread touches them.
            unsafe {
                param._internal_set_normalized_value(normalized);
            }
        }

        unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}

        fn get_state(&self) -> nice_plug::prelude::PluginState {
            nice_plug::prelude::PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }

        fn set_state(&self, _state: nice_plug::prelude::PluginState) {}
    }

    #[test]
    fn applying_every_kit_and_init_through_the_write_funnels_leaves_routing_alone() {
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        for slot in &params.slots {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                slot.output._internal_set_plain_value(7);
                slot.midi_channel._internal_set_plain_value(3);
            }
        }
        let untouched = |params: &MxmDrumMachineParams| {
            params
                .slots
                .iter()
                .all(|slot| slot.output.value() == 7 && slot.midi_channel.value() == 3)
        };
        let host = ApplyingHost;
        let setter = nice_plug::prelude::ParamSetter::new(&host);

        let bank = factory(&params);
        for preset in &bank {
            let (applied, problems) =
                mxm_preset::ui::apply_preset_checked(&params, &setter, preset);
            assert!(
                applied && problems.is_empty(),
                "{}: {problems:?}",
                preset.name
            );
            assert!(untouched(&params), "{} rewired the instance", preset.name);
        }
        // The funnel really wrote the sound: the last kit's first model is in place.
        assert_eq!(bank.last().unwrap().name, "Early 2L");
        assert_eq!(params.slots[0].model.value(), 85);

        mxm_preset::ui::init_patch(&params, &setter);
        assert!(untouched(&params), "Init rewired the instance");
        assert_eq!(
            params.slots[0].model.value(),
            1,
            "Init did not write the sound"
        );
    }

    #[test]
    fn init_is_exactly_the_parameter_defaults() {
        let params = MxmDrumMachineParams::default();
        for (id, parameter) in params.parameters() {
            assert_eq!(
                parameter.normalised(),
                parameter.default_normalised(),
                "{id} does not start at its host default"
            );
        }
    }
}
