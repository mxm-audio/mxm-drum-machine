//! Rendering a kit to one-shots, and playing those one-shots back (plan §4.7, D9).
//!
//! Resample freezes each slot into a buffer and plays the buffer instead of the circuit. This
//! module owns both halves and nothing else: it renders, it reads back, and it never touches a
//! file, a parameter or a host. Writing a sample pack is the plugin's, because
//! [`../AGENTS.md`](../AGENTS.md) keeps `mxm-audio-file` out of the shipped graph.
//!
//! # One pass, not sixteen
//!
//! [`capture_kit`] strikes every slot at the same instant and reads each one from its own
//! auxiliary destination, so all sixteen buffers come from **one** engine and **one** bus history.
//! That costs a single pass over the shared sources rather than sixteen, and it means two slots
//! struck together draw the noise the hardware would have given them. Two slots struck *apart* do
//! not, because a buffer rendered against bus samples 0…N is replayed at a position it never saw:
//! the plan names that as a bounded loss rather than pretending otherwise.
//!
//! # The capture context is fixed, so a capture is a pure function
//!
//! A slot's sound depends on more than its patch. [`CAPTURE_VELOCITY`] fixes the strike, the mix
//! controls are neutralised (see [`capture_patch`]), routing is left off — every route that
//! survives a freeze is applied *after* the capture, so none belongs inside it — and the engine
//! starts from its deterministic reset origin. What remains is the patch and the sample rate,
//! which is exactly what invalidation watches.
//!
//! # What the reader does and does not do
//!
//! Playback applies the sample-domain Pitch and Decay of plan §4.7: pitch is a playback rate, so
//! pitch and duration move together as on any sampler, and decay only ever shortens **the capture
//! it is given** — there is no material past the end of a buffer.
//!
//! Which is why Decay is *baked into* the capture and Pitch is not. Pitch reads correctly in both
//! directions from one reference capture; Decay does not, because lengthening is the direction a
//! sampler cannot do. Capturing at the reference made every Decay above it unreachable — frozen
//! and in an exported pack — which is the owner's report of 2026-09-21. The capture is therefore
//! taken at the Decay that was set, remembers it (`SlotCapture::decay`), and the axis shortens
//! relative to that. Asking for more tail re-renders instead, which is why Decay is part of the
//! plugin's cache key rather than only a playback control.

use crate::SLOT_COUNT;
use crate::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_part_routing::Destination;

/// The strike every capture is taken at.
///
/// The same value the reference bank and the editor's sound trace already render at, so a capture
/// and the catalogue's own listening renders describe the same hit. Velocity below this scales the
/// buffer's gain rather than re-entering the circuit, which the plan lists as a named loss.
pub const CAPTURE_VELOCITY: f32 = 0.82;

/// The tempo every capture is rendered at.
///
/// Fixed rather than taken from the host so that a capture stays a function of the patch alone.
/// One model reads tempo — its open hat lengthens as the tempo falls — and keying the cache on
/// tempo would re-render the whole kit whenever a project's tempo moved. A frozen kit therefore
/// has that model's decay at this tempo, which the plan accepts and records.
pub const CAPTURE_TEMPO_BPM: f32 = 120.0;

/// The longest capture, in seconds, before truncation.
///
/// The catalogue's tails are long: legacy voices carry a sixty-second emergency bound and the
/// deep bridge kick can run to thirty. Capturing to those bounds would cost far more memory than
/// the sound is worth, so a capture that has not gone quiet by here is faded out and **reported**
/// as truncated. Reaching the cap is not a failure — those models must still be usable frozen.
pub const MAX_CAPTURE_SECONDS: f32 = 8.0;

/// Below this a captured sample counts as silence when trimming a tail.
///
/// The engine reaches *exact* zero at idle, so this only has to survive the last denormal-flushed
/// step rather than guess where a decay became inaudible.
const SILENCE: f32 = 1.0e-9;

/// The fade applied to a capture that reached [`MAX_CAPTURE_SECONDS`], in seconds.
///
/// Long enough that a truncated tail does not click, short enough not to audibly shorten a decay
/// that was nearly over anyway.
pub const TRUNCATION_FADE_SECONDS: f32 = 0.010;

/// Why a kit could not be captured.
///
/// Reaching the length cap is deliberately absent: that is truncation, which is reported on the
/// capture itself rather than failing the kit.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureError {
    /// A buffer could not be reserved. The kit keeps whatever it had.
    Allocation { slot: usize, frames: usize },
    /// A model produced a non-finite sample, which must never reach a buffer or a file.
    NonFinite { slot: usize, frame: usize },
    /// The sample rate was not a usable audio rate.
    SampleRate(f32),
    /// The caller asked to stop before the kit was finished.
    ///
    /// A capture is long next to anything else a plugin does — sixteen models rendered to their
    /// tails is a good fraction of a second — and the thread running it is joined when the
    /// plugin goes away. Without a way to stop, teardown waits for a render nobody will hear.
    Cancelled,
}

/// One slot's frozen one-shot: immutable once built.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotCapture {
    samples: Vec<f32>,
    sample_rate: f32,
    truncated: bool,
    peak: f32,
    decay: f32,
    follows_pitch: bool,
}

impl SlotCapture {
    /// The captured audio, mono, at [`Self::sample_rate`].
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The rate the capture was rendered at. A kit captured at another rate is stale.
    #[must_use]
    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Whether the model was still sounding when [`MAX_CAPTURE_SECONDS`] ran out.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// The largest absolute sample in the capture.
    #[must_use]
    pub const fn peak(&self) -> f32 {
        self.peak
    }

    /// The Decay the capture was rendered at, which the playback envelope shortens *from*.
    ///
    /// Stored because Decay is baked in rather than neutralised: the sample is the drum that was
    /// heard, long tail and all, and the axis then deviates from that point instead of from the
    /// model's reference. Without it a reader would shorten a capture that is already short.
    #[must_use]
    pub const fn decay(&self) -> f32 {
        self.decay
    }

    /// The playback rate, in semitones, a frozen hit is read at for the slot's `pitch_semitones`.
    ///
    /// **Only a model with Tune follows it** (2026-10-07). Live, a model without one
    /// (`Capabilities::pitch`) reads none of Tune, a chromatic key, bend or tuning, so its frozen
    /// hit reads none of them either: a value with no knob to see it once moved a frozen hat. The
    /// capture remembers its model's answer, because a Model edit while frozen renders nothing.
    #[must_use]
    pub const fn playback_semitones(&self, pitch_semitones: f32) -> f32 {
        if self.follows_pitch {
            pitch_semitones
        } else {
            0.0
        }
    }

    /// The capture's length in frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the slot captured nothing, which is what an `Off` slot does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Sixteen one-shots rendered together from one bus history.
#[derive(Debug, Clone, PartialEq)]
pub struct KitCapture {
    slots: [SlotCapture; SLOT_COUNT],
    sample_rate: f32,
    /// Which request this kit answers — a number the owner stamps and compares, opaque here, and
    /// zero until stamped. The plugin numbers its engages, so a kit made for one it has since
    /// left is never installed.
    request: u32,
}

impl KitCapture {
    /// One slot's capture.
    #[must_use]
    pub fn slot(&self, slot: usize) -> &SlotCapture {
        &self.slots[slot]
    }

    /// The rate every slot in this kit was rendered at.
    #[must_use]
    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Whether any slot ran out of capture length.
    #[must_use]
    pub fn any_truncated(&self) -> bool {
        self.slots.iter().any(SlotCapture::truncated)
    }

    /// The request this kit answers, as stamped by [`Self::answering`]; zero if never stamped.
    #[must_use]
    pub const fn request(&self) -> u32 {
        self.request
    }

    /// This kit, stamped as the answer to `request`.
    #[must_use]
    pub fn answering(mut self, request: u32) -> Self {
        self.request = request;
        self
    }
}

/// The patch a slot is captured with: its own sound, none of the mix around it.
///
/// The auxiliary tap the capture reads sits **after** Level, so leaving it live would bake a fader
/// position into a buffer whose level the plan promises stays live. Mute and Solo reach the engine
/// as Level zero rather than as fields of their own, so neutralising Level neutralises all three
/// and a muted slot still captures. Pan is bypassed on the individual outputs already, and Master
/// is the plugin's and never reaches the engine at all.
///
/// **Pitch is captured at its reference detent; Decay is not.** A frozen slot reads Pitch as a
/// playback rate, which works in both directions from one capture, so baking it in would only add
/// a resample. Decay has no such symmetry — a sampler cannot lengthen — so a capture taken at the
/// reference put every longer Decay out of reach, frozen and in an exported pack alike (owner,
/// 2026-09-21). It is baked in instead, and `SlotCapture::decay` carries the baseline the reader
/// subtracts. Nothing has to survive a save for that: a capture is a cache re-derived from the
/// parameters, so the baseline is rebuilt with it.
#[must_use]
pub fn capture_patch(patch: SlotPatch) -> SlotPatch {
    SlotPatch {
        pitch_semitones: 0.0,
        level: 1.0,
        pan: 0.0,
        ..patch
    }
}

/// Renders every slot of `patches` to its own one-shot.
///
/// **Never call this from an audio thread**: it allocates and renders seconds of audio. The engine
/// it builds is its own, so the live one is untouched and nothing is read across a thread.
///
/// # Errors
///
/// [`CaptureError`] when the rate is unusable, a buffer cannot be reserved, or a model produces a
/// non-finite sample. A failure leaves the caller's existing kit alone — nothing partial escapes.
pub fn capture_kit(
    patches: &[SlotPatch; SLOT_COUNT],
    sample_rate: f32,
) -> Result<KitCapture, CaptureError> {
    capture_kit_until(patches, sample_rate, &|| false)
}

/// How often `capture_kit_until` asks whether to stop.
///
/// Small enough that teardown is not held up perceptibly, large enough that the check costs
/// nothing against the rendering between them.
const CANCEL_CHECK_FRAMES: usize = 1_024;

/// Renders every slot, stopping early if `cancelled` says to.
///
/// The cancellable form of [`capture_kit`]. **Use it wherever the render outlives the caller's
/// certainty that the result is still wanted** — which on a background thread is always, because
/// the plugin can be destroyed while the render is in flight and the thread is joined on the way
/// out.
///
/// # Errors
///
/// As [`capture_kit`], plus [`CaptureError::Cancelled`].
pub fn capture_kit_until(
    patches: &[SlotPatch; SLOT_COUNT],
    sample_rate: f32,
    cancelled: &dyn Fn() -> bool,
) -> Result<KitCapture, CaptureError> {
    let mut progress = KitCaptureInProgress::begin(patches, sample_rate)?;
    loop {
        if cancelled() {
            return Err(CaptureError::Cancelled);
        }
        if progress.render(CANCEL_CHECK_FRAMES)? {
            return Ok(progress.finish());
        }
    }
}

/// A kit capture that can be rendered a piece at a time.
///
/// **A whole kit is far too long for one background task.** Measured on the development machine,
/// sixteen models to their tails is about 190 ms at 48 kHz and 750 ms at 192 — and the worker
/// thread is joined when the plugin goes away, so a task that long can be in flight when a host
/// tears down. The validator found exactly that, as a crash rather than a stall.
///
/// So the render is resumable: the caller asks for a bounded number of frames and comes back for
/// the rest. Nothing is published until [`Self::finish`], so a half-rendered kit is never
/// audible, and abandoning one costs only the memory it had reserved.
///
/// **The engine is boxed, so this value stays small.** An `Engine` is about 100 KB, and the plugin
/// captures on the host's main thread when it activates with Resample on. By value, `begin` and
/// `capture_kit_until` each held copies of it, and a debug build overflowed the 1 MB thread
/// `clap-validator`'s parameter fuzzing activates on. Boxing roughly halved what a capture needs
/// in both profiles; `a_capture_fits_a_one_megabyte_stack` holds the bound.
pub struct KitCaptureInProgress {
    engine: Box<Engine>,
    patches: [SlotPatch; SLOT_COUNT],
    destinations: [Destination; SLOT_COUNT],
    buffers: [Vec<f32>; SLOT_COUNT],
    rendered: usize,
    cap: usize,
    finished: bool,
    sample_rate: f32,
}

impl KitCaptureInProgress {
    /// Prepares the render and strikes every loaded slot.
    ///
    /// All the allocation happens here, so a chunk never grows a buffer part-way.
    ///
    /// # Errors
    ///
    /// [`CaptureError::SampleRate`] or [`CaptureError::Allocation`].
    pub fn begin(
        patches: &[SlotPatch; SLOT_COUNT],
        sample_rate: f32,
    ) -> Result<Self, CaptureError> {
        if !sample_rate.is_finite() || sample_rate < crate::resonator::MIN_SAMPLE_RATE {
            return Err(CaptureError::SampleRate(sample_rate));
        }

        let capture_patches: [SlotPatch; SLOT_COUNT] =
            core::array::from_fn(|i| capture_patch(patches[i]));
        let destinations: [Destination; SLOT_COUNT] = core::array::from_fn(Destination::Auxiliary);

        let mut engine = Box::new(Engine::new());
        engine.set_sample_rate(sample_rate);
        engine.set_tempo_bpm(CAPTURE_TEMPO_BPM);
        engine.prepare(&capture_patches);
        // Installed rather than transitioned: `route_outputs` crossfades between destinations,
        // which would fade the first milliseconds of every capture.
        engine.reset_output_destinations(&destinations);

        let cap = (sample_rate * MAX_CAPTURE_SECONDS) as usize;
        let mut buffers: [Vec<f32>; SLOT_COUNT] = core::array::from_fn(|_| Vec::new());
        for (slot, buffer) in buffers.iter_mut().enumerate() {
            if capture_patches[slot].model == crate::model::ModelId::OFF {
                continue;
            }
            // Reserved up front so a render cannot grow a buffer mid-pass, and fallibly because
            // an infallible allocation that fails aborts the host.
            buffer
                .try_reserve_exact(cap)
                .map_err(|_| CaptureError::Allocation { slot, frames: cap })?;
        }

        let mut triggers = TriggerGroup::new();
        for (slot, patch) in capture_patches.iter().enumerate() {
            if patch.model != crate::model::ModelId::OFF {
                triggers.push(slot, CAPTURE_VELOCITY);
            }
        }
        engine.trigger_group(&capture_patches, triggers);

        Ok(Self {
            engine,
            patches: capture_patches,
            destinations,
            buffers,
            rendered: 0,
            cap,
            finished: false,
            sample_rate,
        })
    }

    /// Renders at most `frames` more, and says whether the kit is complete.
    ///
    /// # Errors
    ///
    /// [`CaptureError::NonFinite`], which must never reach a buffer because it would reach a file
    /// next.
    pub fn render(&mut self, frames: usize) -> Result<bool, CaptureError> {
        let until = (self.rendered + frames).min(self.cap);
        while self.rendered < until {
            let frame =
                self.engine
                    .process_routed(&self.patches, Default::default(), &self.destinations);
            for (slot, buffer) in self.buffers.iter_mut().enumerate() {
                if buffer.capacity() == 0 {
                    continue;
                }
                let sample = frame.individual[slot];
                if !sample.is_finite() {
                    return Err(CaptureError::NonFinite {
                        slot,
                        frame: self.rendered,
                    });
                }
                buffer.push(sample);
            }
            self.rendered += 1;
            if !self.engine.is_active() {
                self.finished = true;
                return Ok(true);
            }
        }
        if self.rendered >= self.cap {
            self.finished = true;
            return Ok(true);
        }
        Ok(false)
    }

    /// Trims, fades and hands over the finished kit.
    #[must_use]
    pub fn finish(mut self) -> KitCapture {
        // Only a kit that filled the cap was still sounding when it ran out.
        let truncated = self.rendered >= self.cap;
        let fade = ((self.sample_rate * TRUNCATION_FADE_SECONDS) as usize).max(1);
        let sample_rate = self.sample_rate;
        let cap = self.cap;
        // Copied out before the closure borrows `self` mutably for its buffers.
        let decays: [f32; SLOT_COUNT] = core::array::from_fn(|slot| self.patches[slot].decay);
        let follows_pitch: [bool; SLOT_COUNT] =
            core::array::from_fn(|slot| self.patches[slot].model.capabilities().pitch);
        let slots = core::array::from_fn(|slot| {
            let mut samples = std::mem::take(&mut self.buffers[slot]);
            // The pass runs until the *last* slot goes quiet, so a short rim would otherwise
            // carry seconds of silence waiting for a cymbal. Each buffer is cut back to its own
            // tail, which is what makes a slot's capture independent of what it was captured
            // beside — and what keeps an exported one-shot from being mostly padding.
            let sounding = samples
                .iter()
                .rposition(|s| s.abs() > SILENCE)
                .map_or(0, |last| last + 1);
            samples.truncate(sounding);
            let cut = samples.len() == cap && truncated;
            if cut {
                fade_out(&mut samples, fade);
            }
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            SlotCapture {
                samples,
                sample_rate,
                truncated: cut,
                peak,
                decay: decays[slot],
                follows_pitch: follows_pitch[slot],
            }
        });
        KitCapture {
            slots,
            sample_rate,
            request: 0,
        }
    }
}

/// Ramps the last `fade` frames to zero, so a truncated tail does not click.
fn fade_out(samples: &mut [f32], fade: usize) {
    let len = samples.len();
    let fade = fade.min(len);
    if fade == 0 {
        return;
    }
    let start = len - fade;
    for (offset, sample) in samples[start..].iter_mut().enumerate() {
        let gain = 1.0 - (offset as f32 + 1.0) / fade as f32;
        *sample *= gain;
    }
}

/// How many hits one frozen slot can have sounding at once.
///
/// A reader is a position and an envelope over the one immutable buffer, not a copy, so overlap
/// costs reader state and no memory. A trigger with none free steals the oldest, which bounds the
/// count without allocating — the plan's requirement that the playback model be bounded before a
/// family's restart-or-overlap choice is made.
pub const READERS_PER_SLOT: usize = 4;

/// Half the interpolator's width, in source frames.
const SINC_TAPS: isize = 8;

/// How a slot answers a second hit while the first is still sounding.
///
/// The choice is per family and belongs in the catalogue ledger, because it is a claim about a
/// circuit: a reset-VCO or PCM voice restarts because the hardware hard-resets it, while a
/// resonator rings on and a second strike adds to it. Neither reproduces the circuit exactly,
/// which is why retrigger stays in the plan's table of named losses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Retrigger {
    /// One reader, re-armed from the start. What a hard-resetting circuit does.
    #[default]
    Restart,
    /// A new reader beside the old, so the tails sum. What a ringing resonator does.
    Overlap,
}

/// One sounding hit: a position in the buffer and the envelope over it.
#[derive(Debug, Clone, Copy, Default)]
struct Reader {
    position: f64,
    gain: f32,
    /// Per-frame multiplier for the shortening envelope; 1.0 leaves the captured decay alone.
    decay_step: f32,
    envelope: f32,
    age: u64,
    active: bool,
}

/// Plays one slot's capture, with the sample-domain Pitch and Decay of plan §4.7.
#[derive(Debug, Clone)]
pub struct CaptureVoice {
    readers: [Reader; READERS_PER_SLOT],
    struck: u64,
}

impl Default for CaptureVoice {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureVoice {
    /// A voice with nothing sounding.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            readers: [Reader {
                position: 0.0,
                gain: 0.0,
                decay_step: 1.0,
                envelope: 1.0,
                age: 0,
                active: false,
            }; READERS_PER_SLOT],
            struck: 0,
        }
    }

    /// Starts a hit.
    ///
    /// `velocity` scales the buffer's gain, because the capture was taken at one strike and the
    /// circuit's own velocity law is baked into it — the plan's named loss. The engine passes it on
    /// the Dynamics curve every family shares (`crate::velocity::curve`). `decay` is the slot's
    /// reference-centred axis: negative shortens, and positive does nothing, because there is no
    /// material past the end of a capture. A longer tail comes from pitching down.
    pub fn trigger(
        &mut self,
        capture: &SlotCapture,
        velocity: f32,
        decay: f32,
        retrigger: Retrigger,
        sample_rate: f32,
    ) {
        if capture.is_empty() {
            return;
        }
        self.struck = self.struck.wrapping_add(1);
        let index = match retrigger {
            Retrigger::Restart => {
                for reader in &mut self.readers {
                    reader.active = false;
                }
                0
            }
            Retrigger::Overlap => self
                .readers
                .iter()
                .position(|r| !r.active)
                .unwrap_or_else(|| self.oldest()),
        };
        self.readers[index] = Reader {
            position: 0.0,
            gain: velocity.clamp(0.0, 1.0),
            decay_step: decay_step(decay, capture, sample_rate),
            envelope: 1.0,
            age: self.struck,
            active: true,
        };
    }

    /// Ends every hit at once, for a choke or a panic.
    ///
    /// Immediate rather than faded: the caller owns the de-click, exactly as it does for a live
    /// circuit, so this does not invent a second fade law beside the engine's.
    pub fn choke(&mut self) {
        for reader in &mut self.readers {
            reader.active = false;
        }
    }

    /// Clears every reader and the strike counter.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Whether any hit is still sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.readers.iter().any(|r| r.active)
    }

    /// Renders one frame, at `pitch_semitones` of playback rate when the capture's model has Tune
    /// ([`SlotCapture::playback_semitones`]).
    ///
    /// Pitch is a clock: rate and duration move together, as on any sampler and as §3.4's
    /// PCM-metal row already describes for the one family that was always a clocked capture.
    #[must_use]
    pub fn process(&mut self, capture: &SlotCapture, pitch_semitones: f32) -> f32 {
        if capture.is_empty() {
            return 0.0;
        }
        let pitch_semitones = capture.playback_semitones(pitch_semitones);
        let rate = f64::from(2.0_f32.powf(pitch_semitones / 12.0));
        let samples = capture.samples();
        let mut sum = 0.0_f32;
        for reader in &mut self.readers {
            if !reader.active {
                continue;
            }
            let value = read(samples, reader.position, rate);
            sum += value * reader.gain * reader.envelope;
            reader.envelope *= reader.decay_step;
            reader.position += rate;
            // A reader stops at the end of its material, or when the shortening envelope has
            // taken it below audibility — whichever comes first. Stopping exactly is what keeps
            // a frozen kit's idle as exact as a live one's.
            if reader.position >= samples.len() as f64 || reader.envelope < SILENCE {
                reader.active = false;
            }
        }
        sum
    }

    fn oldest(&self) -> usize {
        self.readers
            .iter()
            .enumerate()
            .min_by_key(|(_, r)| r.age)
            .map_or(0, |(i, _)| i)
    }
}

/// The per-frame envelope multiplier for `decay`, **relative to the value the capture holds**.
///
/// Matching the capture leaves it alone. Below it the hit is shortened, reaching a stated fraction
/// of the captured length at the axis floor. Above it nothing happens *here*, because a capture has
/// no material past its end — the longer tail comes from the capture being re-rendered at the new
/// value, which is why Decay is part of the cache key and not only a playback control.
///
/// The relative reading is the whole of the fix for the owner's report of 2026-09-21: Decay used
/// to be neutralised at capture and read as an absolute offset from the model's reference, so a
/// Decay *longer* than the reference was unreachable both frozen and in an exported pack.
fn decay_step(decay: f32, capture: &SlotCapture, sample_rate: f32) -> f32 {
    let decay = decay - capture.decay;
    if decay >= 0.0 || capture.is_empty() {
        return 1.0;
    }
    // At the floor the hit reaches silence in this fraction of its captured length.
    const SHORTEST: f32 = 0.05;
    let shrink = SHORTEST + (1.0 - SHORTEST) * (1.0 + decay.max(-1.0));
    let frames = (capture.len() as f32 * shrink).max(1.0);
    // Decay to `SILENCE` over `frames`, as a per-frame multiplier.
    let _ = sample_rate;
    (SILENCE.ln() / frames).exp()
}

/// Reads `samples` at a fractional `position`, band-limited for the playback `rate`.
///
/// Pitching up folds content that no longer fits, and mxm-kit's
/// [`docs/oscillators/14-samplers.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/14-samplers.md)
/// §14.5 measures
/// why a better interpolator alone is the wrong half of that problem: the cutoff has to come down
/// with the rate. So the kernel is a windowed sinc whose cutoff narrows above unity, and at unity
/// the whole thing short-circuits to a plain fetch — the common case must not pay for the rare
/// one.
fn read(samples: &[f32], position: f64, rate: f64) -> f32 {
    let len = samples.len() as f64;
    if position < 0.0 || position >= len {
        return 0.0;
    }
    let index = position.floor() as isize;
    let fraction = position - position.floor();
    // Unity rate on an integer position is the overwhelmingly common case: a kit played at
    // reference pitch is a straight copy, and paying sixteen taps for it would make the frozen
    // path slower than it needs to be for no audible gain.
    if rate == 1.0 && fraction == 0.0 {
        return samples[index as usize];
    }
    let cutoff = if rate > 1.0 { 1.0 / rate } else { 1.0 };
    let mut sum = 0.0_f64;
    let mut weight = 0.0_f64;
    for tap in -SINC_TAPS + 1..=SINC_TAPS {
        let at = index + tap;
        if at < 0 || at as usize >= samples.len() {
            continue;
        }
        let x = f64::from(tap as i32) - fraction;
        let w = sinc(x * cutoff) * blackman(x);
        sum += f64::from(samples[at as usize]) * w;
        weight += w;
    }
    if weight.abs() < 1.0e-12 {
        return 0.0;
    }
    (sum / weight) as f32
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1.0e-9 {
        1.0
    } else {
        (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
    }
}

/// The Blackman window over the kernel's full width, zero at the ends.
fn blackman(x: f64) -> f64 {
    let half = SINC_TAPS as f64;
    if x.abs() >= half {
        return 0.0;
    }
    let t = (x + half) / (2.0 * half);
    0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos()
        + 0.08 * (4.0 * std::f64::consts::PI * t).cos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AVAILABLE_MODELS, ModelId};

    fn kit(model: ModelId) -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        patches
    }

    #[test]
    fn report_engine_size() {
        println!(
            "size_of::<Engine>() = {} bytes ({:.2} MB)",
            std::mem::size_of::<Engine>(),
            std::mem::size_of::<Engine>() as f64 / 1_048_576.0
        );
    }

    /// The plugin captures inside `activate`, on the host's main thread, and 1 MB is the Windows
    /// default for one — `clap-validator`'s included. A debug capture that kept its engine by value
    /// needed more than that and crashed the validator's parameter fuzzing, so this runs in the
    /// profile that needs the most stack. Overflowing aborts the whole test binary, loudly.
    #[test]
    fn a_capture_fits_a_one_megabyte_stack() {
        // A model from every sixth catalogue entry, so the strike and render paths of several
        // families are on the stack, not only one.
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        for (slot, patch) in patches.iter_mut().enumerate() {
            patch.model = AVAILABLE_MODELS[slot * 6].id;
        }
        let captured = std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(move || capture_kit(&patches, 48_000.0).is_ok())
            .expect("a thread")
            .join()
            .expect("the capture thread did not panic");
        assert!(captured, "the kit captured");
    }

    #[test]
    fn a_capture_is_the_slots_own_render_and_off_slots_capture_nothing() {
        let patches = kit(ModelId::DEEP_BRIDGE_KICK);
        let captured = capture_kit(&patches, 48_000.0).expect("a kit");
        assert!(!captured.slot(0).is_empty(), "the loaded slot captured");
        assert!(captured.slot(0).peak() > 0.0, "and it is audible");
        for slot in 1..SLOT_COUNT {
            assert!(
                captured.slot(slot).is_empty(),
                "slot {slot} is Off and captured nothing"
            );
        }
    }

    #[test]
    fn the_same_patch_captures_the_same_bytes() {
        // The cache rests on this: a capture is a function of the patch and the rate, so the same
        // input must give the same buffer or invalidation could never be trusted.
        let patches = kit(ModelId::DEEP_BRIDGE_KICK);
        let first = capture_kit(&patches, 48_000.0).expect("a kit");
        let second = capture_kit(&patches, 48_000.0).expect("a kit");
        assert_eq!(first, second);
    }

    #[test]
    fn the_mix_is_not_baked_into_a_capture() {
        // Level and Pan act after the capture point, so moving them must not change a single
        // captured sample. Mute and Solo reach the engine as Level zero, so a muted slot is a
        // Level-zero slot and must still capture its sound — which is what this pins.
        let plain = kit(ModelId::DEEP_BRIDGE_KICK);
        let mut mixed = plain;
        // Mute and Solo arrive as Level zero, so silencing the slot is the same test.
        mixed[0].level = 0.0;
        mixed[0].pan = -1.0;

        let a = capture_kit(&plain, 48_000.0).expect("a kit");
        let b = capture_kit(&mixed, 48_000.0).expect("a kit");
        assert_eq!(
            a.slot(0).samples(),
            b.slot(0).samples(),
            "a silenced, panned slot captures identically"
        );
    }

    #[test]
    fn pitch_is_captured_at_its_reference() {
        // Pitch survives a freeze as playback rate, which works in both directions from one
        // capture, so baking it in would only cost a resample on the way out.
        let plain = kit(ModelId::DEEP_BRIDGE_KICK);
        let mut shaped = plain;
        shaped[0].pitch_semitones = 7.0;

        let a = capture_kit(&plain, 48_000.0).expect("a kit");
        let b = capture_kit(&shaped, 48_000.0).expect("a kit");
        assert_eq!(a.slot(0).samples(), b.slot(0).samples());
    }

    #[test]
    fn a_longer_decay_is_baked_into_the_capture() {
        // The owner's report of 2026-09-21: set a long decay, resample, and hear the preset's
        // decay instead. Decay used to be neutralised at capture and read back as an offset from
        // the model's reference, and the reader only ever shortens — so a decay *longer* than the
        // reference was unreachable frozen, and could never reach an exported pack either.
        let plain = kit(ModelId::DEEP_BRIDGE_KICK);
        let mut longer = plain;
        longer[0].decay = 0.8;

        let a = capture_kit(&plain, 48_000.0).expect("a kit");
        let b = capture_kit(&longer, 48_000.0).expect("a kit");
        assert!(
            b.slot(0).samples().len() > a.slot(0).samples().len(),
            "a longer decay must make a longer capture: {} frames against {}",
            b.slot(0).samples().len(),
            a.slot(0).samples().len()
        );
        assert_eq!(b.slot(0).decay(), 0.8, "and the capture remembers it");
    }

    #[test]
    fn the_decay_envelope_is_relative_to_the_capture() {
        // With Decay baked in, an envelope that still read the axis absolutely would shorten a
        // capture that is already exactly the right length. Playing a capture at the decay it was
        // taken at must leave it alone; only asking for less than it holds may shorten it.
        let mut patches = kit(ModelId::DEEP_BRIDGE_KICK);
        patches[0].decay = 0.5;
        let capture = capture_kit(&patches, 48_000.0).expect("a kit");
        let slot = capture.slot(0);

        let length = |decay: f32| {
            let mut voice = CaptureVoice::new();
            voice.trigger(slot, 1.0, decay, Retrigger::Restart, 48_000.0);
            let mut frames = 0;
            while voice.is_active() && frames < slot.samples().len() * 2 {
                let _ = voice.process(slot, 0.0);
                frames += 1;
            }
            frames
        };

        assert_eq!(
            length(0.5),
            slot.samples().len(),
            "at the captured decay the buffer plays whole"
        );
        assert!(
            length(-0.2) < slot.samples().len(),
            "below it the hit is shortened"
        );
        assert_eq!(
            length(0.9),
            slot.samples().len(),
            "above it there is no material to lengthen with"
        );
    }

    #[test]
    fn another_shaping_axis_does_change_the_capture() {
        // The mirror of the two tests above: axes that do *not* survive a freeze must be baked in,
        // or freezing would quietly discard them.
        let plain = kit(ModelId::DEEP_BRIDGE_KICK);
        let mut shaped = plain;
        shaped[0].tone = 0.9;
        let a = capture_kit(&plain, 48_000.0).expect("a kit");
        let b = capture_kit(&shaped, 48_000.0).expect("a kit");
        assert_ne!(a.slot(0).samples(), b.slot(0).samples());
    }

    #[test]
    fn a_capture_equals_the_slots_isolated_live_render() {
        // The claim the whole feature rests on: a capture is a recording of the approved drum,
        // not a re-voicing of it. Rendered here the long way — one engine, one slot, read from
        // its own auxiliary output — and required to match sample for sample.
        let patches = kit(ModelId::DEEP_BRIDGE_KICK);
        let captured = capture_kit(&patches, 48_000.0).expect("a kit");

        let capture_patches: [SlotPatch; SLOT_COUNT] =
            core::array::from_fn(|i| capture_patch(patches[i]));
        let destinations: [Destination; SLOT_COUNT] = core::array::from_fn(Destination::Auxiliary);
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.set_tempo_bpm(CAPTURE_TEMPO_BPM);
        engine.prepare(&capture_patches);
        engine.reset_output_destinations(&destinations);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, CAPTURE_VELOCITY);
        engine.trigger_group(&capture_patches, triggers);

        for (frame, expected) in captured.slot(0).samples().iter().enumerate() {
            let live = engine.process_routed(&capture_patches, Default::default(), &destinations);
            assert_eq!(
                live.individual[0], *expected,
                "frame {frame} differs from the live circuit"
            );
        }
    }

    #[test]
    fn freezing_at_a_long_decay_is_transparent_at_the_same_knob_position() {
        // The owner's question of 2026-09-21: after resampling, is Decay still at the same
        // absolute rotation as the live sound — or does it jump? Nothing writes the parameter, so
        // it stays; what has to be proved is that *at that rotation* the frozen slot and the live
        // circuit are the same sound. Capture bakes the decay in and the envelope subtracts the
        // same number, so the two cancel and the reader plays the buffer untouched.
        const DECAY: f32 = 0.7;
        let mut patches = kit(ModelId::DEEP_BRIDGE_KICK);
        patches[0].decay = DECAY;

        let captured = capture_kit(&patches, 48_000.0).expect("a kit");
        let slot = captured.slot(0);
        assert!(
            !slot.is_empty(),
            "a long decay must capture something to compare"
        );

        // The live circuit at the same knob position, rendered the long way.
        let shaped: [SlotPatch; SLOT_COUNT] = core::array::from_fn(|i| capture_patch(patches[i]));
        let destinations: [Destination; SLOT_COUNT] = core::array::from_fn(Destination::Auxiliary);
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.set_tempo_bpm(CAPTURE_TEMPO_BPM);
        engine.prepare(&shaped);
        engine.reset_output_destinations(&destinations);
        let mut triggers = TriggerGroup::new();
        triggers.push(0, CAPTURE_VELOCITY);
        engine.trigger_group(&shaped, triggers);

        // The frozen slot, read back at the very same Decay the capture was taken at.
        let mut voice = CaptureVoice::new();
        voice.trigger(slot, 1.0, DECAY, Retrigger::Restart, 48_000.0);

        for frame in 0..slot.len() {
            let live = engine.process_routed(&shaped, Default::default(), &destinations);
            let frozen = voice.process(slot, 0.0);
            assert_eq!(
                frozen, live.individual[0],
                "frame {frame}: the frozen slot drifted from the live circuit at the same Decay"
            );
        }
        assert!(
            !voice.is_active(),
            "and it ends with the capture rather than running past it"
        );
    }

    #[test]
    fn sixteen_slots_capture_in_one_pass_with_a_shared_bus_history() {
        // Capturing the kit together is what keeps a simultaneous snare and clap drawing the same
        // noise. Each slot's own stream must still be exactly its own, which is what the engine's
        // per-destination routing already guarantees and what this pins for the capture path.
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        for (slot, patch) in patches.iter_mut().enumerate() {
            patch.model = AVAILABLE_MODELS[slot].id;
        }
        let together = capture_kit(&patches, 48_000.0).expect("a kit");
        for slot in 0..SLOT_COUNT {
            assert!(
                !together.slot(slot).is_empty(),
                "slot {slot} captured something"
            );
        }
        let alone = capture_kit(&kit(AVAILABLE_MODELS[0].id), 48_000.0).expect("a kit");
        assert_eq!(
            together.slot(0).samples(),
            alone.slot(0).samples(),
            "a slot's capture does not depend on what else was struck"
        );
    }

    #[test]
    fn a_hostile_sample_rate_is_refused_rather_than_rendered() {
        let patches = kit(ModelId::DEEP_BRIDGE_KICK);
        // NaN never equals itself, so the shape is what is asserted rather than the value.
        assert!(matches!(
            capture_kit(&patches, f32::NAN),
            Err(CaptureError::SampleRate(rate)) if rate.is_nan()
        ));
        assert_eq!(
            capture_kit(&patches, 0.0),
            Err(CaptureError::SampleRate(0.0))
        );
    }

    #[test]
    fn every_capture_is_finite_and_bounded() {
        // A non-finite sample must never reach a buffer, because it would reach a file next.
        for spec in &AVAILABLE_MODELS {
            let captured = capture_kit(&kit(spec.id), 48_000.0)
                .unwrap_or_else(|e| panic!("model {} ({}): {e:?}", spec.id.raw(), spec.label));
            let slot = captured.slot(0);
            assert!(
                slot.samples().iter().all(|s| s.is_finite()),
                "model {} ({}) captured a non-finite sample",
                spec.id.raw(),
                spec.label
            );
            assert!(
                slot.len() <= (48_000.0 * MAX_CAPTURE_SECONDS) as usize,
                "model {} ({}) exceeded the capture cap",
                spec.id.raw(),
                spec.label
            );
        }
    }

    #[test]
    fn a_truncated_capture_ends_at_silence() {
        // Reaching the cap is allowed; ending on a step is not.
        for spec in &AVAILABLE_MODELS {
            let captured = capture_kit(&kit(spec.id), 48_000.0).expect("a kit");
            let slot = captured.slot(0);
            if slot.truncated() {
                let last = slot.samples().last().copied().unwrap_or(0.0);
                assert!(
                    last.abs() < 1.0e-6,
                    "model {} ({}) was truncated but ends at {last}",
                    spec.id.raw(),
                    spec.label
                );
            }
        }
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use crate::model::ModelId;

    fn one(model: ModelId) -> SlotCapture {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        kit.slot(0).clone()
    }

    fn render(
        voice: &mut CaptureVoice,
        capture: &SlotCapture,
        pitch: f32,
        frames: usize,
    ) -> Vec<f32> {
        (0..frames).map(|_| voice.process(capture, pitch)).collect()
    }

    #[test]
    fn at_reference_the_reader_replays_the_capture_exactly() {
        // Unity rate short-circuits to a plain fetch, so a kit sitting at its reference detents
        // plays back the bytes that were captured and pays nothing for an interpolator.
        let capture = one(ModelId::DEEP_BRIDGE_KICK);
        let mut voice = CaptureVoice::new();
        voice.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        let played = render(&mut voice, &capture, 0.0, capture.len());
        assert_eq!(played.as_slice(), capture.samples());
    }

    #[test]
    fn pitch_moves_rate_and_duration_together() {
        // The sampler law: an octave down is twice as long, an octave up is half. This is what
        // makes "pitch it down for a longer tail" true rather than a slogan.
        let capture = one(ModelId::DEEP_BRIDGE_KICK);
        for (semitones, factor) in [(-12.0_f32, 2.0_f64), (12.0, 0.5)] {
            let mut voice = CaptureVoice::new();
            voice.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
            let mut frames = 0;
            while voice.is_active() && frames < capture.len() * 4 {
                let _ = voice.process(&capture, semitones);
                frames += 1;
            }
            let expected = (capture.len() as f64 * factor).round();
            let error = (frames as f64 - expected).abs() / expected;
            assert!(
                error < 0.01,
                "{semitones:+} semitones played {frames} frames, expected about {expected}"
            );
        }
    }

    #[test]
    fn a_frozen_hit_follows_pitch_only_where_its_model_has_tune() {
        // Live, a model without Tune reads no pitch at all, so its frozen hit plays the capture
        // as it is whatever the slot's pitch holds: Tune, a chromatic key, bend or a route. Sixteen
        // models a kit, so the catalogue is six captures.
        let models: Vec<ModelId> = (1..=94).map(ModelId::new).collect();
        for chunk in models.chunks(SLOT_COUNT) {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            for (patch, &model) in patches.iter_mut().zip(chunk) {
                patch.model = model;
            }
            let kit = capture_kit(&patches, 48_000.0).expect("a kit");
            for (slot, &model) in chunk.iter().enumerate() {
                let capture = kit.slot(slot);
                assert!(!capture.is_empty(), "model {model:?} captured nothing");
                let mut at_reference = CaptureVoice::new();
                let mut pitched = CaptureVoice::new();
                at_reference.trigger(capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
                pitched.trigger(capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
                let frames = capture.len().min(4_800);
                let same = render(&mut at_reference, capture, 0.0, frames)
                    == render(&mut pitched, capture, 7.0, frames);
                assert_eq!(
                    same,
                    !model.capabilities().pitch,
                    "model {model:?}: a frozen hit follows pitch exactly when the model has Tune"
                );
            }
        }
    }

    #[test]
    fn decay_shortens_and_never_lengthens() {
        // The owner's ruling, held as arithmetic: a negative axis ends the hit sooner, and a
        // positive one cannot reach past the end of the buffer, so it changes nothing.
        let capture = one(ModelId::DEEP_BRIDGE_KICK);
        let length = |decay: f32| {
            let mut voice = CaptureVoice::new();
            voice.trigger(&capture, 1.0, decay, Retrigger::Restart, 48_000.0);
            let mut frames = 0;
            while voice.is_active() && frames < capture.len() * 2 {
                let _ = voice.process(&capture, 0.0);
                frames += 1;
            }
            frames
        };
        let reference = length(0.0);
        assert!(length(-0.5) < reference, "a negative axis shortens");
        assert!(
            length(-1.0) < length(-0.5),
            "and shortens further at the floor"
        );
        assert_eq!(length(1.0), reference, "a positive axis cannot lengthen");
    }

    #[test]
    fn velocity_scales_the_buffer() {
        let capture = one(ModelId::DEEP_BRIDGE_KICK);
        let mut loud = CaptureVoice::new();
        let mut quiet = CaptureVoice::new();
        loud.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        quiet.trigger(&capture, 0.5, 0.0, Retrigger::Restart, 48_000.0);
        let a = render(&mut loud, &capture, 0.0, 256);
        let b = render(&mut quiet, &capture, 0.0, 256);
        for (loud, quiet) in a.iter().zip(&b) {
            assert!((loud * 0.5 - quiet).abs() < 1.0e-6);
        }
    }

    #[test]
    fn restart_replaces_and_overlap_sums_within_a_bound() {
        let capture = one(ModelId::DEEP_BRIDGE_KICK);

        let mut restart = CaptureVoice::new();
        restart.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        render(&mut restart, &capture, 0.0, 64);
        restart.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        let after = render(&mut restart, &capture, 0.0, 8);
        assert_eq!(
            after.as_slice(),
            &capture.samples()[..8],
            "a restart plays from the top with nothing left of the first hit"
        );

        let mut overlap = CaptureVoice::new();
        for _ in 0..READERS_PER_SLOT * 2 {
            overlap.trigger(&capture, 1.0, 0.0, Retrigger::Overlap, 48_000.0);
            render(&mut overlap, &capture, 0.0, 8);
        }
        let sounding = overlap.readers.iter().filter(|r| r.active).count();
        assert!(
            sounding <= READERS_PER_SLOT,
            "overlap stays inside its preallocated bound, got {sounding}"
        );
    }

    #[test]
    fn a_choked_voice_goes_silent_and_idle_at_once() {
        let capture = one(ModelId::DEEP_BRIDGE_KICK);
        let mut voice = CaptureVoice::new();
        voice.trigger(&capture, 1.0, 0.0, Retrigger::Overlap, 48_000.0);
        render(&mut voice, &capture, 0.0, 32);
        assert!(voice.is_active());
        voice.choke();
        assert!(!voice.is_active(), "choke ends every reader");
        assert_eq!(
            voice.process(&capture, 0.0),
            0.0,
            "and it renders exact zero"
        );
    }

    #[test]
    fn an_empty_capture_is_silent_rather_than_a_panic() {
        // An Off slot captures nothing, and the reader must survive being asked to play it.
        let empty = SlotCapture {
            samples: Vec::new(),
            sample_rate: 48_000.0,
            truncated: false,
            peak: 0.0,
            decay: 0.0,
            follows_pitch: true,
        };
        let mut voice = CaptureVoice::new();
        voice.trigger(&empty, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        assert!(!voice.is_active());
        assert_eq!(voice.process(&empty, 0.0), 0.0);
    }

    #[test]
    fn pitching_up_stays_band_limited() {
        // Pitching a bright capture up folds whatever no longer fits. The kernel's cutoff comes
        // down with the rate, so energy above Nyquist must not reappear below it: rendered an
        // octave up, a capture's high band must not gain energy it never had.
        let capture = one(ModelId::SIX_SQUARE_CLOSED_HAT);
        let mut voice = CaptureVoice::new();
        voice.trigger(&capture, 1.0, 0.0, Retrigger::Restart, 48_000.0);
        let played = render(&mut voice, &capture, 12.0, capture.len() / 2);
        assert!(
            played.iter().all(|s| s.is_finite()),
            "an interpolated read stays finite"
        );
        let peak = played.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            peak <= capture.peak() * 1.5,
            "interpolation does not blow up the level: {peak} against {}",
            capture.peak()
        );
    }
}

#[cfg(test)]
mod frozen_engine_tests {
    use super::*;
    use crate::model::ModelId;

    fn kit_of(model: ModelId) -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = model;
        patches
    }

    fn strike(engine: &mut Engine, patches: &[SlotPatch; SLOT_COUNT], velocity: f32) {
        let mut triggers = TriggerGroup::new();
        triggers.push(0, velocity);
        engine.trigger_group(patches, triggers);
    }

    fn main_mono(
        engine: &mut Engine,
        patches: &[SlotPatch; SLOT_COUNT],
        frames: usize,
    ) -> Vec<f32> {
        (0..frames)
            .map(|_| {
                let f = engine.process(patches);
                0.5 * (f[0] + f[1])
            })
            .collect()
    }

    #[test]
    fn a_frozen_engine_plays_the_capture_and_a_live_one_plays_the_circuit() {
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let expected: Vec<f32> = captures
            .slot(0)
            .samples()
            .iter()
            .take(512)
            .copied()
            .collect();

        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));
        // Struck at full velocity, because the reader scales the buffer by the strike and the
        // buffer already contains `CAPTURE_VELOCITY`'s hit. Striking at 0.82 would give 0.82 of
        // it, which is the designed velocity law rather than a replay.
        strike(&mut engine, &patches, 1.0);
        let frozen = main_mono(&mut engine, &patches, 512);

        // Level 1.0 and centre Pan. Equal-power panning puts a centred signal at cos(pi/4) in
        // each channel, so the mono sum of the two is 1/sqrt(2) of the capture, not the capture.
        let pan_centre = std::f32::consts::FRAC_1_SQRT_2;
        for (frame, (played, want)) in frozen.iter().zip(&expected).enumerate() {
            let want = want * pan_centre;
            assert!(
                (played - want).abs() < 1.0e-5,
                "frame {frame}: frozen {played} against captured {want}"
            );
        }
    }

    #[test]
    fn engaging_does_not_change_level_pan_mute_or_solo() {
        // The four the plan promises stay lossless: they act below the capture point, so halving
        // Level must halve the frozen output exactly as it halves the live one.
        let mut patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");

        let mut loud = Engine::new();
        loud.set_sample_rate(48_000.0);
        loud.prepare(&patches);
        loud.set_captures(Some(Box::new(captures.clone())));
        strike(&mut loud, &patches, 1.0);
        let at_unity = main_mono(&mut loud, &patches, 256);

        patches[0].level = 0.5;
        let mut quiet = Engine::new();
        quiet.set_sample_rate(48_000.0);
        quiet.prepare(&patches);
        quiet.set_captures(Some(Box::new(captures)));
        strike(&mut quiet, &patches, 1.0);
        let at_half = main_mono(&mut quiet, &patches, 256);

        for (unity, half) in at_unity.iter().zip(&at_half) {
            assert!((unity * 0.5 - half).abs() < 1.0e-6);
        }
    }

    #[test]
    fn a_frozen_engine_reports_its_readers_not_its_circuits() {
        // If activity reported the parked circuits, `Tail` would end while a captured hit was
        // still playing and a host would cut it off.
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));
        assert!(!engine.is_active(), "nothing struck, nothing active");
        strike(&mut engine, &patches, 1.0);
        assert!(engine.is_active(), "a struck capture is active");
        main_mono(&mut engine, &patches, 8);
        assert!(engine.is_active(), "and stays active while it plays");
    }

    #[test]
    fn a_frozen_engine_reaches_exact_silence() {
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let length = captures.slot(0).len();
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));
        strike(&mut engine, &patches, 1.0);
        main_mono(&mut engine, &patches, length + 16);
        assert!(!engine.is_active(), "the reader ended");
        assert_eq!(
            engine.process(&patches),
            [0.0, 0.0],
            "and renders exact zero"
        );
    }

    #[test]
    fn reset_keeps_the_captures_and_renders_identically_twice() {
        // A capture is derived, like a coefficient, so reset clears the readers and keeps the
        // buffers. Without that, `reset(); render()` would differ between the first and second
        // run, and a DAW's per-transport-stop reset would re-render the whole kit.
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));

        engine.reset();
        strike(&mut engine, &patches, 1.0);
        let first = main_mono(&mut engine, &patches, 256);

        engine.reset();
        assert!(engine.is_frozen(), "reset did not discard the kit");
        strike(&mut engine, &patches, 1.0);
        let second = main_mono(&mut engine, &patches, 256);

        assert_eq!(first, second);
    }

    #[test]
    fn choke_stops_a_frozen_slot_as_it_stops_a_live_one() {
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);
        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));
        strike(&mut engine, &patches, 1.0);
        main_mono(&mut engine, &patches, 32);
        engine.choke(0);
        assert!(!engine.is_active(), "choke ended the reader");
    }

    #[test]
    fn disengaging_returns_the_circuits_unchanged() {
        // Engage, disengage, and the instrument must be the one it was — this is what makes the
        // mode safe to try.
        let patches = kit_of(ModelId::DEEP_BRIDGE_KICK);

        let mut live = Engine::new();
        live.set_sample_rate(48_000.0);
        live.prepare(&patches);
        strike(&mut live, &patches, 1.0);
        let before = main_mono(&mut live, &patches, 512);

        let captures = capture_kit(&patches, 48_000.0).expect("a kit");
        let mut engine = Engine::new();
        engine.set_sample_rate(48_000.0);
        engine.prepare(&patches);
        engine.set_captures(Some(Box::new(captures)));
        strike(&mut engine, &patches, 1.0);
        main_mono(&mut engine, &patches, 64);
        let retired = engine.set_captures(None);
        assert!(
            retired.is_some(),
            "the outgoing kit came back to be dropped off-audio"
        );
        assert!(!engine.is_frozen());
        engine.reset();
        strike(&mut engine, &patches, 1.0);
        let after = main_mono(&mut engine, &patches, 512);

        assert_eq!(before, after);
    }
}

#[cfg(test)]
mod capture_cost {
    use super::*;
    use crate::model::AVAILABLE_MODELS;

    /// Not a gate: prints how long a full kit capture takes, because plan §4.7's background task
    /// has to be short enough that teardown never waits on it.
    #[test]
    #[ignore = "prints a timing; run with --release -- --ignored --nocapture"]
    fn how_long_a_kit_takes() {
        for rate in [48_000.0_f32, 192_000.0] {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            for (slot, patch) in patches.iter_mut().enumerate() {
                patch.model = AVAILABLE_MODELS[slot].id;
            }
            let start = std::time::Instant::now();
            let kit = capture_kit(&patches, rate).expect("a kit");
            let elapsed = start.elapsed();
            let longest = (0..SLOT_COUNT)
                .map(|s| kit.slot(s).len())
                .max()
                .unwrap_or(0);
            println!(
                "{rate:>7} Hz: {:>7.1} ms for the kit, longest slot {longest} frames ({:.2} s)",
                elapsed.as_secs_f64() * 1000.0,
                longest as f32 / rate
            );
        }
    }
}
