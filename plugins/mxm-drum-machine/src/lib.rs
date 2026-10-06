//! `mxm-drum-machine` — sixteen retriggerable circuit slots.
//!
//! This is the thin nice-plug shell. DSP and model identity live in
//! `mxm-drum-machine-dsp`; the current vertical slice implements Deep bridge kick.

macro_rules! plugin_name {
    () => {
        "mxm-drum-machine"
    };
}

pub const NAME: &str = plugin_name!();
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

pub mod capture_bank;
pub mod capture_worker;
pub mod editor;
pub mod pack;
pub mod params;
pub mod preset;
pub mod routes;
pub mod telemetry;

use std::sync::Arc;

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{
    CHOKE_GROUP_MAX, Engine, ModulationPatch, SlotPatch, TriggerGroup,
};
use mxm_drum_machine_dsp::model::ModelId;
use mxm_drum_machine_dsp::routing::Routing;
use mxm_part_routing::{
    Destination, FixedChannelCollision, MonophonicArbitrator, NoteAddress, NoteCandidate,
    NoteOwner, PartAssignment, assignment_matches, claimed_channels,
};
use nice_plug::prelude::*;

use params::MxmDrumMachineParams;

const INTERNAL_BLOCK: usize = 64;
const NUM_CHANNELS: usize = 16;
const MAX_GROUP_NOTE_EVENTS: usize = 256;
const DEV_VIEW_CC: u8 = 119;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";
const INDIVIDUAL_OUTPUT_PORTS: [std::num::NonZeroU32; SLOT_COUNT] =
    [new_nonzero_u32(1); SLOT_COUNT];
/// Named by slot number only (owner, 2026-09-19). A host names a multi-out chain from the port name
/// when the chain is created and keeps it — Bitwig 6.0 ignores a later rename — so a name taken from
/// the slot's current model would go stale on the first model change. A port's name is not its
/// identity: the IDs stay 1…16 in declaration order.
const INDIVIDUAL_OUTPUT_NAMES: [&str; SLOT_COUNT] = [
    "Slot 01", "Slot 02", "Slot 03", "Slot 04", "Slot 05", "Slot 06", "Slot 07", "Slot 08",
    "Slot 09", "Slot 10", "Slot 11", "Slot 12", "Slot 13", "Slot 14", "Slot 15", "Slot 16",
];

/// What the editor can ask the capture thread for (plan §4.7, D9).
///
/// A capture is not here: `process` asks for one through `Telemetry`, and the worker reads it
/// without an editor frame in the path. Only export begins with a person pressing something.
///
/// Export requires Resample engaged (owner, 2026-09-21): you export the samples you have been
/// listening to, which is how you know they sound right before they leave the plugin. The editor
/// refuses it otherwise, and the worker renders rather than reading the audio thread's buffers —
/// a capture is a pure function of the patch, so the two are the same audio.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureTask {
    /// Render the current patch and write it out as a sample pack (`pack`).
    ///
    /// `destination` is `None` for the default sample-pack folder and `Some` for a folder the
    /// person chose. Either way the plugin resolves it before the worker sees it, so the writer
    /// never consults the environment — which is also what keeps a test out of the real folder.
    ExportPack {
        sample_rate: f32,
        destination: Option<std::path::PathBuf>,
    },
}

/// What to call an exported pack: the loaded kit's name, or a plain default.
fn kit_name(params: &MxmDrumMachineParams) -> String {
    let identity = params
        .preset
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match identity.loaded.as_ref() {
        Some(loaded) if !loaded.name.trim().is_empty() => loaded.name.trim().to_owned(),
        _ => "drum kit".to_owned(),
    }
}

/// The patch every capture is rendered from (plan §4.7).
///
/// **Unsmoothed on purpose, twice over.** `next_frame` advances every smoother as it reads, which
/// is a side effect the audio thread owns — a worker calling it would steal steps from the sound.
/// And a smoothed value moves every sample, so it could never serve as the cache key that decides
/// whether a frozen kit still describes the parameters.
///
/// Performance state is deliberately absent: no chromatic offset, no bend, no per-note tuning.
/// Those reach a frozen slot through the reader's playback clock instead, which is what keeps a
/// capture a function of the patch alone. `capture::capture_patch` neutralises the mix on top.
fn capture_patches(params: &MxmDrumMachineParams) -> [SlotPatch; SLOT_COUNT] {
    std::array::from_fn(|index| {
        let slot = &params.slots[index];
        SlotPatch {
            model: ModelId::new(slot.model.value().clamp(0, 255) as u8),
            pitch_semitones: slot.pitch.value(),
            pitch_envelope: slot.pitch_env.value(),
            pitch_decay: slot.pitch_decay.value(),
            decay: slot.decay.value(),
            attack: slot.attack.value(),
            tone: slot.tone.value(),
            body: slot.body.value(),
            noise: slot.noise.value(),
            noise_decay: slot.noise_decay.value(),
            character: slot.character.value(),
            dynamics: slot.dynamics.value(),
            level: 1.0,
            pan: 0.0,
            choke_group: slot
                .choke_group
                .value()
                .clamp(0, i32::from(CHOKE_GROUP_MAX)) as u8,
        }
    })
}

pub struct MxmDrumMachine {
    params: Arc<MxmDrumMachineParams>,
    engine: Box<Engine>,
    sample_rate: f32,
    bend: [f32; NUM_CHANNELS],
    wheel: [f32; NUM_CHANNELS],
    pressure: [f32; NUM_CHANNELS],
    slot_channel: [u8; SLOT_COUNT],
    tuning: [f32; SLOT_COUNT],
    chromatic_pitch: [f32; SLOT_COUNT],
    /// The note that owns each slot's current hit. It addresses the hit until choke, retrigger or
    /// panic, so a NoteChoke still finds a one-shot tail after its note was released.
    owner: [Option<NoteOwner>; SLOT_COUNT],
    /// Whether per-note expression still follows `owner`. NoteOff and CC123 end this, not the hit.
    owns_expression: [bool; SLOT_COUNT],
    multi_output: bool,
    /// The rate the installed kit was rendered at, or `None` for no kit.
    ///
    /// **Not the patch.** A frozen kit is captured once, when Resample is engaged, and a
    /// parameter edit never re-renders it (owner, 2026-09-21) — so the patch it came from is not
    /// something anything needs to compare against. The rate is, because a kit rendered at a rate
    /// the engine has since left is unplayable rather than merely out of date.
    captured: Option<f32>,
    /// The lock-free handoff the capture worker publishes into (`capture_bank`).
    bank: Arc<capture_bank::CaptureBank>,
    /// Whether this engage has already asked for its one capture.
    ///
    /// Cleared by disengaging, which is the only thing that arms another — the owner's rule is
    /// that a frozen kit is captured once and edits happen with Resample off.
    asked: bool,
    /// **Which engage this is**: advanced when a disengage ends one and at every activation.
    ///
    /// A request carries it and the worker stamps it on the kit (`KitCapture::request`), and only
    /// a kit stamped with the current one installs. Off, edit, on is the owner's way to change a
    /// frozen kit, and a kit the old engage asked for can land after it — still rendering, or
    /// waiting in the bank because the disengage came first. Installed, it was the old patch, and
    /// having a kit the new engage never asked for the edited one.
    engagement: u32,
    /// The thread that renders kits, spawned at `activate` and joined when the plugin is dropped.
    ///
    /// The plugin owns it rather than using nice-plug's shared background executor;
    /// `capture_worker` documents the crash that forced that, at length.
    worker: Option<capture_worker::CaptureWorker>,
    /// The editor's way of reaching that thread, which exists whether or not one is running.
    export_gate: capture_worker::Gate,
    telemetry: Arc<telemetry::Telemetry>,
    routing: [Routing; SLOT_COUNT],
    /// Each LFO's rate its sync resolved for this block, or `None` for its free rate.
    synced_lfo_hz: [Option<f32>; 3],
    tempo_bpm: Option<f32>,
    dev_cc: bool,
}

impl Default for MxmDrumMachine {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmDrumMachineParams::default()),
            engine: Box::new(Engine::new()),
            sample_rate: 48_000.0,
            bend: [0.0; NUM_CHANNELS],
            wheel: [0.0; NUM_CHANNELS],
            pressure: [0.0; NUM_CHANNELS],
            slot_channel: [0; SLOT_COUNT],
            tuning: [0.0; SLOT_COUNT],
            chromatic_pitch: [0.0; SLOT_COUNT],
            owner: [None; SLOT_COUNT],
            owns_expression: [false; SLOT_COUNT],
            multi_output: false,
            captured: None,
            bank: Arc::new(capture_bank::CaptureBank::new()),
            asked: false,
            engagement: 0,
            worker: None,
            export_gate: capture_worker::Gate::default(),
            telemetry: telemetry::Telemetry::shared(),
            routing: [Routing::new(); SLOT_COUNT],
            tempo_bpm: None,
            synced_lfo_hz: [None; 3],
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PartEvent {
    Off(NoteAddress),
    Choke(NoteAddress),
    Tuning {
        address: NoteAddress,
        semitones: f32,
    },
}

#[derive(Debug, Clone, Copy)]
struct EventGroup {
    note_ons: MonophonicArbitrator<MAX_GROUP_NOTE_EVENTS>,
    events: [Option<PartEvent>; MAX_GROUP_NOTE_EVENTS],
    local_count: usize,
    /// More releases, chokes and tunings arrived than `events` holds.
    local_overflow: bool,
    /// Any NoteChoke arrived, whether or not it was stored.
    any_choke: bool,
    panic: bool,
    all_notes_off: bool,
    bend: [Option<f32>; NUM_CHANNELS],
    wheel: [Option<f32>; NUM_CHANNELS],
    pressure: [Option<f32>; NUM_CHANNELS],
}

impl EventGroup {
    fn new() -> Self {
        Self {
            note_ons: MonophonicArbitrator::new(),
            events: [None; MAX_GROUP_NOTE_EVENTS],
            local_count: 0,
            local_overflow: false,
            any_choke: false,
            panic: false,
            all_notes_off: false,
            bend: [None; NUM_CHANNELS],
            wheel: [None; NUM_CHANNELS],
            pressure: [None; NUM_CHANNELS],
        }
    }

    fn push_note_on(&mut self, candidate: NoteCandidate) {
        // Refuse an adversarial same-sample NoteOn flood as panic rather than install a partially
        // collected chord whose winner depends on which event crossed the bound.
        if !self.note_ons.push(candidate) {
            self.panic = true;
        }
    }

    /// Stores a release, choke or tuning. Past capacity only the flags remain, and
    /// `resolve_events` replaces the whole group's local events with a superset no host order can
    /// change (see there), so a release flood cannot cut every tail.
    fn push_local(&mut self, event: PartEvent) {
        self.any_choke |= matches!(event, PartEvent::Choke(_));
        if self.local_count < MAX_GROUP_NOTE_EVENTS {
            self.events[self.local_count] = Some(event);
            self.local_count += 1;
        } else {
            self.local_overflow = true;
        }
    }

    fn local_events(&self) -> impl Iterator<Item = PartEvent> + '_ {
        self.events[..self.local_count].iter().flatten().copied()
    }

    fn collect(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn {
                voice_id,
                channel,
                note,
                velocity,
                ..
            } if velocity.is_finite() && velocity > 0.0 => {
                self.push_note_on(NoteCandidate {
                    owner: NoteOwner {
                        channel,
                        key: note,
                        note_id: voice_id,
                    },
                    strike: velocity.clamp(0.0, 1.0),
                });
            }
            NoteEvent::NoteOn {
                voice_id,
                channel,
                note,
                ..
            }
            | NoteEvent::NoteOff {
                voice_id,
                channel,
                note,
                ..
            } => {
                self.push_local(PartEvent::Off(note_address(channel, note, voice_id)));
            }
            NoteEvent::Choke {
                voice_id,
                channel,
                note,
                ..
            } => {
                self.push_local(PartEvent::Choke(note_address(channel, note, voice_id)));
            }
            NoteEvent::PolyTuning {
                voice_id,
                channel,
                note,
                tuning,
                ..
            } if tuning.is_finite() => {
                self.push_local(PartEvent::Tuning {
                    address: note_address(channel, note, voice_id),
                    semitones: tuning.clamp(-48.0, 48.0),
                });
            }
            NoteEvent::MidiPitchBend { channel, value, .. } if value.is_finite() => {
                self.bend[channel as usize % NUM_CHANNELS] =
                    Some((2.0 * (value - 0.5)).clamp(-1.0, 1.0));
            }
            NoteEvent::MidiChannelPressure {
                channel, pressure, ..
            } if pressure.is_finite() => {
                self.pressure[channel as usize % NUM_CHANNELS] = Some(pressure.clamp(0.0, 1.0));
            }
            NoteEvent::MidiCC {
                channel,
                cc: 1,
                value,
                ..
            } if value.is_finite() => {
                self.wheel[channel as usize % NUM_CHANNELS] = Some(value.clamp(0.0, 1.0));
            }
            NoteEvent::MidiCC { cc, .. } if cc == control_change::ALL_SOUND_OFF => {
                self.panic = true
            }
            NoteEvent::MidiCC { cc, .. } if cc == control_change::ALL_NOTES_OFF => {
                self.all_notes_off = true
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedEvents {
    triggers: TriggerGroup,
    chokes: u16,
    panic: bool,
}

fn note_address(channel: u8, note: u8, note_id: Option<i32>) -> NoteAddress {
    // nice-plug 0.3 casts CLAP's legal -1 channel/key wildcards to u8 for choke events.
    NoteAddress {
        channel: (channel != u8::MAX).then_some(channel),
        key: (note != u8::MAX).then_some(note),
        note_id,
    }
}

impl MxmDrumMachine {
    fn midi_assignments(&self) -> [PartAssignment; SLOT_COUNT] {
        std::array::from_fn(|slot| {
            PartAssignment::from_parameter(self.params.slots[slot].midi_channel.value())
        })
    }

    fn output_destinations(&self) -> [Destination; SLOT_COUNT] {
        if self.multi_output {
            std::array::from_fn(|slot| {
                Destination::from_parameter(self.params.slots[slot].output.value(), SLOT_COUNT)
            })
        } else {
            [Destination::Main; SLOT_COUNT]
        }
    }

    fn resolve_events(&mut self, events: &EventGroup) -> ResolvedEvents {
        // A local-event flood keeps only what no host order can change: a choke anywhere in the
        // group becomes panic; otherwise every owner releases expression and no tuning in the group
        // applies. Stored events still apply, which the superset already includes.
        let overflow = events.local_overflow;
        if events.panic || (overflow && events.any_choke) {
            self.bend = [0.0; NUM_CHANNELS];
            self.wheel = [0.0; NUM_CHANNELS];
            self.pressure = [0.0; NUM_CHANNELS];
            self.tuning = [0.0; SLOT_COUNT];
            self.chromatic_pitch = [0.0; SLOT_COUNT];
            self.owner = [None; SLOT_COUNT];
            self.owns_expression = [false; SLOT_COUNT];
            return ResolvedEvents {
                triggers: TriggerGroup::new(),
                chokes: u16::MAX,
                panic: true,
            };
        }

        for channel in 0..NUM_CHANNELS {
            if let Some(value) = events.bend[channel] {
                self.bend[channel] = value;
            }
            if let Some(value) = events.wheel[channel] {
                self.wheel[channel] = value;
            }
            if let Some(value) = events.pressure[channel] {
                self.pressure[channel] = value;
            }
        }

        let assignments = self.midi_assignments();
        let claimed = claimed_channels(&assignments);
        let release_all = events.all_notes_off || overflow;
        // A same-offset choke removes every NoteOn it matches before any slot arbitrates. That
        // depends on the note alone, so it runs once per group rather than once per slot.
        let mut strikes = events.note_ons;
        if events.any_choke {
            strikes.retain(|owner| {
                !events.local_events().any(
                    |event| matches!(event, PartEvent::Choke(address) if address.matches(owner)),
                )
            });
        }
        let mut resolved = ResolvedEvents {
            triggers: TriggerGroup::new(),
            chokes: 0,
            panic: false,
        };

        for (slot, &assignment) in assignments.iter().enumerate() {
            let accepts_owner = |owner: NoteOwner| {
                assignment_matches(
                    assignment,
                    36 + slot as u8,
                    owner,
                    claimed,
                    FixedChannelCollision::ExcludeClaimed,
                )
            };

            // The stored owner is matched on its own terms, not through the current assignment.
            // A winning owner addresses its hit until choke, retrigger or panic, and reassigning
            // the slot's MIDI channel while the tail rings must not strand that note beyond the
            // reach of its own NoteOff, tuning or choke. `accepts_owner` governs which *new*
            // strikes this slot will admit, below, and nothing else.
            if let Some(owner) = self.owner[slot] {
                let (mut choked, mut released, mut tuning) = (false, false, None);
                for event in events.local_events() {
                    match event {
                        PartEvent::Choke(address) if address.matches(owner) => choked = true,
                        PartEvent::Off(address) if address.matches(owner) => released = true,
                        PartEvent::Tuning { address, semitones }
                            if !overflow && address.matches(owner) =>
                        {
                            tuning = Some(semitones);
                        }
                        _ => {}
                    }
                }
                // Reduced, not replayed, so host order cannot matter: a choke ends the hit even
                // after its note was released; otherwise expression reaches the note before its
                // same-offset release ends ownership, and the tail keeps its current pitch.
                if choked {
                    resolved.chokes |= 1 << slot;
                    self.owner[slot] = None;
                    self.owns_expression[slot] = false;
                    self.tuning[slot] = 0.0;
                } else {
                    if let Some(semitones) = tuning.filter(|_| self.owns_expression[slot]) {
                        self.tuning[slot] = semitones;
                    }
                    if released {
                        self.owns_expression[slot] = false;
                    }
                }
            }
            if release_all {
                self.owns_expression[slot] = false;
            }

            let winner = strikes.resolve(
                accepts_owner,
                // A total order, so host order never picks the owner: channel breaks the last tie,
                // which ID-less MIDI on several unclaimed channels reaches for one Kit key.
                |candidate, current| {
                    (
                        candidate.owner.key,
                        candidate.strike.to_bits(),
                        candidate.owner.note_id,
                        candidate.owner.channel,
                    ) > (
                        current.owner.key,
                        current.strike.to_bits(),
                        current.owner.note_id,
                        current.owner.channel,
                    )
                },
                // Combining is still required so that host event order cannot change the sound,
                // but the value this instrument uses is the winner's own: a hit is its own
                // velocity, and a note nobody can hear must not set the level of the one they can
                // (owner, 2026-09-20).
                f32::max,
            );

            if let Some(winner) = winner {
                let owner = winner.owner;
                self.owner[slot] = Some(owner);
                self.owns_expression[slot] = true;
                self.slot_channel[slot] = owner.channel;
                // A pitched model sits at its own concert pitch, so the keyboard plays it in tune;
                // any other is reached at note 60 (owner, 2026-09-18; plan revision 40).
                self.chromatic_pitch[slot] = if assignment == PartAssignment::FixedNote {
                    0.0
                } else {
                    let model =
                        ModelId::new(self.params.slots[slot].model.value().clamp(0, 255) as u8);
                    f32::from(owner.key) - model.chromatic_reference_key()
                };
                self.tuning[slot] = 0.0;
                for event in events.local_events().filter(|_| !overflow) {
                    if let PartEvent::Tuning { address, semitones } = event
                        && address.matches(owner)
                    {
                        self.tuning[slot] = semitones;
                    }
                }
                resolved.triggers.push(slot, winner.owner_strike);
            }
        }
        resolved
    }

    fn apply_events(&mut self, patches: &[SlotPatch; SLOT_COUNT], events: ResolvedEvents) {
        if events.panic {
            self.engine.reset();
            self.engine
                .reset_output_destinations(&self.output_destinations());
            return;
        }
        for slot in 0..SLOT_COUNT {
            if events.chokes & (1 << slot) != 0 {
                self.engine.choke(slot);
            }
        }
        if !events.triggers.is_empty() {
            self.engine.trigger_group(patches, events.triggers);
        }
    }

    #[inline]
    /// Installs a rendered kit and asks for one when the parameters have moved (plan §4.7).
    ///
    /// **Audio thread, once per `process` call.** Everything here is a pointer swap, a handful of
    /// comparisons and at most one task submission: no allocation, no lock, and nothing dropped —
    /// a displaced kit goes back to `capture_bank` for the worker to free.
    ///
    /// Disengaging is immediate, because dropping the mode needs no render. Engaging asks the
    /// worker once (one capture per engage, below), and only the kit that engage asked for is
    /// installed (`engagement`).
    fn service_captures(&mut self) {
        if !self.params.resample.value() {
            // **A disengage ends the engage**, so whatever it asked for — still rendering, or
            // waiting in the bank — is refused when it lands. Only once per engage: after this
            // block nothing is frozen and nothing asked.
            if self.engine.is_frozen() || self.asked {
                self.engagement = self.engagement.wrapping_add(1);
            }
            if self.engine.is_frozen() {
                if let Some(displaced) = self.engine.set_captures(None) {
                    self.bank.retire(displaced);
                }
                self.captured = None;
            }
            // Cleared whether or not a kit was installed: disengaging is what arms the next
            // capture, and a request that never landed must not count as this engage's one.
            self.asked = false;
            return;
        }

        if let Some(kit) = self.bank.take() {
            // **Only a kit this engage asked for, at this rate, installs**; any other is retired
            // unplayed. One from an earlier engage is the patch as it was
            // (`a_kit_from_an_earlier_engage_is_never_installed`), and this engage has asked, or
            // already has its kit, so nothing more is needed. One at a rate the engine has left
            // would play every slot at the wrong speed, so the comparison below asks again.
            let kit_rate = kit.sample_rate();
            let this_engage = kit.request() == self.engagement;
            #[allow(clippy::float_cmp)]
            let usable = this_engage && kit_rate == self.sample_rate;
            if usable {
                // The `Box` goes straight through: unwrapping it would free an allocation here
                // and reboxing the displaced kit would make one, both of which `process` forbids.
                if let Some(displaced) = self.engine.set_captures(Some(kit)) {
                    self.bank.retire(displaced);
                }
                self.captured = Some(kit_rate);
            } else {
                self.bank.retire(kit);
                if this_engage {
                    // Ask again, at the rate the engine is actually running.
                    self.asked = false;
                }
            }
        }

        // **One capture per engage** (owner, 2026-09-21). A parameter edit while frozen renders
        // nothing: to change a frozen kit you turn Resample off, edit, and turn it on again.
        //
        // This is the rule, not an optimisation. Re-rendering on every edit meant a knob sweep
        // queued a kit per block, and installing one resets every sounding reader — so moving
        // Decay cut and crackled through the whole kit. Invalidating on the patch is what made
        // that possible, so the patch is no longer consulted; only the rate is, because a kit
        // rendered at a rate the engine has left is unplayable rather than merely stale.
        #[allow(clippy::float_cmp)]
        let have = self.captured == Some(self.sample_rate);
        if have {
            self.asked = false;
            return;
        }
        if !self.asked {
            // **`process` asks; it does not render.** `capture_worker` owns the thread and the
            // reason it is not nice-plug's, and `telemetry::capture_wanted` is the whole channel.
            self.asked = true;
            self.telemetry
                .request_capture(self.sample_rate, self.engagement);
        }
    }

    /// Brings the frozen kit into line with the parameters (plan §4.7).
    ///
    /// **A control-thread operation.** It renders seconds of audio and allocates, so it belongs to
    /// `activate` and to the background task — never to `process`. Activation is the important one:
    /// a session or an offline bounce that opens with Resample engaged is frozen from its first
    /// sample, with no worker timing anywhere in the restore path.
    ///
    /// A capture that fails leaves the instrument live rather than silent, which is the safe way
    /// round: the circuits are always a correct rendering of the patch.
    fn refresh_captures(&mut self) {
        if !self.params.resample.value() {
            drop(self.engine.set_captures(None));
            self.captured = None;
            self.asked = false;
            return;
        }
        // Already a kit at this rate: nothing to do, and re-rendering would restart every
        // sounding reader for no audible reason.
        #[allow(clippy::float_cmp)]
        if self.captured == Some(self.sample_rate) {
            return;
        }
        let patches = capture_patches(&self.params);
        match mxm_drum_machine_dsp::capture::capture_kit(&patches, self.sample_rate) {
            Ok(kit) => {
                drop(
                    self.engine
                        .set_captures(Some(Box::new(kit.answering(self.engagement)))),
                );
                self.captured = Some(self.sample_rate);
                self.asked = false;
            }
            Err(_) => {
                drop(self.engine.set_captures(None));
                self.captured = None;
                self.asked = false;
            }
        }
    }

    fn next_frame(&self) -> ([SlotPatch; SLOT_COUNT], ModulationPatch, f32) {
        let any_solo = self.params.slots.iter().any(|slot| slot.solo.value());
        let slots = std::array::from_fn(|index| {
            let params = &self.params.slots[index];
            let channel = self.slot_channel[index] as usize % NUM_CHANNELS;
            let level = params.level.smoothed.next();
            let audible = slot_is_audible(params, any_solo);
            SlotPatch {
                model: ModelId::new(params.model.value().clamp(0, 255) as u8),
                // Channel bend is fixed at ±2 semitones in this first slice; per-note tuning is
                // already delivered by CLAP in semitones.
                pitch_semitones: (params.pitch.smoothed.next()
                    + self.chromatic_pitch[index]
                    + 2.0 * self.bend[channel]
                    + self.tuning[index])
                    .clamp(-48.0, 48.0),
                pitch_envelope: params.pitch_env.smoothed.next(),
                pitch_decay: params.pitch_decay.smoothed.next(),
                decay: params.decay.smoothed.next(),
                attack: params.attack.smoothed.next(),
                tone: params.tone.smoothed.next(),
                body: params.body.smoothed.next(),
                noise: params.noise.smoothed.next(),
                noise_decay: params.noise_decay.smoothed.next(),
                character: params.character.smoothed.next(),
                dynamics: params.dynamics.smoothed.next(),
                level: if audible { level } else { 0.0 },
                pan: params.pan.smoothed.next(),
                // Stepped, not smoothed: a group is an assignment, not a signal.
                choke_group: params
                    .choke_group
                    .value()
                    .clamp(0, i32::from(CHOKE_GROUP_MAX)) as u8,
            }
        });
        let free_rates = [
            self.params.lfo1_rate.smoothed.next(),
            self.params.lfo2_rate.smoothed.next(),
            self.params.lfo3_rate.smoothed.next(),
        ];
        let modulation = ModulationPatch {
            // A synced rate is resolved once per block; the free smoother is advanced as it is with sync off, so turning
            // sync off lands on the current knob rather than an old value.
            lfo_rate_hz: std::array::from_fn(|lfo| {
                self.synced_lfo_hz[lfo].unwrap_or(free_rates[lfo])
            }),
            lfo_shape: [
                self.params.lfo1_shape.value().dsp(),
                self.params.lfo2_shape.value().dsp(),
                self.params.lfo3_shape.value().dsp(),
            ],
            wheel: std::array::from_fn(|slot| {
                self.wheel[self.slot_channel[slot] as usize % NUM_CHANNELS]
            }),
            pressure: std::array::from_fn(|slot| {
                self.pressure[self.slot_channel[slot] as usize % NUM_CHANNELS]
            }),
        };
        (slots, modulation, self.params.master.smoothed.next())
    }

    fn handle_developer_event(&self, event: NoteEvent<()>) {
        if !self.dev_cc {
            return;
        }
        if let NoteEvent::MidiCC { cc, value, .. } = event {
            let raw = (value.clamp(0.0, 1.0) * 127.0).round() as u8;
            match cc {
                DEV_VIEW_CC => self.telemetry.request_view(raw),
                DEV_BROWSER_CC => self.telemetry.request_browser(raw >= 64),
                DEV_THEME_CC => self.telemetry.request_theme(raw),
                _ => {}
            }
        }
    }
}

fn slot_is_audible(params: &params::SlotParams, any_solo: bool) -> bool {
    !params.mute.value() && (!any_solo || params.solo.value())
}

/// Each LFO's synced rate for a block, or `None` for its free rate: its **modulated** position picks a
/// division on the LFO ladder (`params::LFO_SYNC`) at the host's tempo, clamped to what the rate's
/// range holds. The collection's one contract (`plans/plan-tempo-sync-controls.md`).
fn synced_lfo_rates(params: &MxmDrumMachineParams, tempo: Option<f64>) -> [Option<f32>; 3] {
    [
        (&params.lfo1_rate, &params.lfo1_sync),
        (&params.lfo2_rate, &params.lfo2_sync),
        (&params.lfo3_rate, &params.lfo3_sync),
    ]
    .map(|(rate, sync)| {
        params::LFO_SYNC
            .resolve(
                sync.value(),
                tempo,
                rate.modulated_normalized_value(),
                f64::from(rate.preview_plain(0.0)),
                f64::from(rate.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    })
}

impl Plugin for MxmDrumMachine {
    const NAME: &'static str = NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Full routing first by owner ruling: default-layout hosts see every individual output.
    /// Stereo-only remains advertised second for MXM Player and other single-port hosts.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            aux_output_ports: &INDIVIDUAL_OUTPUT_PORTS,
            names: PortNames {
                layout: Some("Main plus 16 mono outputs"),
                main_input: None,
                main_output: Some("L+R"),
                aux_inputs: &[],
                aux_outputs: &INDIVIDUAL_OUTPUT_NAMES,
            },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            names: PortNames {
                layout: Some("Stereo compatibility"),
                main_input: None,
                main_output: Some("L+R"),
                aux_inputs: &[],
                aux_outputs: &[],
            },
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmDrumMachineEditor;
    type SysExMessage = ();
    type BackgroundTask = CaptureTask;

    /// Deliberately empty: this plugin does not use nice-plug's shared background thread.
    ///
    /// A task queued on it when its instance is destroyed kills the thread every other instance
    /// shares, and the next teardown then panics out of a `Drop` — `capture_worker` traces the
    /// path through `background_thread.rs`. Captures run on a thread this plugin owns and joins,
    /// so nothing is ever left queued against a dead instance.
    fn task_executor(&mut self) -> TaskExecutor<Self> {
        Box::new(|_task| {})
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        // Captures are asked for through the ordinary parameter path — toggling Resample is a
        // parameter edit, `process` notices it and the capture thread picks it up with no editor
        // involved. **Export is not**: it is a transient act with no value to save or automate,
        // begun by a person pressing a button, so the editor hands it to the same thread.
        //
        // The gate outlives the thread at both ends, so an editor opened before `activate` or
        // closed after teardown is not a special case: a pack asked for before the thread exists
        // waits for it, and one asked for after it has gone is dropped.
        let spawn: editor::Spawn = {
            let gate = self.export_gate.clone();
            let telemetry = self.telemetry.clone();
            Arc::new(move |task| {
                let CaptureTask::ExportPack {
                    sample_rate,
                    destination,
                } = task;
                // No folder chosen means the default one. Where a platform has no local data
                // directory there is nowhere to default to, and the status line says so rather
                // than the button doing nothing.
                match destination.or_else(pack::root) {
                    Some(root) => gate.request_export(sample_rate, root),
                    None => telemetry.publish_export(telemetry::Export::NoDestination),
                }
            })
        };
        editor::create(self.params.clone(), self.telemetry.clone(), spawn)
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // A new activation starts with no tempo and nothing resolved: the first callback reports
        // the tempo, so neither the audio nor an editor frame before it shows the last session's
        // divisions (`plans/plan-tempo-sync-controls.md`).
        self.telemetry.tempo.publish(None);
        self.synced_lfo_hz = [None; 3];
        if !buffer_config.sample_rate.is_finite()
            || buffer_config.sample_rate < mxm_drum_machine_dsp::resonator::MIN_SAMPLE_RATE
        {
            return false;
        }
        self.sample_rate = buffer_config.sample_rate;
        self.multi_output = audio_io_layout.aux_output_ports.len() == SLOT_COUNT;
        self.engine.set_sample_rate(self.sample_rate);
        self.engine.reset();
        self.engine
            .reset_output_destinations(&self.output_destinations());
        self.asked = false;
        // **An activation supersedes everything asked before it**: a kit still rendering was made
        // for the rate and the patch as they were, and `refresh_captures` below installs the
        // current one itself.
        self.engagement = self.engagement.wrapping_add(1);
        self.bank.drain();
        self.refresh_captures();
        // Spawned here rather than at construction, so a host's scan — which builds the plugin to
        // read its parameters and throws it away — never starts a thread. Once only: a second
        // `activate` (a rate change, say) keeps the thread it already has, because the rate
        // travels with each request rather than being fixed when the thread starts.
        if self.worker.is_none() {
            self.worker = Some(capture_worker::CaptureWorker::spawn(
                &self.export_gate,
                capture_worker::Inputs {
                    params: self.params.clone(),
                    bank: self.bank.clone(),
                    telemetry: self.telemetry.clone(),
                },
            ));
        }
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.bend = [0.0; NUM_CHANNELS];
        self.wheel = [0.0; NUM_CHANNELS];
        self.pressure = [0.0; NUM_CHANNELS];
        self.slot_channel = [0; SLOT_COUNT];
        self.tuning = [0.0; SLOT_COUNT];
        self.chromatic_pitch = [0.0; SLOT_COUNT];
        self.owner = [None; SLOT_COUNT];
        self.owns_expression = [false; SLOT_COUNT];
        self.engine
            .reset_output_destinations(&self.output_destinations());
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let tempo = mxm_tempo::tempo(context.transport().tempo);
        self.tempo_bpm = tempo.map(|tempo| tempo as f32);
        self.synced_lfo_hz = synced_lfo_rates(&self.params, tempo);
        self.telemetry.tempo.publish(tempo);
        self.engine.set_tempo_bpm(self.tempo_bpm.unwrap_or(120.0));
        self.service_captures();
        let samples = buffer.samples();
        let output = buffer.as_slice();
        let mut next_event = context.next_event();

        for block_start in (0..samples).step_by(INTERNAL_BLOCK) {
            for index in 0..SLOT_COUNT {
                let was_live = self.routing[index].any();
                let routing = self.params.slots[index]
                    .routes
                    .routing_from(&self.routing[index]);
                if was_live || routing.any() {
                    self.engine.set_slot_routing(index, routing);
                }
                self.routing[index] = routing;
            }
            let block_end = (block_start + INTERNAL_BLOCK).min(samples);
            let mut block_peak = 0.0_f32;

            let mut index = block_start;
            while index < block_end {
                // Collect and resolve only at event offsets: an event-free sample changes no
                // ownership, so it does not pay for the bounded group.
                let events = if next_event.is_some_and(|event| event.timing() as usize <= index) {
                    let mut events = EventGroup::new();
                    while let Some(event) = next_event {
                        if event.timing() as usize > index {
                            break;
                        }
                        self.handle_developer_event(event);
                        events.collect(event);
                        next_event = context.next_event();
                    }
                    Some(self.resolve_events(&events))
                } else {
                    None
                };

                for index in 0..SLOT_COUNT {
                    if self.routing[index].any() {
                        self.params.slots[index]
                            .routes
                            .advance(&mut self.routing[index]);
                        self.engine.set_slot_routing(index, self.routing[index]);
                    }
                }
                let (patches, modulation, master) = self.next_frame();
                self.engine.prepare(&patches);
                // Before triggers, so a hit on a silent slot starts on its current destination.
                let destinations = self.output_destinations();
                self.engine.route_outputs(&destinations);
                if let Some(events) = events {
                    self.apply_events(&patches, events);
                }
                let frame = self
                    .engine
                    .process_routed(&patches, modulation, &destinations);
                let left = frame.main[0] * master;
                let right = frame.main[1] * master;
                block_peak = block_peak.max(left.abs()).max(right.abs());
                output[0][index] = left;
                output[1][index] = right;
                for (port, sample) in aux.outputs.iter_mut().zip(frame.individual) {
                    let value = sample * master;
                    block_peak = block_peak.max(value.abs());
                    port.as_slice()[0][index] = value;
                }
                index += 1;
            }

            self.telemetry.publish_peak(block_peak);
            self.telemetry
                .publish_slot_peaks(self.engine.take_slot_peaks());
        }

        if self.engine.is_active() {
            ProcessStatus::Tail((7.0 * self.sample_rate).min(u32::MAX as f32) as u32)
        } else {
            ProcessStatus::Normal
        }
    }
}

impl ClapPlugin for MxmDrumMachine {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A sixteen-slot drum machine whose drums are modelled from their circuits, not sampled",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Drum,
        ClapFeature::Stereo,
    ];
}

nice_export_clap!(MxmDrumMachine);

#[cfg(test)]
mod tests {
    use super::*;

    fn note_on(note: u8, velocity: f32) -> NoteEvent<()> {
        note_on_as(0, note, None, velocity)
    }

    fn note_on_as(channel: u8, note: u8, voice_id: Option<i32>, velocity: f32) -> NoteEvent<()> {
        NoteEvent::NoteOn {
            timing: 0,
            voice_id,
            channel,
            note,
            velocity,
        }
    }

    #[test]
    fn every_lfo_can_follow_tempo_and_free_rate_is_the_no_tempo_fallback() {
        use mxm_tempo::Division;
        use nice_plug::params::InternalParamMut;
        let params = MxmDrumMachineParams::default();
        let set = |rate: &FloatParam, division: Division| {
            // SAFETY: the same call the wrapper makes when a host writes a parameter.
            unsafe { rate._internal_set_normalized_value(params::LFO_SYNC.position(division)) };
        };
        set(&params.lfo1_rate, Division::Quarter);
        set(&params.lfo2_rate, Division::Whole);
        set(&params.lfo3_rate, Division::ThirtySecond);
        assert_eq!(
            synced_lfo_rates(&params, Some(120.0)),
            [None; 3],
            "off, free"
        );
        for sync in [&params.lfo1_sync, &params.lfo2_sync, &params.lfo3_sync] {
            // SAFETY: as above.
            unsafe { sync._internal_set_plain_value(true) };
        }
        assert_eq!(
            synced_lfo_rates(&params, Some(120.0)),
            [Some(2.0), Some(0.5), Some(16.0)]
        );
        assert_eq!(synced_lfo_rates(&params, None), [None; 3], "no tempo, free");
    }

    #[test]
    fn the_developer_channel_is_off_unless_the_environment_asked_for_it() {
        // Removing the `dev_cc` guard leaves every other test green while ordinary CC 116/117/119
        // start driving the editor from a MIDI track.
        let cc = |cc: u8, value: f32| -> NoteEvent<()> {
            NoteEvent::MidiCC {
                timing: 0,
                channel: 0,
                cc,
                value,
            }
        };
        let requests = [
            (DEV_VIEW_CC, 1.0),
            (DEV_BROWSER_CC, 1.0),
            (DEV_THEME_CC, 1.0 / 127.0),
            // CC118 is the disclosure channel elsewhere in the collection. This editor has no
            // expander, so it is deliberately a no-op here rather than an omission.
            (118, 1.0),
        ];

        let mut plugin = MxmDrumMachine {
            dev_cc: false,
            ..Default::default()
        };
        for (number, value) in requests {
            plugin.handle_developer_event(cc(number, value));
        }
        assert_eq!(
            plugin.telemetry.take_view_request(),
            None,
            "view without the environment"
        );
        assert_eq!(
            plugin.telemetry.take_browser_request(),
            None,
            "browser without the environment"
        );
        assert_eq!(
            plugin.telemetry.take_theme_request(),
            None,
            "theme without the environment"
        );

        plugin.dev_cc = true;
        for (number, value) in requests {
            plugin.handle_developer_event(cc(number, value));
        }
        assert!(
            plugin.telemetry.take_view_request().is_some(),
            "the gate never opens"
        );
        assert_eq!(plugin.telemetry.take_browser_request(), Some(true));
        assert!(plugin.telemetry.take_theme_request().is_some());
    }

    #[test]
    fn two_same_offset_hits_each_keep_their_own_velocity() {
        let collect = |reverse: bool| {
            let mut plugin = MxmDrumMachine::default();
            let mut group = EventGroup::new();
            if reverse {
                group.collect(note_on(37, 0.9));
                group.collect(note_on(36, 0.2));
            } else {
                group.collect(note_on(36, 0.2));
                group.collect(note_on(37, 0.9));
            }
            plugin.resolve_events(&group).triggers
        };
        // Host order cannot change the result...
        assert_eq!(collect(false), collect(true));
        // ...and neither hit takes the other's velocity. Comparing the permutations alone would
        // pass just as happily if both slots were struck at 0.9, which is the accent this
        // instrument does not have (owner, 2026-09-20).
        let mut expected = TriggerGroup::new();
        expected.push(0, 0.2);
        expected.push(1, 0.9);
        assert_eq!(
            collect(false),
            expected,
            "a simultaneous louder hit reached another slot"
        );
    }

    #[test]
    fn claimed_channel_is_chromatic_only_and_chords_have_one_deterministic_owner() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmDrumMachine::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            plugin.params.slots[5]
                .midi_channel
                ._internal_set_plain_value(1)
        };

        let mut events = EventGroup::new();
        events.collect(note_on_as(0, 37, Some(1), 0.9));
        let resolved = plugin.resolve_events(&events);
        let mut expected = TriggerGroup::new();
        expected.push(5, 0.9);
        assert_eq!(
            resolved.triggers, expected,
            "Kit slot 2 must ignore claimed channel 1"
        );

        let mut chord = EventGroup::new();
        chord.collect(note_on_as(0, 64, Some(3), 0.2));
        chord.collect(note_on_as(0, 62, Some(4), 0.8));
        chord.collect(NoteEvent::PolyTuning {
            timing: 0,
            voice_id: Some(3),
            channel: 0,
            note: 64,
            tuning: 0.35,
        });
        let resolved = plugin.resolve_events(&chord);
        let mut expected = TriggerGroup::new();
        // Key 64 outranks key 62 and so owns the hit; a hit is its own velocity, so the slot
        // strikes at the owner's 0.2 rather than the louder losing note's 0.8. The reduction still
        // runs, so host event order cannot reach the result, but its value is not what sounds.
        expected.push(5, 0.2);
        assert_eq!(
            resolved.triggers, expected,
            "the chord struck with a note nobody can hear"
        );
        assert_eq!(plugin.owner[5].unwrap().key, 64);
        // Slot 6's default model is a pitched tom, so key 64 is measured from its concert pitch.
        assert_eq!(
            plugin.chromatic_pitch[5],
            64.0 - ModelId::new(20).chromatic_reference_key()
        );
        assert_eq!(plugin.tuning[5], 0.35);

        let mut reversed_plugin = MxmDrumMachine::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            reversed_plugin.params.slots[5]
                .midi_channel
                ._internal_set_plain_value(1)
        };
        let mut reversed = EventGroup::new();
        reversed.collect(NoteEvent::PolyTuning {
            timing: 0,
            voice_id: Some(3),
            channel: 0,
            note: 64,
            tuning: 0.35,
        });
        reversed.collect(note_on_as(0, 62, Some(4), 0.8));
        reversed.collect(note_on_as(0, 64, Some(3), 0.2));
        let reversed_resolved = reversed_plugin.resolve_events(&reversed);
        assert_eq!(reversed_resolved.triggers, resolved.triggers);
        assert_eq!(reversed_plugin.owner[5], plugin.owner[5]);
        assert_eq!(reversed_plugin.tuning[5], plugin.tuning[5]);
    }

    #[test]
    fn chord_owner_tie_break_is_total_for_idless_and_minimum_note_ids() {
        use nice_plug::params::InternalParamMut;
        let owner_for = |reverse: bool| {
            let mut plugin = MxmDrumMachine::default();
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                plugin.params.slots[0]
                    .midi_channel
                    ._internal_set_plain_value(1)
            };
            let mut events = EventGroup::new();
            let first = note_on_as(0, 60, None, 0.8);
            let second = note_on_as(0, 60, Some(i32::MIN), 0.8);
            if reverse {
                events.collect(second);
                events.collect(first);
            } else {
                events.collect(first);
                events.collect(second);
            }
            let _ = plugin.resolve_events(&events);
            plugin.owner[0].unwrap()
        };
        assert_eq!(owner_for(false), owner_for(true));
        assert_eq!(owner_for(false).note_id, Some(i32::MIN));
    }

    #[test]
    fn an_unpitched_model_plays_around_note_60_and_clamps_the_combined_domain() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmDrumMachine::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            plugin.params.slots[0]
                .midi_channel
                ._internal_set_plain_value(1);
            plugin.params.slots[0]
                .model
                // A closed hat: Tune moves its colour, but it has no note to play in tune.
                ._internal_set_plain_value(15);
        };

        let pitch_for = |plugin: &mut MxmDrumMachine, note| {
            let mut events = EventGroup::new();
            events.collect(note_on_as(0, note, Some(i32::from(note)), 0.8));
            let _ = plugin.resolve_events(&events);
            plugin.next_frame().0[0].pitch_semitones
        };
        assert_eq!(pitch_for(&mut plugin, 60), 0.0);
        assert_eq!(pitch_for(&mut plugin, 61), 1.0);
        assert_eq!(pitch_for(&mut plugin, 0), -48.0);
        assert_eq!(pitch_for(&mut plugin, 127), 48.0);
    }

    #[test]
    fn a_pitched_model_plays_at_concert_pitch_on_a_claimed_channel() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmDrumMachine::default();
        // Low falling conga rests near 185 Hz, F#3, MIDI note 54. A claimed channel plays it at its
        // measured rest pitch, so every note moves it by its distance from that key.
        let conga = ModelId::new(4);
        let key = conga.chromatic_reference_key();
        assert_eq!(key.round(), 54.0);
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            plugin.params.slots[0]
                .midi_channel
                ._internal_set_plain_value(1);
            plugin.params.slots[0]
                .model
                ._internal_set_plain_value(i32::from(conga.raw()));
        };
        let pitch_for = |plugin: &mut MxmDrumMachine, note| {
            let mut events = EventGroup::new();
            events.collect(note_on_as(0, note, Some(i32::from(note)), 0.8));
            let _ = plugin.resolve_events(&events);
            plugin.next_frame().0[0].pitch_semitones
        };
        for note in [54_u8, 55, 66, 69, 42] {
            let semitones = f32::from(note) - key;
            assert!(
                (pitch_for(&mut plugin, note) - semitones).abs() < 0.01,
                "note {note} should move the conga {semitones} semitones"
            );
        }
        // Kit mode is untouched: the slot's own note plays the unmoved circuit.
        // SAFETY: as above.
        unsafe {
            plugin.params.slots[0]
                .midi_channel
                ._internal_set_plain_value(0)
        };
        assert_eq!(pitch_for(&mut plugin, 36), 0.0);
    }

    #[test]
    fn a_choke_for_a_losing_note_does_not_cut_the_chromatic_owner() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmDrumMachine::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            plugin.params.slots[0]
                .midi_channel
                ._internal_set_plain_value(1)
        };
        let mut strike = EventGroup::new();
        strike.collect(note_on_as(0, 60, Some(10), 0.9));
        strike.collect(note_on_as(0, 64, Some(11), 0.7));
        let _ = plugin.resolve_events(&strike);

        let mut choke = EventGroup::new();
        choke.collect(NoteEvent::Choke {
            timing: 0,
            voice_id: Some(10),
            channel: 0,
            note: 60,
        });
        assert_eq!(plugin.resolve_events(&choke).chokes, 0);
        assert_eq!(plugin.owner[0].unwrap().note_id, Some(11));
    }

    #[test]
    fn panic_dominates_triggers_in_the_same_group() {
        let mut plugin = MxmDrumMachine::default();
        let mut events = EventGroup::new();
        events.collect(note_on(36, 1.0));
        events.collect(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        let events = plugin.resolve_events(&events);
        let (patches, _, _) = plugin.next_frame();
        plugin.engine.prepare(&patches);
        plugin.apply_events(&patches, events);
        assert!(!plugin.engine.is_active());
    }

    fn group(events: impl IntoIterator<Item = NoteEvent<()>>) -> EventGroup {
        let mut group = EventGroup::new();
        for event in events {
            group.collect(event);
        }
        group
    }

    fn note_off(channel: u8, note: u8, voice_id: Option<i32>) -> NoteEvent<()> {
        NoteEvent::NoteOff {
            timing: 0,
            voice_id,
            channel,
            note,
            velocity: 0.0,
        }
    }

    fn choke(channel: u8, note: u8, voice_id: Option<i32>) -> NoteEvent<()> {
        NoteEvent::Choke {
            timing: 0,
            voice_id,
            channel,
            note,
        }
    }

    fn poly_tuning(channel: u8, note: u8, voice_id: Option<i32>, tuning: f32) -> NoteEvent<()> {
        NoteEvent::PolyTuning {
            timing: 0,
            voice_id,
            channel,
            note,
            tuning,
        }
    }

    fn with_midi_channel(slot: usize, value: i32) -> MxmDrumMachine {
        use nice_plug::params::InternalParamMut;
        let plugin = MxmDrumMachine::default();
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            plugin.params.slots[slot]
                .midi_channel
                ._internal_set_plain_value(value)
        };
        plugin
    }

    #[test]
    fn reassigning_a_slot_does_not_strand_the_note_that_owns_its_tail() {
        // The owner addresses its hit until choke, retrigger or panic. Moving the slot's MIDI
        // channel under a ringing tail changes which strikes it will admit next, not who owns the
        // hit already sounding.
        use nice_plug::params::InternalParamMut;
        for (label, event, expect_choke) in [
            ("its own choke", choke(0, 36, None), true),
            ("its own release", note_off(0, 36, None), false),
        ] {
            let mut kit = MxmDrumMachine::default();
            let _ = kit.resolve_events(&group([note_on(36, 0.8)]));
            assert!(kit.owner[0].is_some(), "the Kit note must own slot 0");
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                kit.params.slots[0]
                    .midi_channel
                    ._internal_set_plain_value(7)
            };
            let resolved = kit.resolve_events(&group([event]));
            if expect_choke {
                assert_eq!(
                    resolved.chokes, 1,
                    "{label} did not reach the tail after the slot was reassigned"
                );
            } else {
                assert!(
                    !kit.owns_expression[0],
                    "{label} did not release expression after the slot was reassigned"
                );
            }
        }

        // And tuning, which is the third owner-directed event.
        let mut kit = MxmDrumMachine::default();
        let _ = kit.resolve_events(&group([note_on(36, 0.8)]));
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            kit.params.slots[0]
                .midi_channel
                ._internal_set_plain_value(7)
        };
        let _ = kit.resolve_events(&group([poly_tuning(0, 36, None, 0.5)]));
        assert_ne!(
            kit.tuning[0], 0.0,
            "tuning did not reach the tail after the slot was reassigned"
        );
    }

    #[test]
    fn a_choke_after_release_still_ends_the_one_shot_hit() {
        let all_notes_off = NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_NOTES_OFF,
            value: 0.0,
        };
        for (slot, strike, release, choke) in [
            (
                0,
                note_on(36, 0.8),
                note_off(0, 36, None),
                choke(0, 36, None),
            ),
            (1, note_on(37, 0.8), all_notes_off, choke(0, 37, None)),
        ] {
            let mut kit = MxmDrumMachine::default();
            let _ = kit.resolve_events(&group([strike]));
            let _ = kit.resolve_events(&group([release]));
            assert!(!kit.owns_expression[slot]);
            assert_eq!(kit.resolve_events(&group([choke])).chokes, 1 << slot);
        }

        let mut chromatic = with_midi_channel(4, 2);
        let _ = chromatic.resolve_events(&group([note_on_as(1, 64, Some(5), 0.8)]));
        let _ = chromatic.resolve_events(&group([note_off(1, 64, Some(5))]));
        let _ = chromatic.resolve_events(&group([poly_tuning(1, 64, Some(5), 0.5)]));
        assert_eq!(
            chromatic.tuning[4], 0.0,
            "released expression still followed its note"
        );
        assert_eq!(
            chromatic
                .resolve_events(&group([choke(1, 64, Some(5))]))
                .chokes,
            1 << 4
        );
    }

    #[test]
    fn identical_kit_notes_on_two_channels_pick_one_owner_in_any_order() {
        let owner_for = |reverse: bool| {
            let mut plugin = MxmDrumMachine::default();
            // Different channel states, so the owner's channel is audible through bend.
            let _ = plugin.resolve_events(&group([
                NoteEvent::MidiPitchBend {
                    timing: 0,
                    channel: 2,
                    value: 1.0,
                },
                NoteEvent::MidiPitchBend {
                    timing: 0,
                    channel: 5,
                    value: 0.0,
                },
            ]));
            let mut strikes = [note_on_as(2, 36, None, 0.7), note_on_as(5, 36, None, 0.7)];
            if reverse {
                strikes.reverse();
            }
            let _ = plugin.resolve_events(&group(strikes));
            (
                plugin.owner[0],
                plugin.slot_channel[0],
                plugin.next_frame().0[0].pitch_semitones,
            )
        };
        assert_eq!(owner_for(false), owner_for(true));
        assert_eq!(owner_for(false).1, 5);
    }

    #[test]
    fn same_offset_events_for_an_existing_owner_do_not_depend_on_host_order() {
        let resolve = |second: NoteEvent<()>, reverse: bool| {
            let mut plugin = with_midi_channel(0, 1);
            let _ = plugin.resolve_events(&group([note_on_as(0, 60, Some(9), 0.8)]));
            let mut events = [poly_tuning(0, 60, Some(9), 0.25), second];
            if reverse {
                events.reverse();
            }
            let chokes = plugin.resolve_events(&group(events)).chokes;
            (
                chokes,
                plugin.tuning[0],
                plugin.owner[0],
                plugin.owns_expression[0],
            )
        };
        let choked = resolve(choke(0, 60, Some(9)), false);
        assert_eq!(choked, resolve(choke(0, 60, Some(9)), true));
        assert_eq!(choked, (1, 0.0, None, false));
        let released = resolve(note_off(0, 60, Some(9)), false);
        assert_eq!(released, resolve(note_off(0, 60, Some(9)), true));
        assert_eq!((released.0, released.1, released.3), (0, 0.25, false));
        assert!(
            released.2.is_some(),
            "a release must keep the hit addressable"
        );
    }

    #[test]
    fn a_release_flood_releases_expression_without_cutting_tails() {
        let mut plugin = MxmDrumMachine::default();
        let _ = plugin.resolve_events(&group([note_on(36, 0.8)]));
        let flood =
            (0..=MAX_GROUP_NOTE_EVENTS).map(|n| note_off((n % 16) as u8, (n % 128) as u8, None));
        let resolved = plugin.resolve_events(&group(flood));
        assert!(!resolved.panic);
        assert_eq!(resolved.chokes, 0);
        assert!(!plugin.owns_expression[0]);
        assert_eq!(
            plugin.resolve_events(&group([choke(0, 36, None)])).chokes,
            1
        );
    }

    #[test]
    fn a_saturated_group_holding_a_choke_or_too_many_strikes_is_panic_in_any_order() {
        for choke_first in [true, false] {
            let mut plugin = MxmDrumMachine::default();
            let _ = plugin.resolve_events(&group([note_on(36, 0.8)]));
            let offs = (0..=MAX_GROUP_NOTE_EVENTS).map(|n| note_off(0, (n % 128) as u8, None));
            let events: Vec<_> = if choke_first {
                std::iter::once(choke(0, 40, None)).chain(offs).collect()
            } else {
                offs.chain(std::iter::once(choke(0, 40, None))).collect()
            };
            assert!(
                plugin.resolve_events(&group(events)).panic,
                "choke first: {choke_first}"
            );
        }
        let strikes = (0..=MAX_GROUP_NOTE_EVENTS).map(|n| note_on_as(0, 36, Some(n as i32), 0.5));
        assert!(
            MxmDrumMachine::default()
                .resolve_events(&group(strikes))
                .panic
        );
    }

    #[test]
    fn stereo_compatibility_folds_stored_individual_outputs_to_main() {
        use nice_plug::params::InternalParamMut;
        let mut plugin = MxmDrumMachine::default();
        for (slot, params) in plugin.params.slots.iter().enumerate() {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { params.output._internal_set_plain_value(16 - slot as i32) };
        }
        plugin.multi_output = false;
        assert_eq!(
            plugin.output_destinations(),
            [Destination::Main; SLOT_COUNT]
        );
        plugin.multi_output = true;
        assert_eq!(
            plugin.output_destinations(),
            std::array::from_fn(|slot| Destination::Auxiliary(15 - slot))
        );
    }

    #[test]
    fn full_layout_is_first_and_stereo_compatibility_is_second() {
        let layouts = <MxmDrumMachine as Plugin>::AUDIO_IO_LAYOUTS;
        assert_eq!(layouts.len(), 2);
        assert_eq!(layouts[0].names.layout, Some("Main plus 16 mono outputs"));
        assert_eq!(layouts[0].main_output_channels.unwrap().get(), 2);
        assert_eq!(layouts[0].aux_output_ports.len(), 16);
        assert!(
            layouts[0]
                .aux_output_ports
                .iter()
                .all(|channels| channels.get() == 1)
        );
        assert_eq!(layouts[0].names.aux_outputs, INDIVIDUAL_OUTPUT_NAMES);
        // Slot-numbered, in slot order: port N serves slot N under the one-gesture pattern.
        for (index, name) in INDIVIDUAL_OUTPUT_NAMES.iter().enumerate() {
            assert_eq!(*name, format!("Slot {:02}", index + 1));
        }
        assert_eq!(layouts[1].names.layout, Some("Stereo compatibility"));
        assert_eq!(layouts[1].main_output_channels.unwrap().get(), 2);
        assert!(layouts[1].aux_output_ports.is_empty());
    }

    #[test]
    fn mute_wins_over_solo_and_any_solo_isolates_the_soloed_slots() {
        use nice_plug::params::InternalParamMut;

        let plugin = MxmDrumMachine::default();
        assert!(
            plugin
                .params
                .slots
                .iter()
                .all(|slot| slot_is_audible(slot, false))
        );

        // SAFETY: the test owns the plugin and no process or GUI thread can access it.
        unsafe {
            plugin.params.slots[0].mute._internal_set_plain_value(true);
        }
        assert!(!slot_is_audible(&plugin.params.slots[0], false));
        assert!(slot_is_audible(&plugin.params.slots[1], false));

        // SAFETY: the test owns the plugin and no process or GUI thread can access it.
        unsafe {
            plugin.params.slots[1].solo._internal_set_plain_value(true);
        }
        assert!(!slot_is_audible(&plugin.params.slots[0], true));
        assert!(slot_is_audible(&plugin.params.slots[1], true));
        assert!(!slot_is_audible(&plugin.params.slots[2], true));

        // SAFETY: the test owns the plugin and no process or GUI thread can access it.
        unsafe {
            plugin.params.slots[1].mute._internal_set_plain_value(true);
        }
        assert!(!slot_is_audible(&plugin.params.slots[1], true));
    }
}

#[cfg(test)]
mod identity {
    use super::{CLAP_ID, NAME};

    #[test]
    fn clap_id_and_bundle_name_follow_the_single_plugin_literal() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }
}

/// The full layout through the plugin's own `process()`: main stereo plus sixteen mono ports, as a
/// multi-output host hands them over. MXM Player selects stereo compatibility, so only this can see
/// auxiliary port indexing and Master/Mute/Solo on the individual outputs.
#[cfg(test)]
mod full_layout {
    use std::collections::VecDeque;

    use nice_plug::params::InternalParamMut;

    use super::*;

    const SAMPLE_RATE: f32 = 48_000.0;
    /// Deliberately not a multiple of the 64-sample internal block.
    const FRAMES: usize = 1000;

    struct Activation;

    impl ActivateContext<MxmDrumMachine> for Activation {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: CaptureTask) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    struct Events {
        transport: Transport,
        events: VecDeque<NoteEvent<()>>,
    }

    impl ProcessContext<MxmDrumMachine> for Events {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        // Captures are rendered by the real executor in the plugin; these tests drive
        // `process` directly and install kits through `activate`, so a submitted task is
        // counted as a request and otherwise ignored.
        fn execute_background(&self, _task: CaptureTask) {}
        fn execute_gui(&self, _task: CaptureTask) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            self.events.pop_front()
        }
        fn send_event(&mut self, _event: NoteEvent<()>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    /// Slot `s` on individual output `(5 s + 3) mod 16`, a permutation.
    #[test]
    fn engaging_resample_at_activation_plays_captures_through_the_real_callback() {
        // The whole chain in one place: the parameter, the synchronous capture at activation, the
        // engine substitution and the ordinary mix below it. Frozen and live must both sound, and
        // must not sound the same — if they were identical the substitution never happened.
        use nice_plug::params::InternalParamMut;

        let mut live = activated(1, |_| {});
        let live_out = render(&mut live, &[36]);
        assert!(!silent(&live_out.main[0]), "the live kit sounds");

        let mut frozen = activated(1, |params| {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { params.resample._internal_set_plain_value(true) };
        });
        assert!(
            frozen.engine.is_frozen(),
            "activation captured the kit, so a session that opens frozen is frozen at once"
        );
        let frozen_out = render(&mut frozen, &[36]);
        assert!(!silent(&frozen_out.main[0]), "the frozen kit sounds");
        assert_ne!(
            live_out.main[0], frozen_out.main[0],
            "a frozen slot plays its capture, not its circuit"
        );
    }

    #[test]
    fn resample_off_at_activation_leaves_the_instrument_live() {
        let plugin = activated(1, |_| {});
        assert!(
            !plugin.engine.is_frozen(),
            "Init is live, and no capture was rendered for it"
        );
    }

    #[test]
    fn toggling_resample_while_running_swaps_the_kit_in_through_the_bank() {
        // The live path end to end: `process` notices the toggle and asks, the worker renders and
        // publishes, and a later `process` installs it without allocating or dropping anything.
        // Driven by hand here — the point under test is the handoff and the servicing, not the
        // worker's timing.
        use nice_plug::params::InternalParamMut;

        let mut plugin = activated(1, |_| {});
        // The capture thread is stopped and joined first, because this test stands in for it: a
        // live worker would answer the same request with a kit of its own, on its own clock,
        // under every assertion below.
        plugin.worker = None;
        assert!(!plugin.engine.is_frozen(), "starts live");

        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { plugin.params.resample._internal_set_plain_value(true) };

        // One block: the request goes out, and nothing is frozen yet.
        render(&mut plugin, &[]);
        assert!(
            !plugin.engine.is_frozen(),
            "still live while the worker renders"
        );
        assert!(plugin.asked, "and a capture was asked for");

        // Stand in for the worker, which stamps the kit with the engage that asked.
        let patches = capture_patches(&plugin.params);
        let kit = mxm_drum_machine_dsp::capture::capture_kit(&patches, SAMPLE_RATE).expect("a kit");
        plugin.bank.publish(kit.answering(plugin.engagement));

        render(&mut plugin, &[]);
        assert!(plugin.engine.is_frozen(), "the next block installed it");
        let frozen = render(&mut plugin, &[36]);
        assert!(!silent(&frozen.main[0]), "and the frozen kit sounds");

        // Disengaging needs no render, so it takes effect at once.
        // SAFETY: as above.
        unsafe { plugin.params.resample._internal_set_plain_value(false) };
        render(&mut plugin, &[]);
        assert!(!plugin.engine.is_frozen(), "disengaging is immediate");
    }

    #[test]
    fn a_kit_rendered_at_the_wrong_rate_is_refused_and_asked_for_again() {
        // The request carries the engine's rate because the editor frame that submits it cannot
        // know one — and if a rate change beats the render home anyway, the stale-rate kit is
        // retired unplayed rather than played back at the wrong speed.
        use nice_plug::params::InternalParamMut;

        let mut plugin = activated(1, |_| {});
        // The capture thread is stopped and joined first: this is about what `service_captures`
        // does with a kit it is handed, and a live worker would both eat the request and race
        // the bank with a correct kit of its own.
        plugin.worker = None;

        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { plugin.params.resample._internal_set_plain_value(true) };
        render(&mut plugin, &[]);
        assert_eq!(
            plugin
                .telemetry
                .take_capture_request()
                .map(|request| request.sample_rate),
            Some(SAMPLE_RATE),
            "the request carries the rate the engine is running at"
        );

        // The worker answers at a rate the engine is no longer using.
        let patches = capture_patches(&plugin.params);
        let wrong = SAMPLE_RATE * 2.0;
        let kit = mxm_drum_machine_dsp::capture::capture_kit(&patches, wrong).expect("a kit");
        plugin.bank.publish(kit.answering(plugin.engagement));

        render(&mut plugin, &[]);
        assert!(
            !plugin.engine.is_frozen(),
            "a kit at {wrong} Hz must not play in a {SAMPLE_RATE} Hz engine"
        );
        assert_eq!(
            plugin
                .telemetry
                .take_capture_request()
                .map(|request| request.sample_rate),
            Some(SAMPLE_RATE),
            "and the engine asks again rather than giving up"
        );

        // Stand in for the worker, which drains before it publishes: `take` refuses a new kit
        // while a retired one is still parked, which is what keeps the handoff a ring.
        plugin.bank.drain();

        // The right rate is accepted, so the refusal is about the rate and nothing else.
        let kit = mxm_drum_machine_dsp::capture::capture_kit(&patches, SAMPLE_RATE).expect("a kit");
        plugin.bank.publish(kit.answering(plugin.engagement));
        render(&mut plugin, &[]);
        assert!(plugin.engine.is_frozen(), "a matching kit installs");
    }

    /// **A kit from an earlier engage is never installed.** Resample engaged asks for the patch
    /// as it is; the kit lands in the bank, and before `process` takes it Resample is turned off,
    /// slot 1's model changed and Resample turned on again — the owner's way to change a frozen
    /// kit. The waiting kit is the old patch. It must be refused, the new engage must ask for the
    /// edited patch, and only the kit answering that request installs. A kit still rendering when
    /// the new engage began lands the same way, stamped with the old engage.
    ///
    /// Falsified before trusted: accepting a kit by its rate alone installs the old patch and asks
    /// for nothing.
    #[test]
    fn a_kit_from_an_earlier_engage_is_never_installed() {
        use nice_plug::params::InternalParamMut;

        let mut plugin = activated(1, |_| {});
        // Stopped and joined: this test stands in for it.
        plugin.worker = None;

        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { plugin.params.resample._internal_set_plain_value(true) };
        render(&mut plugin, &[]);
        let first = plugin
            .telemetry
            .take_capture_request()
            .expect("the engage asked");
        let old = mxm_drum_machine_dsp::capture::capture_kit(
            &capture_patches(&plugin.params),
            first.sample_rate,
        )
        .expect("a kit");
        plugin.bank.publish(old.answering(first.engagement));

        // Off, edit, on — before any `process` took the kit.
        // SAFETY: as above.
        unsafe { plugin.params.resample._internal_set_plain_value(false) };
        render(&mut plugin, &[]);
        // SAFETY: as above.
        unsafe {
            plugin.params.slots[0]
                .model
                ._internal_set_plain_value(i32::from(ModelId::TWIN_MODE_SNARE.raw()));
            plugin.params.resample._internal_set_plain_value(true);
        }
        render(&mut plugin, &[]);
        assert!(
            !plugin.engine.is_frozen(),
            "the old patch's kit was installed"
        );
        let second = plugin
            .telemetry
            .take_capture_request()
            .expect("the new engage asked for the edited patch");
        assert_ne!(second.engagement, first.engagement);

        // Stand in for the worker, which frees what was retired before it renders.
        plugin.bank.drain();
        let new = mxm_drum_machine_dsp::capture::capture_kit(
            &capture_patches(&plugin.params),
            second.sample_rate,
        )
        .expect("a kit");
        plugin.bank.publish(new.answering(second.engagement));
        render(&mut plugin, &[]);
        assert!(plugin.engine.is_frozen(), "the edited patch's kit installs");
    }

    /// **An activation supersedes a kit still rendering, and leaves room to disengage.** Resample
    /// engaged asks for the patch as it is; before the kit lands the host loads an edit (slot 1's
    /// model) and re-activates, and activation captures the edited patch itself. The worker's kit
    /// then lands at the same rate, the old patch: it is refused, and the kit playing is the
    /// edited one — the note sounds exactly as a fresh instance frozen on the edited patch. Nothing
    /// asks again, so no capture comes to drain the refused kit, and turning Resample off retires
    /// the playing kit beside it.
    ///
    /// Falsified before trusted: without activation advancing the engage the old kit installs and
    /// the note sounds the old model; with one retired slot the disengage trips `retire`'s
    /// assertion (and in a release build leaks a kit).
    #[test]
    fn an_activation_supersedes_a_kit_still_rendering_and_leaves_room_to_disengage() {
        use nice_plug::params::InternalParamMut;

        let edit = |params: &MxmDrumMachineParams| {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                params.slots[0]
                    .model
                    ._internal_set_plain_value(i32::from(ModelId::TWIN_MODE_SNARE.raw()));
            }
        };
        let engaged = |params: &MxmDrumMachineParams| {
            // SAFETY: as above.
            unsafe { params.resample._internal_set_plain_value(true) };
        };

        let mut plugin = activated(1, |_| {});
        // Stopped and joined: this test stands in for it.
        plugin.worker = None;
        engaged(&plugin.params);
        render(&mut plugin, &[]);
        let asked = plugin
            .telemetry
            .take_capture_request()
            .expect("the engage asked");
        let old = mxm_drum_machine_dsp::capture::capture_kit(
            &capture_patches(&plugin.params),
            asked.sample_rate,
        )
        .expect("a kit");

        // The edit lands and the host re-activates at the same rate.
        edit(&plugin.params);
        reactivate(&mut plugin);
        plugin.worker = None;
        assert!(
            plugin.engine.is_frozen(),
            "activation captured the edited patch"
        );

        plugin.bank.publish(old.answering(asked.engagement));
        render(&mut plugin, &[]);
        assert_eq!(
            plugin.telemetry.take_capture_request(),
            None,
            "the premise: nothing asks again, so no capture comes to drain the refused kit"
        );
        let heard = render(&mut plugin, &[36]);

        let mut reference = activated(1, |params| {
            edit(params);
            engaged(params);
        });
        reference.worker = None;
        render(&mut reference, &[]);
        let expected = render(&mut reference, &[36]);
        let mut unedited = activated(1, engaged);
        unedited.worker = None;
        render(&mut unedited, &[]);
        assert_ne!(
            render(&mut unedited, &[36]).main,
            expected.main,
            "the premise: the two models sound different"
        );
        assert_eq!(heard.main, expected.main, "the old patch's kit is playing");

        // SAFETY: as above.
        unsafe { plugin.params.resample._internal_set_plain_value(false) };
        render(&mut plugin, &[]);
        assert!(!plugin.engine.is_frozen(), "disengaging is immediate");
    }

    #[test]
    fn an_edit_while_frozen_renders_nothing_at_all() {
        // **One capture per engage** (owner, 2026-09-21). Editing a captured axis while frozen
        // must not schedule a render: installing one resets every sounding reader, so a knob
        // sweep that re-rendered per block cut and crackled through the whole kit. To change a
        // frozen kit you turn Resample off, edit, and turn it back on.
        use nice_plug::params::InternalParamMut;

        let mut plugin = activated(1, |params| {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { params.resample._internal_set_plain_value(true) };
        });
        assert!(
            plugin.engine.is_frozen(),
            "activation captured synchronously"
        );
        // Stopped and joined: this is about what `service_captures` asks for, and a live worker
        // consumes requests on its own clock, which would race every assertion below.
        plugin.worker = None;
        // Clear anything activation left behind, so the sweep's own behaviour is what is measured.
        let _ = plugin.telemetry.take_capture_request();

        // Sweep two captured axes the way a host automating them would.
        for step in 0..8 {
            // SAFETY: as above.
            unsafe {
                plugin.params.slots[0]
                    .decay
                    ._internal_set_plain_value(-1.0 + 0.25 * step as f32);
                plugin.params.slots[0]
                    .tone
                    ._internal_set_plain_value(0.1 * step as f32);
            }
            render(&mut plugin, &[36]);
            assert_eq!(
                plugin.telemetry.take_capture_request(),
                None,
                "step {step} asked for a re-render while frozen"
            );
        }

        // And the off/on cycle the owner named as the way to pick the edits up does ask.
        // SAFETY: as above.
        unsafe { plugin.params.resample._internal_set_plain_value(false) };
        render(&mut plugin, &[]);
        assert!(!plugin.engine.is_frozen(), "disengaging is immediate");
        // SAFETY: as above.
        unsafe { plugin.params.resample._internal_set_plain_value(true) };
        render(&mut plugin, &[]);
        assert_eq!(
            plugin
                .telemetry
                .take_capture_request()
                .map(|request| request.sample_rate),
            Some(SAMPLE_RATE),
            "re-engaging is what picks the edits up"
        );
    }

    #[test]
    fn the_capture_thread_answers_an_engage_with_no_editor_open() {
        // The whole path with nothing stubbed: `activate` spawns the plugin's own capture thread,
        // engaging Resample inside `process` asks for a kit, and the thread renders and publishes
        // it with no editor frame anywhere. Submission used to come from an editor frame, which
        // meant a closed GUI silently never froze at all.
        use nice_plug::params::InternalParamMut;
        // Activated disengaged, so nothing is captured synchronously and the only route to a kit
        // is the thread.
        let mut plugin = activated(1, |_| {});
        assert!(plugin.worker.is_some(), "activate spawned the thread");
        assert!(!plugin.engine.is_frozen(), "and starts live");

        // SAFETY: exclusive test ownership; no GUI thread exists.
        unsafe { plugin.params.resample._internal_set_plain_value(true) };

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            render(&mut plugin, &[]);
            if plugin.engine.is_frozen() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the capture thread never answered an engage"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            plugin.captured,
            Some(SAMPLE_RATE),
            "and the installed kit is recorded at the engine's rate"
        );
    }

    #[test]
    fn the_export_gate_writes_where_it_is_told_and_reports() {
        // The export arm on the worker, end to end, **into a temporary directory**.
        //
        // This test used to let the worker resolve `pack::root()` itself, so every run wrote a
        // real sixteen-file pack into the runner's own data folder — fourteen of them piled up in
        // the owner's AppData before it was spotted. The destination travels with the job now,
        // which is what makes an injected root possible at all.
        let into = std::env::temp_dir().join(format!(
            "mxm-drum-machine-export-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let plugin = activated(1, |_| {});
        plugin.export_gate.request_export(SAMPLE_RATE, into.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            // Either it wrote somewhere or it said why; both are reports, neither is a crash.
            if plugin.telemetry.take_export().is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the export never reported"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn port_for(slot: usize) -> usize {
        (slot * 5 + 3) % SLOT_COUNT
    }

    /// Activates `layout` after `configure`, with every smoother at its target.
    fn activated(layout: usize, configure: impl FnOnce(&MxmDrumMachineParams)) -> MxmDrumMachine {
        let mut plugin = MxmDrumMachine::default();
        for (slot, params) in plugin.params.slots.iter().enumerate() {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe {
                params
                    .output
                    ._internal_set_plain_value(port_for(slot) as i32 + 1)
            };
        }
        configure(&plugin.params);
        for (_, ptr, _) in plugin.params.param_map() {
            // SAFETY: as above.
            unsafe { ptr._internal_update_smoother(SAMPLE_RATE, true) };
        }
        assert!(plugin.activate(
            &MxmDrumMachine::AUDIO_IO_LAYOUTS[layout],
            &BufferConfig {
                sample_rate: SAMPLE_RATE,
                min_buffer_size: Some(1),
                max_buffer_size: 4096,
                process_mode: ProcessMode::Realtime,
            },
            &mut Activation,
        ));
        plugin
    }

    /// Activates `plugin` again at the same rate, as a host does after a buffer or state change.
    fn reactivate(plugin: &mut MxmDrumMachine) {
        assert!(plugin.activate(
            &MxmDrumMachine::AUDIO_IO_LAYOUTS[1],
            &BufferConfig {
                sample_rate: SAMPLE_RATE,
                min_buffer_size: Some(1),
                max_buffer_size: 4096,
                process_mode: ProcessMode::Realtime,
            },
            &mut Activation,
        ));
    }

    struct Rendered {
        main: [Vec<f32>; 2],
        individual: Vec<Vec<f32>>,
    }

    fn silent(samples: &[f32]) -> bool {
        samples.iter().all(|sample| *sample == 0.0)
    }

    /// One `process()` call of `FRAMES`, striking Kit `notes` at sample 0.
    fn render(plugin: &mut MxmDrumMachine, notes: &[u8]) -> Rendered {
        let ports = if plugin.multi_output { SLOT_COUNT } else { 0 };
        let mut left = vec![0.0_f32; FRAMES];
        let mut right = vec![0.0_f32; FRAMES];
        let mut individual = vec![vec![0.0_f32; FRAMES]; ports];
        {
            let mut main = Buffer::default();
            // SAFETY: the slices outlive the buffers, which are dropped at the end of this block.
            unsafe {
                main.set_slices(FRAMES, |channels| {
                    channels.clear();
                    channels.push(left.as_mut_slice());
                    channels.push(right.as_mut_slice());
                });
            }
            let mut outputs: Vec<Buffer> = individual
                .iter_mut()
                .map(|port| {
                    let port = port.as_mut_slice();
                    let mut buffer = Buffer::default();
                    // SAFETY: as above.
                    unsafe {
                        buffer.set_slices(FRAMES, move |channels| {
                            channels.clear();
                            channels.push(port);
                        });
                    }
                    buffer
                })
                .collect();
            let mut inputs = [];
            let mut aux = AuxiliaryBuffers {
                inputs: &mut inputs,
                outputs: &mut outputs,
            };
            let events = notes
                .iter()
                .map(|&note| NoteEvent::NoteOn {
                    timing: 0,
                    voice_id: None,
                    channel: 0,
                    note,
                    velocity: 0.8,
                })
                .collect();
            plugin.process(
                &mut main,
                &mut aux,
                &mut Events {
                    transport: Transport::new(SAMPLE_RATE),
                    events,
                },
            );
        }
        Rendered {
            main: [left, right],
            individual,
        }
    }

    #[test]
    fn each_slot_reaches_exactly_its_selected_port_and_folds_to_main_in_stereo() {
        for slot in 0..SLOT_COUNT {
            let note = 36 + slot as u8;
            let mut full = activated(0, |_| {});
            assert!(full.multi_output);
            let rendered = render(&mut full, &[note]);
            assert!(
                rendered.main.iter().all(|channel| silent(channel)),
                "slot {slot} leaked to main"
            );
            for (port, samples) in rendered.individual.iter().enumerate() {
                assert_eq!(
                    !silent(samples),
                    port == port_for(slot),
                    "slot {slot} on port {}",
                    port + 1
                );
            }

            let mut stereo = activated(1, |_| {});
            assert!(!stereo.multi_output);
            let folded = render(&mut stereo, &[note]);
            assert!(folded.individual.is_empty());
            assert!(
                !silent(&folded.main[0]),
                "slot {slot} went silent in stereo compatibility"
            );
        }
    }

    #[test]
    fn master_mute_and_solo_act_on_individual_ports() {
        let (kick, snare) = (port_for(0), port_for(1));
        let unity = render(&mut activated(0, |_| {}), &[36]).individual[kick].clone();
        assert!(!silent(&unity));

        let halved = render(
            &mut activated(0, |params| {
                // SAFETY: exclusive test ownership.
                unsafe { params.master._internal_set_plain_value(0.5) };
            }),
            &[36],
        );
        for (half, full) in halved.individual[kick].iter().zip(&unity) {
            assert!((half - 0.5 * full).abs() <= 1.0e-7 * full.abs().max(1.0));
        }

        let muted = render(
            &mut activated(0, |params| {
                // SAFETY: exclusive test ownership.
                unsafe { params.slots[0].mute._internal_set_plain_value(true) };
            }),
            &[36, 37],
        );
        assert!(
            silent(&muted.individual[kick]),
            "Mute did not reach its port"
        );
        assert!(!silent(&muted.individual[snare]));

        let soloed = render(
            &mut activated(0, |params| {
                // SAFETY: exclusive test ownership.
                unsafe { params.slots[1].solo._internal_set_plain_value(true) };
            }),
            &[36, 37],
        );
        assert!(silent(&soloed.individual[kick]), "Solo did not isolate");
        assert!(!silent(&soloed.individual[snare]));
    }

    #[test]
    fn a_live_output_edit_moves_a_tail_between_ports_and_retires_the_old_one() {
        let mut plugin = activated(0, |_| {});
        let (old, new) = (port_for(0), port_for(7));
        let before = render(&mut plugin, &[36]);
        assert!(!silent(&before.individual[old]));
        // SAFETY: exclusive test ownership; the output selector is unsmoothed.
        unsafe {
            plugin.params.slots[0]
                .output
                ._internal_set_plain_value(new as i32 + 1)
        };
        let after = render(&mut plugin, &[]);
        // 2 ms at 48 kHz.
        let window = 96;
        assert!(
            !silent(&after.individual[old][..window]),
            "the tail vanished"
        );
        assert!(
            silent(&after.individual[old][window..]),
            "the old port kept sounding"
        );
        assert!(!silent(&after.individual[new]));
        for (port, samples) in after.individual.iter().enumerate() {
            if port != old && port != new {
                assert!(silent(samples), "a third port sounded: {}", port + 1);
            }
        }
        assert!(after.main.iter().all(|channel| silent(channel)));
    }
}

/// **Activation forgets the last session's tempo and resolved syncs**: the first callback reports the
/// tempo, so nothing — the audio, or an editor frame before it — starts from the previous session's
/// divisions.
#[cfg(test)]
mod activation_forgets_the_tempo {
    use super::*;

    #[test]
    fn activation_forgets_the_last_tempo_and_resolved_syncs() {
        use nice_plug::prelude::Plugin as _;
        let mut plugin = MxmDrumMachine::default();
        plugin.telemetry.tempo.publish(Some(120.0));
        plugin.synced_lfo_hz = [Some(1.0); 3];
        let layout = MxmDrumMachine::AUDIO_IO_LAYOUTS[0];
        let config = BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: 512,
            process_mode: ProcessMode::Realtime,
        };
        let _ = plugin.activate(&layout, &config, &mut NoInit);
        assert_eq!(plugin.telemetry.tempo.get(), None);
        assert_eq!(plugin.synced_lfo_hz, [None; 3]);
    }

    /// An activation context that asks nothing of a host.
    struct NoInit;

    impl ActivateContext<MxmDrumMachine> for NoInit {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: <MxmDrumMachine as Plugin>::BackgroundTask) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
