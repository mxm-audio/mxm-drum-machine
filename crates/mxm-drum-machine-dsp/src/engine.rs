//! Sixteen-slot render framework.

use crate::SLOT_COUNT;
use crate::analogue_909::{FourCellClap, TripleRim};
use crate::deep_bridge_kick::{DeepBridgeKick, Patch as KickPatch};
use crate::economy_55::{Kind as EconomyKind, Noise as EconomyNoise, Voice as EconomyVoice};
use crate::falling_drum::{FallingDrum, Kind as FallingKind, Patch as FallingPatch};
use crate::legacy::{Sources as LegacySources, Voice as LegacyVoice, machine as legacy_machine};
use crate::metal_808::{Cowbell, Cymbal, Hat, HatKind, MetalBank, Patch as MetalPatch};
use crate::model::{ImplementedModel, ModelId};
use crate::noise_percussion::{Maraca, Patch as NoisePatch, PulseClap};
use crate::pcm_909::{Kind as PcmKind, PcmMetal};
use crate::reset_vco_909::{
    HardwareNoise, Patch as ResetPatch, ResetKick, ResetSnare, ResetTom, TomKind,
};
use crate::rim_clave::{Kind as RimClaveKind, Patch as RimClavePatch, RimClave};
use crate::routing::{self, Graph, Routing, Sources};
use crate::twin_mode_snare::{Patch as SnarePatch, TwinModeSnare};
use mxm_modulation::standard;
use mxm_part_routing::{Destination, DestinationRouter};

const RANDOM_SEED: u64 = 0x243f_6a88_85a3_08d3;
const SNARE_NOISE_SEED: u64 = 0x1319_8a2e_0370_7344;

/// Highest assignable choke group; 0 means the slot is in none.
pub const CHOKE_GROUP_MAX: u8 = 16;
const OUTPUT_TRANSFER_SECONDS: f32 = 0.002;
/// CHOSEN length of the shared/private metallic-bank crossfade. Long enough that six decorrelated
/// square pairs cross without a step, short enough to read as immediate under automation. Replace
/// it from a click/latency measurement on a Pitch automation ramp through the reference detent.
pub(crate) const METAL_TRANSITION_SECONDS: f32 = 0.005;
/// CHOSEN length of the fade that retires a replaced or choked model. The brief requires an
/// audible outgoing model to reach silence through a finite transition rather than a cut, and sets
/// the duration by click/latency measurement; this is short enough to feel immediate under
/// selector automation and long enough to bury the step. Replace it from that measurement.
const MODEL_RETIRE_SECONDS: f32 = 0.005;

/// Main stereo plus the sixteen mono individual destinations for one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputFrame {
    pub main: [f32; 2],
    pub individual: [f32; SLOT_COUNT],
}

impl Default for OutputFrame {
    fn default() -> Self {
        Self {
            main: [0.0; 2],
            individual: [0.0; SLOT_COUNT],
        }
    }
}

/// Static output adaptation outside the modelled circuits. Each value brings one isolated reference
/// hit at 48 kHz, zero deviation and velocity 0.82 to a −1 dBFS mono peak. This is peak consistency
/// for model switching, not dynamic normalisation or a claim about the machines' relative loudness.
/// Re-measure the complete table whenever a reference renderer's internal gain changes.
const REFERENCE_OUTPUT_TRIM: [f32; 95] = [
    1.0, // 0: legacy Off
    // 1…16: Bridge 808
    1.058771, 1.719879, 1.691843, 1.568266, 1.681684, 1.543575, 1.636395, 1.553904, 1.524234,
    1.623053, 1.075469, 0.964231, 1.446723, 1.569505, 0.3444, 0.199197,
    // 17…27: Reset 909
    2.503262, 1.256602, 3.203948, 3.019835, 2.446152, 1.401633, 6.296215, 2.718797, 1.966378,
    2.007905, 1.765836, // 28…31: Economy 55
    2.340038, 1.283503, 2.173318, 2.110582, // 32…46: Expanded 8000
    0.828161, 2.152855, 0.701462, 0.605058, 0.889859, 0.929256, 1.319128, 1.90651, 0.741165,
    4.409347, 17.783913, 9.185535, 1.636582, 1.429247, 2.832283,
    // 47…53: Compact 606
    0.998834, 1.827729, 1.163226, 1.386866, 14.962159, 18.915209, 12.501283,
    // 54…59: Snap 110
    1.010114, 1.275126, 8.405419, 3.841616, 3.366803, 10.110293,
    // 60…73: Classic 78
    0.816602, 0.706217, 0.965955, 2.609481, 2.185913, 3.036741, 1.430079, 2.531811, 0.869672,
    1.317498, 1.249654, 1.783516, 4.047873, 2.842586, // 74…84: Discrete 66
    1.649967, 1.404957, 1.499292, 1.482551, 1.512634, 1.292205, 1.414851, 1.647161, 5.098765,
    1.909953, 1.820863, // 85…94: Early 2L
    1.262159, 1.415426, 1.414449, 1.445848, 1.43609, 0.903669, 0.717374, 2.203399, 5.595761,
    4.735054,
];

fn reference_output_trim(model: ModelId) -> f32 {
    REFERENCE_OUTPUT_TRIM
        .get(usize::from(model.raw()))
        .copied()
        .unwrap_or(1.0)
}

/// Shapes shared by the three kit-wide free-running modulation oscillators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    RampUp,
    RampDown,
    Square,
    SampleHold,
}

/// Global/per-slot performed sources for one sample.
#[derive(Debug, Clone, Copy)]
pub struct ModulationPatch {
    pub lfo_rate_hz: [f32; 3],
    pub lfo_shape: [LfoShape; 3],
    pub wheel: [f32; SLOT_COUNT],
    pub pressure: [f32; SLOT_COUNT],
}

impl Default for ModulationPatch {
    fn default() -> Self {
        Self {
            lfo_rate_hz: [0.7, 1.3, 4.0],
            lfo_shape: [LfoShape::Sine, LfoShape::Triangle, LfoShape::Sine],
            wheel: [0.0; SLOT_COUNT],
            pressure: [0.0; SLOT_COUNT],
        }
    }
}

/// Fixed seeds, one per kit-wide LFO.
///
/// **Distinct, because three LFOs sharing a seed are one LFO.** Sample and hold is the only shape
/// that reads them, and three identical pseudo-random streams would step together for ever.
const LFO_SEEDS: [u64; 3] = [
    0x2545_F491_4F6C_DD1D,
    0x1319_8A2E_0370_7344,
    0x5851_F42D_4C95_7F2D,
];

#[derive(Debug, Clone, Copy)]
struct Lfo {
    phase: f64,
    held: f32,
    /// The sample-and-hold sequence's state, advanced once per cycle.
    noise: u64,
    /// What `noise` returns to on [`Self::reset`], so a reset is deterministic.
    seed: u64,
}

impl Lfo {
    fn new(seed: u64) -> Self {
        let mut lfo = Self {
            phase: 0.0,
            held: 0.0,
            noise: seed,
            seed,
        };
        lfo.draw();
        lfo
    }

    /// Advances the sample-and-hold sequence and takes its next value.
    ///
    /// **Its own generator, not a hash of the phase.** The phase after a wrap is a *small*
    /// remainder, and `hash_bipolar` keeps the exponent and the top mantissa bits — precisely the
    /// bits that barely move between cycles. Measured, it emitted 0.0 for the first cycle and then
    /// about −0.509 for ever, varying in the fourth decimal: a DC offset rather than modulation,
    /// which is what the owner heard as "the decay does not change" (2026-09-22). A drawn value
    /// is also why a slot is modulated from its very first hit rather than after one full cycle.
    fn draw(&mut self) {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        self.held = hash_bipolar(self.noise);
    }

    fn tick(&mut self, rate_hz: f32, shape: LfoShape, sample_rate: f32) -> f32 {
        let phase = self.phase as f32;
        let value = match shape {
            LfoShape::Sine => (std::f32::consts::TAU * phase).sin(),
            LfoShape::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
            LfoShape::RampUp => 2.0 * phase - 1.0,
            LfoShape::RampDown => 1.0 - 2.0 * phase,
            LfoShape::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleHold => self.held,
        };
        let rate = finite_or(rate_hz, 1.0).clamp(0.01, 40.0) as f64;
        let previous = self.phase;
        self.phase = (self.phase + rate / f64::from(sample_rate)).fract();
        if self.phase < previous {
            self.draw();
        }
        value
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.noise = self.seed;
        self.draw();
    }
}

/// Complete plain-value patch for one slot and one sample.
///
/// `PartialEq` because a capture is a pure function of this and the sample rate, so comparing two
/// patches is how a consumer knows whether a frozen kit still describes the parameters
/// (`capture.rs`). Plain values throughout, so the comparison means what it looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotPatch {
    pub model: ModelId,
    pub pitch_semitones: f32,
    /// Reference-centred depth of the circuit's native downward pitch excursion.
    pub pitch_envelope: f32,
    /// Reference-centred time scale for that pitch excursion.
    pub pitch_decay: f32,
    pub decay: f32,
    pub attack: f32,
    pub tone: f32,
    pub body: f32,
    pub noise: f32,
    /// Reference-centred time scale for a distinct noise/wire envelope.
    pub noise_decay: f32,
    pub character: f32,
    pub dynamics: f32,
    pub level: f32,
    pub pan: f32,
    /// Exclusive choke group, 0 for none and 1…16 for a group. A triggered slot silences every
    /// other slot sharing its group, whatever models they are — which is what lets one machine's
    /// closed hat close another's open hat, or a short kick close a long one.
    pub choke_group: u8,
}

impl Default for SlotPatch {
    fn default() -> Self {
        Self {
            model: ModelId::OFF,
            pitch_semitones: 0.0,
            pitch_envelope: 0.0,
            pitch_decay: 0.0,
            decay: 0.0,
            attack: 0.0,
            tone: 0.0,
            body: 0.0,
            noise: 0.0,
            noise_decay: 0.0,
            character: 0.0,
            dynamics: 0.0,
            level: 1.0,
            pan: 0.0,
            choke_group: 0,
        }
    }
}

impl SlotPatch {
    fn kick(self) -> KickPatch {
        KickPatch {
            pitch_semitones: self.pitch_semitones,
            pitch_envelope: self.pitch_envelope,
            pitch_decay: self.pitch_decay,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn snare(self) -> SnarePatch {
        SnarePatch {
            pitch_semitones: self.pitch_semitones,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            noise_decay: self.noise_decay,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn falling(self) -> FallingPatch {
        FallingPatch {
            pitch_semitones: self.pitch_semitones,
            pitch_envelope: self.pitch_envelope,
            pitch_decay: self.pitch_decay,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            noise_decay: self.noise_decay,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn rim_clave(self) -> RimClavePatch {
        RimClavePatch {
            pitch_semitones: self.pitch_semitones,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn noise_percussion(self) -> NoisePatch {
        NoisePatch {
            pitch_semitones: self.pitch_semitones,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            noise_decay: self.noise_decay,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn metal(self) -> MetalPatch {
        MetalPatch {
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            character: self.character,
            dynamics: self.dynamics,
        }
    }

    fn reset_vco(self) -> ResetPatch {
        ResetPatch {
            pitch_semitones: self.pitch_semitones,
            pitch_envelope: self.pitch_envelope,
            pitch_decay: self.pitch_decay,
            decay: self.decay,
            attack: self.attack,
            tone: self.tone,
            body: self.body,
            noise: self.noise,
            noise_decay: self.noise_decay,
            character: self.character,
            dynamics: self.dynamics,
        }
    }
}

#[derive(Debug, Clone)]
struct Slot {
    model: ImplementedModel,
    kick: DeepBridgeKick,
    snare: TwinModeSnare,
    falling: FallingDrum,
    rim_clave: RimClave,
    maraca: Maraca,
    clap: PulseClap,
    private_metal: MetalBank,
    cowbell: Cowbell,
    cymbal: Cymbal,
    hat: Hat,
    reset_kick: ResetKick,
    reset_snare: ResetSnare,
    reset_tom: ResetTom,
    triple_rim: TripleRim,
    four_cell_clap: FourCellClap,
    pcm_metal: PcmMetal,
    economy: EconomyVoice,
    legacy: LegacyVoice,
    routing: Graph,
    output: DestinationRouter,
    /// Plays this slot's frozen one-shot while Resample is engaged (plan §4.7). Preallocated with
    /// the slot, like every voice here, so engaging allocates nothing.
    capture_voice: crate::capture::CaptureVoice,
    /// The model the render path is sounding. It equals `model` except while a replaced or choked
    /// circuit fades out, during which `model` stays authoritative for every new trigger.
    rendering: ImplementedModel,
    /// Catalogue id of the selected model, and of the one the render path is sounding. The output
    /// trim is a per-model constant, so a retiring circuit must keep using its own.
    model_id: ModelId,
    rendering_id: ModelId,
    /// Gain on the retiring circuit, and the per-sample step that takes it to silence. Zero gain
    /// means nothing is retiring.
    retire_gain: f32,
    retire_step: f32,
    /// Where this slot reads its metallic bank: 0 the machine-shared one, 1 its own private one,
    /// and in between a bounded crossfade. Creative Pitch moves the target; this follows it.
    metal_blend: f32,
    /// The deviation the private bank is running at. Held while the blend returns to shared so the
    /// outgoing tuning fades out as itself rather than snapping to the reference first.
    metal_pitch: f32,
    /// Whether the last rendered sample carried audio: circuit active at a non-zero level.
    sounding: bool,
    velocity: f32,
    random: f32,
    peak: f32,
}

impl Slot {
    fn new() -> Self {
        Self {
            model: ImplementedModel::Off,
            kick: DeepBridgeKick::new(),
            snare: TwinModeSnare::new(),
            falling: FallingDrum::new(),
            rim_clave: RimClave::new(),
            maraca: Maraca::new(),
            clap: PulseClap::new(),
            private_metal: MetalBank::new(),
            cowbell: Cowbell::new(),
            cymbal: Cymbal::new(),
            hat: Hat::new(),
            reset_kick: ResetKick::new(),
            reset_snare: ResetSnare::new(),
            reset_tom: ResetTom::new(),
            triple_rim: TripleRim::new(),
            four_cell_clap: FourCellClap::new(),
            pcm_metal: PcmMetal::new(),
            economy: EconomyVoice::new(),
            legacy: LegacyVoice::new(),
            rendering: ImplementedModel::Off,
            model_id: ModelId::OFF,
            rendering_id: ModelId::OFF,
            retire_gain: 0.0,
            retire_step: 1.0,
            metal_blend: 0.0,
            metal_pitch: 0.0,
            routing: Graph::new(),
            output: DestinationRouter::new(Destination::Main, 96),
            capture_voice: crate::capture::CaptureVoice::new(),
            sounding: false,
            // Full before any hit, so the Velocity source rests at zero.
            velocity: 1.0,
            random: 0.0,
            peak: 0.0,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.kick.set_sample_rate(sample_rate);
        self.snare.set_sample_rate(sample_rate);
        self.falling.set_sample_rate(sample_rate);
        self.rim_clave.set_sample_rate(sample_rate);
        self.maraca.set_sample_rate(sample_rate);
        self.clap.set_sample_rate(sample_rate);
        self.cowbell.set_sample_rate(sample_rate);
        self.cymbal.set_sample_rate(sample_rate);
        self.hat.set_sample_rate(sample_rate);
        self.reset_kick.set_sample_rate(sample_rate);
        self.reset_snare.set_sample_rate(sample_rate);
        self.reset_tom.set_sample_rate(sample_rate);
        self.triple_rim.set_sample_rate(sample_rate);
        self.four_cell_clap.set_sample_rate(sample_rate);
        self.pcm_metal.set_sample_rate(sample_rate);
        self.economy.set_sample_rate(sample_rate);
        self.legacy.set_sample_rate(sample_rate);
        self.output.set_transition_samples(
            (sample_rate * OUTPUT_TRANSFER_SECONDS).round().max(2.0) as u32,
        );
    }

    /// Installs a selected model. An outgoing circuit that is still sounding is retired through a
    /// bounded fade instead of being cut; the incoming model is authoritative for triggers at once.
    fn set_model(&mut self, id: ModelId, retire_step: f32) {
        let model = id.implemented();
        if model == self.model {
            // An unassigned id resolving to the same silence is not a model change.
            self.model_id = id;
            return;
        }
        // A retirement already under way keeps running. The circuit fading out is still audible,
        // and the selection it is being replaced by never sounded — so rapid A->B->C, and a model
        // change during a choke, only move the selection and leave A's fade alone.
        if self.retire_gain > 0.0 {
            self.model = model;
            self.model_id = id;
            return;
        }
        let retire = self.is_active();
        self.model = model;
        self.model_id = id;
        if retire {
            // `rendering` deliberately keeps the outgoing circuit, which is still sounding.
            self.retire_gain = 1.0;
            self.retire_step = retire_step;
        } else {
            self.reset();
        }
    }

    /// Terminates the addressed hit through the same bounded fade, per the brief's de-click.
    fn begin_choke(&mut self, retire_step: f32) {
        if self.retire_gain > 0.0 {
            // Already fading, and a second choke cannot beat the bound. Restarting the ramp at
            // full gain, or resetting here, would be exactly the cut this fade exists to avoid.
            return;
        }
        if !self.is_active() {
            // Nothing audible to de-click; clear any pending state and take the choke at once.
            self.finish_retire();
            return;
        }
        self.retire_gain = 1.0;
        self.retire_step = retire_step;
    }

    /// Ends any retirement at once: the outgoing state is cleared and the selected model renders.
    fn finish_retire(&mut self) {
        // `peak` is a meter the host may not have read yet, not circuit state. A fade completing
        // part way through a buffer must not discard the loudest sample of the hit that just
        // ended; only an explicit engine reset clears the meters.
        let peak = self.peak;
        self.reset();
        self.peak = peak;
    }

    /// Whether the circuit the render path is sounding still carries energy. While a model is
    /// retiring this is the outgoing one, which is what keeps the slot audible.
    fn is_active(&self) -> bool {
        if falling_kind(self.rendering).is_some() {
            self.falling.is_active()
        } else if reset_tom_kind(self.rendering).is_some() {
            self.reset_tom.is_active()
        } else if pcm_kind(self.rendering).is_some() {
            self.pcm_metal.is_active()
        } else if economy_kind(self.rendering).is_some() {
            self.economy.is_active()
        } else if matches!(self.rendering, ImplementedModel::Legacy(_)) {
            self.legacy.is_active()
        } else if rim_clave_kind(self.rendering).is_some() {
            self.rim_clave.is_active()
        } else {
            match self.rendering {
                ImplementedModel::Off => false,
                ImplementedModel::DeepBridgeKick => self.kick.is_active(),
                ImplementedModel::TwinModeSnare => self.snare.is_active(),
                ImplementedModel::BrightShortMaraca => self.maraca.is_active(),
                ImplementedModel::TriplePulseClap => self.clap.is_active(),
                ImplementedModel::TwinSquareCowbell => self.cowbell.is_active(),
                ImplementedModel::ThreePathCymbal => self.cymbal.is_active(),
                ImplementedModel::SixSquareClosedHat | ImplementedModel::SixSquareOpenHat => {
                    self.hat.is_active()
                }
                ImplementedModel::ResetPunchKick => self.reset_kick.is_active(),
                ImplementedModel::ResetTwinSnare => self.reset_snare.is_active(),
                ImplementedModel::TripleResonatorRim => self.triple_rim.is_active(),
                ImplementedModel::FourCellClap => self.four_cell_clap.is_active(),
                _ => unreachable!("family models are handled above"),
            }
        }
    }

    /// A slot that carried audio at its last sample transfers to a new destination. One that did
    /// not — idle, or muted, soloed out or at zero level while its circuit runs — has nothing to
    /// carry, so it takes the destination at once. A hit or unmute therefore never starts inside a
    /// transfer the host had no reason to process, even when it lands on the edit's own sample.
    fn route_output(&mut self, destination: Destination) {
        let destination = destination.clamped(SLOT_COUNT);
        if self.sounding {
            self.output.request(destination);
        } else {
            self.output.reset(destination);
        }
    }

    fn reset(&mut self) {
        self.kick.reset();
        self.snare.reset();
        self.falling.reset();
        self.rim_clave.reset();
        self.maraca.reset();
        self.clap.reset();
        self.private_metal.reset();
        self.cowbell.reset();
        self.cymbal.reset();
        self.hat.reset();
        self.reset_kick.reset();
        self.reset_snare.reset();
        self.reset_tom.reset();
        self.triple_rim.reset();
        self.four_cell_clap.reset();
        self.pcm_metal.reset();
        self.economy.reset();
        self.legacy.reset();
        self.routing.reset();
        // A reset ends any retirement, so the render path must not be left pointing at a circuit
        // that triggers no longer address — panic during a fade would otherwise leave the slot
        // sounding the outgoing model while the selected one silently takes every hit.
        self.rendering = self.model;
        self.rendering_id = self.model_id;
        self.retire_gain = 0.0;
        self.metal_blend = 0.0;
        self.metal_pitch = 0.0;
        self.velocity = 1.0;
        self.random = 0.0;
        self.peak = 0.0;
        self.sounding = false;
    }
}

/// Trigger assertions collected at one sample offset. One bit/velocity per slot, no allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerGroup {
    velocities: [f32; SLOT_COUNT],
    mask: u16,
}

impl Default for TriggerGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerGroup {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            velocities: [0.0; SLOT_COUNT],
            mask: 0,
        }
    }

    pub fn push(&mut self, slot: usize, velocity: f32) {
        if slot < SLOT_COUNT && velocity.is_finite() && velocity > 0.0 {
            self.mask |= 1 << slot;
            self.velocities[slot] = self.velocities[slot].max(velocity.clamp(0.0, 1.0));
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mask == 0
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    slots: [Slot; SLOT_COUNT],
    routing: [Routing; SLOT_COUNT],
    lfos: [Lfo; 3],
    random_state: u64,
    noise_808_state: u64,
    noise_808_colour: f64,
    /// The low-tom pink tail's one-pole coefficient. Derived from the sample rate alone, so it is
    /// held here rather than rebuilt per sample on the render path.
    noise_808_colour_pole: f64,
    shared_metal_808: MetalBank,
    shared_noise_909: HardwareNoise,
    shared_noise_55: EconomyNoise,
    legacy_sources: LegacySources,
    sample_rate: f32,
    tempo_bpm: f32,
    /// The frozen kit, when Resample is engaged. `None` is the live instrument.
    ///
    /// **Boxed, so installing one is a pointer move.** The audio thread hands a `Box` in and gets
    /// the displaced one back; unwrapping the value would free the incoming allocation and
    /// reboxing the outgoing one would make a new one, both inside `process`.
    captures: Option<Box<crate::capture::KitCapture>>,
    /// Whether any slot has a model that reads the shared 808 metal bank (plan §4.4).
    ///
    /// Selected, not sounding: a bank with any reader keeps running, so a tail can never be cut
    /// off from the source underneath it. Gating on *sounding* would save more and is the riskier
    /// half — this is the conservative predicate that already removes the bank from every kit
    /// that has no metal in it, which includes the idle floor.
    metal_has_reader: bool,
    /// Samples the metal bank has been dormant, waiting to be jumped forward.
    metal_dormant: u64,
    /// Whether any slot has a supporting-machine model, which reads `legacy_sources`.
    ///
    /// The largest of the shared buses at roughly 205 ns a sample, and the one a kit of 808 and
    /// 909 models never touches — so this is where gating is worth the most.
    legacy_has_reader: bool,
    /// Samples the supporting-machine sources have been dormant.
    legacy_dormant: u64,
    /// Whether any slot has a model that reads the 808's shared white or pink noise.
    noise_808_has_reader: bool,
    /// Samples the 808 noise has been dormant.
    noise_808_dormant: u64,
    /// Whether any slot has a model that reads the 909's hardware noise.
    noise_909_has_reader: bool,
    /// Samples the 909 noise has been dormant.
    noise_909_dormant: u64,
    /// Whether any slot has a DR-55-family model, which reads `shared_noise_55`.
    noise_55_has_reader: bool,
    /// Samples the DR-55 noise has been dormant.
    noise_55_dormant: u64,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::new()),
            routing: [Routing::new(); SLOT_COUNT],
            lfos: std::array::from_fn(|index| Lfo::new(LFO_SEEDS[index])),
            random_state: RANDOM_SEED,
            noise_808_state: SNARE_NOISE_SEED,
            noise_808_colour: 0.0,
            noise_808_colour_pole: noise_808_colour_pole(48_000.0),
            shared_metal_808: MetalBank::new(),
            shared_noise_909: HardwareNoise::new(),
            shared_noise_55: EconomyNoise::new(),
            legacy_sources: LegacySources::new(),
            sample_rate: 48_000.0,
            tempo_bpm: 120.0,
            captures: None,
            metal_has_reader: false,
            metal_dormant: 0,
            legacy_has_reader: false,
            legacy_dormant: 0,
            noise_808_has_reader: false,
            noise_808_dormant: 0,
            noise_909_has_reader: false,
            noise_909_dormant: 0,
            noise_55_has_reader: false,
            noise_55_dormant: 0,
        }
    }

    /// Installs the frozen kit, or `None` to play the circuits again (plan §4.7).
    ///
    /// The buffers are built by `capture::capture_kit` off the audio thread and handed over
    /// whole, so the swap itself is a pointer move and the audio thread never sees a
    /// half-installed kit. The outgoing kit is returned rather than dropped here, so its
    /// deallocation happens elsewhere — `process` must neither allocate nor free, and a kit is
    /// megabytes of `Vec<f32>`.
    pub fn set_captures(
        &mut self,
        captures: Option<Box<crate::capture::KitCapture>>,
    ) -> Option<Box<crate::capture::KitCapture>> {
        // Readers of the outgoing kit must not survive it: they index into buffers that are about
        // to go away. The caller owns the audible transition, exactly as it does for a model
        // change.
        for slot in &mut self.slots {
            slot.capture_voice.reset();
        }
        std::mem::replace(&mut self.captures, captures)
    }

    /// Whether the engine is playing captures rather than circuits.
    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        self.captures.is_some()
    }

    pub fn set_tempo_bpm(&mut self, tempo_bpm: f32) {
        self.tempo_bpm = finite_or(tempo_bpm, 120.0).clamp(30.0, 300.0);
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = finite_or(sample_rate, 48_000.0).max(1_000.0);
        self.noise_808_colour_pole = noise_808_colour_pole(self.sample_rate);
        for slot in &mut self.slots {
            slot.set_sample_rate(self.sample_rate);
        }
    }

    /// Replaces one slot's route values and rebuilds its topology only when a presence changed.
    pub fn set_slot_routing(&mut self, index: usize, routing: Routing) {
        let Some((slot, previous)) = self.slots.get_mut(index).zip(self.routing.get_mut(index))
        else {
            return;
        };
        if routing.present != previous.present {
            slot.routing.set_topology(&routing);
        }
        *previous = routing;
    }

    /// Installs destinations without a transfer, for activation/state restore before audio starts.
    pub fn reset_output_destinations(&mut self, destinations: &[Destination; SLOT_COUNT]) {
        for (slot, destination) in self.slots.iter_mut().zip(destinations) {
            slot.output.reset(destination.clamped(SLOT_COUNT));
        }
    }

    /// Applies the current destination selectors: slots that carried audio at their last sample
    /// transfer, the rest move at once. Call before an event group's triggers so a hit on a silent
    /// slot starts where it is sent; `process_routed` applies the same law again, which is then a
    /// no-op.
    pub fn route_outputs(&mut self, destinations: &[Destination; SLOT_COUNT]) {
        for (slot, &destination) in self.slots.iter_mut().zip(destinations) {
            slot.route_output(destination);
        }
    }

    /// Applies discrete model changes before an event group or render interval.
    pub fn prepare(&mut self, patches: &[SlotPatch; SLOT_COUNT]) {
        let retire_step = self.retire_step();
        for (slot, patch) in self.slots.iter_mut().zip(patches) {
            slot.set_model(patch.model, retire_step);
        }
        // A retiring circuit still renders, so the outgoing model counts as a reader too.
        self.metal_has_reader = self
            .slots
            .iter()
            .any(|slot| is_metal(slot.model) || is_metal(slot.rendering));
        self.legacy_has_reader = self.slots.iter().any(|slot| {
            matches!(slot.model, ImplementedModel::Legacy(_))
                || matches!(slot.rendering, ImplementedModel::Legacy(_))
        });
        self.noise_808_has_reader = self
            .slots
            .iter()
            .any(|slot| reads_808_noise(slot.model) || reads_808_noise(slot.rendering));
        self.noise_909_has_reader = self
            .slots
            .iter()
            .any(|slot| reads_909_noise(slot.model) || reads_909_noise(slot.rendering));
        self.noise_55_has_reader = self.slots.iter().any(|slot| {
            economy_kind(slot.model).is_some() || economy_kind(slot.rendering).is_some()
        });
    }

    /// Per-sample step of the bounded model-retirement fade at the current rate.
    fn retire_step(&self) -> f32 {
        1.0 / (self.sample_rate * MODEL_RETIRE_SECONDS).max(1.0)
    }

    /// Resolves one same-offset trigger group.
    ///
    /// The first implemented family uses a common accent voltage, so every struck deep bridge kick
    /// sees the maximum velocity in this group. This makes host event iteration order irrelevant.
    pub fn trigger_group(&mut self, patches: &[SlotPatch; SLOT_COUNT], triggers: TriggerGroup) {
        // Choke groups. A triggered slot silences every other slot sharing its group, through the
        // same bounded de-click a NoteChoke uses, so a long circuit cut by a short one does not
        // click. Slots triggered in this same group are exempt — they are all starting, not being
        // cut — which is also what keeps the result independent of host event order.
        //
        // This replaces the source machines' hardwired closed/open hat pairs. Those could only
        // ever couple one machine's own two hats; a group couples any two slots, which is what a
        // player actually wants when the open hat is an 808 and the closed one a 909.
        let mut struck_groups: u32 = 0;
        for (index, patch) in patches.iter().enumerate() {
            if triggers.mask & (1 << index) != 0 && patch.choke_group != 0 {
                struck_groups |= 1 << patch.choke_group.min(CHOKE_GROUP_MAX);
            }
        }
        if struck_groups != 0 {
            let retire_step = self.retire_step();
            for (index, (slot, patch)) in self.slots.iter_mut().zip(patches).enumerate() {
                if triggers.mask & (1 << index) != 0 || patch.choke_group == 0 {
                    continue;
                }
                if struck_groups & (1 << patch.choke_group.min(CHOKE_GROUP_MAX)) != 0 {
                    slot.begin_choke(retire_step);
                }
            }
        }
        for (index, (slot, patch)) in self.slots.iter_mut().zip(patches).enumerate() {
            if triggers.mask & (1 << index) == 0 {
                continue;
            }
            // A hit is its own velocity, whatever else is struck on the same sample. The hardware's
            // common and global accent buses are deliberately not modelled: this instrument has no
            // accent control and one slot's level never depends on another's. Host event order
            // therefore cannot reach the result either.
            let common_accent = triggers.velocities[index];
            // A hit on a slot whose previous circuit is still fading ends that fade: the
            // selected model owns this trigger, and the two never share voice state.
            if slot.retire_gain > 0.0 {
                slot.finish_retire();
            }
            slot.velocity = triggers.velocities[index];
            self.random_state = xorshift64(self.random_state);
            slot.random = hash_bipolar(self.random_state);
            // Frozen, the hit starts a reader over this slot's capture and the circuit below is
            // left alone. Velocity scales the buffer, because the capture was taken at one strike
            // and the model's own velocity law is inside it — plan §4.7's named loss — on the
            // velocity curve Dynamics bends on every family (`crate::velocity`).
            if let Some(captures) = &self.captures {
                slot.capture_voice.trigger(
                    captures.slot(index),
                    crate::velocity::curve(triggers.velocities[index], patch.dynamics),
                    patch.decay,
                    crate::capture::Retrigger::Restart,
                    self.sample_rate,
                );
                continue;
            }
            if let Some(kind) = falling_kind(slot.model) {
                slot.falling.trigger(kind, common_accent, patch.falling());
            } else if reset_tom_kind(slot.model).is_some() {
                slot.reset_tom.trigger(common_accent, patch.reset_vco());
            } else if pcm_kind(slot.model).is_some() {
                slot.pcm_metal.trigger(common_accent, *patch);
            } else if economy_kind(slot.model).is_some() {
                slot.economy.trigger(common_accent, *patch);
            } else if matches!(slot.model, ImplementedModel::Legacy(_)) {
                slot.legacy.trigger(common_accent, *patch);
            } else if rim_clave_kind(slot.model).is_some() {
                slot.rim_clave.trigger(common_accent, patch.rim_clave());
            } else {
                match slot.model {
                    ImplementedModel::Off => {}
                    ImplementedModel::DeepBridgeKick => {
                        slot.kick
                            .trigger(common_accent, patch.dynamics, patch.attack);
                    }
                    ImplementedModel::TwinModeSnare => {
                        slot.snare.trigger(
                            common_accent,
                            patch.dynamics,
                            patch.attack,
                            patch.noise,
                        );
                    }
                    ImplementedModel::BrightShortMaraca => {
                        slot.maraca.trigger(common_accent, patch.noise_percussion());
                    }
                    ImplementedModel::TriplePulseClap => {
                        slot.clap.trigger(common_accent, patch.noise_percussion());
                    }
                    ImplementedModel::TwinSquareCowbell => {
                        slot.cowbell.trigger(common_accent, patch.metal());
                    }
                    ImplementedModel::ThreePathCymbal => {
                        slot.cymbal.trigger(common_accent, patch.metal());
                    }
                    ImplementedModel::SixSquareClosedHat | ImplementedModel::SixSquareOpenHat => {
                        slot.hat.trigger(common_accent, patch.metal());
                    }
                    ImplementedModel::ResetPunchKick => {
                        slot.reset_kick.trigger(common_accent, patch.reset_vco());
                    }
                    ImplementedModel::ResetTwinSnare => {
                        slot.reset_snare.trigger(common_accent, patch.reset_vco());
                    }
                    ImplementedModel::TripleResonatorRim => {
                        slot.triple_rim.trigger(common_accent, *patch);
                    }
                    ImplementedModel::FourCellClap => {
                        slot.four_cell_clap.trigger(common_accent, *patch);
                    }
                    _ => unreachable!("family models are handled above"),
                }
            }
        }
    }

    /// Circuit-defined immediate choke for one slot.
    /// One sample of the 808's shared white and pink noise.
    ///
    /// CHOSEN one-pole colour for the low-tom's documented pink tail. This is machine-shared
    /// state rather than a private statistically similar source. Replace from isolated tail
    /// spectrum measurement; the coefficient follows the sample rate only and is held on the
    /// engine by `noise_808_colour_pole`.
    fn step_808_noise(&mut self) -> (f32, f32) {
        self.noise_808_state = xorshift64(self.noise_808_state);
        let white = hash_bipolar(self.noise_808_state);
        let colour_pole = self.noise_808_colour_pole;
        self.noise_808_colour =
            colour_pole * self.noise_808_colour + (1.0 - colour_pole) * f64::from(white);
        let pink = (0.35 * f64::from(white) + 1.8 * self.noise_808_colour) as f32;
        (white, pink)
    }

    /// Brings the 808 noise up to date after `dormant` samples.
    ///
    /// **The noise state jumps exactly and the colour filter is replayed, because the two are
    /// different kinds of state.** A shift register is linear over GF(2), so `jump.rs` can move it
    /// any distance in constant time. A one-pole filter's value depends on every sample that fed
    /// it, and no closed form recovers that — but it *forgets*: a wrong starting value is
    /// multiplied by the pole once per sample, so after enough samples its contribution falls
    /// below what an `f64` can represent beside the running value, and the replay is then exact
    /// rather than merely close. `colour_forgetting_window` is how many samples that takes.
    ///
    /// So a long gap costs a fixed replay and no more, and a short one is just ticked.
    fn noise_808_catch_up(&mut self, dormant: u64) {
        let window = self.colour_forgetting_window();
        if dormant <= window {
            for _ in 0..dormant {
                let _ = self.step_808_noise();
            }
            return;
        }
        self.noise_808_state =
            crate::jump::advance_xorshift64(self.noise_808_state, dormant - window);
        for _ in 0..window {
            let _ = self.step_808_noise();
        }
    }

    /// How many samples the colour filter takes to forget a wrong starting value, exactly.
    ///
    /// The pole raised to this power, times the largest error the state can hold, is below the
    /// `f64` resolution of a state of order one. It grows with the sample rate — about 240
    /// samples at 48 kHz and 960 at 192 — and is bounded so a hostile rate cannot make a resume
    /// unbounded.
    fn colour_forgetting_window(&self) -> u64 {
        let pole = self.noise_808_colour_pole;
        if !(0.0..1.0).contains(&pole) {
            return 1;
        }
        // `f64`, not `f32`: the filter state is `f64`, and using the smaller epsilon gave a
        // window less than half what is needed — which the forgetting test caught. A quarter of
        // an epsilon covers a starting error of up to 2, and a quarter again is margin.
        let samples = 1.25 * (f64::EPSILON / 4.0).ln() / pole.ln();
        (samples.ceil().max(1.0) as u64).min(8_192)
    }

    pub fn choke(&mut self, slot: usize) {
        let retire_step = self.retire_step();
        if let Some(slot) = self.slots.get_mut(slot) {
            slot.begin_choke(retire_step);
            // Choke is an assignment, not circuit wiring, so it means the same thing frozen: the
            // slot stops. A buffer honours that exactly, which is why choke loses nothing here.
            slot.capture_voice.choke();
        }
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, patches: &[SlotPatch; SLOT_COUNT]) -> [f32; 2] {
        self.process_modulated(patches, ModulationPatch::default())
    }

    #[inline]
    #[must_use]
    pub fn process_modulated(
        &mut self,
        patches: &[SlotPatch; SLOT_COUNT],
        modulation: ModulationPatch,
    ) -> [f32; 2] {
        self.process_routed(patches, modulation, &[Destination::Main; SLOT_COUNT])
            .main
    }

    /// Renders main stereo and sixteen mono individual destinations.
    ///
    /// `Destination::Main` addresses stereo; the sixteen auxiliary values address
    /// `individual[0]`…`individual[15]`. The router receives the post-Level, pre-Pan mono signal.
    /// Pan therefore applies only to main and is bypassed on every individual output, including
    /// while a transfer crosses between the two domains.
    #[inline]
    #[must_use]
    pub fn process_routed(
        &mut self,
        patches: &[SlotPatch; SLOT_COUNT],
        modulation: ModulationPatch,
        destinations: &[Destination; SLOT_COUNT],
    ) -> OutputFrame {
        // Free-running, as their name says: phase advances on every rendered sample whether or
        // not a route currently reads them, so emptying the matrix and filling it again resumes
        // the phase the elapsed samples imply rather than the one it stopped on. Gating the first
        // two did not move `drum_machine_cpu_cost` beyond its run-to-run spread; the third follows
        // the same law rather than quietly meaning something different while nothing listens.
        let lfo: [f32; 3] = std::array::from_fn(|index| {
            self.lfos[index].tick(
                modulation.lfo_rate_hz[index],
                modulation.lfo_shape[index],
                self.sample_rate,
            )
        });

        // The source is a machine-level free-running transistor-noise approximation. Advance it
        // once per rendered sample even when no current consumer uses it; every consumer reads the
        // same sample below.
        // Gated with the same rule as the banks: counted while nothing reads it, brought up to
        // date when something does. The noise state jumps exactly; the colour filter is replayed,
        // for the reason `noise_808_catch_up` gives.
        let (shared_white_noise, shared_pink_noise) = if self.noise_808_has_reader {
            if self.noise_808_dormant > 0 {
                let dormant = self.noise_808_dormant;
                self.noise_808_dormant = 0;
                self.noise_808_catch_up(dormant);
            }
            self.step_808_noise()
        } else {
            self.noise_808_dormant = self.noise_808_dormant.saturating_add(1);
            (0.0, 0.0)
        };
        // **Gated (plan §4.4).** A bus nothing reads is not computed, but it is not stopped
        // either: the elapsed samples are counted and handed to `advance`, which moves the bank
        // to exactly where running would have left it. Free-running behaviour without the
        // free-running cost — and never a reseed, which would lose the phase relationship
        // between the cowbell's two squares and the cymbal's six.
        let shared_metal = if self.metal_has_reader {
            if self.metal_dormant > 0 {
                self.shared_metal_808
                    .advance(self.metal_dormant, self.sample_rate, 0.0);
                self.metal_dormant = 0;
            }
            self.shared_metal_808.tick(self.sample_rate, 0.0)
        } else {
            self.metal_dormant = self.metal_dormant.saturating_add(1);
            crate::metal_808::Frame::default()
        };
        let shared_noise_909 = if self.noise_909_has_reader {
            if self.noise_909_dormant > 0 {
                self.shared_noise_909
                    .advance(self.noise_909_dormant, self.sample_rate);
                self.noise_909_dormant = 0;
            }
            self.shared_noise_909.tick(self.sample_rate)
        } else {
            self.noise_909_dormant = self.noise_909_dormant.saturating_add(1);
            0.0
        };
        let shared_noise_55 = if self.noise_55_has_reader {
            if self.noise_55_dormant > 0 {
                self.shared_noise_55.advance(self.noise_55_dormant);
                self.noise_55_dormant = 0;
            }
            self.shared_noise_55.tick()
        } else {
            self.noise_55_dormant = self.noise_55_dormant.saturating_add(1);
            0.0
        };
        // Gated exactly as the metal bank is: counted while nothing reads it, jumped forward
        // when something does. This is the biggest of the buses, and a kit with no supporting
        // model never pays for it.
        let legacy_frames = if self.legacy_has_reader {
            if self.legacy_dormant > 0 {
                self.legacy_sources
                    .advance(self.legacy_dormant, self.sample_rate);
                self.legacy_dormant = 0;
            }
            self.legacy_sources.tick(self.sample_rate)
        } else {
            self.legacy_dormant = self.legacy_dormant.saturating_add(1);
            [crate::legacy::Frame::default(); 6]
        };

        let metal_step = 1.0 / (self.sample_rate * METAL_TRANSITION_SECONDS).max(1.0);
        let mut output = OutputFrame::default();
        for (index, (slot, base_patch)) in self.slots.iter_mut().zip(patches).enumerate() {
            // Judged from the previous sample, before this one renders.
            slot.route_output(destinations[index]);
            let mut patch = *base_patch;
            let routing = &self.routing[index];
            if routing.any() {
                slot.routing.publish(
                    routing,
                    Sources {
                        lfo1: lfo[0],
                        lfo2: lfo[1],
                        lfo3: lfo[2],
                        // The performance sources through the collection's standard, each zero
                        // at its rest: Velocity at the hardest hit.
                        wheel: standard::wheel(finite_or(modulation.wheel[index], 0.0)),
                        pressure: standard::pressure(finite_or(modulation.pressure[index], 0.0)),
                        velocity: standard::velocity(slot.velocity),
                        random: slot.random,
                    },
                );
                apply_routes(&mut patch, &slot.routing, routing);
            }
            // Two slots cannot ask one physical bank for two tunings, so a creative deviation
            // takes a slot-private constellation. The banks free-run on unrelated phases, so the
            // move between them is a bounded crossfade rather than a switch on one sample;
            // returning to the reference rejoins the shared phase without resetting either bank.
            let metal = {
                let private = is_metal(slot.rendering) && patch.pitch_semitones.abs() > 1.0e-6;
                if private {
                    slot.metal_pitch = patch.pitch_semitones;
                }
                let target = if private { 1.0 } else { 0.0 };
                slot.metal_blend += (target - slot.metal_blend).clamp(-metal_step, metal_step);
                if slot.metal_blend > 0.0 {
                    let private = slot.private_metal.tick(self.sample_rate, slot.metal_pitch);
                    shared_metal.blend(private, slot.metal_blend)
                } else {
                    // Parked on shared: the private bank keeps its phase and costs nothing.
                    shared_metal
                }
            };
            let mono = if let Some(kind) = falling_kind(slot.rendering) {
                slot.falling
                    .process(kind, patch.falling(), shared_pink_noise)
            } else if let Some(kind) = reset_tom_kind(slot.rendering) {
                slot.reset_tom
                    .process(kind, patch.reset_vco(), shared_noise_909)
            } else if let Some(kind) = pcm_kind(slot.rendering) {
                slot.pcm_metal.process(kind, patch)
            } else if let Some(kind) = economy_kind(slot.rendering) {
                slot.economy.process(kind, patch, shared_noise_55)
            } else if let ImplementedModel::Legacy(id) = slot.rendering {
                slot.legacy
                    .process(id, patch, legacy_frames[legacy_machine(id)], self.tempo_bpm)
            } else if let Some(kind) = rim_clave_kind(slot.rendering) {
                slot.rim_clave.process(kind, patch.rim_clave())
            } else {
                match slot.rendering {
                    ImplementedModel::Off => 0.0,
                    ImplementedModel::DeepBridgeKick => slot.kick.process(patch.kick()),
                    ImplementedModel::TwinModeSnare => {
                        slot.snare.process(patch.snare(), shared_white_noise)
                    }
                    ImplementedModel::BrightShortMaraca => slot
                        .maraca
                        .process(patch.noise_percussion(), shared_white_noise),
                    ImplementedModel::TriplePulseClap => slot
                        .clap
                        .process(patch.noise_percussion(), shared_white_noise),
                    ImplementedModel::TwinSquareCowbell => {
                        slot.cowbell.process(patch.metal(), metal)
                    }
                    ImplementedModel::ThreePathCymbal => slot.cymbal.process(patch.metal(), metal),
                    ImplementedModel::SixSquareClosedHat => {
                        slot.hat.process(HatKind::Closed, patch.metal(), metal)
                    }
                    ImplementedModel::SixSquareOpenHat => {
                        slot.hat.process(HatKind::Open, patch.metal(), metal)
                    }
                    ImplementedModel::ResetPunchKick => {
                        slot.reset_kick.process(patch.reset_vco(), shared_noise_909)
                    }
                    ImplementedModel::ResetTwinSnare => slot
                        .reset_snare
                        .process(patch.reset_vco(), shared_noise_909),
                    ImplementedModel::TripleResonatorRim => slot.triple_rim.process(patch),
                    ImplementedModel::FourCellClap => {
                        slot.four_cell_clap.process(patch, shared_noise_909)
                    }
                    _ => unreachable!("family models are handled above"),
                }
            };
            // Keep circuit gain inside each renderer; adapt catalogue models to one usable output
            // plane only after their complete topology has rendered.
            // The trim belongs to the circuit that is sounding, which while retiring is the
            // outgoing one, not the newly selected model.
            //
            // **Frozen, the capture replaces everything up to here and nothing below it.** A
            // capture was taken from the auxiliary tap, which is already past the trim, so
            // applying the trim again would double it. Every stage below — Level, Pan, the
            // destination transfer, the peak meter — is the live path unchanged, which is what
            // makes Level, Pan, Mute and Solo lossless while frozen.
            let mut mono = match &self.captures {
                Some(captures) => slot
                    .capture_voice
                    .process(captures.slot(index), patch.pitch_semitones),
                None => mono * reference_output_trim(slot.rendering_id),
            };
            if slot.retire_gain > 0.0 {
                mono *= slot.retire_gain;
                slot.retire_gain -= slot.retire_step;
                if slot.retire_gain <= 0.0 {
                    slot.finish_retire();
                }
            }
            slot.peak = slot.peak.max(mono.abs());
            let level = finite_or(patch.level, 0.0).clamp(0.0, 4.0);
            let pan = finite_or(patch.pan, 0.0).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let signal = mono * level;
            slot.sounding = level > 0.0
                && match &self.captures {
                    Some(_) => slot.capture_voice.is_active(),
                    None => slot.is_active(),
                };
            for route in slot.output.next_partition().into_iter().flatten() {
                let routed = signal * route.gain;
                match route.destination {
                    Destination::Main => {
                        // The pan law's two transcendentals are the slot's own cost, but a silent
                        // slot's contribution is zero whatever the angle, and sixteen idle slots
                        // paid for them every sample. Skipping a zero add is exact: `routed` is
                        // finite here or already non-finite, and a non-finite value fails this
                        // test and still propagates.
                        if routed != 0.0 {
                            output.main[0] += routed * angle.cos();
                            output.main[1] += routed * angle.sin();
                        }
                    }
                    Destination::Auxiliary(index) => output.individual[index] += routed,
                }
            }
        }
        output
    }

    /// Max-combined activity since the last read, for one slot.
    pub fn take_slot_peaks(&mut self) -> [f32; SLOT_COUNT] {
        std::array::from_fn(|index| std::mem::take(&mut self.slots[index].peak))
    }

    /// What slot `slot`'s frame holds for `source`, for tests.
    #[cfg(test)]
    pub(crate) fn published_for_test(&self, slot: usize, source: usize) -> f32 {
        self.slots[slot].routing.read_for_test(source)
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        // Frozen, the circuits are parked and the readers are what is sounding. Reporting the
        // circuits here would end `Tail` while a captured hit was still playing and let a host
        // cut it off.
        match &self.captures {
            Some(_) => self.slots.iter().any(|slot| slot.capture_voice.is_active()),
            None => self.slots.iter().any(Slot::is_active),
        }
    }

    pub fn reset(&mut self) {
        for (slot, routing) in self.slots.iter_mut().zip(&self.routing) {
            slot.reset();
            slot.routing.set_topology(routing);
            // Reset clears live state and leaves parameters alone. A capture is neither: it is
            // derived, like a coefficient, so it survives — which is also what keeps
            // `reset(); render()` identical twice, and what stops a DAW's per-stop reset
            // re-rendering the whole kit.
            slot.capture_voice.reset();
        }
        for lfo in &mut self.lfos {
            lfo.reset();
        }
        self.random_state = RANDOM_SEED;
        self.noise_808_state = SNARE_NOISE_SEED;
        self.noise_808_colour = 0.0;
        self.shared_metal_808.reset();
        self.shared_noise_909.reset();
        self.shared_noise_55.reset();
        self.legacy_sources.reset();
    }
}

fn economy_kind(model: ImplementedModel) -> Option<EconomyKind> {
    match model {
        ImplementedModel::Economy62Kick => Some(EconomyKind::Kick),
        ImplementedModel::EconomyBodySnare => Some(EconomyKind::Snare),
        ImplementedModel::EconomyShortRim => Some(EconomyKind::Rim),
        ImplementedModel::InductorNoiseHat => Some(EconomyKind::Hat),
        _ => None,
    }
}

fn pcm_kind(model: ImplementedModel) -> Option<PcmKind> {
    match model {
        ImplementedModel::SixBitClosedHat => Some(PcmKind::ClosedHat),
        ImplementedModel::SixBitOpenHat => Some(PcmKind::OpenHat),
        ImplementedModel::SixBitCrash => Some(PcmKind::Crash),
        ImplementedModel::SixBitRide => Some(PcmKind::Ride),
        _ => None,
    }
}

fn reset_tom_kind(model: ImplementedModel) -> Option<TomKind> {
    match model {
        ImplementedModel::LowResetTriadTom => Some(TomKind::Low),
        ImplementedModel::MidResetTriadTom => Some(TomKind::Mid),
        ImplementedModel::HighResetTriadTom => Some(TomKind::High),
        _ => None,
    }
}

/// Whether a model reads the 808's shared white or pink noise.
///
/// The toms take the pink tail, the snare its wire noise, and the maraca and clap their bursts.
/// Listed against the render arms rather than by family, because that is where the reads are.
fn reads_808_noise(model: ImplementedModel) -> bool {
    falling_kind(model).is_some()
        || matches!(
            model,
            ImplementedModel::TwinModeSnare
                | ImplementedModel::BrightShortMaraca
                | ImplementedModel::TriplePulseClap
        )
}

/// Whether a model reads the 909's shared hardware noise.
fn reads_909_noise(model: ImplementedModel) -> bool {
    reset_tom_kind(model).is_some()
        || matches!(
            model,
            ImplementedModel::ResetPunchKick
                | ImplementedModel::ResetTwinSnare
                | ImplementedModel::FourCellClap
        )
}

fn is_metal(model: ImplementedModel) -> bool {
    matches!(
        model,
        ImplementedModel::TwinSquareCowbell
            | ImplementedModel::ThreePathCymbal
            | ImplementedModel::SixSquareClosedHat
            | ImplementedModel::SixSquareOpenHat
    )
}

fn falling_kind(model: ImplementedModel) -> Option<FallingKind> {
    match model {
        ImplementedModel::LowFallingTom => Some(FallingKind::LowTom),
        ImplementedModel::LowFallingConga => Some(FallingKind::LowConga),
        ImplementedModel::MidFallingTom => Some(FallingKind::MidTom),
        ImplementedModel::MidFallingConga => Some(FallingKind::MidConga),
        ImplementedModel::HighFallingTom => Some(FallingKind::HighTom),
        ImplementedModel::HighFallingConga => Some(FallingKind::HighConga),
        _ => None,
    }
}

fn rim_clave_kind(model: ImplementedModel) -> Option<RimClaveKind> {
    match model {
        ImplementedModel::LayeredShortRim => Some(RimClaveKind::Rim),
        ImplementedModel::PureHighClave => Some(RimClaveKind::Clave),
        _ => None,
    }
}

fn apply_routes(patch: &mut SlotPatch, graph: &Graph, routing: &Routing) {
    patch.pitch_semitones = (patch.pitch_semitones
        + graph.sum(routing::target::PITCH, routing, 24.0))
    .clamp(-48.0, 48.0);
    patch.pitch_envelope = (patch.pitch_envelope
        + graph.sum(routing::target::PITCH_ENV, routing, 2.0))
    .clamp(-1.0, 1.0);
    patch.pitch_decay = (patch.pitch_decay + graph.sum(routing::target::PITCH_DECAY, routing, 2.0))
        .clamp(-1.0, 1.0);
    patch.decay = (patch.decay + graph.sum(routing::target::DECAY, routing, 2.0)).clamp(-1.0, 1.0);
    patch.attack =
        (patch.attack + graph.sum(routing::target::ATTACK, routing, 2.0)).clamp(-1.0, 1.0);
    patch.tone = (patch.tone + graph.sum(routing::target::TONE, routing, 2.0)).clamp(-1.0, 1.0);
    patch.body = (patch.body + graph.sum(routing::target::BODY, routing, 2.0)).clamp(-1.0, 1.0);
    patch.noise = (patch.noise + graph.sum(routing::target::NOISE, routing, 2.0)).clamp(-1.0, 1.0);
    patch.noise_decay = (patch.noise_decay + graph.sum(routing::target::NOISE_DECAY, routing, 2.0))
        .clamp(-1.0, 1.0);
    patch.character =
        (patch.character + graph.sum(routing::target::CHARACTER, routing, 2.0)).clamp(-1.0, 1.0);
    patch.dynamics =
        (patch.dynamics + graph.sum(routing::target::DYNAMICS, routing, 2.0)).clamp(-1.0, 1.0);
    // Amplitude: the collection's one law, a factor on the level, silence to double.
    patch.level *= standard::amplitude_factor(graph.sum(
        routing::target::LEVEL,
        routing,
        routing::AMPLITUDE_BOUND,
    ));
    patch.pan = (patch.pan + graph.sum(routing::target::PAN, routing, 2.0)).clamp(-1.0, 1.0);
}

#[inline]
fn xorshift64(mut value: u64) -> u64 {
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value
}

#[inline]
/// One Random draw per hit: `standard::random` of a uniform draw.
fn hash_bipolar(bits: u64) -> f32 {
    let unit = ((bits >> 40) as u32) as f32 / ((1_u32 << 24) - 1) as f32;
    standard::random(unit)
}

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// The 808 low-tom pink tail's one-pole coefficient at one sample rate. Called from activation and
/// sample-rate change, never per sample; the expression is the one the tail was calibrated with.
fn noise_808_colour_pole(sample_rate: f32) -> f64 {
    (-std::f64::consts::TAU * 1_200.0 / f64::from(sample_rate)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_kick() -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        patches
    }

    #[test]
    fn sixteen_slots_are_simultaneous_and_invalid_models_are_silent() {
        let mut engine = Engine::new();
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        for patch in &mut patches {
            patch.model = ModelId::DEEP_BRIDGE_KICK;
            patch.level = 0.02;
        }
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        for slot in 0..SLOT_COUNT {
            triggers.push(slot, 0.8);
        }
        engine.trigger_group(&patches, triggers);
        assert!(engine.process(&patches).iter().any(|sample| *sample != 0.0));

        for patch in &mut patches {
            patch.model = ModelId::new(255);
        }
        engine.prepare(&patches);
        // The sounding kicks are retired through the bounded fade rather than cut, so silence is
        // owed by the end of the transition, not on its first sample. What the unassigned id must
        // never do is sound on its own account.
        let transition = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 1;
        for _ in 0..transition {
            let frame = engine.process(&patches);
            assert!(
                frame.iter().all(|sample| sample.is_finite()),
                "the retiring circuit left a non-finite sample"
            );
        }
        assert_eq!(engine.process(&patches), [0.0, 0.0]);
        assert!(!engine.is_active());
    }

    #[test]
    fn routed_main_is_the_legacy_stereo_render_and_mono_outputs_bypass_pan() {
        let render = |pan: f32, destination: Destination| {
            let mut engine = Engine::new();
            let mut patches = one_kick();
            patches[0].pan = pan;
            let destinations = std::array::from_fn(|slot| {
                if slot == 0 {
                    destination
                } else {
                    Destination::Main
                }
            });
            engine.reset_output_destinations(&destinations);
            engine.prepare(&patches);
            let mut trigger = TriggerGroup::new();
            trigger.push(0, 0.82);
            engine.trigger_group(&patches, trigger);
            (0..256)
                .map(|_| engine.process_routed(&patches, ModulationPatch::default(), &destinations))
                .collect::<Vec<_>>()
        };

        let left = render(-1.0, Destination::Auxiliary(0));
        let right = render(1.0, Destination::Auxiliary(0));
        assert!(left.iter().any(|frame| frame.individual[0] != 0.0));
        assert!(left.iter().all(|frame| frame.main == [0.0; 2]));
        assert_eq!(
            left.iter()
                .map(|frame| frame.individual[0])
                .collect::<Vec<_>>(),
            right
                .iter()
                .map(|frame| frame.individual[0])
                .collect::<Vec<_>>()
        );

        let centred = render(0.0, Destination::Main);
        assert!(
            centred
                .iter()
                .all(|frame| frame.individual == [0.0; SLOT_COUNT])
        );
        assert!(centred.iter().any(|frame| frame.main != [0.0; 2]));
    }

    #[test]
    fn several_slots_share_one_mono_destination_by_summing_once() {
        let render = |second_level: f32| {
            let mut engine = Engine::new();
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            for (slot, level) in [(0, 1.0), (1, second_level)] {
                patches[slot].model = ModelId::DEEP_BRIDGE_KICK;
                patches[slot].level = level;
            }
            let destinations = std::array::from_fn(|slot| {
                if slot < 2 {
                    Destination::Auxiliary(0)
                } else {
                    Destination::Main
                }
            });
            engine.reset_output_destinations(&destinations);
            engine.prepare(&patches);
            let mut trigger = TriggerGroup::new();
            trigger.push(0, 0.82);
            trigger.push(1, 0.82);
            engine.trigger_group(&patches, trigger);
            (0..256)
                .map(|_| {
                    engine
                        .process_routed(&patches, ModulationPatch::default(), &destinations)
                        .individual[0]
                })
                .collect::<Vec<_>>()
        };
        let one = render(0.0);
        let two = render(1.0);
        for (one, two) in one.into_iter().zip(two) {
            assert!((two - 2.0 * one).abs() < 1.0e-6);
        }
    }

    /// 2 ms at the engine's default 48 kHz.
    const TRANSFER: usize = 96;

    /// Sixteen kicks panned hard left, each at its own level so a slot's audio is its own tag. Hard
    /// left makes a main share land on `main[0]` at unit gain and leaves `main[1]` exactly zero.
    fn tagged_kicks() -> [SlotPatch; SLOT_COUNT] {
        std::array::from_fn(|slot| SlotPatch {
            model: ModelId::DEEP_BRIDGE_KICK,
            level: 0.2 + 0.05 * slot as f32,
            pan: -1.0,
            ..SlotPatch::default()
        })
    }

    /// Slot `s` goes to individual output `(5 s + shift) mod 16`: a permutation, since 5 and 16 are
    /// coprime, and every slot's destination differs between two shifts.
    fn permutation(shift: usize) -> [Destination; SLOT_COUNT] {
        std::array::from_fn(|slot| Destination::Auxiliary((slot * 5 + shift) % SLOT_COUNT))
    }

    /// Strikes `struck` at sample 0 and renders `frames` samples in the plugin's order: selectors
    /// are applied before each sample renders. `schedule[0]` is installed before the hit; every
    /// later entry takes effect from its sample.
    fn render_tagged(
        struck: &[usize],
        schedule: &[(usize, [Destination; SLOT_COUNT])],
        frames: usize,
    ) -> Vec<OutputFrame> {
        let mut engine = Engine::new();
        let patches = tagged_kicks();
        let mut destinations = schedule[0].1;
        engine.reset_output_destinations(&destinations);
        engine.prepare(&patches);
        let mut trigger = TriggerGroup::new();
        for &slot in struck {
            trigger.push(slot, 0.82);
        }
        engine.trigger_group(&patches, trigger);
        (0..frames)
            .map(|frame| {
                if let Some((_, next)) = schedule.iter().skip(1).find(|(at, _)| *at == frame) {
                    destinations = *next;
                }
                engine.route_outputs(&destinations);
                engine.process_routed(&patches, ModulationPatch::default(), &destinations)
            })
            .collect()
    }

    /// The slot's own audio: rendered alone on main, where hard-left Pan passes it at unit gain.
    fn solo_reference(slot: usize, frames: usize) -> Vec<f32> {
        render_tagged(&[slot], &[(0, [Destination::Main; SLOT_COUNT])], frames)
            .iter()
            .map(|frame| frame.main[0])
            .collect()
    }

    #[test]
    fn a_silent_slot_takes_its_new_destination_before_the_next_hit() {
        let mut engine = Engine::new();
        let patches = one_kick();
        engine.reset_output_destinations(&[Destination::Main; SLOT_COUNT]);
        // A sleeping host processes nothing between the edit and the hit, so no transfer could run.
        let mut destinations = [Destination::Main; SLOT_COUNT];
        destinations[0] = Destination::Auxiliary(4);
        engine.route_outputs(&destinations);
        engine.prepare(&patches);
        let mut trigger = TriggerGroup::new();
        trigger.push(0, 0.82);
        engine.trigger_group(&patches, trigger);
        let frames = (0..256)
            .map(|_| engine.process_routed(&patches, ModulationPatch::default(), &destinations))
            .collect::<Vec<_>>();
        assert!(frames.iter().all(|frame| frame.main == [0.0; 2]));
        assert!(frames.iter().any(|frame| frame.individual[4] != 0.0));
    }

    #[test]
    fn a_muted_running_slot_takes_its_new_destination_before_an_unmuted_hit() {
        let mut engine = Engine::new();
        let mut patches = one_kick();
        // Mute and Solo reach the engine as level zero; the circuit keeps running underneath.
        patches[0].level = 0.0;
        let main = [Destination::Main; SLOT_COUNT];
        engine.reset_output_destinations(&main);
        engine.prepare(&patches);
        let mut trigger = TriggerGroup::new();
        trigger.push(0, 0.82);
        engine.trigger_group(&patches, trigger);
        for _ in 0..64 {
            let _ = engine.process_routed(&patches, ModulationPatch::default(), &main);
        }
        assert!(
            engine.is_active(),
            "the muted circuit must still be running"
        );

        // One sample carries the output edit, the unmute and a retrigger, in the plugin's order.
        let mut destinations = main;
        destinations[0] = Destination::Auxiliary(4);
        patches[0].level = 1.0;
        engine.route_outputs(&destinations);
        engine.trigger_group(&patches, trigger);
        let frames = (0..256)
            .map(|_| engine.process_routed(&patches, ModulationPatch::default(), &destinations))
            .collect::<Vec<_>>();
        assert!(frames.iter().all(|frame| frame.main == [0.0; 2]));
        assert!(frames.iter().any(|frame| frame.individual[4] != 0.0));
    }

    #[test]
    fn every_slot_reaches_exactly_its_own_destination_and_nowhere_else() {
        let frames = 256;
        let all = (0..SLOT_COUNT).collect::<Vec<_>>();
        let routed = render_tagged(&all, &[(0, permutation(3))], frames);
        assert!(routed.iter().all(|frame| frame.main == [0.0; 2]));
        for (slot, destination) in permutation(3).into_iter().enumerate() {
            let Destination::Auxiliary(output) = destination else {
                unreachable!()
            };
            let reference = solo_reference(slot, frames);
            assert!(reference.iter().any(|sample| *sample != 0.0));
            let carried = routed
                .iter()
                .map(|frame| frame.individual[output])
                .collect::<Vec<_>>();
            assert_eq!(carried, reference, "slot {slot} on output {}", output + 1);
        }
    }

    #[test]
    fn reassigning_every_slot_mid_tail_touches_only_the_old_and_new_outputs() {
        let switch = 64;
        let frames = switch + TRANSFER + 64;
        let (before, after) = (permutation(0), permutation(1));
        for slot in 0..SLOT_COUNT {
            let (Destination::Auxiliary(old), Destination::Auxiliary(new)) =
                (before[slot], after[slot])
            else {
                unreachable!()
            };
            let reference = solo_reference(slot, frames);
            let routed = render_tagged(&[slot], &[(0, before), (switch, after)], frames);
            for (n, (frame, &expected)) in routed.iter().zip(&reference).enumerate() {
                assert_eq!(frame.main, [0.0; 2], "slot {slot} leaked to main at {n}");
                for (output, &sample) in frame.individual.iter().enumerate() {
                    if output != old && output != new {
                        assert_eq!(sample, 0.0, "slot {slot} reached output {output} at {n}");
                    }
                }
                let (old, new) = (frame.individual[old], frame.individual[new]);
                if n < switch {
                    assert_eq!((old, new), (expected, 0.0), "slot {slot} at {n}");
                } else if n < switch + TRANSFER {
                    assert!(
                        (old + new - expected).abs() <= 1.0e-6,
                        "slot {slot} partition at {n}"
                    );
                } else {
                    assert_eq!((old, new), (0.0, expected), "slot {slot} retired at {n}");
                }
            }
        }
    }

    #[test]
    fn a_transfer_at_every_sample_phase_is_a_continuous_partition() {
        let reference = solo_reference(0, 3 * TRANSFER + 32);
        let mut destinations = [Destination::Main; SLOT_COUNT];
        destinations[0] = Destination::Auxiliary(0);
        // From sample 1: at sample 0 the struck slot has carried nothing yet, so it moves at once
        // (see `a_silent_slot_takes_its_new_destination_before_the_next_hit`).
        for switch in 1..2 * TRANSFER {
            let routed = render_tagged(
                &[0],
                &[(0, [Destination::Main; SLOT_COUNT]), (switch, destinations)],
                reference.len(),
            );
            let mut previous_gain = 0.0_f32;
            let mut previous_n = 0_usize;
            for (n, (frame, &expected)) in routed.iter().zip(&reference).enumerate() {
                let (main, individual) = (frame.main[0], frame.individual[0]);
                assert_eq!(frame.main[1], 0.0);
                assert!(frame.individual[1..].iter().all(|sample| *sample == 0.0));
                assert!(
                    (main + individual - expected).abs() <= 1.0e-6,
                    "switch {switch}: dropout or duplicate at {n}"
                );
                if n >= switch + TRANSFER {
                    assert_eq!(main, 0.0, "switch {switch}: main not retired at {n}");
                }
                // The individual share's gain may only rise, by at most one transfer step per
                // sample since the last one checked: samples near a zero crossing are skipped.
                if expected.abs() > 1.0e-3 {
                    let gain = individual / expected;
                    let steps = (n - previous_n).max(1) as f32;
                    assert!(
                        gain >= previous_gain - 1.0e-4
                            && gain - previous_gain <= steps / (TRANSFER - 1) as f32 + 1.0e-4,
                        "switch {switch}: gain jumped from {previous_gain} to {gain} at {n}"
                    );
                    previous_gain = gain;
                    previous_n = n;
                }
            }
        }
    }

    #[test]
    fn reversal_and_rapid_reassignment_never_reach_a_third_output() {
        let frames = 4 * TRANSFER;
        let reference = solo_reference(0, frames);
        let to = |output: Option<usize>| {
            let mut destinations = [Destination::Main; SLOT_COUNT];
            if let Some(output) = output {
                destinations[0] = Destination::Auxiliary(output);
            }
            destinations
        };
        // Main → 1, back to main while moving (reversal), then 2 and 3 while still reversing: 2 is
        // superseded before it can start, so only 3 follows the reversal.
        let routed = render_tagged(
            &[0],
            &[
                (0, to(None)),
                (20, to(Some(0))),
                (50, to(None)),
                (55, to(Some(1))),
                (60, to(Some(2))),
            ],
            frames,
        );
        for (n, (frame, &expected)) in routed.iter().zip(&reference).enumerate() {
            let carriers = [frame.main[0], frame.individual[0], frame.individual[2]];
            assert!(carriers.iter().filter(|sample| **sample != 0.0).count() <= 2);
            assert_eq!(frame.individual[1], 0.0, "superseded output sounded at {n}");
            assert!((carriers.iter().sum::<f32>() - expected).abs() <= 1.0e-6);
        }
        let last = routed.last().unwrap();
        assert_eq!(last.main, [0.0; 2]);
        assert_eq!(last.individual[0], 0.0);
        assert_eq!(last.individual[2], *reference.last().unwrap());
    }

    #[test]
    fn same_offset_common_accent_is_independent_of_push_order() {
        let mut patches = one_kick();
        patches[1] = patches[0];
        let render = |reverse: bool| {
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut triggers = TriggerGroup::new();
            if reverse {
                triggers.push(1, 0.9);
                triggers.push(0, 0.2);
            } else {
                triggers.push(0, 0.2);
                triggers.push(1, 0.9);
            }
            engine.trigger_group(&patches, triggers);
            (0..512)
                .map(|_| engine.process(&patches))
                .collect::<Vec<_>>()
        };
        assert_eq!(render(false), render(true));
    }

    #[test]
    fn every_available_circuit_renders_in_the_fixed_slot_engine() {
        for spec in crate::model::AVAILABLE_MODELS.iter() {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = spec.id;
            let mut triggers = TriggerGroup::new();
            triggers.push(0, 0.8);
            let mut engine = Engine::new();
            engine.prepare(&patches);
            engine.trigger_group(&patches, triggers);
            for _ in 0..4_800 {
                let frame = engine.process(&patches);
                assert!(frame[0].is_finite() && frame[1].is_finite());
            }
            let peak = engine.take_slot_peaks()[0];
            assert!(peak > 0.001, "model {:?}: {peak}", spec.id);
        }
    }

    /// The mono slot peak of one isolated reference hit, in dBFS, through the current trim.
    fn reference_peak_db(model: ModelId) -> f32 {
        const FRAMES: usize = 96_000;
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 0.82);
        engine.trigger_group(&patches, triggers);
        for _ in 0..FRAMES {
            let _ = engine.process(&patches);
        }
        20.0 * engine.take_slot_peaks()[0].log10()
    }

    #[test]
    fn every_reference_model_reaches_the_common_peak_plane() {
        for spec in &crate::model::AVAILABLE_MODELS {
            let peak_db = reference_peak_db(spec.id);
            assert!(
                (peak_db + 1.0).abs() < 0.05,
                "model {} ({}) reached {peak_db:.3} dBFS",
                spec.id.raw(),
                spec.label,
            );
        }
    }

    /// The energy of one isolated hit on slot 0 over its first 0.2 s. Energy rather than peak: a
    /// timed-burst clap's first burst is the same whatever the velocity, while the hit's body
    /// follows it.
    fn hit_energy(model: ModelId, velocity: f32, dynamics: f32) -> f32 {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        patches[0].dynamics = dynamics;
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, velocity);
        engine.trigger_group(&patches, triggers);
        (0..9_600)
            .map(|_| {
                let [left, right] = engine.process(&patches);
                left * left + right * right
            })
            .sum()
    }

    /// **Dynamics means one thing on every model family** (`crate::velocity`,
    /// `plans/plan-modulation-standard.md`): positive flattens the velocity curve, so a soft hit
    /// comes up against a full one, and negative steepens it — the same way on every model that
    /// offers the control. Before the standard, four families read it as a level trim that left the
    /// curve alone, and two used mappings of their own. That Dynamics 0 is the old law to the bit is
    /// `crate::velocity`'s test and a before/after render of the whole catalogue (the plan's
    /// revision record); this is the other half.
    ///
    /// Falsified before trusted: with the linear families' old trim restored, every one of them
    /// fails, a soft hit's share of a full one the same at each Dynamics.
    #[test]
    fn dynamics_bends_every_familys_velocity_curve_the_same_way() {
        let mut failures = Vec::new();
        for spec in &crate::model::AVAILABLE_MODELS {
            if !spec.id.capabilities().dynamics {
                continue;
            }
            let share = |dynamics: f32| {
                hit_energy(spec.id, 0.25, dynamics) / hit_energy(spec.id, 1.0, dynamics)
            };
            let (steep, flat, level) = (share(-1.0), share(1.0), share(0.0));
            if !(steep < level && level < flat) {
                failures.push(format!(
                    "{} ({}): soft/full {steep:.3} at −1, {level:.3} at 0, {flat:.3} at +1",
                    spec.id.raw(),
                    spec.label
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{}",
            failures.join(
                "
"
            )
        );
    }

    /// Re-measures the complete trim table after a renderer gain change:
    /// `cargo test -p mxm-drum-machine-dsp --lib -- --ignored print_the_reference_output_trims --nocapture`
    /// prints one `id trim` line per available model, the current trim corrected onto −1 dBFS.
    #[test]
    #[ignore = "a measurement tool, not a check"]
    fn print_the_reference_output_trims() {
        for spec in &crate::model::AVAILABLE_MODELS {
            let peak_db = reference_peak_db(spec.id);
            let trim = reference_output_trim(spec.id) * 10f32.powf((-1.0 - peak_db) / 20.0);
            println!("{} {trim:.6}", spec.id.raw());
        }
    }

    #[test]
    fn zero_deviation_metal_slots_read_one_shared_free_running_bank() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::TWIN_SQUARE_COWBELL;
        patches[0].pan = -1.0;
        patches[1] = patches[0];
        patches[1].pan = 1.0;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 0.8);
        triggers.push(1, 0.8);
        engine.trigger_group(&patches, triggers);
        for _ in 0..512 {
            let [left, right] = engine.process(&patches);
            assert!((left - right).abs() < 1.0e-6);
        }
    }

    #[test]
    fn a_grouped_choke_silences_the_open_hat_without_resetting_the_bank() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::SIX_SQUARE_OPEN_HAT;
        patches[1].model = ModelId::SIX_SQUARE_CLOSED_HAT;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        // The pair is coupled by a choke group now, not by the machine's wiring.
        patches[0].choke_group = 1;
        patches[1].choke_group = 1;
        let mut open = TriggerGroup::new();
        open.push(0, 0.8);
        engine.trigger_group(&patches, open);
        for _ in 0..128 {
            let _ = engine.process(&patches);
        }
        let _ = engine.take_slot_peaks();
        let mut closed = TriggerGroup::new();
        closed.push(1, 0.8);
        engine.trigger_group(&patches, closed);
        // The choke is a bounded fade, so silence is owed by the end of it rather than at once.
        for _ in 0..((48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4) {
            let _ = engine.process(&patches);
        }
        let _ = engine.take_slot_peaks();
        for _ in 0..128 {
            let _ = engine.process(&patches);
        }
        let peaks = engine.take_slot_peaks();
        assert_eq!(peaks[0].to_bits(), 0.0_f32.to_bits());
        // The closed hat still reads its machine's free-running bank, which the choke never touched.
        assert!(peaks[1] > 0.001);
    }

    #[test]
    fn simultaneous_low_toms_read_one_shared_coloured_noise_sample() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::LOW_FALLING_TOM;
        patches[0].pan = -1.0;
        patches[1] = patches[0];
        patches[1].pan = 1.0;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 0.8);
        triggers.push(1, 0.8);
        engine.trigger_group(&patches, triggers);
        for _ in 0..512 {
            let [left, right] = engine.process(&patches);
            assert!((left - right).abs() < 1.0e-6);
        }
    }

    #[test]
    fn simultaneous_snares_read_one_shared_noise_sample() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::TWIN_MODE_SNARE;
        patches[0].pan = -1.0;
        patches[1] = patches[0];
        patches[1].pan = 1.0;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 0.8);
        triggers.push(1, 0.8);
        engine.trigger_group(&patches, triggers);
        for _ in 0..512 {
            let [left, right] = engine.process(&patches);
            assert!((left - right).abs() < 1.0e-6);
        }
    }

    #[test]
    fn route_depths_apply_in_the_declared_target_units() {
        let mut routing = Routing::new();
        routing.present[routing::target::PITCH][routing::source::WHEEL] = true;
        routing.amounts[routing::target::PITCH][routing::source::WHEEL] = 0.5;
        routing.present[routing::target::LEVEL][routing::source::PRESSURE] = true;
        routing.amounts[routing::target::LEVEL][routing::source::PRESSURE] = -0.5;
        routing.compact();
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.publish(
            &routing,
            Sources {
                wheel: 1.0,
                pressure: 1.0,
                ..Sources::default()
            },
        );
        let mut patch = SlotPatch::default();
        apply_routes(&mut patch, &graph, &routing);
        assert_eq!(patch.pitch_semitones, 6.0);
        // The standard amplitude factor: half depth down from full pressure is half the level.
        assert_eq!(patch.level, SlotPatch::default().level * 0.5);
    }

    #[test]
    fn a_slots_routes_do_not_modulate_another_slot() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        patches[1].model = ModelId::DEEP_BRIDGE_KICK;
        let mut routing = Routing::new();
        routing.present[routing::target::LEVEL][routing::source::PRESSURE] = true;
        routing.amounts[routing::target::LEVEL][routing::source::PRESSURE] = -0.5;
        routing.compact();

        let mut engine = Engine::new();
        engine.prepare(&patches);
        engine.set_slot_routing(0, routing);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 0.8);
        triggers.push(1, 0.8);
        engine.trigger_group(&patches, triggers);
        let mut destinations = [Destination::Main; SLOT_COUNT];
        destinations[0] = Destination::Auxiliary(0);
        destinations[1] = Destination::Auxiliary(1);
        let modulation = ModulationPatch {
            pressure: [1.0; SLOT_COUNT],
            ..ModulationPatch::default()
        };

        for _ in 0..512 {
            let frame = engine.process_routed(&patches, modulation, &destinations);
            if frame.individual[1].abs() > 1.0e-6 {
                // Half depth down from full pressure: the standard factor halves slot 0 alone.
                let expected = 0.5;
                assert!((frame.individual[0] / frame.individual[1] - expected).abs() < 1.0e-5);
                return;
            }
        }
        panic!("the paired kick never sounded");
    }

    #[test]
    fn reset_restores_random_modulation_and_audio_deterministically() {
        let patches = one_kick();
        let mut routing = Routing::new();
        routing.present[routing::target::PITCH][routing::source::RANDOM] = true;
        routing.amounts[routing::target::PITCH][routing::source::RANDOM] = 0.25;
        routing.compact();
        let mut engine = Engine::new();
        engine.set_slot_routing(0, routing);
        let render = |engine: &mut Engine| {
            engine.prepare(&patches);
            let mut trigger = TriggerGroup::new();
            trigger.push(0, 0.8);
            engine.trigger_group(&patches, trigger);
            (0..512)
                .map(|_| engine.process_modulated(&patches, ModulationPatch::default()))
                .collect::<Vec<_>>()
        };
        let first = render(&mut engine);
        engine.reset();
        let second = render(&mut engine);
        assert_eq!(first, second);
    }

    #[test]
    fn every_declared_unsupported_axis_is_an_exact_dsp_no_op() {
        let render = |patch: SlotPatch| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0] = patch;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut trigger = TriggerGroup::new();
            trigger.push(0, 0.8);
            engine.trigger_group(&patches, trigger);
            (0..1024)
                .map(|_| {
                    let frame = engine.process(&patches);
                    (frame, engine.is_active())
                })
                .collect::<Vec<_>>()
        };
        for spec in crate::model::AVAILABLE_MODELS.iter() {
            let base = SlotPatch {
                model: spec.id,
                ..SlotPatch::default()
            };
            let expected = render(base);
            let caps = spec.id.capabilities();
            let variants = [
                (
                    !caps.pitch,
                    SlotPatch {
                        pitch_semitones: 11.0,
                        ..base
                    },
                ),
                (
                    !caps.pitch_envelope,
                    SlotPatch {
                        pitch_envelope: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.pitch_decay,
                    SlotPatch {
                        pitch_decay: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.decay,
                    SlotPatch {
                        decay: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.attack,
                    SlotPatch {
                        attack: 0.73,
                        ..base
                    },
                ),
                (!caps.tone, SlotPatch { tone: 0.73, ..base }),
                (!caps.body, SlotPatch { body: 0.73, ..base }),
                (
                    !caps.noise,
                    SlotPatch {
                        noise: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.noise_decay,
                    SlotPatch {
                        noise_decay: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.character,
                    SlotPatch {
                        character: 0.73,
                        ..base
                    },
                ),
                (
                    !caps.dynamics,
                    SlotPatch {
                        dynamics: 0.73,
                        ..base
                    },
                ),
            ];
            for (unsupported, variant) in variants {
                if unsupported {
                    assert_eq!(
                        expected,
                        render(variant),
                        "{} unsupported axis changed audio",
                        spec.label
                    );
                }
            }
        }
    }

    #[test]
    fn every_declared_supported_axis_changes_the_reference_render() {
        let render = |patch: SlotPatch| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0] = patch;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut trigger = TriggerGroup::new();
            trigger.push(0, 0.72);
            engine.trigger_group(&patches, trigger);
            (0..4096)
                .map(|_| engine.process(&patches))
                .collect::<Vec<_>>()
        };
        for spec in crate::model::AVAILABLE_MODELS.iter() {
            let base = SlotPatch {
                model: spec.id,
                ..SlotPatch::default()
            };
            let expected = render(base);
            let caps = spec.id.capabilities();
            let variants = [
                (
                    caps.pitch,
                    "Pitch",
                    SlotPatch {
                        pitch_semitones: 7.0,
                        ..base
                    },
                ),
                (
                    caps.pitch_envelope,
                    "Pitch envelope",
                    SlotPatch {
                        pitch_envelope: 0.73,
                        ..base
                    },
                ),
                (
                    caps.pitch_decay,
                    "Pitch decay",
                    SlotPatch {
                        pitch_envelope: if matches!(spec.id.raw(), 28 | 32 | 47 | 49 | 50 | 54 | 60 | 67..=69 | 74..=77 | 85..=88)
                        {
                            0.73
                        } else {
                            0.0
                        },
                        pitch_decay: 0.73,
                        ..base
                    },
                ),
                (
                    caps.decay,
                    "Decay",
                    SlotPatch {
                        decay: 0.73,
                        ..base
                    },
                ),
                (
                    caps.attack,
                    "Attack",
                    SlotPatch {
                        attack: 0.73,
                        ..base
                    },
                ),
                (caps.tone, "Tone", SlotPatch { tone: 0.73, ..base }),
                (caps.body, "Body", SlotPatch { body: 0.73, ..base }),
                (
                    caps.noise,
                    "Noise",
                    SlotPatch {
                        noise: 0.73,
                        ..base
                    },
                ),
                (
                    caps.noise_decay,
                    "Noise decay",
                    SlotPatch {
                        noise_decay: 0.73,
                        ..base
                    },
                ),
                (
                    caps.character,
                    "Character",
                    SlotPatch {
                        character: 0.73,
                        ..base
                    },
                ),
                (
                    caps.dynamics,
                    "Dynamics",
                    SlotPatch {
                        dynamics: 0.73,
                        ..base
                    },
                ),
            ];
            for (supported, axis, variant) in variants {
                if supported {
                    assert_ne!(
                        expected,
                        render(variant),
                        "{} declared {axis} but audio did not change",
                        spec.label
                    );
                }
            }
        }
    }

    #[test]
    fn panic_clears_every_slot_to_exact_silence() {
        let patches = one_kick();
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 1.0);
        engine.trigger_group(&patches, triggers);
        let _ = engine.process(&patches);
        engine.reset();
        assert_eq!(engine.process(&patches), [0.0, 0.0]);
        assert!(!engine.is_active());
    }

    /// Renders one slot's own contribution. The companion sits at zero level, so it triggers and
    /// joins its machine's buses without adding audio to compare against.
    fn render_isolated(
        subject: (ModelId, f32),
        companion: Option<(ModelId, f32)>,
        samples: usize,
    ) -> Vec<f32> {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = subject.0;
        let mut group = TriggerGroup::new();
        group.push(0, subject.1);
        if let Some((model, velocity)) = companion {
            patches[1].model = model;
            patches[1].level = 0.0;
            group.push(1, velocity);
        }
        let mut engine = Engine::new();
        engine.prepare(&patches);
        engine.trigger_group(&patches, group);
        (0..samples).map(|_| engine.process(&patches)[0]).collect()
    }

    #[test]
    fn a_hit_is_its_own_velocity_whatever_else_is_struck() {
        // This instrument has no accent. The hardware's common and global accent buses are
        // deliberately not modelled (owner, 2026-09-20), so a loud simultaneous hit must leave
        // every other slot bit-identical — inside a machine family as well as across families.
        for (subject, companion) in [
            // Same family, which the 808's common trigger-voltage bus used to couple.
            (ModelId::DEEP_BRIDGE_KICK, ModelId::TWIN_MODE_SNARE),
            // Across families, in both directions.
            (ModelId::DEEP_BRIDGE_KICK, ModelId::RESET_PUNCH_KICK),
            (ModelId::RESET_PUNCH_KICK, ModelId::DEEP_BRIDGE_KICK),
            // Two supporting machines that carried global accent buses.
            (ModelId::new(60), ModelId::new(54)),
        ] {
            let alone = render_isolated((subject, 0.2), None, 2048);
            let beside = render_isolated((subject, 0.2), Some((companion, 1.0)), 2048);
            assert_eq!(
                alone,
                beside,
                "model {} was moved by a loud simultaneous {}",
                subject.raw(),
                companion.raw()
            );
        }
    }

    /// Rings a kick, disturbs the sounding slot, and returns the last sample before the
    /// disturbance followed by the whole transition. Checking one sample is not enough: a fade
    /// short enough to be a cut still renders its first sample at full gain and drops on the next,
    /// so the test has to look at the largest step anywhere in the window.
    fn ring_then(
        disturb: impl FnOnce(&mut Engine, &mut [SlotPatch; SLOT_COUNT]),
    ) -> (f32, Vec<f32>, Engine, [SlotPatch; SLOT_COUNT]) {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut group = TriggerGroup::new();
        group.push(0, 0.9);
        engine.trigger_group(&patches, group);
        // Far enough in that the kick is a smooth ring rather than its attack transient, so one
        // sample of the circuit's own travel is small next to the step a cut would make.
        let mut last = 0.0;
        for _ in 0..600 {
            last = engine.process(&patches)[0];
        }
        assert!(
            last.abs() > 0.0,
            "the kick must still be ringing to be cut off"
        );
        disturb(&mut engine, &mut patches);
        let window = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4;
        let mut trace = vec![last];
        trace.extend((0..window).map(|_| engine.process(&patches)[0]));
        (last, trace, engine, patches)
    }

    /// The largest sample-to-sample move anywhere in the transition, against the level it started
    /// from. A cut shows up as a step of the whole remaining amplitude.
    fn assert_no_step(last: f32, trace: &[f32], what: &str) {
        let worst = mxm_measure::observe::worst_step(trace)
            .expect("a finite transition")
            .1;
        assert!(
            worst < f64::from(last.abs()) * 0.25,
            "{what} stepped by {worst:.6} from a ring of {:.6}; it was cut, not faded",
            last.abs()
        );
    }

    /// The fade is bounded: past its length the slot is exactly silent and reports itself idle.
    fn assert_reaches_silence(engine: &mut Engine, patches: &[SlotPatch; SLOT_COUNT]) {
        let transition = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 2;
        for _ in 0..transition {
            let _ = engine.process(patches);
        }
        assert_eq!(
            engine.process(patches),
            [0.0, 0.0],
            "silence is owed by the bound"
        );
        assert!(!engine.is_active());
    }

    #[test]
    fn replacing_a_sounding_model_retires_it_through_a_bounded_fade() {
        let (last, trace, mut engine, patches) = ring_then(|engine, patches| {
            // A different family, so the incoming model cannot share the outgoing voice state.
            patches[0].model = ModelId::TWIN_MODE_SNARE;
            engine.prepare(patches);
        });
        assert_no_step(last, &trace, "replacing the model");
        assert_reaches_silence(&mut engine, &patches);
    }

    #[test]
    fn a_choke_declicks_rather_than_cutting_the_hit() {
        let (last, trace, mut engine, patches) = ring_then(|engine, _| engine.choke(0));
        assert_no_step(last, &trace, "the choke");
        assert_reaches_silence(&mut engine, &patches);
    }

    #[test]
    fn a_second_choke_during_the_fade_does_not_cut_it() {
        // The first choke starts the fade; the second lands part way through it. Restarting or
        // finishing the retirement there would put back the hard cut the fade replaced.
        let (last, trace, mut engine, patches) = {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = ModelId::DEEP_BRIDGE_KICK;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut group = TriggerGroup::new();
            group.push(0, 0.9);
            engine.trigger_group(&patches, group);
            let mut last = 0.0;
            for _ in 0..600 {
                last = engine.process(&patches)[0];
            }
            engine.choke(0);
            let mut trace = vec![last];
            // A quarter of the way into the fade, choke again.
            let window = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4;
            trace.extend((0..window / 4).map(|_| engine.process(&patches)[0]));
            engine.choke(0);
            trace.extend((0..window).map(|_| engine.process(&patches)[0]));
            (last, trace, engine, patches)
        };
        assert_no_step(last, &trace, "a second choke during the fade");
        assert_reaches_silence(&mut engine, &patches);
    }

    /// Rings a kick, then disturbs it twice: once to start a fade and once part way through it.
    fn ring_then_twice(
        first: impl FnOnce(&mut Engine, &mut [SlotPatch; SLOT_COUNT]),
        second: impl FnOnce(&mut Engine, &mut [SlotPatch; SLOT_COUNT]),
    ) -> (f32, Vec<f32>, Engine, [SlotPatch; SLOT_COUNT]) {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut group = TriggerGroup::new();
        group.push(0, 0.9);
        engine.trigger_group(&patches, group);
        let mut last = 0.0;
        for _ in 0..600 {
            last = engine.process(&patches)[0];
        }
        first(&mut engine, &mut patches);
        let window = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4;
        let mut trace = vec![last];
        trace.extend((0..window / 4).map(|_| engine.process(&patches)[0]));
        second(&mut engine, &mut patches);
        trace.extend((0..window).map(|_| engine.process(&patches)[0]));
        (last, trace, engine, patches)
    }

    #[test]
    fn the_lfos_keep_running_while_no_route_reads_them() {
        // LFO1 to Pan, which sits after the circuit: the slot's own state at any sample is the
        // same whether or not the route was connected earlier, so the only thing that can differ
        // once the route is back is the LFO's phase.
        let mut routing = Routing::new();
        routing.present[routing::target::PAN][routing::source::LFO1] = true;
        routing.amounts[routing::target::PAN][routing::source::LFO1] = 1.0;
        routing.compact();

        // `silent` decides whether the matrix is empty over the first `GAP` samples.
        const GAP: usize = 977;
        let render = |disconnect: bool| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = ModelId::DEEP_BRIDGE_KICK;
            let mut engine = Engine::new();
            engine.set_slot_routing(0, if disconnect { Routing::new() } else { routing });
            engine.prepare(&patches);
            let mut group = TriggerGroup::new();
            group.push(0, 0.9);
            engine.trigger_group(&patches, group);
            for _ in 0..GAP {
                let _ = engine.process(&patches);
            }
            if disconnect {
                engine.set_slot_routing(0, routing);
            }
            (0..512)
                .map(|_| engine.process(&patches))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            render(false),
            render(true),
            "the LFOs stopped while the matrix was empty: restoring a route resumed a stale phase \
             instead of the one {GAP} elapsed samples imply"
        );
    }

    #[test]
    fn a_second_model_change_during_the_fade_leaves_the_outgoing_circuit_alone() {
        // A -> B starts A's fade. B -> C must not reset A: B never sounded, but A still is.
        let (last, trace, mut engine, patches) = ring_then_twice(
            |engine, patches| {
                patches[0].model = ModelId::TWIN_MODE_SNARE;
                engine.prepare(patches);
            },
            |engine, patches| {
                patches[0].model = ModelId::BRIGHT_SHORT_MARACA;
                engine.prepare(patches);
            },
        );
        assert_no_step(last, &trace, "a second model change during the fade");
        assert_reaches_silence(&mut engine, &patches);
    }

    #[test]
    fn a_model_change_during_a_choke_leaves_the_choked_circuit_alone() {
        let (last, trace, mut engine, patches) = ring_then_twice(
            |engine, _| engine.choke(0),
            |engine, patches| {
                patches[0].model = ModelId::TWIN_MODE_SNARE;
                engine.prepare(patches);
            },
        );
        assert_no_step(last, &trace, "a model change during a choke");
        assert_reaches_silence(&mut engine, &patches);
    }

    #[test]
    fn a_reset_during_retirement_leaves_the_selected_model_renderable() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut group = TriggerGroup::new();
        group.push(0, 0.9);
        engine.trigger_group(&patches, group);
        for _ in 0..600 {
            let _ = engine.process(&patches);
        }
        // Replace the sounding kick, so the slot is mid-retirement, then panic.
        patches[0].model = ModelId::TWIN_MODE_SNARE;
        engine.prepare(&patches);
        engine.reset();
        assert!(!engine.is_active(), "panic must leave no ghost");

        // The selected model must now be the one that renders.
        let mut group = TriggerGroup::new();
        group.push(0, 0.9);
        engine.trigger_group(&patches, group);
        let sounded = (0..512).any(|_| engine.process(&patches).iter().any(|s| *s != 0.0));
        assert!(
            sounded,
            "after a reset during retirement the selected model never rendered"
        );
    }

    #[test]
    fn a_completed_fade_keeps_the_peak_the_host_has_not_read() {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut group = TriggerGroup::new();
        group.push(0, 0.9);
        engine.trigger_group(&patches, group);
        // How loud the hit actually got, so the assertion is about the hit rather than about the
        // last near-silent sample of the fade — which is non-zero, and would satisfy `peak > 0`
        // even if the meter had been wiped.
        let mut rung = 0.0_f32;
        for _ in 0..600 {
            rung = rung.max(engine.process(&patches)[0].abs());
        }
        assert!(rung > 0.0, "the kick must have sounded");
        patches[0].model = ModelId::TWIN_MODE_SNARE;
        engine.prepare(&patches);
        // Render past the end of the fade without reading the meters in between.
        let transition = (48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4;
        for _ in 0..transition {
            let _ = engine.process(&patches);
        }
        let peak = engine.take_slot_peaks()[0];
        assert!(
            peak > rung * 0.5,
            "the meter read {peak:.6} after a hit that rang to {rung:.6}; completing the fade \
             discarded the peak before the host read it"
        );
    }

    #[test]
    fn returning_the_metal_bank_to_reference_crosses_rather_than_steps() {
        // Renders a settled private-bank cymbal, then either holds the deviation or returns it to
        // the reference detent. Holding is the control: whatever steps the circuit makes on its own
        // appear in both, so only the bank change is under test.
        let render = |return_to_reference: bool| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = ModelId::THREE_PATH_CYMBAL;
            patches[0].pitch_semitones = 7.0;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut group = TriggerGroup::new();
            group.push(0, 0.9);
            engine.trigger_group(&patches, group);
            for _ in 0..4096 {
                let _ = engine.process(&patches);
            }
            if return_to_reference {
                patches[0].pitch_semitones = 0.0;
            }
            (0..4096)
                .map(|_| engine.process(&patches)[0])
                .collect::<Vec<f32>>()
        };
        let held = render(false);
        let returned = render(true);
        // Both engines share their whole history up to the change, so they are sample-aligned and
        // the only difference is the bank move. A cymbal's own output slews hard every sample, so
        // measuring the worst step inside the window hides the switch entirely — the question is
        // how far the *first* sample after the change departs from the render that did not change.
        // How far the two have parted, over a window. The cymbal filters its bank heavily, so the
        // size of the departure says little; what separates a crossfade from a switch is that the
        // departure *grows* over the transition instead of arriving whole on the first sample.
        let divergence = |range: std::ops::Range<usize>| {
            let count = range.len();
            let power: f64 = returned[range.clone()]
                .iter()
                .zip(&held[range])
                .map(|(a, b)| {
                    let difference = f64::from(*a) - f64::from(*b);
                    difference * difference
                })
                .sum();
            (power / count as f64).sqrt()
        };
        // 5 ms is 240 samples at 48 kHz, so the first 32 sit early in the fade.
        let early = divergence(0..32);
        let settled = divergence(1024..4096);
        assert!(
            settled > 0.0,
            "the two renders never parted; the bank never reached shared"
        );
        assert!(
            early < settled * 0.25,
            "the bank arrived {early:.6} of the way out of {settled:.6} within 32 samples; \
             it was switched, not crossed"
        );
    }

    /// Rings slot 0, then strikes slot 1, and reports whether slot 0 still sounds once the
    /// bounded choke fade has had time to finish. Slot 1 sits at zero level so only slot 0's own
    /// meter is read.
    fn slot_zero_survives(first: ModelId, second: ModelId, groups: (u8, u8)) -> bool {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = first;
        patches[0].choke_group = groups.0;
        patches[1].model = second;
        patches[1].level = 0.0;
        patches[1].choke_group = groups.1;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut first_hit = TriggerGroup::new();
        first_hit.push(0, 0.9);
        engine.trigger_group(&patches, first_hit);
        for _ in 0..256 {
            let _ = engine.process(&patches);
        }
        let mut second_hit = TriggerGroup::new();
        second_hit.push(1, 0.9);
        engine.trigger_group(&patches, second_hit);
        for _ in 0..((48_000.0 * MODEL_RETIRE_SECONDS).ceil() as usize + 4) {
            let _ = engine.process(&patches);
        }
        let _ = engine.take_slot_peaks();
        for _ in 0..256 {
            let _ = engine.process(&patches);
        }
        engine.take_slot_peaks()[0] > 0.0
    }

    #[test]
    fn a_choke_group_silences_the_other_slots_in_it() {
        // The whole point of a group over the machines' hardwired pairs: an 808 open hat closed by
        // a 909 closed hat, which no hardware wiring could do.
        assert!(
            !slot_zero_survives(
                ModelId::SIX_SQUARE_OPEN_HAT,
                ModelId::SIX_BIT_CLOSED_HAT,
                (1, 1)
            ),
            "a shared group must choke across machines"
        );
        // And any two models at all: a long kick closed by a short one.
        assert!(
            !slot_zero_survives(ModelId::DEEP_BRIDGE_KICK, ModelId::ECONOMY_62_KICK, (3, 3)),
            "a shared group must choke models that are not hats"
        );
        // Different groups, and no group, never couple.
        assert!(
            slot_zero_survives(
                ModelId::SIX_SQUARE_OPEN_HAT,
                ModelId::SIX_BIT_CLOSED_HAT,
                (1, 2)
            ),
            "different groups must not choke each other"
        );
        assert!(
            slot_zero_survives(
                ModelId::SIX_SQUARE_OPEN_HAT,
                ModelId::SIX_BIT_CLOSED_HAT,
                (0, 0)
            ),
            "ungrouped slots must never choke: nothing chokes unless it is assigned"
        );
        // The machines' own hat pairs are no longer wired together behind the user's back.
        assert!(
            slot_zero_survives(
                ModelId::SIX_SQUARE_OPEN_HAT,
                ModelId::SIX_SQUARE_CLOSED_HAT,
                (0, 0)
            ),
            "the hardwired 808 hat pair must not choke without a group"
        );
    }

    #[test]
    fn slots_struck_together_in_one_group_do_not_choke_each_other() {
        // Struck together, the slots are all starting rather than cutting one another, so sharing
        // a group must change nothing. The test only bites while they are already ringing: a
        // retrigger adds to whatever state is live, and a choke would reset that state first, so
        // grouping would otherwise quietly turn a retrigger into a restart.
        let render = |group: u8| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = ModelId::SIX_SQUARE_OPEN_HAT;
            patches[0].choke_group = group;
            patches[1].model = ModelId::DEEP_BRIDGE_KICK;
            patches[1].choke_group = group;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            let mut both = TriggerGroup::new();
            both.push(0, 0.9);
            both.push(1, 0.9);
            engine.trigger_group(&patches, both);
            // Let them ring, so the second strike lands on live state.
            for _ in 0..512 {
                let _ = engine.process(&patches);
            }
            engine.trigger_group(&patches, both);
            (0..2048)
                .map(|_| engine.process(&patches))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            render(1),
            render(0),
            "sharing a group changed a simultaneous retrigger; struck slots must be exempt"
        );
    }
}

#[cfg(test)]
mod bus_gating {
    use super::*;

    /// A kit with one model in slot 0.
    fn kit(model: ModelId) -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        patches
    }

    /// Renders `frames` and returns the slot-0 mono output.
    fn render(engine: &mut Engine, patches: &[SlotPatch; SLOT_COUNT], frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|_| {
                let f = engine.process(patches);
                0.5 * (f[0] + f[1])
            })
            .collect()
    }

    #[test]
    fn a_gated_bus_resumes_exactly_where_an_ungated_one_is() {
        // The promise of the whole optimisation, end to end through the engine. One kit leaves a
        // noise-reading model loaded the whole time, so its bus never gates. The other loads a
        // model that reads nothing of the kind, gates the bus, then switches to the same model —
        // and from that point the two must agree sample for sample, because a bus that resumes
        // on the wrong sample is a different instrument.
        // A snare reads the white noise only; a low tom reads the pink tail, which is the path
        // with the colour filter in it and therefore the one the bounded replay has to get right.
        for (rate, model) in [
            (44_100.0_f32, ModelId::TWIN_MODE_SNARE),
            (48_000.0, ModelId::LOW_FALLING_TOM),
            (96_000.0, ModelId::LOW_FALLING_TOM),
            (192_000.0, ModelId::LOW_FALLING_TOM),
            // The 909's register, clocked at 300 kHz: six steps a sample at 48, and the one
            // whose catch-up has to count missed clock periods as well as jump the register.
            (48_000.0, ModelId::RESET_TWIN_SNARE),
            (44_100.0, ModelId::RESET_PUNCH_KICK),
            (192_000.0, ModelId::FOUR_CELL_CLAP),
            // A supporting machine, which reads both `legacy_sources` and, for the DR-55 family,
            // its own noise — the last two buses.
            (48_000.0, ModelId::new(28)),
            (48_000.0, ModelId::new(60)),
        ] {
            let reader = kit(model);

            // Never gated: the reader is loaded from the start.
            let mut ungated = Engine::new();
            ungated.set_sample_rate(rate);
            ungated.prepare(&reader);
            let gap = 3_000;
            render(&mut ungated, &reader, gap);

            // Gated: a model that reads no shared noise, for exactly as long.
            let silent = kit(ModelId::LAYERED_SHORT_RIM);
            let mut gated = Engine::new();
            gated.set_sample_rate(rate);
            gated.prepare(&silent);
            render(&mut gated, &silent, gap);
            gated.prepare(&reader);

            let mut triggers = TriggerGroup::new();
            triggers.push(0, 0.82);
            gated.trigger_group(&reader, triggers);
            let mut triggers = TriggerGroup::new();
            triggers.push(0, 0.82);
            ungated.trigger_group(&reader, triggers);

            let a = render(&mut gated, &reader, 2_048);
            let b = render(&mut ungated, &reader, 2_048);
            assert_eq!(a, b, "a gated 808 noise bus resumed wrong at {rate} Hz");
        }
    }

    #[test]
    fn the_colour_filter_forgets_within_its_window() {
        // The bounded reconstruction the catch-up rests on: after the window, a wrong starting
        // value has been multiplied below what an `f64` holds beside the running one, so the
        // replay is exact rather than close. If this ever fails the window is too short.
        for rate in [44_100.0_f32, 48_000.0, 96_000.0, 192_000.0] {
            let mut engine = Engine::new();
            engine.set_sample_rate(rate);
            let window = engine.colour_forgetting_window();

            let mut truth = Engine::new();
            truth.set_sample_rate(rate);
            for _ in 0..window {
                let _ = truth.step_808_noise();
            }

            let mut wrong = Engine::new();
            wrong.set_sample_rate(rate);
            wrong.noise_808_colour = 1.0;
            for _ in 0..window {
                let _ = wrong.step_808_noise();
            }

            assert_eq!(
                truth.noise_808_colour, wrong.noise_808_colour,
                "the colour filter had not forgotten after {window} samples at {rate} Hz"
            );
        }
    }

    #[test]
    fn a_dormant_bus_costs_nothing_and_a_loaded_one_still_runs() {
        // The gate itself: a kit with no reader must leave the bus untouched, and one with a
        // reader must not gate it at all.
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);

        let silent = kit(ModelId::LAYERED_SHORT_RIM);
        engine.prepare(&silent);
        assert!(!engine.noise_808_has_reader, "a rim reads no shared noise");
        assert!(!engine.metal_has_reader, "nor the metal bank");
        assert!(!engine.legacy_has_reader, "nor a supporting machine");
        render(&mut engine, &silent, 64);
        assert_eq!(engine.noise_808_dormant, 64, "the bus was counted, not run");

        let reader = kit(ModelId::TWIN_MODE_SNARE);
        engine.prepare(&reader);
        assert!(engine.noise_808_has_reader);
        render(&mut engine, &reader, 1);
        assert_eq!(
            engine.noise_808_dormant, 0,
            "and caught up on the first read"
        );
    }

    #[test]
    fn a_retiring_model_keeps_its_bus_alive() {
        // The conservative half of the predicate. A model swapped out is still rendering while it
        // fades, so gating on the *selected* model alone would cut the source out from under a
        // sounding tail.
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        let reader = kit(ModelId::TWIN_MODE_SNARE);
        engine.prepare(&reader);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, 1.0);
        engine.trigger_group(&reader, triggers);
        render(&mut engine, &reader, 8);

        let silent = kit(ModelId::LAYERED_SHORT_RIM);
        engine.prepare(&silent);
        assert!(
            engine.noise_808_has_reader,
            "the outgoing snare is still fading and still reading"
        );
    }
}

#[cfg(test)]
mod sample_hold {
    use super::*;

    /// Ticks one LFO until `wanted` distinct held values have come out, or the patience runs out.
    fn drawn(lfo: &mut Lfo, rate: f32, wanted: usize) -> Vec<f32> {
        let mut seen = Vec::new();
        let mut last = f32::NAN;
        for _ in 0..(48_000 * 120) {
            let value = lfo.tick(rate, LfoShape::SampleHold, 48_000.0);
            if value != last {
                seen.push(value);
                last = value;
            }
            if seen.len() == wanted {
                break;
            }
        }
        seen
    }

    #[test]
    fn sample_and_hold_actually_moves() {
        // The owner's report of 2026-09-22: routing an LFO set to sample and hold at Decay did
        // nothing, while a sine worked. It was not the routing. The held value was hashed from
        // the wrapped *phase*, whose remainder is a small float, and `hash_bipolar` keeps the
        // exponent and top mantissa bits — the ones that barely move. Measured, it emitted 0.0
        // for one cycle and then about -0.509 for ever: a DC offset, not modulation.
        let mut lfo = Lfo::new(LFO_SEEDS[0]);
        let seen = drawn(&mut lfo, 2.0, 16);
        assert_eq!(seen.len(), 16, "sample and hold stopped producing values");

        let low = seen.iter().copied().fold(f32::MAX, f32::min);
        let high = seen.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            high - low > 1.0,
            "sample and hold spans only {:.4}, which is a DC offset rather than modulation: {seen:?}",
            high - low
        );
        assert!(
            seen.iter().all(|v| (-1.0..=1.0).contains(v)),
            "a value left the bipolar range: {seen:?}"
        );
    }

    #[test]
    fn sample_and_hold_holds_a_value_from_the_first_sample() {
        // It used to sit at exactly zero until the first cycle wrapped — at a slow synced
        // division that is many seconds of a route doing nothing at all.
        let mut lfo = Lfo::new(LFO_SEEDS[0]);
        let first = lfo.tick(0.25, LfoShape::SampleHold, 48_000.0);
        assert!(
            first != 0.0,
            "sample and hold began at exact zero, so a slow LFO modulates nothing until it wraps"
        );
    }

    #[test]
    fn the_three_lfos_do_not_share_one_sequence() {
        // Three LFOs built from one seed are one LFO with three names.
        let mut first = Lfo::new(LFO_SEEDS[0]);
        let mut second = Lfo::new(LFO_SEEDS[1]);
        let mut third = Lfo::new(LFO_SEEDS[2]);
        let a = drawn(&mut first, 4.0, 8);
        let b = drawn(&mut second, 4.0, 8);
        let c = drawn(&mut third, 4.0, 8);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn a_reset_restores_the_same_sequence() {
        // `NOTES.md` (*Deterministic events and randomness*; the rule is in `AGENTS.md`): all
        // randomness uses explicit fixed seeds, and a reset restores deterministic startup.
        let mut lfo = Lfo::new(LFO_SEEDS[2]);
        let before = drawn(&mut lfo, 4.0, 8);
        lfo.reset();
        let after = drawn(&mut lfo, 4.0, 8);
        assert_eq!(before, after);
    }
}
