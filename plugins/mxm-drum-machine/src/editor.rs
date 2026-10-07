//! Reflowing editor for the sixteen-slot vertical slice.
//!
//! **The panel names a control by the model; the host by its number** (mxm-model-drums' scheme, the
//! owner, 2026-09-30): a knob paints its model's name for it — Tune, Decay, Snappy — over a general
//! `Control k`, and each knob's routes are rows over the slot's four route slots, named as the knob
//! is. **A model shows only the controls its code reads** (the owner, 2026-10-07; [`controls`]).

pub mod binding;
pub mod controls;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, Capabilities, ModelId, available};
use mxm_ui::control::{self, ParamView, Size, Steps, Wave, Wheel};
use mxm_ui::space::{MIN_TARGET, SPACE_2, SPACE_3, SPACE_5};
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{
    self, Height, Kind, Node, group, leaf, pad, pad_all, row_gap, stack, stack_gap,
};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use self::binding::{
    Bound, selector as bound_selector, set_together, toggle_compact as bound_toggle_compact,
};
use crate::params::control as general;
use crate::params::{CONTROLS, MxmDrumMachineParams, SlotParams, at, slot_ids};
use crate::routes::{ROUTES, RouteParams, RouteTarget, SourceChoice};
use crate::telemetry::Telemetry;

pub use mxm_preset::PresetUi;

/// The opening size: the quarter-4K budget, hugged. MEASURED by
/// `the_opening_size_is_the_budget_hugged`, which fails with the size it should be — it was
/// (1600, 850) by hand, which opened 320 px narrower and 124 px taller than the panel wanted; it fell
/// again on 2026-10-07, when the kick at Init stopped drawing the Noise knobs it does not read.
const REFERENCE: (u32, u32) = (1755, 1042);
/// The smallest window this editor advertises: the wider of the widest card with the workspace's
/// gutters and the app bar at its last compact step, Resample and Export samples in its `…` menu.
/// The bar decides it, and `the_app_bar_holds_in_the_minimum_window` measures it; the card alone
/// needed 392.
const MINIMUM: (u32, u32) = (444, 320);

/// How much of the minimum window a card actually gets, once the workspace gutters are spent.
/// A card floor above this cannot fit at the advertised minimum, whatever the window says.
pub const MINIMUM_CARD_WIDTH: f32 = MINIMUM.0 as f32 - 2.0 * SPACE_5;
/// The keyboard cursor's card for the app bar's Master, outside the paging keys 0…7.
const MASTER_CARD: u64 = 64;
/// The same, for the app bar's Resample toggle.
const RESAMPLE_CARD: u64 = 65;
/// **The model button's minimum width**: how wide a slot's model name must be to read. The button
/// truncates to whatever it is given, so without a minimum of its own a slot card would be a column
/// nobody can read a kit from. The width is the owner's judgement — the compaction of 2026-09-18
/// set the slot cards at 290 points, where the longest names end in an ellipsis — held since R2 on
/// the control it protects rather than as a card minimum (`plans/plan-editor-standard.md` A2): 290
/// less the card's chrome and the row's number, Mute, Solo and spacing.
const SLOT_MODEL_MIN: f32 = 140.0;
/// The slot number's box, before the model button.
const SLOT_NUMBER_WIDTH: f32 = 24.0;
/// A slot row's frame margin, left and right.
const SLOT_MARGIN: f32 = 2.0;
/// The Model card's mechanism display is this tall and fills the card's width.
const MECHANISM_HEIGHT: f32 = 72.0;
/// The cards, in paging order.
const TITLES: [&str; 8] = [
    "Slots 1–8",
    "Slots 9–16",
    "LFOs",
    "Model",
    "Excitation",
    "Body",
    "Envelopes & tone",
    // The output stage ends the signal chain (design system §3.4), so it is the last card.
    "Output",
];
const PREVIEW_BINS: usize = 96;
const PREVIEW_SAMPLE_RATE: f32 = 48_000.0;
const PREVIEW_SECONDS: f32 = 2.0;
const PREVIEW_ATTACK_SECONDS: f32 = 0.080;
const PREVIEW_ATTACK_BINS: usize = 64;
/// The rate a sample pack is written at, whatever the host is running.
///
/// Deliberately fixed, and not the host's: a pack is for a tracker or a groovebox, several of
/// which resample anything that is not 44.1 or 48 on import, and a folder whose rate depends on
/// whichever session happened to export it is a trap. `kit.txt` records it either way.
const PACK_SAMPLE_RATE: f32 = 48_000.0;
/// The text on the export half of the app bar's Resample pair.
///
/// Named once because the pair is sized from it: the label used to be shorter than the width the
/// pair reserved, and when it grew the button overflowed its allocation, wrapped the app bar and
/// cost the window 335 pt of height. Measuring the string is what stops that happening again.
const EXPORT_LABEL: &str = "Export samples";
/// The export's label while its folder dialog is open.
const EXPORT_CHOOSING: &str = "Choosing…";
/// The export's help, on the bar's button and on its row in the `…` menu alike.
const EXPORT_HELP: &str = "Choose a folder and write this kit into it as numbered 24-bit WAV one-shots, for a tracker or a sampler. Turn Resample on first, so you hear them before they leave.";
/// Why the export cannot be used, on both forms.
const EXPORT_DISABLED: &str = "Turn Resample on first: a pack is the samples you are hearing.";
/// Resample's help, on the bar's toggle and on its row in the `…` menu alike.
const RESAMPLE_HELP: &str = "Plays every slot from a recording of itself; Tune, Decay and Soft hits still work. To change anything else, turn this off, edit, and turn it on again.";
/// The pair's rows in the app bar's `…` menu at the bar's last step ([`resample_menu_items`]).
const RESAMPLE_ITEM: usize = 0;
const EXPORT_ITEM: usize = 1;
/// The floor for each half of the Resample/Export pair, before either label is measured.
const RESAMPLE_MIN_WIDTH: f32 = 86.0;

/// How wide each half of the app-bar pair is drawn.
///
/// The widest of the two labels wins, so the two always match and neither ever overflows. It is
/// measured rather than guessed because mxm-kit's `crates/ui` (*The app bar*, in its `AGENTS.md`
/// and `NOTES.md`) requires a bar control to reserve its widest form — an under-reserved one does
/// not clip, it wraps the whole bar.
fn resample_pair_width(ui: &Ui, params: &MxmDrumMachineParams) -> f32 {
    let button = egui::TextStyle::Button.resolve(ui.style());
    let export = ui
        .painter()
        .layout_no_wrap(EXPORT_LABEL.to_owned(), button, egui::Color32::PLACEHOLDER)
        .size()
        .x;
    mxm_ui::control::toggle_min_width(ui, params.resample.name())
        .max(export + SPACE_5 * 2.0)
        .max(RESAMPLE_MIN_WIDTH)
}

#[derive(Clone)]
struct SoundPreview {
    signature: u64,
    low: [f32; PREVIEW_BINS],
    high: [f32; PREVIEW_BINS],
}

const OUTPUT_OPTIONS: [&str; 17] = [
    "L+R", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
];
const CHOKE_GROUP_OPTIONS: [&str; 17] = [
    "Off", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
];
/// The kit-wide output row's cells, in [`KitOutputs`] order.
const KIT_OUTPUT_OPTIONS: [&str; 2] = ["L+R", "Own outputs"];
/// The keyboard cursor's name for the kit-wide output row. Not a parameter id: the row writes the
/// sixteen `output_*` settings and stores nothing of its own.
const KIT_OUTPUTS_ID: &str = "outputs_all";
const MIDI_CHANNEL_OPTIONS: [&str; 17] = [
    "Kit", "Ch 1", "Ch 2", "Ch 3", "Ch 4", "Ch 5", "Ch 6", "Ch 7", "Ch 8", "Ch 9", "Ch 10",
    "Ch 11", "Ch 12", "Ch 13", "Ch 14", "Ch 15", "Ch 16",
];

pub type MxmDrumMachineEditor = nice_plug_egui::EguiEditor<MxmDrumMachineApp>;

pub fn create(
    params: Arc<MxmDrumMachineParams>,
    telemetry: Arc<Telemetry>,
    spawn: Spawn,
) -> Option<MxmDrumMachineEditor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );
    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: crate::NAME.to_owned(),
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmDrumMachineApp::new(params, telemetry, Some(spawn)),
    )
}

/// Submits a background task from the editor.
///
/// Export renders and writes files, so it belongs to the worker and never to an editor frame or
/// the audio thread. Boxing the executor keeps `panel` testable: a headless test passes `None`
/// and draws the same card without a host.
pub type Spawn = Arc<dyn Fn(crate::CaptureTask) + Send + Sync>;

pub struct MxmDrumMachineApp {
    params: Arc<MxmDrumMachineParams>,
    telemetry: Arc<Telemetry>,
    spawn: Option<Spawn>,
    gui_context: Option<GuiContext>,
    selected_slot: usize,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    nav: mxm_ui::navigation::State,
}

impl MxmDrumMachineApp {
    fn new(
        params: Arc<MxmDrumMachineParams>,
        telemetry: Arc<Telemetry>,
        spawn: Option<Spawn>,
    ) -> Self {
        let params_for_presets = Arc::clone(&params);
        Self {
            params,
            telemetry,
            spawn,
            gui_context: None,
            selected_slot: 0,
            text_entry: HashMap::new(),
            presets: PresetUi::new(params_for_presets.as_ref()),
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmDrumMachineApp {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&egui_ctx);
        mxm_ui::typography::apply(&egui_ctx);
        egui_ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(context) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &context.param_setter(),
            &mut self.selected_slot,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
            self.spawn.as_ref(),
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &MxmDrumMachineParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    selected_slot: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
    spawn: Option<&Spawn>,
) {
    let tokens = tokens_for(ui);
    // No capture is serviced here. `process` asks for one through telemetry and the plugin's own
    // capture thread answers, so that an instrument whose GUI is closed still re-renders when a
    // parameter moves — `capture_worker` has the history. Only export starts in a frame.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));

    if let Some(request) = telemetry.take_view_request()
        && let Some(category) = mxm_ui::paging::Category::from_request(request)
    {
        mxm_ui::paging::editor::request_category(ui.ctx(), category);
    }
    if let Some(theme) = telemetry.take_theme_request()
        && let Some(preference) = mxm_ui::theme::from_index(theme)
    {
        ui.ctx().set_theme(preference);
    }
    if let Some(open) = telemetry.take_browser_request() {
        presets.set_browser_open(open);
    }

    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[RESAMPLE_CARD, MASTER_CARD]);

    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        &tokens,
        |ui| mxm_preset::ui::preset_row(ui, &tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, &tokens, telemetry.take_peak(), telemetry.clipped()) {
                telemetry.clear_clip();
            }
            // Design system §3.1 slot 6: the whole kit's output control sits beside its meter,
            // not among one slot's controls.
            mxm_ui::navigation::bar_card(ui, MASTER_CARD, |ui| {
                ui.scope(|ui| {
                    Bound::new(
                        "master",
                        &params.master,
                        "Overall volume of the whole kit, on every output.",
                    )
                    .slider_inline(ui, &tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);
            mxm_ui::shell::editor_theme_control(ui);
            // Added last, so the right-to-left group puts the pair at its left edge.
            //
            // **Kit-wide, so it belongs here and not on a card** (owner, 2026-09-22). Every card
            // but the slot pickers and the LFOs is the *selected slot's*, so a global Resample
            // sitting on the Output card read as "resample this drum". It is the same reason
            // §3.1 slot 6 already keeps Master beside the meter rather than among one slot's
            // controls — see the comment above.
            //
            // **The last thing a narrow bar gives up** (the owner, 2026-09-26): past every other
            // compact step the pair is listed in the `…` menu, so the menu stays whole in a window
            // as narrow as one card. Its cursor card leaves with it.
            let items = resample_menu_items(ui.ctx(), params, telemetry, spawn);
            if mxm_ui::shell::product_actions(ui, items, |ui| {
                resample_pair(ui, &tokens, params, telemetry, setter, spawn);
            })
            .is_none()
            {
                mxm_ui::navigation::bar_card_absent(ui.ctx(), RESAMPLE_CARD);
            }
        },
    );
    match mxm_ui::shell::take_product_action(ui.ctx()) {
        Some(RESAMPLE_ITEM) => {
            let engaged = params.resample.value();
            set_together(
                setter,
                &[(&params.resample, if engaged { 0.0 } else { 1.0 })],
            );
        }
        Some(EXPORT_ITEM) => start_export(ui.ctx()),
        _ => {}
    }
    collect_export(ui.ctx(), spawn);
    mxm_preset::ui::overlays(ui, &tokens, params, setter, presets);

    let slot_peaks = telemetry.take_slot_peaks();
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            cards(
                ui,
                &tokens,
                params,
                setter,
                selected_slot,
                text_entry,
                slot_peaks,
                telemetry.tempo.get(),
            );
        });
}

#[allow(clippy::too_many_arguments)]
fn cards(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmDrumMachineParams,
    setter: &ParamSetter<'_>,
    selected_slot: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    slot_peaks: [f32; SLOT_COUNT],
    tempo: Option<f64>,
) {
    // Every card is built around the slot selected when the frame began. A click on another slot
    // lands on the next frame, which rebuilds every tree around it — so there is nothing to
    // invalidate and no revision to pass.
    let slot = *selected_slot;
    let items = page_items(ui, params, slot);
    let text_editing = text_entry.values().any(Option::is_some);
    let mut live = Live {
        params,
        setter,
        slot,
        selected_slot,
        entries: text_entry,
        peaks: slot_peaks,
        text_editing,
        tempo,
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[&[mxm_ui::paging::Key(0), mxm_ui::paging::Key(1)]],
        text_editing,
        &mut |ui, index| card(ui, index, params, slot),
        &mut |ui, _, leaf, rect| paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// Every paging item, each floor computed from its card's tree in `ui`'s fonts for the slot being
/// edited, and each card exactly as wide as that floor: its ceiling is its floor
/// (`plans/plan-editor-standard.md` A1).
pub fn page_items(
    ui: &Ui,
    params: &MxmDrumMachineParams,
    selected_slot: usize,
) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::flow::Card;
    use mxm_ui::paging::{Category, Item, Key};
    const CATEGORIES: [Category; 8] = [
        Category::Performance,
        Category::Performance,
        Category::Modulators,
        Category::Generators,
        Category::Generators,
        Category::Generators,
        Category::Tone,
        Category::Tone,
    ];
    const KINDS: [&str; 8] = [
        "Slots", "Slots", "LFO", "Model", "Shape", "Shape", "Shape", "Output",
    ];
    TITLES
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let content =
                mxm_ui::tree::card_floor(ui, title, &card(ui, index, params, selected_slot));
            Item {
                key: Key(index as u64),
                card: Card::new(title, content).capped(content),
                category: CATEGORIES[index],
                kind: KINDS[index],
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The cards, as trees (plans/plan-layout-tree.md). Each card is described once — `card` — and that
// one description is both measured (its floor and its height) and drawn, leaf by leaf, through the
// bindings (`paint`). Nothing is typed and nothing is drawn to learn a size. What a card holds
// follows the selected slot: its model's labels, what it can shape, and its route stacks.
// ---------------------------------------------------------------------------------------------

/// One of the selected slot's knobs and the route stack beneath it: a general control by number,
/// or the output stage's Level or Pan.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Axis {
    /// General control `k`, 1…20 (`params::control` names the eleven this machine uses).
    Control(u8),
    Level,
    Pan,
}

/// The shaping cards' knobs, by control number, in rows: **each a model shows** (`controls`).
const EXCITATION: &[&[usize]] = &[&[general::TUNE, general::ATTACK, general::DYNAMICS]];
const BODY: &[&[usize]] = &[&[general::BODY, general::NOISE, general::CHARACTER]];
const ENVELOPES_TONE: &[&[usize]] = &[
    &[general::DECAY, general::TONE, general::NOISE_DECAY],
    &[general::PITCH_ENV, general::PITCH_DECAY],
];

impl Axis {
    #[cfg(test)]
    const TUNE: Self = Self::Control(general::TUNE as u8);
    #[cfg(test)]
    const DECAY: Self = Self::Control(general::DECAY as u8);
    #[cfg(test)]
    const BODY: Self = Self::Control(general::BODY as u8);

    /// The general control it is, by number; `None` for Level and Pan.
    fn control(self) -> Option<usize> {
        match self {
            Self::Control(k) => Some(usize::from(k)),
            Self::Level | Self::Pan => None,
        }
    }

    /// **Whether `model`'s panel shows it** (the owner, 2026-10-07: only the controls that do
    /// something for that model, from `controls`' table). Level and Pan always; a legacy Off or an
    /// unavailable id, which plays nothing, keeps the eleven general controls, every one disabled,
    /// so its cards stay what they are.
    fn shown(self, model: ModelId) -> bool {
        match self.control() {
            None => true,
            Some(k) if available(model).is_none() => k <= general::USED,
            Some(k) => controls::face(model, k).is_some(),
        }
    }

    /// What its knob and its route rows are called on `model`'s panel: the model's own name for
    /// the control, or — where the model does not show it — the control's general name.
    fn name(self, model: ModelId) -> &'static str {
        match self {
            Self::Level => "Level",
            Self::Pan => "Pan",
            Self::Control(k) => {
                let k = usize::from(k);
                controls::face(model, k).map_or_else(|| controls::general_name(k), |f| f.name)
            }
        }
    }

    /// What it tells the player, on hover: what it does to `model`.
    fn help(self, model: ModelId) -> &'static str {
        match self {
            Self::Level => "Volume of the selected slot.",
            Self::Pan => "Stereo position; a slot on an output of its own ignores it.",
            Self::Control(k) => {
                let k = usize::from(k);
                controls::face(model, k).map_or_else(|| controls::general_help(k), |f| f.help)
            }
        }
    }

    /// The axis's parameter in `slot`, as its knob draws it.
    fn bound(self, params: &MxmDrumMachineParams, slot: usize) -> Bound<'_> {
        let p = &params.slots[slot];
        let model = p.model_id();
        let ids = slot_ids(slot);
        match self {
            Self::Level => Bound::new(ids[at::LEVEL], &p.level, self.help(model)),
            Self::Pan => Bound::new(ids[at::PAN], &p.pan, self.help(model)).bipolar(),
            Self::Control(k) => {
                let k = usize::from(k);
                Bound::new(ids[at::control(k)], p.controls.get(k), self.help(model))
                    .labelled(self.name(model))
                    .bipolar()
            }
        }
    }

    /// Whether the selected slot can move it now: a control its model uses, narrowed while Resample
    /// holds the slot frozen ([`shaping_capabilities`]). A shown control it cannot move draws
    /// disabled.
    fn enabled(self, capabilities: &Capabilities) -> bool {
        match self.control() {
            None => true,
            Some(general::TUNE) => capabilities.pitch,
            Some(general::DECAY) => capabilities.decay,
            Some(general::TONE) => capabilities.tone,
            Some(general::ATTACK) => capabilities.attack,
            Some(general::DYNAMICS) => capabilities.dynamics,
            Some(general::PITCH_ENV) => capabilities.pitch_envelope,
            Some(general::PITCH_DECAY) => capabilities.pitch_decay,
            Some(general::BODY) => capabilities.body,
            Some(general::NOISE) => capabilities.noise,
            Some(general::NOISE_DECAY) => capabilities.noise_decay,
            Some(general::CHARACTER) => capabilities.character,
            Some(_) => false,
        }
    }

    /// What its route rows carry.
    fn target(self) -> RouteTarget {
        match self {
            Self::Control(k) => RouteTarget::Control(k),
            Self::Level => RouteTarget::Level,
            Self::Pan => RouteTarget::Pan,
        }
    }

    /// The line over its route rows: its name, and — for a control `model` does not show, whose
    /// routes are kept on the panel so that nothing is dropped unseen — that it is unused.
    fn stack_label(self, model: ModelId) -> String {
        if self.shown(model) {
            self.name(model).to_owned()
        } else {
            format!("{} (unused)", self.name(model))
        }
    }
}

/// What a leaf of this editor's cards draws. Hashed by what it names — a slot, an LFO, an axis of a
/// slot — which is also what keeps its widget ids stable when a route appears above it.
#[derive(Clone, Debug, Hash)]
enum Leaf {
    /// One slot's inventory row.
    Slot(usize),
    /// A kit LFO's rate knob, its Sync switch and its shapes.
    Rate(usize),
    Sync(usize),
    Shape(usize),
    /// The selected slot's model selector, what the model is, and its sound's trace. The selector
    /// names no slot: it keeps its search under its widget id, and a search must survive choosing
    /// another slot, as it did when the card drew it directly.
    Model,
    Mechanism(usize),
    /// An axis's knob and its routes, by slot.
    Knob(usize, Axis),
    Routes(usize, Axis),
    Output(usize),
    KitOutputs,
    ChokeGroup(usize),
    MidiChannel(usize),
}

/// The painted label on each LFO's Sync switch; its parameter's name says which LFO.
/// The Model selector's painted label.
const MODEL_LABEL: &str = "Model";
/// The kit-wide output row's painted label.
const KIT_OUTPUTS_LABEL: &str = "All slots";

/// Card `index`'s body, as a tree, for the slot being edited.
fn card(ui: &Ui, index: usize, params: &MxmDrumMachineParams, slot: usize) -> Node<Leaf> {
    let capabilities = shaping_capabilities(params, slot);
    match index {
        // Eight inventory rows share one card. Every control keeps the pointer floor, and the rows
        // add no decorative height beyond it — no gap between them.
        0 | 1 => stack_gap(
            0.0,
            (index * 8..index * 8 + 8)
                .map(|row| {
                    leaf(
                        Leaf::Slot(row),
                        Kind::Custom {
                            min_width: slot_row_min_width(ui),
                            height: Height::Fixed(MIN_TARGET),
                            fills: true,
                        },
                    )
                })
                .collect(),
        ),
        2 => stack(
            (0..3)
                .map(|index| {
                    let lfo = group(vec![lfo_row(ui, params, index)]);
                    if index == 0 { lfo } else { pad(SPACE_3, lfo) }
                })
                .collect(),
        ),
        3 => {
            let model = params.slots[slot].model_id();
            stack(vec![
                leaf(
                    Leaf::Model,
                    Kind::Selector {
                        label: MODEL_LABEL.to_owned(),
                        options: model_options(model),
                        caption: false,
                        width: None,
                    },
                ),
                pad(
                    SPACE_3,
                    leaf(
                        Leaf::Mechanism(slot),
                        Kind::Custom {
                            min_width: 0.0,
                            height: Height::Fixed(MECHANISM_HEIGHT),
                            fills: true,
                        },
                    ),
                ),
            ])
        }
        4 => shaping(ui, params, slot, &capabilities, EXCITATION, &[]),
        5 => shaping(ui, params, slot, &capabilities, BODY, &[]),
        // The last shaping card also keeps any route still aimed at a control no model of this
        // machine uses (12–20), so a route the host set there is seen, never silently dropped.
        6 => shaping(
            ui,
            params,
            slot,
            &capabilities,
            ENVELOPES_TONE,
            &(general::USED + 1..=CONTROLS).collect::<Vec<_>>(),
        ),
        // The selected slot's output stage: how loud it is, where it sits and where it leaves,
        // followed by the one kit-wide row that routes every slot at once. The kit-wide controls
        // proper — Master, and Resample with its export — are in the app bar, where design system
        // §3.1 puts what belongs to the whole instrument rather than to one slot.
        _ => {
            let p = &params.slots[slot];
            stack(vec![
                knob_row(ui, params, slot, &capabilities, &[Axis::Level, Axis::Pan]),
                pad(
                    SPACE_3,
                    routes_leaf(ui, params, slot, Axis::Level, &capabilities),
                ),
                routes_leaf(ui, params, slot, Axis::Pan, &capabilities),
                pad(
                    SPACE_3,
                    selector_leaf(Leaf::Output(slot), &p.output, &OUTPUT_OPTIONS),
                ),
                leaf(
                    Leaf::KitOutputs,
                    Kind::Segmented {
                        label: KIT_OUTPUTS_LABEL.to_owned(),
                        options: KIT_OUTPUT_OPTIONS.map(str::to_owned).to_vec(),
                        beside: None,
                    },
                ),
                selector_leaf(Leaf::ChokeGroup(slot), &p.choke_group, &CHOKE_GROUP_OPTIONS),
                selector_leaf(
                    Leaf::MidiChannel(slot),
                    &p.midi_channel,
                    &MIDI_CHANNEL_OPTIONS,
                ),
            ])
        }
    }
}

/// A shaping card: rows of the knobs the slot's model shows (`rows`, by control number), then —
/// `SPACE_3` below them — each shown knob's route stack, in the order its knob reads. **A control
/// the model does not show has no knob**, and its stack is drawn only while a route is still aimed
/// at it, as are those of `unused` controls: so a route left on a control by a change of model is
/// seen and can be removed, never kept unseen. A shown control the slot cannot move now (Resample)
/// is disabled, knob and stack both.
fn shaping(
    ui: &Ui,
    params: &MxmDrumMachineParams,
    slot: usize,
    capabilities: &Capabilities,
    rows: &[&[usize]],
    unused: &[usize],
) -> Node<Leaf> {
    let p = &params.slots[slot];
    let model = p.model_id();
    let axis = |k: usize| Axis::Control(k as u8);
    let mut body: Vec<Node<Leaf>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|&k| axis(k))
                .filter(|axis| axis.shown(model))
                .collect::<Vec<_>>()
        })
        .filter(|axes| !axes.is_empty())
        .map(|axes| knob_row(ui, params, slot, capabilities, &axes))
        .collect();
    let stacks = rows
        .iter()
        .flat_map(|row| row.iter())
        .chain(unused)
        .map(|&k| axis(k))
        .filter(|axis| axis.shown(model) || !routes_on(p, axis.target()).is_empty());
    for (n, axis) in stacks.enumerate() {
        let routes = routes_leaf(ui, params, slot, axis, capabilities);
        body.push(if n == 0 { pad(SPACE_3, routes) } else { routes });
    }
    stack(body)
}

/// The collection's knob row (`mxm_ui::tree::knob_row`), an axis the slot cannot move now dimmed in
/// its column.
fn knob_row(
    ui: &Ui,
    params: &MxmDrumMachineParams,
    slot: usize,
    capabilities: &Capabilities,
    axes: &[Axis],
) -> Node<Leaf> {
    mxm_ui::tree::knob_row(
        ui,
        axes.iter()
            .map(|axis| {
                let bound = axis.bound(params, slot);
                let knob = leaf(
                    Leaf::Knob(slot, *axis),
                    Kind::Knob {
                        name: bound.painted().to_owned(),
                        widest: mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                        size: Size::Standard,
                        // In the collection's knob row, which sizes the columns.
                        column: 0.0,
                    },
                );
                let knob = if axis.enabled(capabilities) {
                    knob
                } else {
                    tree::disabled(knob)
                };
                (Size::Standard, knob)
            })
            .collect(),
    )
}

/// An axis's routes. A composite with a rule of its own — its floor is every source a row could
/// show — so it states its size ([`stack_size`]) and fills the card's width.
fn routes_leaf(
    ui: &Ui,
    params: &MxmDrumMachineParams,
    slot: usize,
    axis: Axis,
    capabilities: &Capabilities,
) -> Node<Leaf> {
    let p = &params.slots[slot];
    let model = p.model_id();
    let size = stack_size(ui, &axis.stack_label(model), p, axis, model);
    let stack = leaf(
        Leaf::Routes(slot, axis),
        Kind::Custom {
            min_width: size.x,
            height: Height::Fixed(size.y),
            fills: true,
        },
    );
    // An unused control's routes stay live, so they can be removed.
    if !axis.shown(model) || axis.enabled(capabilities) {
        stack
    } else {
        tree::disabled(stack)
    }
}

/// One of the Output card's caret selectors, labelled by its parameter.
fn selector_leaf(key: Leaf, param: &dyn binding::ErasedParam, options: &[&str]) -> Node<Leaf> {
    leaf(
        key,
        Kind::Selector {
            label: param.name().to_owned(),
            options: options.iter().map(|option| (*option).to_owned()).collect(),
            caption: false,
            width: None,
        },
    )
}

/// One LFO: **the knob carries its number, and there is no heading** (owner, 2026-09-22) — three
/// strong titles made the card the tallest in the instrument for three words the knob beside them
/// could say instead. Beside the knob, `SPACE_3` past the row's spacing, Sync and then the shapes,
/// **stacked beside it rather than below it** (owner, 2026-09-22): that spends the empty column
/// beside the knob instead of another full-width row. Sync is dropped clear of the knob's name box,
/// so its top edge sits on the bottom of "LFO rate N"; the shapes carry **no label line**, because
/// the rate knob names the LFO and a label would cost each row a line of height.
fn lfo_row(ui: &Ui, params: &MxmDrumMachineParams, index: usize) -> Node<Leaf> {
    let lfo = lfo(params, index);
    row_gap(
        ui.spacing().item_spacing.x,
        vec![
            leaf(
                Leaf::Rate(index),
                Kind::Knob {
                    name: lfo.label.to_owned(),
                    widest: binding::synced_widest(lfo.rate, crate::params::LFO_SYNC.span),
                    size: Size::Standard,
                    column: mxm_ui::control::KNOB_COLUMN_MIN,
                },
            ),
            pad_all(
                0.0,
                SPACE_3,
                0.0,
                stack_gap(
                    SPACE_2,
                    vec![
                        // Tempo sync, the collection's quarter note (`plans/plan-tempo-sync-controls.md`).
                        pad(
                            mxm_ui::control::name_box_height(ui),
                            leaf(Leaf::Sync(index), Kind::SyncToggle),
                        ),
                        leaf(
                            Leaf::Shape(index),
                            Kind::Waves {
                                label: None,
                                count: LFO_SHAPES.len(),
                                marks: Vec::new(),
                                beside: None,
                            },
                        ),
                    ],
                ),
            ),
        ],
    )
}

/// Everything a leaf draws with: the parameters and their host, the editor's selection and text
/// buffers, and the slot peaks taken once before the frame (mxm-kit's
/// `docs/plugin-conventions.md`, *Editor contract*: destructive telemetry is read once).
struct Live<'a, 'b> {
    params: &'a MxmDrumMachineParams,
    setter: &'a ParamSetter<'b>,
    /// The slot every card was built around this frame. A slot row's click writes `selected_slot`
    /// and lands next frame; a leaf that names no slot paints this one, as its tree was built.
    slot: usize,
    selected_slot: &'a mut usize,
    entries: &'a mut HashMap<&'static str, Option<String>>,
    peaks: [f32; SLOT_COUNT],
    /// A value is being typed somewhere, so the slot rows hold still.
    text_editing: bool,
    /// The host tempo in force: a synced rate reads its division with one and its hertz without.
    tempo: Option<f64>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings — so the controls,
/// their gestures and their names are exactly what they were.
fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: egui::Rect, live: &mut Live<'_, '_>) {
    let params = live.params;
    let setter = live.setter;
    match *leaf {
        Leaf::Slot(index) => slot_row(
            ui,
            tokens,
            params,
            index,
            live.selected_slot,
            live.peaks[index],
            setter,
            live.text_editing,
        ),
        Leaf::Rate(index) => {
            let lfo = lfo(params, index);
            let ladder = crate::params::LFO_SYNC;
            let reading = lfo
                .sync
                .value()
                .then(|| {
                    ladder.shown(
                        lfo.rate.unmodulated_normalized_value(),
                        live.tempo,
                        f64::from(lfo.rate.preview_plain(0.0)),
                        f64::from(lfo.rate.preview_plain(1.0)),
                    )
                })
                .flatten()
                .map(mxm_tempo::Division::label);
            let rate = Bound::new(
                lfo.rate_id,
                lfo.rate,
                "How fast this LFO runs; with Sync on, a note length.",
            )
            .labelled(lfo.label);
            if let Some(reading) = reading {
                rate.knob_with_reading(
                    ui,
                    tokens,
                    setter,
                    Size::Standard,
                    rect.width(),
                    live.entries,
                    reading,
                );
            } else {
                rate.knob(
                    ui,
                    tokens,
                    setter,
                    Size::Standard,
                    rect.width(),
                    live.entries,
                );
            }
        }
        Leaf::Sync(index) => {
            let lfo = lfo(params, index);
            binding::sync_picture(ui, tokens, lfo.sync_id, lfo.sync, setter);
        }
        Leaf::Shape(index) => {
            let lfo = lfo(params, index);
            // Each picture is still named in full for a screen reader and on hover.
            binding::segmented_waves_unlabelled(
                ui,
                tokens,
                lfo.shape_id,
                lfo.shape,
                &LFO_SHAPES,
                &LFO_DETAILS,
                setter,
            );
        }
        Leaf::Model => {
            let slot = live.slot;
            mapped_model_selector(ui, tokens, slot, &params.slots[slot], setter);
        }
        Leaf::Mechanism(slot) => mechanism_display(ui, tokens, &params.slots[slot]),
        Leaf::Knob(slot, axis) => axis.bound(params, slot).knob(
            ui,
            tokens,
            setter,
            Size::Standard,
            rect.width(),
            live.entries,
        ),
        Leaf::Routes(slot, axis) => route_stack(
            ui,
            tokens,
            &params.slots[slot],
            slot,
            axis,
            setter,
            live.entries,
        ),
        Leaf::Output(slot) => bound_selector(
            ui,
            tokens,
            slot_ids(slot)[at::OUTPUT],
            &params.slots[slot].output,
            &OUTPUT_OPTIONS,
            None,
            "Where this slot plays: the main stereo output, or an output of its own.",
            setter,
        ),
        Leaf::KitOutputs => kit_outputs_row(ui, tokens, params, setter),
        Leaf::ChokeGroup(slot) => bound_selector(
            ui,
            tokens,
            slot_ids(slot)[at::CHOKE_GROUP],
            &params.slots[slot].choke_group,
            &CHOKE_GROUP_OPTIONS,
            None,
            "Slots sharing a group cut each other, whichever models they are: an open and a closed sound, or a long and a short one.",
            setter,
        ),
        Leaf::MidiChannel(slot) => bound_selector(
            ui,
            tokens,
            slot_ids(slot)[at::MIDI_CHANNEL],
            &params.slots[slot].midi_channel,
            &MIDI_CHANNEL_OPTIONS,
            None,
            "Use the kit note, or claim a MIDI channel and play this slot by note: pitched drums in tune at concert pitch, others around note 60.",
            setter,
        ),
    }
}

/// What a slot row occupies at its narrowest, without drawing it: the frame's margins, the number's
/// box, the model button at its own minimum ([`SLOT_MODEL_MIN`]) — above that it truncates to
/// whatever it is given — Mute and Solo, and the row's spacing between the four. It is
/// [`MIN_TARGET`] tall.
fn slot_row_min_width(ui: &Ui) -> f32 {
    2.0 * SLOT_MARGIN
        + SLOT_NUMBER_WIDTH
        + SLOT_MODEL_MIN
        + 2.0 * mxm_ui::control::toggle_compact_size().x
        + 3.0 * ui.spacing().item_spacing.x
}

/// One slot's inventory row: its number, its model — a button that selects the slot for editing —
/// and its Mute and Solo, in a frame that carries the selection's fill. The activity bar and the
/// selection's outline and rail are painted over it. [`slot_row_min_width`] states its size.
#[allow(clippy::too_many_arguments)]
fn slot_row(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmDrumMachineParams,
    index: usize,
    selected_slot: &mut usize,
    peak: f32,
    setter: &ParamSetter<'_>,
    text_editing: bool,
) {
    let name = model_name(params.slots[index].model_id());
    let selected = *selected_slot == index;
    let row = slot_frame(tokens, selected).show(ui, |ui| {
        ui.horizontal(|ui| {
            let slot = &params.slots[index];
            ui.add_sized(
                egui::vec2(SLOT_NUMBER_WIDTH, MIN_TARGET),
                egui::Label::new(egui::RichText::new(format!("{:02}", index + 1)).monospace()),
            );
            // What still follows the model, the number having taken its own box and gap: Mute and
            // Solo, and the gap before each — `slot_row_min_width`'s terms.
            let controls =
                2.0 * (mxm_ui::control::toggle_compact_size().x + ui.spacing().item_spacing.x);
            let width = (ui.available_width() - controls).max(SLOT_MODEL_MIN);
            let response = ui.add_enabled_ui(!text_editing, |ui| {
                ui.add_sized(
                    egui::vec2(width, MIN_TARGET),
                    // **Truncated, so the row is a column and not a ragged edge.** `add_sized` is a
                    // floor, not a ceiling: without this a long model name — "Six-square closed
                    // hat", "Triple-resonator rim" — grew the button past the width computed above
                    // and shoved that row's M and S out of line with every other row's. The names
                    // that need it are rare and the ellipsis is a backstop, exactly as
                    // `control::fixed_label` argues for the knob name boxes.
                    egui::Button::new(name).selected(selected).truncate(),
                )
            });
            if response.inner.clicked() {
                *selected_slot = index;
            }
            let model_rect = response.inner.rect;
            let solo_clicked = ui
                .add_enabled_ui(!text_editing, |ui| {
                    bound_toggle_compact(
                        ui,
                        tokens,
                        slot_ids(index)[at::MUTE],
                        "M",
                        &slot.mute,
                        "Silence this slot without cutting off the drum's motion.",
                        setter,
                    );
                    bound_toggle_compact(
                        ui,
                        tokens,
                        slot_ids(index)[at::SOLO],
                        "S",
                        &slot.solo,
                        "Hear this slot on its own and select it for editing.",
                        setter,
                    )
                })
                .inner;
            if solo_clicked {
                *selected_slot = index;
            }
            model_rect
        })
        .inner
    });
    activity_bar(ui, tokens, row.inner, peak);
    if selected {
        // Paint inward and after activity: adjacent minimum-height rows leave no spare clipping area
        // for a centred stroke, and the activity line must not cover it.
        ui.painter().rect_stroke(
            row.response.rect,
            4.0,
            egui::Stroke::new(2.0, tokens.accent),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                row.response.rect.min,
                egui::pos2(row.response.rect.left() + 4.0, row.response.rect.bottom()),
            ),
            2.0,
            tokens.accent,
        );
    }
}

fn slot_frame(tokens: &Tokens, selected: bool) -> egui::Frame {
    // Selection must not resize its model button: the frame owns only fill and fixed margin; its
    // border is painted inside the final row rectangle.
    egui::Frame::new()
        .fill(if selected {
            tokens.selection
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(SLOT_MARGIN as i8, 0))
}

fn activity_bar(ui: &mut Ui, tokens: &Tokens, button: egui::Rect, peak: f32) {
    let rect = egui::Rect::from_min_max(
        egui::pos2(button.left() + 3.0, button.bottom() - 4.0),
        egui::pos2(button.right() - 3.0, button.bottom() - 2.0),
    );
    ui.painter().rect_filled(rect, 1.0, tokens.surface_2);
    let fraction = if peak.is_finite() {
        peak.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fraction, rect.height())),
        1.0,
        tokens.accent,
    );
}

/// The kit LFOs' six shapes, in `LfoShape`'s order, drawn rather than named (design system §7.3)
/// in two rows of three — the owner's choice for a list one longer than a row may hold.
/// What each LFO shape does, in [`LFO_SHAPES`]' order (design system §7.3; the owner, 2026-09-27:
/// the cells of a row do not share one sentence).
const LFO_DETAILS: [&str; 6] = [
    "A smooth wobble.",
    "Rises and falls in straight lines.",
    "Rises, then snaps back down.",
    "Falls, then snaps back up.",
    "Jumps between two values, like a trill.",
    "A new random value on every cycle.",
];
const LFO_SHAPES: [(Wave, &str); 6] = [
    (Wave::Sine, "Sine"),
    (Wave::Triangle, "Triangle"),
    (Wave::RampUp, "Ramp up"),
    (Wave::RampDown, "Ramp down"),
    (Wave::Square, "Square"),
    (Wave::Random, "Sample & hold"),
];
/// One kit LFO's parameters and the name its rate knob paints.
struct Lfo<'a> {
    label: &'static str,
    rate_id: &'static str,
    shape_id: &'static str,
    sync_id: &'static str,
    rate: &'a FloatParam,
    shape: &'a dyn binding::ErasedParam,
    sync: &'a BoolParam,
}

/// LFO `index`, from zero. **The knob carries the LFO's number** ("Rate 1" — the card already says
/// *LFOs*): a name box is
/// `NAME_LINES` tall whatever the name does, so the number costs no height.
fn lfo(params: &MxmDrumMachineParams, index: usize) -> Lfo<'_> {
    match index {
        0 => Lfo {
            label: "Rate 1",
            rate_id: "lfo1_rate",
            shape_id: "lfo1_shape",
            sync_id: "lfo1_sync",
            rate: &params.lfo1_rate,
            shape: &params.lfo1_shape,
            sync: &params.lfo1_sync,
        },
        1 => Lfo {
            label: "Rate 2",
            rate_id: "lfo2_rate",
            shape_id: "lfo2_shape",
            sync_id: "lfo2_sync",
            rate: &params.lfo2_rate,
            shape: &params.lfo2_shape,
            sync: &params.lfo2_sync,
        },
        _ => Lfo {
            label: "Rate 3",
            rate_id: "lfo3_rate",
            shape_id: "lfo3_shape",
            sync_id: "lfo3_sync",
            rate: &params.lfo3_rate,
            shape: &params.lfo3_shape,
            sync: &params.lfo3_sync,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Routes: four slots a drum (mxm-model-drums' scheme), shown as rows under the knob each moves. The
// rows are derived from parameter values alone, so a preset decides the panel. The collection's
// route stack (`mxm_modulation_params::ui`) draws one fixed pair per source; a route here is a slot
// any source and target can take, so this draws the same rows over its own parameters, as
// model-drums' editor does.
// ---------------------------------------------------------------------------------------------

/// The route slots, 0-based, that move `target`: aimed at it with a source on.
fn routes_on(p: &SlotParams, target: RouteTarget) -> Vec<usize> {
    let all = p.routes.all();
    (0..ROUTES)
        .filter(|&r| all[r].source.value() != SourceChoice::Off && all[r].target() == target)
        .collect()
}

/// The route slot adding a source to `target` takes: a free one already aimed at it first (a
/// source removed from this knob, so adding one back keeps its depth), else the first free one. A
/// slot is free when its source or its target is Off.
fn free_route(p: &SlotParams, target: RouteTarget) -> Option<usize> {
    let all = p.routes.all();
    let free = |r: usize| !all[r].in_use();
    (0..ROUTES)
        .find(|&r| free(r) && all[r].target() == target)
        .or_else(|| (0..ROUTES).find(|&r| free(r)))
}

/// The sources `axis`'s add menu offers on `model`'s panel: none on a control the model does not
/// show — a route already there is drawn but never offered more — and otherwise [`offered`].
fn offered_on(p: &SlotParams, axis: Axis, model: ModelId) -> Vec<SourceChoice> {
    if axis.shown(model) {
        offered(p, axis.target())
    } else {
        Vec::new()
    }
}

/// The sources the add menu offers on `target`: those not already moving it, while a slot is free.
fn offered(p: &SlotParams, target: RouteTarget) -> Vec<SourceChoice> {
    if free_route(p, target).is_none() {
        return Vec::new();
    }
    let all = p.routes.all();
    let on: Vec<SourceChoice> = routes_on(p, target)
        .into_iter()
        .map(|r| all[r].source.value())
        .collect();
    SourceChoice::ALL
        .into_iter()
        .filter(|s| !on.contains(s))
        .collect()
}

/// Adds `source` to `target` in one gesture: the free slot's source and target, and its amount to
/// zero unless the slot already aimed there.
fn add_route(p: &SlotParams, target: RouteTarget, source: SourceChoice, setter: &ParamSetter<'_>) {
    let Some(r) = free_route(p, target) else {
        return;
    };
    let route = p.routes.all()[r];
    let mut edits: Vec<(&dyn binding::ErasedParam, f32)> = vec![
        (&route.source, route.source.preview_normalized(source)),
        (
            &route.target,
            route.target.preview_normalized(target.index() as i32),
        ),
    ];
    if route.target() != target {
        edits.push((&route.amount, route.amount.preview_normalized(0.0)));
    }
    set_together(setter, &edits);
}

/// What [`route_stack`] occupies, without drawing it: at its narrowest the widest row any source
/// could draw, or the target's line; its height as the patch stands. The collection's own stack's
/// rule (`mxm_modulation_params::ui::stack_size`), over this plugin's route slots.
fn stack_size(ui: &Ui, panel: &str, p: &SlotParams, axis: Axis, model: ModelId) -> egui::Vec2 {
    let target = axis.target();
    let spacing = ui.spacing().item_spacing;
    let inset = 2.0 * mxm_ui::tree::GROUP_INSET;
    let amount: &dyn binding::ErasedParam = &p.routes.route1.amount;
    let widest = control::widest_value(|n| amount.format(n as f32));
    let row = |source: SourceChoice| {
        let slider = control::slider_size(ui, source.name(), true, &widest);
        egui::Vec2::new(
            slider.x.max(mxm_modulation_params::ui::TRACK_MIN) + spacing.x + control::REMOVE_SIZE,
            slider.y.max(control::REMOVE_SIZE),
        )
    };
    let line = control::selector_size(ui, panel, &["modulate"]);
    let widest_row = SourceChoice::ALL
        .iter()
        .map(|s| row(*s).x)
        .fold(0.0, f32::max);
    let width = (widest_row.max(line.x) + inset).max(line.x);
    let all = p.routes.all();
    let rows: Vec<f32> = routes_on(p, target)
        .into_iter()
        .map(|r| row(all[r].source.value()).y)
        .collect();
    let height = if !rows.is_empty() {
        inset + line.y + SPACE_2 + rows.iter().sum::<f32>() + SPACE_2 * (rows.len() - 1) as f32
    } else if !offered_on(p, axis, model).is_empty() {
        line.y
    } else {
        0.0
    };
    egui::Vec2::new(width, height)
}

/// One knob's routes: its line offering `‹ modulate ›`, and a row for each route that moves it.
fn route_stack(
    ui: &mut Ui,
    tokens: &Tokens,
    p: &SlotParams,
    slot: usize,
    axis: Axis,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let model = p.model_id();
    let on = routes_on(p, axis.target());
    let offered = offered_on(p, axis, model);
    if !on.is_empty() {
        mxm_ui::shell::group(ui, tokens, |ui| {
            target_line(ui, tokens, p, axis, &offered, setter);
            for r in on {
                route_row(ui, tokens, p, slot, r, axis, setter, entries);
            }
        });
    } else if !offered.is_empty() {
        target_line(ui, tokens, p, axis, &offered, setter);
    }
}

/// The knob's line: its routes' name — the knob's own — and `‹ modulate ›` offering the sources it
/// can still take. Over a control the model does not show, the line says it is unused and offers
/// nothing.
fn target_line(
    ui: &mut Ui,
    tokens: &Tokens,
    p: &SlotParams,
    axis: Axis,
    offered: &[SourceChoice],
    setter: &ParamSetter<'_>,
) {
    let model = p.model_id();
    let name = axis.stack_label(model);
    if offered.is_empty() {
        ui.horizontal(|ui| {
            ui.set_min_height(MIN_TARGET);
            let label = ui.label(egui::RichText::new(&name).color(tokens.text_primary));
            if !axis.shown(model) {
                label.on_hover_text(format!(
                    "This drum has no {}, so these routes move nothing. Remove them to free their slots.",
                    axis.name(model)
                ));
            }
        });
        return;
    }
    let labels: Vec<&str> = std::iter::once("modulate")
        .chain(offered.iter().map(|s| s.name()))
        .collect();
    let mut chosen = 0usize;
    let description = format!("Add a modulation source to {name}");
    let changed = control::selector(
        ui,
        tokens,
        &name,
        &labels,
        &mut chosen,
        None,
        Some(0),
        &description,
    );
    if changed && chosen > 0 {
        add_route(p, axis.target(), offered[chosen - 1], setter);
    }
}

/// One route: its amount, painted as its source and named in full, with a remove beside it that
/// switches the source off — one write, the depth kept.
#[allow(clippy::too_many_arguments)]
fn route_row(
    ui: &mut Ui,
    tokens: &Tokens,
    p: &SlotParams,
    slot: usize,
    r: usize,
    axis: Axis,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let route: &RouteParams = p.routes.all()[r];
    let ids = slot_ids(slot);
    let model = p.model_id();
    let source = route.source.value().name();
    // Named as its knob is: Level, not the standard's Amplitude; a snare's Snappy, not Noise.
    let target = axis.name(model);
    let amount: &dyn binding::ErasedParam = &route.amount;
    let name = format!("{target} from {source}");
    let text = amount.text();
    let description = if axis.shown(model) {
        format!("How much {source} moves {target}. Signed: below the centre inverts it.")
    } else {
        format!("This drum has no {target}, so {source} moves nothing here.")
    };
    let (fine_up, fine_down, coarse_up, coarse_down) = amount.stepping();
    let view = ParamView {
        name: &name,
        label: source,
        text: &text,
        widest: "",
        description: &description,
        default: f64::from(amount.default_normalised()),
        bipolar: true,
        read_only: false,
        modulation: f64::from(amount.modulation()),
        marked: amount.modulation() != 0.0,
        steps: Steps {
            fine_up,
            fine_down,
            coarse_up,
            coarse_down,
        },
        next: None,
        quiet_label: true,
    };
    let mut normalised = f64::from(amount.normalised());
    let entry = entries.entry(ids[at::amount(r)]).or_default();
    let mut removed = false;
    let outcome = ui
        .horizontal(|ui| {
            // The remove's width is reserved before the slider takes the rest.
            let gap = ui.spacing().item_spacing.x;
            let track = (ui.available_width() - control::REMOVE_SIZE - gap)
                .max(mxm_modulation_params::ui::TRACK_MIN);
            let out = ui
                .scope(|ui| {
                    ui.set_max_width(track);
                    mxm_ui::navigation::at(ui, ids[at::amount(r)], |ui| {
                        control::slider(
                            ui,
                            tokens,
                            &view,
                            &mut normalised,
                            track,
                            entry,
                            Wheel::FocusOrModifier,
                        )
                    })
                })
                .inner;
            // Bottom-aligned onto the slider's track, as the collection's route rows are.
            let height = ui.min_rect().height().max(control::REMOVE_SIZE);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(control::REMOVE_SIZE, height),
                egui::Layout::bottom_up(egui::Align::Center),
                |ui| {
                    mxm_ui::navigation::at(ui, ids[at::source(r)], |ui| {
                        if control::remove_mark(
                            ui,
                            tokens,
                            control::REMOVE_SIZE,
                            true,
                            &format!("Remove {source} from {target}"),
                            binding::ErasedParam::modulation(&route.source) != 0.0,
                            "The depth stays, so adding the source back restores it.",
                        ) {
                            set_together(
                                setter,
                                &[(
                                    &route.source as &dyn binding::ErasedParam,
                                    route.source.preview_normalized(SourceChoice::Off),
                                )],
                            );
                            removed = true;
                        }
                    });
                },
            );
            out
        })
        .inner;
    if removed {
        return;
    }
    if outcome.gesture_started {
        amount.begin(setter);
    }
    if outcome.changed {
        amount.set(setter, normalised as f32);
    }
    if outcome.gesture_ended {
        amount.end(setter);
    }
}

/// What the selected model sounds like, in a line — the Model selector's hover text (the owner,
/// 2026-09-27: no help text on the panel, and hover text written for the player, never the
/// circuit's vocabulary).
fn explanation(model: ModelId) -> &'static str {
    match model {
        ModelId::DEEP_BRIDGE_KICK => "A deep, resonant kick that keeps ringing when struck again.",
        ModelId::TWIN_MODE_SNARE => "A two-tone snare with a noisy rattle.",
        ModelId::LOW_FALLING_TOM => "A tom that falls in pitch, with a soft noisy tail.",
        ModelId::LOW_FALLING_CONGA
        | ModelId::MID_FALLING_TOM
        | ModelId::MID_FALLING_CONGA
        | ModelId::HIGH_FALLING_TOM
        | ModelId::HIGH_FALLING_CONGA => "A resonant drum that falls in pitch.",
        ModelId::LAYERED_SHORT_RIM => "A short, clipped rimshot.",
        ModelId::PURE_HIGH_CLAVE => "A high, short clave click.",
        ModelId::BRIGHT_SHORT_MARACA => "A bright, short shaker.",
        ModelId::TRIPLE_PULSE_CLAP => "A handclap: a quick burst of claps and a roomy tail.",
        ModelId::TWIN_SQUARE_COWBELL => "A two-tone cowbell with an abrupt decay.",
        ModelId::THREE_PATH_CYMBAL => "A metallic cymbal with a long, layered decay.",
        ModelId::SIX_SQUARE_CLOSED_HAT => "A bright, short closed hi-hat.",
        ModelId::SIX_SQUARE_OPEN_HAT => "A bright open hi-hat with a long ring.",
        ModelId::RESET_PUNCH_KICK => "A punchy kick that falls in pitch, with a noisy attack.",
        ModelId::RESET_TWIN_SNARE => "A snappy snare with a bright, noisy crack.",
        ModelId::LOW_RESET_TRIAD_TOM
        | ModelId::MID_RESET_TRIAD_TOM
        | ModelId::HIGH_RESET_TRIAD_TOM => "A tom that falls in pitch, with a noisy attack.",
        ModelId::TRIPLE_RESONATOR_RIM => "A short, clipped rimshot with a filtered edge.",
        ModelId::FOUR_CELL_CLAP => "A handclap with a four-part attack and a roomy tail.",
        ModelId::SIX_BIT_CLOSED_HAT | ModelId::SIX_BIT_OPEN_HAT => {
            "A gritty, lo-fi hi-hat; tuning it changes its length."
        }
        ModelId::SIX_BIT_CRASH | ModelId::SIX_BIT_RIDE => {
            "A gritty, lo-fi cymbal; tuning it changes its length."
        }
        ModelId::ECONOMY_62_KICK | ModelId::ECONOMY_BODY_SNARE | ModelId::ECONOMY_SHORT_RIM => {
            "A small, simple drum-machine voice."
        }
        ModelId::INDUCTOR_NOISE_HAT => "A metallic, noisy hi-hat.",
        id if (32..=94).contains(&id.raw()) => "A drum-machine percussion voice.",
        ModelId::OFF => "This slot is silent.",
        _ => "This model is not available in this version.",
    }
}

/// What a slot row and the Model selector call a model id: its public label, or what it is when it
/// has none.
fn model_name(model: ModelId) -> String {
    available(model).map_or_else(
        || {
            if model == ModelId::OFF {
                "Legacy Off".to_owned()
            } else {
                format!("Unavailable {}", model.raw())
            }
        },
        |spec| spec.label.to_owned(),
    )
}

/// The Model selector's options, as [`mapped_model_selector`] draws them: every available model, and
/// the current id after them when it is not one of them.
fn model_options(current: ModelId) -> Vec<String> {
    let mut labels: Vec<String> = AVAILABLE_MODELS
        .iter()
        .map(|spec| spec.label.to_owned())
        .collect();
    if model_selector_ids(current, &AVAILABLE_MODELS).0.len() > AVAILABLE_MODELS.len() {
        labels.push(model_name(current));
    }
    labels
}

fn model_selector_ids(
    current: ModelId,
    models: &[mxm_drum_machine_dsp::model::ModelSpec],
) -> (Vec<ModelId>, usize) {
    let mut ids: Vec<_> = models.iter().map(|spec| spec.id).collect();
    let selected = models.iter().position(|spec| spec.id == current);
    if let Some(selected) = selected {
        (ids, selected)
    } else {
        ids.push(current);
        let selected = ids.len() - 1;
        (ids, selected)
    }
}

fn mapped_model_selector(
    ui: &mut Ui,
    tokens: &Tokens,
    slot: usize,
    slot_params: &SlotParams,
    setter: &ParamSetter<'_>,
) {
    let current = slot_params.model_id();
    let (ids, mut selected) = model_selector_ids(current, &AVAILABLE_MODELS);
    let mut labels: Vec<&str> = AVAILABLE_MODELS.iter().map(|spec| spec.label).collect();
    let mut groups: Vec<&str> = AVAILABLE_MODELS.iter().map(|spec| spec.group).collect();
    let unavailable = model_name(current);
    if ids.len() > AVAILABLE_MODELS.len() {
        labels.push(&unavailable);
        groups.push("Unavailable");
    }
    let default = AVAILABLE_MODELS
        .iter()
        .position(|spec| i32::from(spec.id.raw()) == slot_params.model.default_plain_value())
        .unwrap_or(0);
    let changed = mxm_ui::navigation::at(ui, slot_ids(slot)[at::MODEL], |ui| {
        mxm_ui::control::selector_grouped(
            ui,
            tokens,
            MODEL_LABEL,
            &labels,
            &groups,
            &mut selected,
            None,
            Some(default),
            &format!(
                "Choose the drum sound for this slot. {}",
                explanation(current)
            ),
        )
    });
    if changed && selected < AVAILABLE_MODELS.len() {
        setter.begin_set_parameter(&slot_params.model);
        setter.set_parameter(&slot_params.model, i32::from(ids[selected].raw()));
        setter.end_set_parameter(&slot_params.model);
    }
}

fn mechanism_display(ui: &mut Ui, tokens: &Tokens, params: &SlotParams) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), MECHANISM_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
    let patch = preview_patch(params);
    if patch.model == ModelId::OFF || available(patch.model).is_none() {
        return;
    }
    let signature = preview_signature(patch);
    let cache_id = egui::Id::new("mxm-drum-machine-sound-preview");
    let cached = ui
        .ctx()
        .data_mut(|data| data.get_temp::<SoundPreview>(cache_id));
    let preview = match cached {
        Some(preview) if preview.signature == signature => preview,
        _ => {
            let preview = render_sound_preview(patch, signature);
            ui.ctx()
                .data_mut(|data| data.insert_temp(cache_id, preview.clone()));
            preview
        }
    };

    let plot = rect.shrink2(egui::vec2(12.0, 9.0));
    let middle = plot.center().y;
    ui.painter().line_segment(
        [
            egui::pos2(plot.left(), middle),
            egui::pos2(plot.right(), middle),
        ],
        egui::Stroke::new(1.0, tokens.border),
    );
    let point = |index: usize, sample: f32| {
        let x = index as f32 / (PREVIEW_BINS - 1) as f32;
        egui::pos2(
            plot.left() + plot.width() * x,
            middle - sample * plot.height() * 0.46,
        )
    };
    let upper: Vec<_> = preview
        .high
        .iter()
        .copied()
        .enumerate()
        .map(|(index, sample)| point(index, sample))
        .collect();
    let lower: Vec<_> = preview
        .low
        .iter()
        .copied()
        .enumerate()
        .map(|(index, sample)| point(index, sample))
        .collect();
    for (top, bottom) in upper.iter().zip(&lower) {
        ui.painter().line_segment(
            [*top, *bottom],
            egui::Stroke::new(1.0, tokens.accent.gamma_multiply(0.16)),
        );
    }
    ui.painter().add(egui::Shape::line(
        upper,
        egui::Stroke::new(1.25, tokens.accent),
    ));
    ui.painter().add(egui::Shape::line(
        lower,
        egui::Stroke::new(1.25, tokens.accent),
    ));
}

fn preview_patch(params: &SlotParams) -> SlotPatch {
    use crate::params::control;
    let read = |k: usize| quantize_preview(params.controls.get(k).value());
    SlotPatch {
        model: params.model_id(),
        pitch_semitones: read(control::TUNE),
        pitch_envelope: read(control::PITCH_ENV),
        pitch_decay: read(control::PITCH_DECAY),
        decay: read(control::DECAY),
        attack: read(control::ATTACK),
        tone: read(control::TONE),
        body: read(control::BODY),
        noise: read(control::NOISE),
        noise_decay: read(control::NOISE_DECAY),
        character: read(control::CHARACTER),
        dynamics: read(control::DYNAMICS),
        level: 1.0,
        pan: 0.0,
        // A preview renders one slot alone, so nothing can choke it.
        choke_group: 0,
    }
}

fn quantize_preview(value: f32) -> f32 {
    if value.is_finite() {
        (value * 64.0).round() / 64.0
    } else {
        0.0
    }
}

fn preview_signature(patch: SlotPatch) -> u64 {
    let mut signature = 0xcbf2_9ce4_8422_2325_u64 ^ u64::from(patch.model.raw());
    for value in [
        patch.pitch_semitones,
        patch.pitch_envelope,
        patch.pitch_decay,
        patch.decay,
        patch.attack,
        patch.tone,
        patch.body,
        patch.noise,
        patch.noise_decay,
        patch.character,
        patch.dynamics,
    ] {
        signature ^= u64::from(value.to_bits());
        signature = signature.wrapping_mul(0x100_0000_01b3);
    }
    signature
}

fn render_sound_preview(patch: SlotPatch, signature: u64) -> SoundPreview {
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    let mut engine = Engine::new();
    engine.set_sample_rate(PREVIEW_SAMPLE_RATE);
    engine.prepare(&patches);
    let mut triggers = TriggerGroup::new();
    triggers.push(0, 0.82);
    engine.trigger_group(&patches, triggers);

    let total = (PREVIEW_SECONDS * PREVIEW_SAMPLE_RATE) as usize;
    let mut low = [0.0_f32; PREVIEW_BINS];
    let mut high = [0.0_f32; PREVIEW_BINS];
    let mut cursor = 0;
    let mut peak = 0.0_f32;
    let attack_samples = (PREVIEW_ATTACK_SECONDS * PREVIEW_SAMPLE_RATE) as usize;
    for bin in 0..PREVIEW_BINS {
        let end = if bin < PREVIEW_ATTACK_BINS {
            let progress = (bin + 1) as f32 / PREVIEW_ATTACK_BINS as f32;
            (progress * attack_samples as f32).round() as usize
        } else {
            let tail_bin = bin + 1 - PREVIEW_ATTACK_BINS;
            let tail_bins = PREVIEW_BINS - PREVIEW_ATTACK_BINS;
            let progress = tail_bin as f32 / tail_bins as f32;
            attack_samples + (progress.powf(2.0) * (total - attack_samples) as f32).round() as usize
        }
        .max(cursor + 1)
        .min(total);
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        while cursor < end {
            let frame = engine.process(&patches);
            let sample = 0.5 * (frame[0] + frame[1]);
            minimum = minimum.min(sample);
            maximum = maximum.max(sample);
            peak = peak.max(sample.abs());
            cursor += 1;
        }
        low[bin] = if minimum.is_finite() { minimum } else { 0.0 };
        high[bin] = if maximum.is_finite() { maximum } else { 0.0 };
    }
    if peak > 1.0e-9 {
        for sample in low.iter_mut().chain(high.iter_mut()) {
            *sample /= peak;
        }
    }
    SoundPreview {
        signature,
        low,
        high,
    }
}

/// Which shaping axes the selected slot can actually use, including while frozen.
///
/// The model's own capabilities, narrowed by Resample: a frozen slot plays a recording, so only
/// the three axes that mean something over a buffer keep working — Tune as a playback rate, Decay
/// as a shortening envelope (plan §4.7) and Soft hits, the velocity curve the buffer is played at
/// (`mxm_drum_machine_dsp::velocity`). Everything else holds still until the mode is switched off.
/// *Until 2026-10-07 Soft hits (then Dynamics) drew disabled while frozen, though the frozen hit
/// read it.*
///
/// **Narrowing the same mask the model already uses is deliberate**: a frozen control is one the
/// slot cannot use for now, so it stays on the panel, disabled — the collection's treatment for a
/// control that cannot be used. A control the model's code never reads is another thing: it has no
/// knob at all (`controls`, since 2026-10-07; until then it, too, was drawn disabled).
fn shaping_capabilities(params: &MxmDrumMachineParams, slot: usize) -> Capabilities {
    let capabilities = params.slots[slot].model_id().capabilities();
    if !params.resample.value() {
        return capabilities;
    }
    Capabilities {
        pitch: capabilities.pitch,
        decay: capabilities.decay,
        pitch_envelope: false,
        pitch_decay: false,
        attack: false,
        tone: false,
        body: false,
        noise: false,
        noise_decay: false,
        character: false,
        dynamics: capabilities.dynamics,
    }
}

/// Where the whole kit leaves, read from the sixteen per-slot Output settings.
///
/// **Derived, never stored** (plan revision 42): a stored mode would need an `Auto` value in every
/// per-slot selector, changing a frozen parameter's range. The row therefore shows whichever
/// pattern the settings already form, and neither cell when they form none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KitOutputs {
    /// Every slot on main `L+R`.
    Main = 0,
    /// Slot N on individual output N, for every slot.
    Own = 1,
    /// Anything else, such as two hats sharing one output.
    Mixed = 2,
}

impl KitOutputs {
    fn of(params: &MxmDrumMachineParams) -> Self {
        let outputs = params.slots.iter().map(|slot| slot.output.value());
        if outputs.clone().all(|output| output == 0) {
            Self::Main
        } else if outputs.zip(1..).all(|(output, own)| output == own) {
            Self::Own
        } else {
            Self::Mixed
        }
    }
}

/// Routes every slot at once: to its own output (`own`), or back to main `L+R`.
///
/// One balanced edit of the sixteen existing instance settings through the host parameter path, so
/// automation, host state and the DSP's tail transfer treat it exactly as sixteen selector edits.
fn route_every_slot(params: &MxmDrumMachineParams, setter: &ParamSetter<'_>, own: bool) {
    let edits: Vec<(&dyn binding::ErasedParam, f32)> = params
        .slots
        .iter()
        .zip(1..)
        .map(|(slot, number)| {
            let target = if own { number } else { 0 };
            let param: &dyn binding::ErasedParam = &slot.output;
            (param, slot.output.preview_normalized(target))
        })
        .collect();
    set_together(setter, &edits);
}

/// The sample-pack export action, drawn beside Resample (plan §4.7).
///
/// **Available only while Resample is engaged** (owner, 2026-09-21): you export the samples you
/// have been listening to, which is how you know they sound right before they leave the plugin.
/// Disengaged, the button is present and disabled — the collection's treatment for a control that
/// cannot be used — so the feature is discoverable rather than hidden.
///
/// `width` is the toggle's, because the two are one decision and sat as a wide control above a
/// narrow one until the owner said so (2026-09-21). The caller measures it once and gives it to
/// both, so neither can drift from the other.
///
/// The work goes to the plugin's capture thread and never through the audio thread: it renders and
/// writes files, neither of which belongs in `process` or in an editor frame.
fn export_button(ui: &mut Ui, engaged: bool, width: f32, spawn: Option<&Spawn>) {
    // **"Export samples", not "Export pack"** (owner, 2026-09-22, and their original wording).
    // The preset browser's footer already has an `Export…` that packs *presets* into a bank file,
    // a few centimetres away in the same window; two buttons reading "Export" that do unrelated
    // things is a trap. "Samples" is the word that separates them.
    let picking = mxm_ui::offthread::running::<std::path::PathBuf>(ui.ctx(), folder_picker());
    let button = ui
        .add_enabled(
            engaged && spawn.is_some() && !picking,
            egui::Button::new(if picking {
                EXPORT_CHOOSING
            } else {
                EXPORT_LABEL
            })
            .min_size(egui::vec2(width, mxm_ui::space::MIN_TARGET)),
        )
        .on_hover_text(EXPORT_HELP)
        .on_disabled_hover_text(EXPORT_DISABLED);
    if button.clicked() {
        start_export(ui.ctx());
    }
}

/// Asks where to write the pack, off the frame.
///
/// A pack's natural destination is removable media — an SD card for a tracker — so the export asks
/// where (owner, 2026-09-21). The dialog is **blocking**, and blocking an editor frame hangs the
/// host, so `mxm_ui::offthread` runs it on its own thread and [`collect_export`] takes the answer on
/// a later frame. That is the pattern the sampler and the convolution already use for their file
/// picks; a *folder* pick is the collection's first, and its Linux portal path is unverified.
fn start_export(ctx: &egui::Context) {
    let start = crate::pack::root();
    mxm_ui::offthread::start(ctx, folder_picker(), move || {
        let mut dialog = rfd::FileDialog::new().set_title("Write the sample pack into");
        // Opening on the default pack folder makes "the usual place" one more click rather
        // than a navigation, without taking the choice away.
        if let Some(start) = start.filter(|path| path.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    });
}

/// Hands a chosen folder to the capture thread, on whichever frame the dialog finished on.
///
/// **Every frame, wherever the export was started** — from the bar's button or from the `…` menu
/// that holds it at the bar's last step — so the answer is collected even when neither is drawn.
/// Cancelling picks nothing and writes nothing, which is the whole of the cancel path.
fn collect_export(ctx: &egui::Context, spawn: Option<&Spawn>) {
    if let Some(chosen) = mxm_ui::offthread::take::<std::path::PathBuf>(ctx, folder_picker())
        && let Some(spawn) = spawn
    {
        spawn(crate::CaptureTask::ExportPack {
            sample_rate: PACK_SAMPLE_RATE,
            destination: Some(chosen),
        });
    }
}

/// The pair as the app bar's `…` menu lists it at the bar's last step, in [`RESAMPLE_ITEM`] and
/// [`EXPORT_ITEM`] order, with the export's status under them when there is one: the same words,
/// the same enabled state and the same help as the pair drawn on the bar.
fn resample_menu_items(
    ctx: &egui::Context,
    params: &MxmDrumMachineParams,
    telemetry: &Telemetry,
    spawn: Option<&Spawn>,
) -> Vec<mxm_ui::shell::MenuItem> {
    use mxm_ui::shell::{MenuItem, MenuKind};
    let engaged = params.resample.value();
    let picking = mxm_ui::offthread::running::<std::path::PathBuf>(ctx, folder_picker());
    let mut items = vec![
        MenuItem {
            label: params.resample.name().to_owned(),
            hover: RESAMPLE_HELP.to_owned(),
            kind: MenuKind::Toggle(engaged),
            disabled: None,
        },
        MenuItem {
            label: (if picking {
                EXPORT_CHOOSING
            } else {
                EXPORT_LABEL
            })
            .to_owned(),
            hover: EXPORT_HELP.to_owned(),
            kind: MenuKind::Action,
            disabled: (!(engaged && spawn.is_some() && !picking))
                .then(|| EXPORT_DISABLED.to_owned()),
        },
    ];
    let status = export_status_text(ctx, telemetry);
    if !status.is_empty() {
        items.push(MenuItem {
            label: status,
            hover: String::new(),
            kind: MenuKind::Note,
            disabled: None,
        });
    }
    items
}

/// Where the folder pick is parked while the dialog is open.
fn folder_picker() -> egui::Id {
    egui::Id::new("mxm-drum-machine-pack-folder")
}

/// The export status line, under the pair rather than beside them.
///
/// It reserves its space at rest so the card's measured floor does not move when a result arrives.
fn export_status(ui: &mut Ui, tokens: &Tokens, telemetry: &Telemetry) {
    let text = export_status_text(ui.ctx(), telemetry);
    ui.label(egui::RichText::new(text).color(tokens.text_secondary));
}

/// The last export's outcome in words, empty until there is one. It takes a fresh outcome off the
/// telemetry and remembers it, so it can be asked twice in a frame — by the bar and by its menu.
fn export_status_text(ctx: &egui::Context, telemetry: &Telemetry) -> String {
    if let Some(outcome) = telemetry.take_export() {
        ctx.memory_mut(|memory| memory.data.insert_temp(export_status_id(), outcome));
    }
    let status = ctx.memory(|memory| {
        memory
            .data
            .get_temp::<crate::telemetry::Export>(export_status_id())
    });
    match status {
        Some(crate::telemetry::Export::Written { files, clipped: 0 }) => {
            format!("Wrote {files} samples")
        }
        Some(crate::telemetry::Export::Written { files, clipped }) => {
            format!("Wrote {files} samples, {clipped} clipped")
        }
        Some(crate::telemetry::Export::NoDestination) => {
            "No folder to write to on this system".to_owned()
        }
        Some(crate::telemetry::Export::Failed(why)) => format!("Could not write: {why}"),
        // Reserved rather than absent: a line that appears only after an export would move the
        // card's floor, which the paging tests measure.
        None => String::new(),
    }
}

/// Where the last export's outcome is remembered between frames.
fn export_status_id() -> egui::Id {
    egui::Id::new("mxm-drum-machine-export-status")
}

/// The kit-wide Resample toggle and the export beside it (plan §4.7).
///
/// Kit-wide like the row above it, and on this card because a frozen kit is an output-stage
/// decision rather than a per-slot sound one — and because design system §3.1 keeps a rare action
/// out of the app bar. Unlike *All slots* Resample **is** a parameter, so it goes through the
/// ordinary bracketed gesture; it is an instance setting, so no kit moves it.
///
/// **The two share a row and a width** (owner, 2026-09-21). They are one decision — freeze the
/// kit, then write the frozen kit out — and drawing a wide toggle above a narrow button read as
/// two unrelated controls.
///
/// **In the app bar, laid out along it** (owner, 2026-09-22), which mxm-kit's `crates/ui` requires
/// of anything in the bar (its `NOTES.md`, *A control in the app bar is laid out along it*): the
/// pair opens its own left-to-right region inside the bar's right-to-left group, and reserves a
/// fixed width so nothing downstream jitters as the status text changes. The status is a bar-width
/// label rather than the card's line, because the bar is one row high.
fn resample_pair(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmDrumMachineParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    spawn: Option<&Spawn>,
) {
    let engaged = params.resample.value();
    let width = resample_pair_width(ui, params);
    ui.allocate_ui_with_layout(
        egui::vec2(
            width * 2.0 + ui.spacing().item_spacing.x,
            mxm_ui::space::MIN_TARGET,
        ),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            mxm_ui::navigation::bar_card(ui, RESAMPLE_CARD, |ui| {
                // The pair shares one width: the toggle is at least the Export button's.
                mxm_ui::control::toggle_stack(ui, width, |ui| {
                    ui.scope(|ui| {
                        binding::toggle(
                            ui,
                            tokens,
                            "resample",
                            &params.resample,
                            RESAMPLE_HELP,
                            setter,
                        );
                    })
                    .response
                    .rect
                })
            });
            export_button(ui, engaged, width, spawn);
        },
    );
    export_status(ui, tokens, telemetry);
}

/// The kit-wide output row, under the selected slot's own Output selector: one gesture instead of
/// sixteen, for the multi-out session a host such as Bitwig then builds with its own chains.
fn kit_outputs_row(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmDrumMachineParams,
    setter: &ParamSetter<'_>,
) {
    let current = KitOutputs::of(params);
    let mut selected = current as usize;
    let changed = mxm_ui::navigation::at(ui, KIT_OUTPUTS_ID, |ui| {
        mxm_ui::control::segmented(
            ui,
            tokens,
            KIT_OUTPUTS_LABEL,
            &KIT_OUTPUT_OPTIONS,
            &mut selected,
            None,
            None,
            // Neither is lit when the slots go to different places.
            &[
                "Every slot plays through the main stereo output.",
                "Each slot plays through its own output, for mixing drums separately.",
            ],
        )
    });
    if changed && selected != current as usize {
        route_every_slot(params, setter, selected == KitOutputs::Own as usize);
    }
}

fn tokens_for(ui: &Ui) -> Tokens {
    if ui.visuals().dark_mode {
        mxm_ui::DARK
    } else {
        mxm_ui::LIGHT
    }
}

/// The paging items as the editor computes them for slot 1 at Init, from a context set up as an
/// editor's is — three passes in, so the weighted font cuts are bound — for tests, which have no
/// editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    items_in(&MxmDrumMachineParams::default(), 0)
}

/// [`page_items`] for `params` with `slot` selected, in a scratch editor context.
#[cfg(test)]
pub(crate) fn items_in(
    params: &MxmDrumMachineParams,
    slot: usize,
) -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, params, slot);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in paging order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_drum_machine_dsp::model::ModelSpec;

    use mxm_plugin_test::keyboard_checks;
    use mxm_plugin_test::{opening_size, paging_checks};

    /// What this editor keeps behind a disclosure, opened so the checks see it.
    const REVEAL: fn(&egui::Context) = |_| {};

    /// Drives `panel` with throwaway editor state, which the paging and keyboard checks each run
    /// many times over.
    fn drive<'a>(
        params: &'a MxmDrumMachineParams,
        telemetry: &'a Telemetry,
        setter: &'a ParamSetter<'a>,
        state: &'a mut (
            usize,
            HashMap<&'static str, Option<String>>,
            PresetUi,
            mxm_ui::navigation::State,
        ),
    ) -> impl FnMut(&mut Ui) + 'a {
        move |ui| {
            let (slot, text_entry, presets, nav) = state;
            // No host here, so no executor: the export button draws disabled, which is the
            // same state it has whenever Resample is off.
            panel(
                ui, params, telemetry, setter, slot, text_entry, presets, nav, None,
            );
        }
    }

    #[test]
    fn engaging_resample_freezes_every_axis_but_tune_decay_and_soft_hits() {
        // A frozen slot plays a recording, so only the three axes that mean something over a
        // buffer stay live: its rate, its length and the velocity curve it is played at. The rest are narrowed into the same mask an unsupported axis uses, which is
        // what makes them draw disabled rather than merely inert.
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { params.slots[0].model._internal_set_plain_value(1) };

        let live = shaping_capabilities(&params, 0);
        assert!(
            live.tone || live.body || live.attack,
            "a kick shapes something"
        );

        // SAFETY: as above.
        unsafe { params.resample._internal_set_plain_value(true) };
        let frozen = shaping_capabilities(&params, 0);
        assert_eq!(frozen.pitch, live.pitch, "Tune survives a freeze");
        assert_eq!(frozen.decay, live.decay, "and so does Decay");
        assert_eq!(frozen.dynamics, live.dynamics, "and Soft hits");
        for (name, value) in [
            ("pitch_envelope", frozen.pitch_envelope),
            ("pitch_decay", frozen.pitch_decay),
            ("attack", frozen.attack),
            ("tone", frozen.tone),
            ("body", frozen.body),
            ("noise", frozen.noise),
            ("noise_decay", frozen.noise_decay),
            ("character", frozen.character),
        ] {
            assert!(!value, "{name} should hold still while frozen");
        }
    }

    #[test]
    fn freezing_only_ever_narrows_what_a_model_can_do() {
        // Narrowing, not replacing. Across the whole catalogue, a frozen axis must never be
        // available where the live model does not support it — freezing cannot invent a pitch
        // law for a noise voice, which is the mistake §3.4 exists to prevent.
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        for spec in AVAILABLE_MODELS.iter() {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                params.resample._internal_set_plain_value(false);
                params.slots[0]
                    .model
                    ._internal_set_plain_value(i32::from(spec.id.raw()));
            }
            let live = shaping_capabilities(&params, 0);
            // SAFETY: as above.
            unsafe { params.resample._internal_set_plain_value(true) };
            let frozen = shaping_capabilities(&params, 0);
            let pairs = [
                ("pitch", frozen.pitch, live.pitch),
                ("decay", frozen.decay, live.decay),
                ("pitch_envelope", frozen.pitch_envelope, live.pitch_envelope),
                ("pitch_decay", frozen.pitch_decay, live.pitch_decay),
                ("attack", frozen.attack, live.attack),
                ("tone", frozen.tone, live.tone),
                ("body", frozen.body, live.body),
                ("noise", frozen.noise, live.noise),
                ("noise_decay", frozen.noise_decay, live.noise_decay),
                ("character", frozen.character, live.character),
                ("dynamics", frozen.dynamics, live.dynamics),
            ];
            for (name, frozen, live) in pairs {
                assert!(
                    !frozen || live,
                    "model {} ({}) gained {name} by being frozen",
                    spec.id.raw(),
                    spec.label
                );
            }
        }
    }

    fn editor_state(
        params: &MxmDrumMachineParams,
    ) -> (
        usize,
        HashMap<&'static str, Option<String>>,
        PresetUi,
        mxm_ui::navigation::State,
    ) {
        (
            0,
            HashMap::new(),
            PresetUi::at(crate::preset::Library::at(None), params),
            mxm_ui::navigation::State::default(),
        )
    }

    /// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09.
    /// `REFERENCE` is a constant, so nothing but this says it is still the size the panel wants.
    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &REVEAL,
            &mut drive(&params, &telemetry, &setter, &mut state),
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut drive(&params, &telemetry, &setter, &mut state),
        );
    }

    /// **At the bar's last step Resample and Export samples are rows of its `…` menu** (the owner,
    /// 2026-09-26), and choosing Resample there is one bracketed write, as the toggle's is: in the
    /// minimum window the pair is not on the bar, the menu lists both, Export is disabled while
    /// Resample is off, and choosing Resample engages the mode.
    #[test]
    fn in_the_minimum_window_the_pair_is_in_the_menu_and_resample_works_from_it() {
        use egui_kittest::kittest::{NodeT, Queryable};

        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        let mut panel = drive(&params, &telemetry, &setter, &mut state);
        let fonts = std::cell::Cell::new(false);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32))
            .build_ui(|ui| {
                if !fonts.replace(true) {
                    mxm_ui::typography::apply(ui.ctx());
                    mxm_ui::theme::apply(ui.ctx());
                }
                // kittest frames an app with an 8-point margin; a host gives the editor its window
                // edge to edge, and the minimum is measured that way.
                let screen = ui.ctx().content_rect();
                ui.scope_builder(egui::UiBuilder::new().max_rect(screen), |ui| panel(ui));
            });
        harness.run_steps(4);
        assert!(
            harness.query_by_label(EXPORT_LABEL).is_none(),
            "the pair is not on the bar in the minimum window"
        );
        harness.get_by_label("…").click();
        harness.run_steps(2);
        assert!(
            harness
                .get_by_label(EXPORT_LABEL)
                .accesskit_node()
                .is_disabled(),
            "Export samples is listed, and disabled while Resample is off"
        );
        harness.get_by_label("Resample").click();
        harness.run_steps(2);
        assert!(params.resample.value(), "choosing Resample engages it");
        assert_eq!(
            host.calls()
                .iter()
                .map(|(call, _)| *call)
                .collect::<Vec<_>>(),
            [Call::Begin, Call::Set, Call::End],
            "one bracketed write, as the toggle's"
        );
    }

    /// Every card reachable on some page, at both sizes and both scales, with nothing painted
    /// outside its viewport. A word-presence check cannot see a control pushed off-screen.
    #[test]
    fn every_dynamic_page_fits_and_every_card_is_reachable() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        paging_checks::verify(
            &test_items(),
            &[
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
                // Between the two, because that is where the owner found a bad layout on
                // 2026-09-22 and neither end had covered it.
                egui::vec2(617.0, 562.0),
                egui::vec2(900.0, 700.0),
                egui::vec2(1280.0, 800.0),
            ],
            drive(&params, &telemetry, &setter, &mut state),
        );
    }

    /// The advertised minimum has to be one the widest card actually fits in, measured from the
    /// pager rather than derived from the same constants the layout uses. The arithmetic version of
    /// this check agreed with itself while the widest card was 32 pt too wide for the window.
    #[test]
    fn the_widest_card_fits_the_pager_viewport_at_the_advertised_minimum() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        let mut panel = drive(&params, &telemetry, &setter, &mut state);

        let widest = test_floors().into_iter().fold(0.0_f32, f32::max);

        let context = egui::Context::default();
        mxm_ui::typography::apply(&context);
        mxm_ui::theme::apply(&context);
        context.all_styles_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32);
        for _ in 0..4 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                &mut panel,
            );
            output.textures_delta.clear();
        }
        let report = mxm_ui::paging::editor::report(&context).expect("a paged panel");
        assert!(
            report.viewport.width() >= widest,
            "the minimum window {size:?} gives the pager a {} pt viewport, but the widest card              needs {widest} pt",
            report.viewport.width()
        );
    }

    /// Card geometry: laid out together, no two cards may sit on top of each other.
    #[test]
    fn no_two_cards_overlap_when_they_are_all_laid_out() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        let mut panel = drive(&params, &telemetry, &setter, &mut state);

        // Tall enough to lay every card out at once; the paging check covers the real sizes.
        let canvas = egui::vec2(REFERENCE.0 as f32, 6_000.0);
        let context = egui::Context::default();
        mxm_ui::typography::apply(&context);
        mxm_ui::theme::apply(&context);
        context.all_styles_mut(|style| style.animation_time = 0.0);
        for _ in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
                    ..Default::default()
                },
                &mut panel,
            );
            output.textures_delta.clear();
        }

        let rects = paging_checks::all_rects(&context, TITLES.len());
        for (first, a) in rects.iter().enumerate() {
            for (second, b) in rects.iter().enumerate().skip(first + 1) {
                let shared = a.intersect(*b);
                assert!(
                    shared.width() <= 0.5 || shared.height() <= 0.5,
                    "cards {first} and {second} overlap over {shared:?}"
                );
            }
        }
    }

    /// Geometry, which neither a word-presence check nor a card-rectangle check can see: this
    /// reads what egui actually painted. A label or control squeezed past the edge still
    /// contributes its words, and still sits inside a card whose own rectangle fits.
    ///
    /// Run at both supported sizes and both scales, with every route present so the densest page
    /// this editor can produce is the one under test.
    #[test]
    fn nothing_paints_outside_the_viewport_at_any_supported_size() {
        let params = MxmDrumMachineParams::default();
        present_every_route(&params);
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let items = test_items();

        for physical in [
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
        ] {
            for scale in [1.0, 2.0] {
                let size = physical / scale;
                let mut state = editor_state(&params);
                let mut panel = drive(&params, &telemetry, &setter, &mut state);
                let context = egui::Context::default();
                context.set_pixels_per_point(scale);
                mxm_ui::typography::apply(&context);
                mxm_ui::theme::apply(&context);
                context.all_styles_mut(|style| style.animation_time = 0.0);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);

                for item in &items {
                    mxm_ui::paging::editor::request_card(&context, item.key);
                    let mut last = None;
                    for _ in 0..4 {
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            &mut panel,
                        );
                        output.textures_delta.clear();
                        last = Some(output.shapes);
                    }
                    // egui clips everything to the window, so "did anything land outside" is
                    // answered by construction and says nothing. What a squeezed layout actually
                    // produces is **text cut off by its own clip rectangle**: the galley is laid
                    // out wider than the space it was given and the tail is chopped rather than
                    // wrapped or shortened. That is what this looks for, which is why it runs with
                    // every route present, at the minimum size and at 2x.
                    for clipped in last.into_iter().flatten() {
                        let egui::Shape::Text(text) = &clipped.shape else {
                            continue;
                        };
                        let painted = text.visual_bounding_rect();
                        if !painted.is_positive() {
                            continue;
                        }
                        let visible = painted.intersect(clipped.clip_rect);
                        let lost = painted.width() - visible.width().max(0.0);
                        // The app bar is excluded, and deliberately so rather than to make this
                        // pass. It carries variable-length text — the preset name — which has to
                        // truncate somewhere, so "no text is ever clipped" is not a law that holds
                        // there. Inside a card it is: a card's labels are authored and a clipped
                        // one means the layout squeezed it.
                        //
                        // Excluding it did hide two things, found on 2026-09-20 and belonging to
                        // mxm-kit's `crates/ui` rather than to this plugin, which is where the fix
                        // and its own review go: at the minimum window
                        // `mxm_ui::shell::zoom_control` paints its readout from a negative x,
                        // losing ~9 pt off the left edge at every width tried, and the preset name
                        // loses ~33 pt. The first is a positioning bug, not truncation.
                        let in_the_app_bar = clipped.clip_rect.height() < 64.0;
                        assert!(
                            lost <= 1.0 || in_the_app_bar,
                            "card {:?} at {physical:?} x{scale}: {lost:.1} pt of {:?} is clipped away",
                            item.card.title,
                            text.galley.text()
                        );
                    }
                }
            }
        }
    }

    /// The rollout's own failure mode: a control whose `navigation::at` scope was forgotten paints
    /// exactly as before and is simply unreachable from the keyboard. Nothing else would say so.
    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let recorder = Arc::new(keyboard_checks::Recorder::default());
        let setter = ParamSetter::new(recorder.as_ref());
        let mut state = editor_state(&params);
        let owned = keyboard_reachable_ids(&params);
        let reachable: Vec<&str> = owned.iter().map(String::as_str).collect();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(),
            keyboard_checks::Coverage::Within(&reachable),
            &REVEAL,
            &recorder,
            &mut drive(&params, &telemetry, &setter, &mut state),
        );
    }

    /// The cursor must reach every **slot**, not merely register nothing unexpected. This editor
    /// paints one slot at a time, so a single session only ever sees slot 1: a missing
    /// `navigation::at` scope on slots 2–16 is invisible to it. Each knob's routes are painted with
    /// the card that owns the knob, so walking all cards for every slot covers every route.
    ///
    /// A route row is painted only for a route in use, and Init has none: every slot's four routes
    /// are put to work below, so each one's amount and remove are part of this check.
    #[test]
    fn the_keyboard_cursor_reaches_every_slot_and_every_route() {
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let recorder = Arc::new(keyboard_checks::Recorder::default());
        let setter = ParamSetter::new(recorder.as_ref());
        let items = test_items();
        present_every_route(&params);

        let walk = |slot: usize, found: &mut std::collections::HashSet<String>| {
            let mut state = editor_state(&params);
            state.0 = slot;
            let mut panel = drive(&params, &telemetry, &setter, &mut state);
            let session =
                keyboard_checks::Session::new(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32));
            for item in &items {
                mxm_ui::paging::editor::request_card(session.context(), item.key);
                REVEAL(session.context());
                session.settle(&mut panel);
                found.extend(session.registered());
            }
        };

        let mut found = std::collections::HashSet::new();
        for slot in 0..SLOT_COUNT {
            walk(slot, &mut found);
        }

        // Everything the editor owns, route cells included.
        let expected = keyboard_reachable_ids(&params);
        let missing: Vec<&String> = expected.iter().filter(|id| !found.contains(*id)).collect();
        assert!(
            missing.is_empty(),
            "these are drawn but never join the keyboard cursor's registry, so the cursor cannot              reach them: {missing:?}"
        );
        let owned = keyboard_reachable_ids(&params);
        let unknown: Vec<&String> = found.iter().filter(|id| !owned.contains(id)).collect();
        assert!(
            unknown.is_empty(),
            "the registry names {unknown:?}, which are not parameters of this plugin"
        );
    }

    /// Sets route `r` of `slot` to `source` on `target` at `amount`.
    fn route(
        params: &MxmDrumMachineParams,
        slot: usize,
        r: usize,
        source: SourceChoice,
        target: RouteTarget,
        amount: f32,
    ) {
        use nice_plug::params::InternalParamMut;
        let route = params.slots[slot].routes.all()[r];
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            route.source._internal_set_plain_value(source);
            route
                .target
                ._internal_set_plain_value(target.index() as i32);
            route.amount._internal_set_plain_value(amount);
        }
    }

    /// Every slot's four routes on four knobs of four cards — Tune, Body, Decay, Level — at full
    /// negative depth, the widest reading a row shows, so every route control is painted.
    fn present_every_route(params: &MxmDrumMachineParams) {
        let knobs = [Axis::TUNE, Axis::BODY, Axis::DECAY, Axis::Level];
        for slot in 0..SLOT_COUNT {
            for (r, axis) in knobs.into_iter().enumerate() {
                route(params, slot, r, SourceChoice::ALL[r], axis.target(), -1.0);
            }
        }
    }

    /// Every id the editor may register from the keyboard: this plugin's parameters but the ones no
    /// control draws — a route's target, which is the knob its row sits under and is written by
    /// adding the route, and each slot's controls its model does not show (12–20 on every model) —
    /// plus the kit-wide output row, which carries a cursor scope without storing anything of its
    /// own and writes the sixteen `output_*` settings.
    fn keyboard_reachable_ids(params: &MxmDrumMachineParams) -> Vec<String> {
        use mxm_preset::Instrument;
        let unshown: Vec<&str> = (0..SLOT_COUNT)
            .flat_map(|slot| {
                let model = params.slots[slot].model_id();
                (1..=CONTROLS)
                    .filter(move |&k| !Axis::Control(k as u8).shown(model))
                    .map(move |k| slot_ids(slot)[at::control(k)])
            })
            .collect();
        let hidden =
            |id: &str| id.starts_with("route") && id.contains("_target_") || unshown.contains(&id);
        let mut ids: Vec<String> = params
            .parameters()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !hidden(id))
            .map(str::to_owned)
            .collect();
        ids.push(KIT_OUTPUTS_ID.to_owned());
        ids
    }

    /// Adding a source to a knob takes the first free route in one bracketed gesture — its source,
    /// its target and its depth at zero — and a knob that already had the slot keeps its depth.
    #[test]
    fn adding_a_route_takes_the_first_free_slot_in_one_gesture() {
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        // Route 1 busy on Level; routes 2–4 free.
        route(&params, 0, 0, SourceChoice::Lfo1, RouteTarget::Level, 0.5);
        let p = &params.slots[0];
        // A leftover depth in route 2, aimed elsewhere, is not inherited.
        // SAFETY: exclusive test ownership.
        unsafe { p.routes.route2.amount._internal_set_plain_value(0.7) };
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let tune = Axis::TUNE.target();
        add_route(p, tune, SourceChoice::Wheel, &setter);
        assert_eq!(p.routes.route2.source.value(), SourceChoice::Wheel);
        assert_eq!(p.routes.route2.target(), tune);
        assert_eq!(p.routes.route2.amount.value(), 0.0);
        assert_eq!(routes_on(p, tune), vec![1]);
        let calls = host.calls();
        let phase = |call: Call| match call {
            Call::Begin => 0,
            Call::Set => 1,
            Call::End => 2,
        };
        assert_eq!(calls.len(), 9, "three writes, each bracketed: {calls:?}");
        assert!(calls.windows(2).all(|w| phase(w[0].0) <= phase(w[1].0)));

        // Removed and added back, the route keeps its depth.
        // SAFETY: as above.
        unsafe {
            p.routes.route2.amount._internal_set_plain_value(-0.4);
            p.routes
                .route2
                .source
                ._internal_set_plain_value(SourceChoice::Off);
        }
        assert!(routes_on(p, tune).is_empty());
        add_route(p, tune, SourceChoice::Lfo2, &setter);
        assert_eq!(p.routes.route2.amount.value(), -0.4);
        assert_eq!(p.routes.route2.source.value(), SourceChoice::Lfo2);
    }

    /// With every slot taken, nothing more is offered anywhere; a knob's own sources are never
    /// offered twice.
    #[test]
    fn a_knob_offers_what_it_can_still_take() {
        let params = MxmDrumMachineParams::default();
        let p = &params.slots[0];
        let (tune, decay) = (Axis::TUNE.target(), Axis::DECAY.target());
        assert_eq!(offered(p, tune).len(), SourceChoice::ALL.len());
        route(&params, 0, 0, SourceChoice::Lfo1, tune, 0.2);
        assert!(!offered(p, tune).contains(&SourceChoice::Lfo1));
        assert!(offered(p, decay).contains(&SourceChoice::Lfo1));
        for (r, source) in SourceChoice::ALL.iter().enumerate().take(ROUTES).skip(1) {
            route(&params, 0, r, *source, RouteTarget::Pan, 0.2);
        }
        assert!(offered(p, decay).is_empty());
        assert!(free_route(p, decay).is_none());
    }

    fn measured_slot_frame(selected: bool) -> egui::Vec2 {
        let ctx = egui::Context::default();
        let mut size = egui::Vec2::ZERO;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 120.0),
                )),
                ..Default::default()
            },
            |ui| {
                size = slot_frame(&mxm_ui::LIGHT, selected)
                    .show(ui, |ui| {
                        ui.allocate_exact_size(egui::vec2(240.0, 44.0), egui::Sense::hover());
                    })
                    .response
                    .rect
                    .size();
            },
        );
        output.textures_delta.clear();
        size
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Call {
        Begin,
        Set,
        End,
    }

    /// A host that applies each write at once, as a GUI gesture reaches the plugin, and records
    /// every gesture call in order. A parameter is recorded by its pointer's debug text, because
    /// the pointer itself is not `Send` and a GUI context must be.
    #[derive(Default)]
    struct RecordingHost {
        calls: std::sync::Mutex<Vec<(Call, String)>>,
    }

    impl RecordingHost {
        fn calls(&self) -> Vec<(Call, String)> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: Call, param: nice_plug::params::internals::ParamPtr) {
            self.calls
                .lock()
                .unwrap()
                .push((call, format!("{param:?}")));
        }
    }

    impl nice_plug::context::gui::GuiContextInner for RecordingHost {
        // A test double has no host to ask for a restart (nice-plug 0.4).
        fn request_restart(&self) {}
        fn plugin_api(&self) -> nice_plug::prelude::PluginApi {
            nice_plug::prelude::PluginApi::Clap
        }

        unsafe fn raw_begin_set_parameter(&self, param: nice_plug::params::internals::ParamPtr) {
            self.record(Call::Begin, param);
        }

        unsafe fn raw_set_parameter_normalized(
            &self,
            param: nice_plug::params::internals::ParamPtr,
            normalized: f32,
        ) {
            self.record(Call::Set, param);
            // SAFETY: the test owns the parameters and no other thread touches them.
            unsafe {
                param._internal_set_normalized_value(normalized);
            }
        }

        unsafe fn raw_end_set_parameter(&self, param: nice_plug::params::internals::ParamPtr) {
            self.record(Call::End, param);
        }

        fn get_state(&self) -> nice_plug::prelude::PluginState {
            nice_plug::prelude::PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }

        fn set_state(&self, _state: nice_plug::prelude::PluginState) {}
    }

    fn with_outputs(outputs: [i32; SLOT_COUNT]) -> MxmDrumMachineParams {
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        for (slot, output) in params.slots.iter().zip(outputs) {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { slot.output._internal_set_plain_value(output) };
        }
        params
    }

    const MAIN: [i32; SLOT_COUNT] = [0; SLOT_COUNT];
    const OWN: [i32; SLOT_COUNT] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    /// Own outputs except both hats (slots 12 and 13) sharing output 12: the exception the per-slot
    /// selector exists for.
    const HATS_SHARED: [i32; SLOT_COUNT] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 12, 14, 15, 16];
    const ONE_OFF_MAIN: [i32; SLOT_COUNT] = [0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    #[test]
    fn kit_outputs_are_read_from_the_sixteen_settings() {
        assert_eq!(
            KitOutputs::of(&MxmDrumMachineParams::default()),
            KitOutputs::Main
        );
        assert_eq!(KitOutputs::of(&with_outputs(MAIN)), KitOutputs::Main);
        assert_eq!(KitOutputs::of(&with_outputs(OWN)), KitOutputs::Own);
        assert_eq!(
            KitOutputs::of(&with_outputs(HATS_SHARED)),
            KitOutputs::Mixed
        );
        assert_eq!(
            KitOutputs::of(&with_outputs(ONE_OFF_MAIN)),
            KitOutputs::Mixed
        );
        // Everything on output 1 is a shared bus, not "own outputs".
        assert_eq!(
            KitOutputs::of(&with_outputs([1; SLOT_COUNT])),
            KitOutputs::Mixed
        );
    }

    #[test]
    fn routing_every_slot_is_one_balanced_edit_of_the_outputs_alone() {
        use mxm_preset::Instrument;
        for start in [MAIN, OWN, HATS_SHARED, ONE_OFF_MAIN] {
            for own in [true, false] {
                let params = with_outputs(start);
                let before: Vec<(&str, f32)> = params
                    .parameters()
                    .into_iter()
                    .map(|(id, param)| (id, param.normalised()))
                    .collect();
                let host = RecordingHost::default();
                let setter = ParamSetter::new(&host);
                route_every_slot(&params, &setter, own);

                let expected = if own { OWN } else { MAIN };
                for (slot, output) in params.slots.iter().zip(expected) {
                    assert_eq!(slot.output.value(), output, "{start:?} own={own}");
                }
                let kit = if own {
                    KitOutputs::Own
                } else {
                    KitOutputs::Main
                };
                assert_eq!(KitOutputs::of(&params), kit);
                for ((id, was), (_, param)) in before.iter().zip(params.parameters()) {
                    if !id.starts_with("output_") {
                        assert_eq!(param.normalised(), *was, "{id} moved");
                    }
                }

                // Only the outputs that had to move are written, each exactly once, with every
                // gesture opened before the first value and closed after the last.
                let moved: std::collections::HashSet<String> = params
                    .slots
                    .iter()
                    .zip(start.iter().zip(expected))
                    .filter(|(_, (from, to))| *from != to)
                    .map(|(slot, _)| format!("{:?}", slot.output.as_ptr()))
                    .collect();
                let calls = host.calls();
                for call in [Call::Begin, Call::Set, Call::End] {
                    let touched: Vec<&String> = calls
                        .iter()
                        .filter(|(kind, _)| *kind == call)
                        .map(|(_, param)| param)
                        .collect();
                    assert_eq!(touched.len(), moved.len(), "{call:?} count");
                    assert!(touched.iter().all(|param| moved.contains(*param)));
                }
                let phase = |call: Call| match call {
                    Call::Begin => 0,
                    Call::Set => 1,
                    Call::End => 2,
                };
                assert!(
                    calls
                        .windows(2)
                        .all(|pair| phase(pair[0].0) <= phase(pair[1].0)),
                    "a value was set outside its open gesture: {calls:?}"
                );
            }
        }
    }

    #[test]
    fn routing_every_slot_leaves_a_loaded_kit_clean() {
        let params = MxmDrumMachineParams::default();
        mxm_preset::mark_loaded(&params, "Kit", mxm_preset::Origin::Factory);
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        route_every_slot(&params, &setter, true);
        assert_eq!(KitOutputs::of(&params), KitOutputs::Own);
        assert!(!mxm_preset::loaded(&params).is_modified());
        route_every_slot(&params, &setter, false);
        assert!(!mxm_preset::loaded(&params).is_modified());
    }

    #[test]
    fn slot_selection_changes_paint_without_changing_geometry() {
        assert_eq!(measured_slot_frame(false), measured_slot_frame(true));
    }

    #[test]
    fn appending_a_sparse_model_preserves_old_cells_and_steps_only_available_ids() {
        let mut fixture = AVAILABLE_MODELS.to_vec();
        fixture.push(ModelSpec {
            id: ModelId::new(77),
            label: "Fixture metal",
            group: "Metal banks",
        });
        let (before, selected_before) = model_selector_ids(ModelId::new(94), &AVAILABLE_MODELS);
        let (after, selected_after) = model_selector_ids(ModelId::new(94), &fixture);
        assert_eq!(selected_before, selected_after);
        assert_eq!(&after[..before.len()], before);
        assert_eq!(after[selected_after + 1], ModelId::new(77));
        assert_ne!(after[selected_after + 1], ModelId::new(2));
        assert_eq!(
            ModelId::new(94).normalised(),
            f32::from(ModelId::new(94).raw()) / 255.0
        );
    }

    #[test]
    fn an_unavailable_current_id_is_visible_but_not_an_alias() {
        let current = ModelId::new(200);
        let (ids, selected) = model_selector_ids(current, &AVAILABLE_MODELS);
        assert_eq!(ids[selected], current);
        assert_eq!(selected, AVAILABLE_MODELS.len());
        assert_eq!(
            current.implemented(),
            mxm_drum_machine_dsp::model::ImplementedModel::Off
        );
    }

    #[test]
    fn sound_previews_are_finite_actual_model_traces() {
        let render = |model| {
            let patch = SlotPatch {
                model,
                ..SlotPatch::default()
            };
            render_sound_preview(patch, preview_signature(patch))
        };
        let kick = render(ModelId::DEEP_BRIDGE_KICK);
        let ride = render(ModelId::SIX_BIT_RIDE);
        for sample in kick
            .low
            .iter()
            .chain(&kick.high)
            .chain(&ride.low)
            .chain(&ride.high)
        {
            assert!(sample.is_finite() && (-1.0..=1.0).contains(sample));
        }
        let difference: f32 = kick
            .low
            .iter()
            .chain(&kick.high)
            .zip(ride.low.iter().chain(&ride.high))
            .map(|(kick, ride)| (kick - ride).abs())
            .sum();
        assert!(difference > 1.0, "different DSP models reused one trace");
    }

    use mxm_plugin_test::tree_checks;

    /// Runs the layout tree's checks (plans/plan-layout-tree.md §4.3, `tree_checks::card`) over
    /// `cards` with `slot` selected: each card's computed floor holds its content with nothing
    /// painted outside the card, its content floor is exact, the height its tree states is the height
    /// it draws, and every leaf stays in the room it was given.
    ///
    /// A slot card is checked twice: at its floor, where the usability minimum binds, and again
    /// at its content floor, so the width a slot row states is held to what it paints even though
    /// the editor never draws the card that narrow.
    /// The Model selector keeps its search under its widget id, and a search must survive choosing
    /// another slot — assigning several slots from one search. So the leaf's `Ui`, which the
    /// selector's id is made from, is the same whichever slot the card was built for.
    #[test]
    fn the_model_selector_keeps_its_id_across_slots() {
        let params = MxmDrumMachineParams::default();
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        for slot in [0, SLOT_COUNT - 1] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let tree = card(ui, 3, &params, slot);
                mxm_ui::tree::show(ui, &mxm_ui::LIGHT, &tree, |ui, leaf, _| {
                    if matches!(leaf, Leaf::Model) {
                        ids.push(ui.id());
                    }
                });
            });
            output.textures_delta.clear();
        }
        assert_eq!(
            ids.len(),
            2,
            "the Model card draws its selector for every slot"
        );
        assert_eq!(ids[0], ids[1], "the selector's id follows the slot");
    }

    fn check_cards(state: &str, params: &MxmDrumMachineParams, slot: usize, cards: &[usize]) {
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let floors: Vec<f32> = items_in(params, slot)
            .iter()
            .map(|item| item.card.floor)
            .collect();
        let checks = cards.iter().flat_map(|&index| {
            let content = (index < 2).then(|| content_floor(params, slot, index));
            std::iter::once((index, floors[index])).chain(content.map(|floor| (index, floor)))
        });
        for (index, floor) in checks {
            let mut selected = slot;
            let mut entries = HashMap::new();
            let mut live = Live {
                params,
                setter: &setter,
                slot,
                selected_slot: &mut selected,
                entries: &mut entries,
                // Half-lit, so every row paints its activity bar too.
                peaks: [0.5; SLOT_COUNT],
                text_editing: false,
                // A tempo, so a synced rate reads its division — the widest reading it has.
                tempo: Some(120.0),
            };
            tree_checks::card(
                &|_| {},
                state,
                TITLES[index],
                floor,
                &|ui| card(ui, index, params, slot),
                &mut |ui, leaf, rect| paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live),
            );
        }
    }

    /// Card `index`'s content floor alone — its tree's narrowest and the card's chrome, with no
    /// usability minimum — in a scratch editor context.
    fn content_floor(params: &MxmDrumMachineParams, slot: usize, index: usize) -> f32 {
        let ctx = egui::Context::default();
        mxm_ui::typography::apply(&ctx);
        mxm_ui::theme::apply(&ctx);
        let mut floor = 0.0;
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let body = mxm_ui::shell::body_ui(ui);
                floor = mxm_ui::tree::card_floor(
                    &body,
                    TITLES[index],
                    &card(&body, index, params, slot),
                );
            });
            output.textures_delta.clear();
        }
        floor
    }

    /// Every card, in every state that changes what it holds, passes the layout tree's checks.
    ///
    /// This editor's structural-state matrix: Init; every route slot in use at full negative depth,
    /// where a reading carries its sign and every digit — the widest text a row can show — and all
    /// four on one knob; Resample engaged, which disables every axis but Tune, Decay and Soft hits; every LFO
    /// synced, reading musical divisions; the last slot selected; and every model's cards
    /// (`every_models_cards_pass_the_tree_checks`). Slot activity is forced on.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        use nice_plug::params::InternalParamMut;
        let every = [0, 1, 2, 3, 4, 5, 6, 7];
        check_cards("init", &MxmDrumMachineParams::default(), 0, &every);

        let revealed = MxmDrumMachineParams::default();
        present_every_route(&revealed);
        check_cards("every route present", &revealed, 0, &every);

        let stacked = MxmDrumMachineParams::default();
        for (r, source) in SourceChoice::ALL.iter().take(ROUTES).enumerate() {
            route(&stacked, 0, r, *source, Axis::TUNE.target(), -1.0);
        }
        check_cards("four routes on Tune", &stacked, 0, &[4]);

        let frozen = MxmDrumMachineParams::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { frozen.resample._internal_set_plain_value(true) };
        check_cards("Resample on", &frozen, 0, &every);

        let synced = MxmDrumMachineParams::default();
        for sync in [&synced.lfo1_sync, &synced.lfo2_sync, &synced.lfo3_sync] {
            // SAFETY: as above.
            unsafe { sync._internal_set_plain_value(true) };
        }
        check_cards("every LFO synced", &synced, 0, &[2]);

        check_cards(
            "slot 16 selected",
            &MxmDrumMachineParams::default(),
            SLOT_COUNT - 1,
            &every,
        );
    }

    /// The model half of the matrix, apart so the two halves run side by side. What a model changes
    /// in a card is which knobs its shaping cards show and under what names (`controls`), and — for
    /// a legacy Off or an unavailable id — an entry added to the Model selector and the name on its
    /// slot's row. Its description is the selector's hover text, which changes no card. So: each
    /// shaping card once for every different set of knobs and names a model gives it, and Off and
    /// an unavailable id with their slot card.
    #[test]
    fn every_models_cards_pass_the_tree_checks() {
        use nice_plug::params::InternalParamMut;
        let selected = |model: ModelId| {
            let params = MxmDrumMachineParams::default();
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                params.slots[0]
                    .model
                    ._internal_set_plain_value(i32::from(model.raw()));
            }
            params
        };
        let state = |model: ModelId| format!("{} selected", model_name(model));
        let mut seen = std::collections::HashSet::new();
        for spec in AVAILABLE_MODELS.iter() {
            for (card, rows) in [(4, EXCITATION), (5, BODY), (6, ENVELOPES_TONE)] {
                let faces: Vec<(usize, &str)> = rows
                    .iter()
                    .flat_map(|row| row.iter())
                    .filter(|&&k| Axis::Control(k as u8).shown(spec.id))
                    .map(|&k| (k, Axis::Control(k as u8).name(spec.id)))
                    .collect();
                if seen.insert((card, faces)) {
                    check_cards(&state(spec.id), &selected(spec.id), 0, &[card]);
                }
            }
        }
        for model in [ModelId::OFF, ModelId::new(200)] {
            check_cards(&state(model), &selected(model), 0, &[0, 3, 4, 5, 6]);
        }
    }

    /// **A route on a control the model does not show is drawn, never dropped**: its rows stand
    /// under a line naming the control as unused, they can be removed, and nothing more is
    /// offered there. A kick has no Noise, and no model uses Control 12.
    #[test]
    fn a_route_on_a_control_the_model_does_not_show_is_drawn_and_never_offered() {
        let params = MxmDrumMachineParams::default();
        let kick = params.slots[0].model_id();
        let noise = Axis::Control(general::NOISE as u8);
        let twelve = Axis::Control(12);
        assert!(!noise.shown(kick) && !twelve.shown(kick));
        route(&params, 0, 0, SourceChoice::Lfo1, noise.target(), 0.5);
        route(&params, 0, 1, SourceChoice::Wheel, twelve.target(), -0.25);
        let p = &params.slots[0];
        assert!(offered_on(p, noise, kick).is_empty());
        assert!(!offered_on(p, Axis::TUNE, kick).is_empty());
        assert_eq!(noise.stack_label(kick), "Noise (unused)");
        assert_eq!(twelve.stack_label(kick), "Control 12 (unused)");
        check_cards("routes on unshown controls", &params, 0, &[5, 6]);

        // Drawn on the cards, with their amounts and removes reachable.
        let telemetry = Telemetry::default();
        let recorder = Arc::new(keyboard_checks::Recorder::default());
        let setter = ParamSetter::new(recorder.as_ref());
        let mut state = editor_state(&params);
        let mut panel = drive(&params, &telemetry, &setter, &mut state);
        let session =
            keyboard_checks::Session::new(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32));
        let mut found = std::collections::HashSet::new();
        for item in &test_items() {
            mxm_ui::paging::editor::request_card(session.context(), item.key);
            session.settle(&mut panel);
            found.extend(session.registered());
        }
        let ids = slot_ids(0);
        for id in [at::amount(0), at::source(0), at::amount(1), at::source(1)] {
            assert!(found.contains(ids[id]), "{} is not drawn", ids[id]);
        }
        assert!(!found.contains(ids[at::control(general::NOISE)]));
    }

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-drum-machine/<tag>/`,
    /// where `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-drum-machine --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-drum-machine")
            .join(tag);
        let params = MxmDrumMachineParams::default();
        let telemetry = Telemetry::default();
        let host = RecordingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        let mut panel = drive(&params, &telemetry, &setter, &mut state);
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut panel,
        );
    }

    #[test]
    fn sound_preview_signature_tracks_shaping_controls() {
        let reference = SlotPatch {
            model: ModelId::DEEP_BRIDGE_KICK,
            ..SlotPatch::default()
        };
        let longer = SlotPatch {
            decay: 0.5,
            ..reference
        };
        let tuned = SlotPatch {
            pitch_semitones: 6.0,
            ..reference
        };
        assert_ne!(preview_signature(reference), preview_signature(longer));
        assert_ne!(preview_signature(reference), preview_signature(tuned));
        let reference = render_sound_preview(reference, preview_signature(reference));
        let longer = render_sound_preview(longer, preview_signature(longer));
        let tuned = render_sound_preview(tuned, preview_signature(tuned));
        assert_ne!(reference.high, longer.high);
        assert_ne!(
            &reference.high[..PREVIEW_ATTACK_BINS],
            &tuned.high[..PREVIEW_ATTACK_BINS]
        );
    }
}
