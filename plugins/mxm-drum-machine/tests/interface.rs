use std::collections::{HashMap, HashSet};

use mxm_drum_machine::editor::{page_items, panel};
use mxm_drum_machine::params::MxmDrumMachineParams;
use mxm_drum_machine::telemetry::Telemetry;
use nice_plug::context::gui::GuiContextInner;
use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::{ParamSetter, PluginApi, PluginState};

struct NoHost;

impl GuiContextInner for NoHost {
    // A test double has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

/// The paging items as the editor computes them for `params` with slot 1 selected, in a context set
/// up as an editor's is — three passes in, so the weighted font cuts are bound. Each floor is
/// computed from its card's tree, so there is no list of them to read.
fn page_items_for(params: &MxmDrumMachineParams) -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, params, 0);
        });
        output.textures_delta.clear();
    }
    items
}

fn collect_placed(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
    match shape {
        egui::Shape::Text(text) => out.push((text.galley.text().to_owned(), text.pos)),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_placed(shape, out);
            }
        }
        _ => {}
    }
}

fn collect_text(shape: &egui::Shape, words: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => words.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, words);
            }
        }
        _ => {}
    }
}

fn paint_every_card() -> HashSet<String> {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);

    let params = MxmDrumMachineParams::default();
    let telemetry = Telemetry::default();
    let host = NoHost;
    let setter = ParamSetter::new(&host);
    let mut selected = 0;
    let mut entries = HashMap::new();
    let mut presets = mxm_drum_machine::editor::PresetUi::at(
        mxm_drum_machine::preset::Library::at(None),
        &params,
    );
    let mut nav = mxm_ui::navigation::State::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 850.0),
        )),
        ..Default::default()
    };

    let mut words = HashSet::new();
    for item in page_items_for(&params) {
        mxm_ui::paging::editor::request_card(&ctx, item.key);
        for pass in 0..4 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut selected,
                    &mut entries,
                    &mut presets,
                    &mut nav,
                    // Headless: no executor, so Export draws disabled.
                    None,
                );
            });
            if pass == 3 {
                for clipped in &output.shapes {
                    let mut frame_words = Vec::new();
                    collect_text(&clipped.shape, &mut frame_words);
                    words.extend(frame_words);
                }
            }
            output.textures_delta.clear();
        }
    }
    words
}

#[test]
fn every_card_and_every_control_is_painted_on_some_derived_page() {
    let words = paint_every_card();
    for expected in [
        "Slots 1–8",
        "Slots 9–16",
        "LFOs",
        // The LFO number rides on the Rate knob rather than a heading (owner, 2026-09-22), which
        // is what took the tallest card in the instrument down by three rows.
        "Rate 1",
        "Rate 2",
        "Rate 3",
        "Model",
        "Excitation",
        "Body",
        "Envelopes & tone",
        "Deep bridge kick",
        "Tune",
        "Pitch envelope",
        "Pitch decay",
        "Attack",
        "Dynamics",
        "Noise",
        "Noise decay",
        "Character",
        "Decay",
        "Tone",
        "Level",
        "Pan",
        "Output",
        "Choke group",
        "MIDI channel",
        "L+R",
        "Kit",
        "M",
        "S",
        "Master",
    ] {
        assert!(
            words.contains(expected),
            "missing {expected:?}; painted {words:?}"
        );
    }
}

#[test]
fn authored_cards_have_unique_stable_keys_and_fit_the_minimum_card_width() {
    let items = page_items_for(&MxmDrumMachineParams::default());
    assert_eq!(items.len(), 8);
    for (index, item) in items.iter().enumerate() {
        assert_eq!(item.key, mxm_ui::paging::Key(index as u64));
        // Against the width a card is actually given, not the window's: the workspace spends its
        // gutters first, and comparing with the window hid a card 32 pt too wide for years.
        assert!(
            item.card.floor <= mxm_drum_machine::editor::MINIMUM_CARD_WIDTH,
            "{} has a {} pt floor but the minimum window gives a card {} pt",
            item.card.title,
            item.card.floor,
            mxm_drum_machine::editor::MINIMUM_CARD_WIDTH
        );
        assert_eq!(
            items.iter().filter(|other| other.key == item.key).count(),
            1,
            "duplicate card key"
        );
    }
}

/// Every slot row's Mute and Solo start at the same x, so the card reads as columns.
///
/// The owner's report of 2026-09-22 — the slots looked "thrown in with a pitch fork". `add_sized`
/// is a floor and not a ceiling, so a long model name grew its button past the width the row
/// computes and shoved that row's M and S out of line with the others. The name button truncates
/// now; this is what says so.
#[test]
fn every_slot_rows_mute_and_solo_line_up() {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);

    let params = MxmDrumMachineParams::default();
    let telemetry = Telemetry::default();
    let host = NoHost;
    let setter = ParamSetter::new(&host);
    let mut selected = 0;
    let mut entries = HashMap::new();
    let mut presets = mxm_drum_machine::editor::PresetUi::at(
        mxm_drum_machine::preset::Library::at(None),
        &params,
    );
    let mut nav = mxm_ui::navigation::State::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 850.0),
        )),
        ..Default::default()
    };

    let slots = page_items_for(&params)[0].key;
    mxm_ui::paging::editor::request_card(&ctx, slots);
    let mut placed = Vec::new();
    for pass in 0..4 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            panel(
                ui,
                &params,
                &telemetry,
                &setter,
                &mut selected,
                &mut entries,
                &mut presets,
                &mut nav,
                None,
            );
        });
        if pass == 3 {
            placed.clear();
            for clipped in &output.shapes {
                collect_placed(&clipped.shape, &mut placed);
            }
        }
        output.textures_delta.clear();
    }

    for glyph in ["M", "S"] {
        let mut columns: Vec<f32> = placed
            .iter()
            .filter(|(text, _)| text == glyph)
            .map(|(_, pos)| pos.x)
            .collect();
        assert!(
            columns.len() >= 8,
            "expected at least eight {glyph:?} controls, painted {}",
            columns.len()
        );
        columns.sort_by(f32::total_cmp);
        // Two cards share the page, so there are two legitimate columns — but each must hold all
        // eight of its rows. Before the name button truncated, one card read
        // [210, 210, 210, 210, 210, 210, 210, 215] and the other sprayed across four positions.
        let within_card = columns.iter().filter(|x| **x - columns[0] < 1.0).count();
        assert!(
            within_card >= 8,
            "{glyph:?} columns are ragged: {columns:?}"
        );
    }
}
