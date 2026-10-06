//! Deep struck bridged-T bass drum — the first admitted circuit.
//!
//! `research:instruments/analogue-drum-machines.md` §3.2 and Werner, Abel & Smith's DAFx-14
//! analysis establish the structure: asymmetric pulse-shaper edges strike a bridged-T resonator;
//! a roughly six-millisecond transistor condition raises frequency and Q; its end supplies a
//! retrigger pulse; leakage causes the longer pitch sigh; decay is feedback, followed by passive
//! tone low-pass and output high-pass stages.
//!
//! This renderer is a **reduced physically informed model**, not the paper's complete multi-input
//! time-varying filter. Its reference constants were fitted to the acquired comparison recording
//! (the centre-position unaccented take, `research:instruments/analogue-drum-machines.md` §11.2) in
//! the 2026-09-19 A/B pass by least-squares waveform fitting of this exact structure over the first
//! 300 ms: the 1 ms trigger that reaches the output ahead of the ring, the held attack condition
//! and its end-of-condition strike, the pitch sigh, the waveform-dependent leakage and the
//! asymmetric output. Fidelity remains unverified against authenticated hardware.

use crate::resonator::{MAX_FREQUENCY_FRACTION, MIN_SAMPLE_RATE};
use crate::{Coefficient, flush};

/// Rest pitch of the struck ring once the sweep has ended. FITTED to the acquired comparison
/// recording in the 2026-09-19 A/B pass (48.9 Hz measured, 48.8 Hz before the leakage term's small
/// lift). The service target is 56 Hz and the calculated undisturbed centre 49.5 Hz
/// (`research:instruments/analogue-drum-machines.md` §3.2); a trimmed hardware unit replaces it.
const REFERENCE_HZ: f32 = 48.8;
/// Amplitude T60 of the ring at the reference Decay. FITTED to the acquired comparison recording in
/// the 2026-09-19 A/B pass: its half-cycle peaks fall at 471 ms T60 and the 300 ms waveform fit
/// settles at 440 ms; 460 ms keeps both the first 100 ms and the tail within about 1 dB. The service
/// table's 300 ms describes a −20 dB-style figure, not T60.
const REFERENCE_T60: f32 = 0.460;
/// Shortest stock Decay. MEASURED from the acquired comparison recording's lowest Decay position
/// (about 100 ms T60) in the 2026-09-19 A/B pass; replace from a hardware Decay sweep.
const SHORT_T60: f32 = 0.100;
// The stock long setting measures about 1.1 s T60 in the same recordings. The positive half
// continues into the several-second range reached by common feedback/capacitor extended-decay
// modifications documented in `research:instruments/analogue-drum-machines.md` §4.11. The 4.8 s
// endpoint is CHOSEN.
const EXTENDED_T60: f32 = 4.800;
const MAX_TAIL_SECONDS: f32 = 30.0;
/// Final emergency bound under adversarial retriggering. Ordinary reference hits must not reach it.
pub const OUTPUT_BOUND: f32 = 1.5;
/// CHOSEN conversion from the fitted recording scale (peak about 1) to plugin full scale. The
/// catalogue trim in `engine.rs` owns the absolute level; replace from service-level calibration.
const OUTPUT_TRIM: f32 = 0.85;

/// Accent at the catalogue reference velocity (0.82) with zero Dynamics. Fitted amplitudes below are
/// the recording's scale at this accent, so the reference render reproduces the fitted waveform and
/// other velocities scale the strike, feed-through and sigh from it.
const REFERENCE_ACCENT: f32 = 0.35 + 0.65 * 0.82;

/// Strike amplitude and phase of the struck ring (`amplitude · cos(ωt + phase)`). FITTED to the
/// acquired comparison recording in the 2026-09-19 A/B pass. The phase lead stands for the
/// bridged-T network's direct high-frequency path, which this reduced resonator lacks; a circuit
/// simulation of the pulse shaper and network replaces both numbers.
const STRIKE_AMPLITUDE: f64 = 0.943;
const STRIKE_PHASE: f64 = 0.662;
/// Level at which the 1 ms common trigger reaches the output before the tone low-pass: the sharp
/// positive spike ahead of the first negative half-cycle. FITTED to the acquired comparison
/// recording in the 2026-09-19 A/B pass; a pulse-shaper output measurement replaces it.
const TRIGGER_FEED: f32 = 0.984;
/// The common trigger's width, `research:instruments/analogue-drum-machines.md` §2.1.
const TRIGGER_SECONDS: f32 = 0.001;
/// Frequency lift held while the attack condition lasts (more than an octave with the sigh on top),
/// and the condition's length. FITTED to the acquired comparison recording in the 2026-09-19 A/B
/// pass: the lift is flat, then ends at 5.1 ms rather than decaying (§3.2 gives "about 6 ms").
/// Instantaneous-frequency measurement of the isolated condition replaces both.
const HOLD_LIFT: f32 = 0.935;
const HOLD_SECONDS: f32 = 0.0051;
/// The differentiated pulse the ending condition sends into the ring (§3.2 step 4), as an added
/// strike. FITTED to the acquired comparison recording in the 2026-09-19 A/B pass; replace from the
/// transition branch's measured current.
const TRANSITION_AMPLITUDE: f64 = 0.213;
const TRANSITION_PHASE: f64 = -0.357;
/// The pitch sigh: relative lift and exponential time constant. FITTED to the acquired comparison
/// recording in the 2026-09-19 A/B pass (early cycles near 74, 66 and 54 Hz settling to rest by
/// about 60 ms); instantaneous-frequency fitting of hardware captures replaces them.
const SIGH_LIFT: f32 = 0.736;
const SIGH_SECONDS: f32 = 0.01376;
/// Relative frequency lift per unit of negative ring excursion: the transistor branch's leakage,
/// which makes negative half-cycles shorter than positive ones. FITTED to the acquired comparison
/// recording in the 2026-09-19 A/B pass; per-half-cycle frequency measurement at several levels
/// replaces it.
const LEAKAGE: f32 = 0.0762;
/// Quadratic asymmetry of the output stage, `v − k·v²`: negative half-cycles taller than positive
/// ones while the ring is loud, symmetric as it decays. FITTED to the acquired comparison recording
/// in the 2026-09-19 A/B pass; a transfer-curve measurement of the output buffer replaces it.
const ASYMMETRY: f32 = 0.223;
/// The quadratic term is evaluated on an input clamped here, where `v − k·v²` stops rising, so the
/// curve stays monotonic under retrigger overload.
const ASYMMETRY_LIMIT: f32 = 0.5 / ASYMMETRY;
/// Tone low-pass at the reference position and at the knob ends. MEASURED by fitting the same model
/// to the acquired comparison recordings at Tone positions 1, 5 and 11 in the 2026-09-19 A/B pass
/// (252, 435 and 1,022 Hz one-pole equivalents); the passive network's response replaces them.
const TONE_HZ: f32 = 438.0;
const TONE_DARK_RATIO: f32 = 252.0 / 435.0;
const TONE_BRIGHT_RATIO: f32 = 1_022.0 / 435.0;

/// Plain, reference-centred controls for one sample.
#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub pitch_semitones: f32,
    pub pitch_envelope: f32,
    pub pitch_decay: f32,
    pub decay: f32,
    pub attack: f32,
    pub tone: f32,
    pub body: f32,
    /// Reserved by the common slot vocabulary; this circuit has no noise path.
    pub noise: f32,
    pub character: f32,
    pub dynamics: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            pitch_envelope: 0.0,
            pitch_decay: 0.0,
            decay: 0.0,
            attack: 0.0,
            tone: 0.0,
            body: 0.0,
            noise: 0.0,
            character: 0.0,
            dynamics: 0.0,
        }
    }
}

/// Coupled-form ("magic circle") struck resonator: a decaying phasor rotated by the pole angle.
///
/// Gold & Rader's coupled form keeps the ring's amplitude when its frequency moves, as a physical
/// resonator's stored energy does when a resistor retunes it, and it lets a strike set any phase
/// directly: a strike adds `amplitude · (cos φ, sin φ)` to the live state, so the free response is
/// `amplitude · rⁿ cos(ωn + φ)` on top of whatever was already ringing. Retriggers therefore add to
/// or cancel the live ring according to its phase, which is the documented 808 behaviour. The
/// strike is already unit-scale; there is no all-pole gain to normalise. The Bridge 808 family's
/// struck bodies share this primitive; `f64` because the rotation compounds for the whole tail.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PhasorResonator {
    re: f64,
    im: f64,
}

impl PhasorResonator {
    pub(crate) const fn new() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    /// Adds a strike whose free response is `amplitude · cos(ωn + phase)`.
    pub(crate) fn strike(&mut self, amplitude: f64, phase: f64) {
        if amplitude.is_finite() && phase.is_finite() {
            self.re += amplitude * phase.cos();
            self.im += amplitude * phase.sin();
        }
    }

    /// Returns the current output, then advances one sample at `frequency_hz` with amplitude T60.
    #[inline]
    pub(crate) fn step(&mut self, sample_rate: f32, frequency_hz: f32, t60_seconds: f32) -> f32 {
        let fs = f64::from(sample_rate.max(MIN_SAMPLE_RATE));
        let frequency = f64::from(if frequency_hz.is_finite() {
            frequency_hz.clamp(0.0, MAX_FREQUENCY_FRACTION * fs as f32)
        } else {
            100.0
        });
        let t60 = f64::from(if t60_seconds.is_finite() {
            t60_seconds.clamp(1.0 / fs as f32, 60.0)
        } else {
            0.1
        });
        let output = self.re;
        let radius = (0.001_f64.ln() / (t60 * fs)).exp().min(1.0 - 1.0e-9);
        let (sin, cos) = (std::f64::consts::TAU * frequency / fs).sin_cos();
        let re = radius * (cos * self.re - sin * self.im);
        let im = radius * (sin * self.re + cos * self.im);
        if re.is_finite() && im.is_finite() {
            self.re = if re.abs() < 1.0e-20 { 0.0 } else { re };
            self.im = if im.abs() < 1.0e-20 { 0.0 } else { im };
        } else {
            self.reset();
            return 0.0;
        }
        output as f32
    }

    pub(crate) fn reset(&mut self) {
        self.re = 0.0;
        self.im = 0.0;
    }

    /// The ring's current amplitude. Parking reads this rather than one output sample, which can
    /// pass through zero while the ring is still loud.
    pub(crate) fn magnitude(&self) -> f64 {
        self.re.hypot(self.im)
    }
}

#[derive(Debug, Clone)]
pub struct DeepBridgeKick {
    sample_rate: f32,
    /// Coefficients held against their inputs rather than rebuilt per sample: the 12 Hz output
    /// coupling follows the rate alone, the tone pole its cutoff, the sigh its pitch time.
    output_pole: Coefficient<f32, f32>,
    tone_pole: Coefficient<(f32, f32), f32>,
    sigh_pole: Coefficient<(f32, f32), f32>,
    resonator: PhasorResonator,
    /// Accent of the latest strike relative to [`REFERENCE_ACCENT`].
    accent_scale: f32,
    sigh_envelope: f32,
    feed_remaining: u32,
    feed_level: f32,
    /// Samples since the latest strike, for the held attack condition.
    age: u32,
    condition_active: bool,
    active: bool,
    last_resonator: f32,
    low_pass: f32,
    high_pass_input: f32,
    high_pass_output: f32,
}

impl Default for DeepBridgeKick {
    fn default() -> Self {
        Self::new()
    }
}

impl DeepBridgeKick {
    #[must_use]
    pub fn new() -> Self {
        let mut voice = Self {
            sample_rate: 48_000.0,
            output_pole: Coefficient::new(),
            tone_pole: Coefficient::new(),
            sigh_pole: Coefficient::new(),
            resonator: PhasorResonator::new(),
            accent_scale: 0.0,
            sigh_envelope: 0.0,
            feed_remaining: 0,
            feed_level: 0.0,
            age: 0,
            condition_active: false,
            active: false,
            last_resonator: 0.0,
            low_pass: 0.0,
            high_pass_input: 0.0,
            high_pass_output: 0.0,
        };
        voice.set_sample_rate(48_000.0);
        voice
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = if sample_rate.is_finite() {
            sample_rate.max(MIN_SAMPLE_RATE)
        } else {
            48_000.0
        };
    }

    /// Strikes the live circuit. It deliberately does not reset resonator or output-filter state.
    pub fn trigger(&mut self, velocity: f32, dynamics: f32, attack: f32) {
        let velocity = finite_unit(velocity);
        let dynamics = finite_bipolar(dynamics);
        let attack = finite_bipolar(attack);
        // Reference accent spans the service table's 3.5 Vpp normal to 10 Vpp accented output.
        // Dynamics bends, rather than replaces, that law. CHOSEN exponents need velocity sweeps.
        let exponent = crate::velocity::exponent(dynamics);
        let accent = 0.35 + 0.65 * velocity.powf(exponent);
        let scale = accent / REFERENCE_ACCENT;
        self.accent_scale = scale;
        self.resonator
            .strike(f64::from(scale) * STRIKE_AMPLITUDE, STRIKE_PHASE);
        // CHOSEN Attack law: the trigger's feed-through is the audible click, so Attack scales it.
        self.feed_level = scale * TRIGGER_FEED * (1.0 + 0.6 * attack);
        self.feed_remaining = (TRIGGER_SECONDS * self.sample_rate).round().max(1.0) as u32;
        self.sigh_envelope = scale;
        self.age = 0;
        self.condition_active = true;
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, patch: Patch) -> f32 {
        if !self.active {
            return 0.0;
        }

        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let decay = finite_bipolar(patch.decay);
        let attack = finite_bipolar(patch.attack);
        let pitch_depth = envelope_depth(finite_bipolar(patch.pitch_envelope));
        let pitch_time = envelope_time_scale(finite_bipolar(patch.pitch_decay));
        let tone = finite_bipolar(patch.tone);
        let body = finite_bipolar(patch.body);
        let character = finite_bipolar(patch.character);
        // Read to make the no-op intentional and obvious in review.
        let _unsupported_noise = finite_bipolar(patch.noise);

        // The attack condition holds its lift, then ends at once and sends its transition pulse
        // into the ring. Pitch decay scales the condition's length with the sigh.
        let hold_samples = (HOLD_SECONDS * pitch_time * self.sample_rate).round() as u32;
        if self.condition_active && self.age >= hold_samples {
            self.condition_active = false;
            self.resonator.strike(
                f64::from(self.accent_scale) * TRANSITION_AMPLITUDE,
                TRANSITION_PHASE,
            );
        }
        let hold = if self.condition_active {
            HOLD_LIFT * (1.0 + 0.35 * attack)
        } else {
            0.0
        };
        // CHOSEN Body law: Body deepens the sigh and the leakage, the two waveform-coupled terms.
        let sigh = self.sigh_envelope * SIGH_LIFT * (1.0 + 0.45 * body);
        let leakage = (-self.last_resonator).clamp(0.0, 2.0) * LEAKAGE * (1.0 + 0.6 * body);
        let base = REFERENCE_HZ * 2.0_f32.powf(pitch / 12.0);
        let frequency = base * (1.0 + pitch_depth * (hold + sigh + leakage));
        let t60 = decay_time(decay);
        let resonated = self.resonator.step(self.sample_rate, frequency, t60);
        self.last_resonator = resonated;
        let sigh_decay = self
            .sigh_pole
            .get((pitch_time, self.sample_rate), |(pitch_time, fs)| {
                (-1.0 / (SIGH_SECONDS * pitch_time * fs)).exp()
            });
        self.sigh_envelope = flush(self.sigh_envelope * sigh_decay);

        // The 1 ms common trigger reaches the output with the ring.
        let feed = if self.feed_remaining > 0 {
            self.feed_remaining -= 1;
            self.feed_level
        } else {
            0.0
        };

        // Passive TONE low-pass, as a one-pole equivalent.
        let cutoff = if tone < 0.0 {
            TONE_HZ * TONE_DARK_RATIO.powf(-tone)
        } else {
            TONE_HZ * TONE_BRIGHT_RATIO.powf(tone)
        }
        .min(0.45 * self.sample_rate);
        let lp_coefficient = self
            .tone_pole
            .get((cutoff, self.sample_rate), |(cutoff, fs)| {
                1.0 - (-std::f32::consts::TAU * cutoff / fs).exp()
            });
        self.low_pass = flush(self.low_pass + lp_coefficient * (resonated + feed - self.low_pass));

        let clamped = self.low_pass.clamp(-ASYMMETRY_LIMIT, ASYMMETRY_LIMIT);
        let asymmetric = self.low_pass - ASYMMETRY * clamped * clamped;
        // Character lowers or raises a symmetric soft headroom around the reference; at zero the
        // headroom of 4 sits far enough above the fitted level (peak about 1) to leave the fitted
        // waveform intact. CHOSEN pending a level sweep against hardware.
        let headroom = 4.0 * 4.0_f32.powf(-character);
        let shaped = headroom * (asymmetric / headroom).tanh() * OUTPUT_TRIM;

        // Output coupling high-pass. CHOSEN 12 Hz, to be replaced from the loaded output stage.
        let hp_coefficient = self.output_pole.get(self.sample_rate, |fs| {
            (-std::f32::consts::TAU * 12.0 / fs).exp()
        });
        self.high_pass_output =
            flush(hp_coefficient * (self.high_pass_output + shaped - self.high_pass_input));
        self.high_pass_input = shaped;

        self.age = self.age.saturating_add(1);
        let maximum_age = ((8.0 * t60 + 0.1).min(MAX_TAIL_SECONDS) * self.sample_rate) as u32;
        if self.age > maximum_age
            && self.high_pass_output.abs() < 1.0e-8
            && self.resonator.magnitude() < 1.0e-8
        {
            self.reset();
            return 0.0;
        }
        self.high_pass_output.clamp(-OUTPUT_BOUND, OUTPUT_BOUND)
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        self.resonator.reset();
        self.accent_scale = 0.0;
        self.sigh_envelope = 0.0;
        self.feed_remaining = 0;
        self.feed_level = 0.0;
        self.age = 0;
        self.condition_active = false;
        self.active = false;
        self.last_resonator = 0.0;
        self.low_pass = 0.0;
        self.high_pass_input = 0.0;
        self.high_pass_output = 0.0;
    }
}

#[inline]
fn decay_time(value: f32) -> f32 {
    if value < 0.0 {
        REFERENCE_T60 * (SHORT_T60 / REFERENCE_T60).powf(-value)
    } else {
        REFERENCE_T60 * (EXTENDED_T60 / REFERENCE_T60).powf(value)
    }
}

#[inline]
fn envelope_depth(value: f32) -> f32 {
    if value < 0.0 {
        1.0 + value
    } else {
        1.0 + 3.0 * value
    }
}

#[inline]
fn envelope_time_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.25_f32.powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}

#[inline]
fn finite_unit(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
}

#[inline]
fn finite_bipolar(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(-1.0, 1.0)
}

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(voice: &mut DeepBridgeKick, patch: Patch, count: usize) -> Vec<f32> {
        (0..count).map(|_| voice.process(patch)).collect()
    }

    fn crossings(samples: &[f32]) -> Vec<f32> {
        let mut crossings = Vec::new();
        for index in 1..samples.len() {
            let (before, after) = (samples[index - 1], samples[index]);
            if (before <= 0.0) != (after <= 0.0) {
                crossings.push(index as f32 - 1.0 - before / (after - before));
            }
        }
        crossings
    }

    #[test]
    fn a_strike_sounds_and_reset_is_exact_silence() {
        let mut voice = DeepBridgeKick::new();
        voice.trigger(0.8, 0.0, 0.0);
        let sound = render(&mut voice, Patch::default(), 48_000);
        assert!(sound.iter().any(|sample| sample.abs() > 0.01));
        assert!(sound.iter().all(|sample| sample.is_finite()));
        voice.reset();
        assert!(!voice.is_active());
        assert_eq!(voice.process(Patch::default()).to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn reference_hit_stays_clear_of_the_emergency_bound() {
        let mut voice = DeepBridgeKick::new();
        voice.trigger(0.8, 0.0, 0.0);
        let sound = render(&mut voice, Patch::default(), 48_000);
        let peak = sound
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            peak > 0.2,
            "the repaired strike became inaudible: peak {peak}"
        );
        assert!(
            peak < 0.8 * OUTPUT_BOUND,
            "reference peak {peak} is using the {} safety bound as a waveshaper",
            OUTPUT_BOUND
        );
        assert!(
            sound.iter().all(|sample| sample.abs() != OUTPUT_BOUND),
            "a reference hit contains a flat safety-clamp plateau"
        );
    }

    #[test]
    fn reference_accent_retains_the_service_level_ratio() {
        fn peak_to_peak(velocity: f32) -> f32 {
            let mut voice = DeepBridgeKick::new();
            voice.trigger(velocity, 0.0, 0.0);
            let sound = render(&mut voice, Patch::default(), 4_800);
            let high = sound.iter().copied().fold(f32::MIN, f32::max);
            let low = sound.iter().copied().fold(f32::MAX, f32::min);
            high - low
        }
        let ratio = peak_to_peak(0.0) / peak_to_peak(1.0);
        // Service output is 3.5 Vpp normal against 10 Vpp accented. The asymmetric output stage
        // moves the two peaks apart but not their span, so the ratio is read peak to peak.
        assert!(
            (0.33..=0.42).contains(&ratio),
            "normal/accent peak-to-peak ratio {ratio}"
        );
    }

    #[test]
    fn retrigger_continues_from_live_state_instead_of_replaying_a_sample() {
        let mut live = DeepBridgeKick::new();
        live.trigger(0.8, 0.0, 0.0);
        let _ = render(&mut live, Patch::default(), 1_337);
        live.trigger(0.8, 0.0, 0.0);
        let continued = render(&mut live, Patch::default(), 512);

        let mut fresh = DeepBridgeKick::new();
        fresh.trigger(0.8, 0.0, 0.0);
        let restarted = render(&mut fresh, Patch::default(), 512);
        assert_ne!(continued, restarted, "retrigger reset the live resonator");
    }

    #[test]
    fn the_trigger_reaches_the_output_ahead_of_a_negative_first_half_cycle() {
        // The acquired comparison recording opens with the 1 ms trigger as a positive spike, then
        // the ring's first, deepest half-cycle swings negative.
        let mut voice = DeepBridgeKick::new();
        voice.trigger(0.82, 0.0, 0.0);
        let sound = render(&mut voice, Patch::default(), 480);
        let spike = sound[..48].iter().copied().fold(f32::MIN, f32::max);
        let trough = sound[48..].iter().copied().fold(f32::MAX, f32::min);
        let trough_at = sound
            .iter()
            .position(|sample| *sample == trough)
            .expect("a trough");
        assert!(spike > 0.6 * trough.abs(), "spike {spike}, trough {trough}");
        assert!(
            (96..240).contains(&trough_at),
            "first trough at sample {trough_at}"
        );
    }

    #[test]
    fn early_cycles_are_faster_and_the_tail_settles_at_the_fitted_rest() {
        let sample_rate = 48_000.0;
        let mut voice = DeepBridgeKick::new();
        voice.set_sample_rate(sample_rate);
        voice.trigger(0.82, 0.0, 0.0);
        let sound = render(&mut voice, Patch::default(), sample_rate as usize);
        let times = crossings(&sound);
        // The first negative half-cycle, held above an octave: about 138 Hz in the recording.
        let first_half = 1.0 / (2.0 * (times[1] - times[0]) / sample_rate);
        assert!(
            (110.0..170.0).contains(&first_half),
            "first half-cycle {first_half} Hz"
        );
        let start = (0.20 * sample_rate) as usize;
        let end = (0.50 * sample_rate) as usize;
        let late: Vec<f32> = crossings(&sound[start..end])
            .into_iter()
            .step_by(2)
            .collect();
        let periods = late.len() - 1;
        let measured = sample_rate * periods as f32 / (late[periods] - late[0]);
        assert!(
            (measured - 48.9).abs() < 0.6,
            "settled reference tail measured {measured} Hz"
        );
    }

    #[test]
    fn reference_decay_sits_between_service_short_and_long_settings() {
        fn energy(decay: f32) -> f32 {
            let mut voice = DeepBridgeKick::new();
            voice.trigger(0.8, 0.0, 0.0);
            render(
                &mut voice,
                Patch {
                    decay,
                    ..Patch::default()
                },
                24_000,
            )
            .into_iter()
            .skip(12_000)
            .map(|sample| sample * sample)
            .sum()
        }
        assert_eq!(decay_time(-1.0), SHORT_T60);
        assert_eq!(decay_time(0.0), REFERENCE_T60);
        assert_eq!(decay_time(1.0), EXTENDED_T60);
        let short = energy(-1.0);
        let reference = energy(0.0);
        let long = energy(1.0);
        assert!(
            short < reference && reference < long,
            "{short}, {reference}, {long}"
        );
    }

    #[test]
    fn hostile_controls_and_trigger_values_stay_finite_and_bounded() {
        let mut voice = DeepBridgeKick::new();
        voice.trigger(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);
        let patch = Patch {
            pitch_semitones: f32::INFINITY,
            pitch_envelope: f32::NAN,
            pitch_decay: f32::INFINITY,
            decay: f32::NAN,
            attack: f32::NEG_INFINITY,
            tone: f32::INFINITY,
            body: f32::NAN,
            noise: f32::INFINITY,
            character: f32::NEG_INFINITY,
            dynamics: f32::NAN,
        };
        for sample in render(&mut voice, patch, 96_000) {
            assert!(sample.is_finite());
            assert!(sample.abs() <= OUTPUT_BOUND);
        }
    }
}
