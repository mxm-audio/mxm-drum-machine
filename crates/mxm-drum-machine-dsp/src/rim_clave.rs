//! Two articulations of the short multi-feedback rim/clave resonator section.
//!
//! `research:instruments/analogue-drum-machines.md` §3.5 gives rim modes near 1.667 kHz and 455 Hz
//! with 10 ms decay, and the cleaner clave mode near 2.5 kHz with 25 ms decay. Switching/clipping
//! makes them two articulations of one hardware section; a slot owns live state so retriggers retain
//! phase, while [`Kind`] selects the documented switch configuration.
//!
//! The reference constants were fitted to the acquired comparison recordings (the unaccented takes,
//! §11.2) in the 2026-09-19 A/B pass by least-squares waveform fitting. The rim is the switched
//! articulation: the trigger reaches the output first as a small step, the two modes strike 0.45 ms
//! later and drive a hard, asymmetric transistor clipper whose output is differentiated, so the
//! first milliseconds are a train of sharp edges at the upper mode, and a switch closes the whole
//! section about 9 ms after the trigger (the service note's 10 ms decay), cutting the lower mode's
//! ring.
//! The clave stays below the clipping point: a clean cosine-phase ring.

use crate::deep_bridge_kick::PhasorResonator;
use crate::resonator::MIN_SAMPLE_RATE;
use crate::{Coefficient, flush};

const MAX_TAIL_SECONDS: f32 = 0.5;
/// Accent at the catalogue reference velocity (0.82) with zero Dynamics; fitted amplitudes are the
/// recording's scale at this accent.
const REFERENCE_ACCENT: f32 = 0.35 + 0.65 * 0.82;
/// Final emergency bound; reference hits stay well inside it.
const OUTPUT_BOUND: f32 = 1.0;

/// Rim modes: frequency, pre-clipper strike amplitude, strike phase and amplitude T60. FITTED to the
/// acquired comparison recording in the 2026-09-19 A/B pass by waveform fitting with the lower
/// mode's frequency chosen so the rendered spectrum over the first 12 ms peaks where the
/// recording's does (476 Hz; the upper mode at 1.76 kHz). They sit about 4–6% above the service
/// table's 455 Hz and 1.667 kHz. A hardware mode measurement replaces them.
const RIM_HIGH_HZ: f32 = 1_761.3;
const RIM_HIGH_AMPLITUDE: f64 = 2.471;
const RIM_HIGH_PHASE: f64 = 1.722;
const RIM_HIGH_T60: f32 = 0.009_13;
const RIM_LOW_HZ: f32 = 472.0;
const RIM_LOW_AMPLITUDE: f64 = 0.352;
const RIM_LOW_PHASE: f64 = 0.863;
const RIM_LOW_T60: f32 = 0.042_3;
/// Level at which the 1 ms common trigger reaches the output ahead of the strike, as a share of
/// the reference hit's scale: the small step the recording holds before its first edge. FITTED in
/// the 2026-09-19 A/B pass; a measurement of the trigger's path to the output replaces it.
const RIM_TRIGGER_FEED: f32 = -0.0155;
const TRIGGER_SECONDS: f32 = 0.001;
/// Delay from the trigger to the rim's strike: the recording is nearly silent for its first
/// 0.45 ms. FITTED in the 2026-09-19 A/B pass; the switching stage's measured turn-on replaces it.
const RIM_STRIKE_DELAY_SECONDS: f32 = 0.000_45;
/// Clipper drive and bias (`tanh(g·x + b) − tanh(b)`), the differentiating high-pass after it, and
/// the output low-pass. FITTED to the acquired comparison recording in the 2026-09-19 A/B pass: the
/// edges and their exponential recoveries between them, the negative dip before the first rise,
/// and the bias's even harmonics, which put the recording's energy near 950 and 1,270 Hz and halve
/// the third-octave distance over the first 12 ms. A transfer measurement of the switching stage
/// replaces them.
const RIM_CLIP_DRIVE: f32 = 6.835;
const RIM_CLIP_BIAS: f32 = 0.75;
const RIM_HIGH_PASS_HZ: f64 = 999.2;
const RIM_LOW_PASS_HZ: f32 = 5_946.0;
/// The switch that closes the section: hold from the strike, then an exponential release. FITTED in
/// the 2026-09-19 A/B pass (8.55 ms and a 1.4 ms time constant; the recording falls from −20 to
/// −58 dB between 9 and 13 ms after the trigger). The hold is the Decay control's reference. A
/// measurement of the switch's control pulse replaces them.
const RIM_GATE_HOLD_SECONDS: f32 = 0.008_55;
const RIM_GATE_RELEASE_SECONDS: f32 = 0.001_42;
const RIM_OUTPUT_GAIN: f32 = 0.532;

/// Clave mode: frequency, strike amplitude and phase, amplitude T60 and the output low-pass. FITTED
/// to the acquired comparison recording in the 2026-09-19 A/B pass; the recording decays at 45 ms
/// T60 (its −20 dB point near 16 ms), and its clipping stage stays essentially linear. A hardware
/// mode measurement replaces them.
const CLAVE_HZ: f32 = 2_497.0;
const CLAVE_AMPLITUDE: f64 = 1.643;
const CLAVE_PHASE: f64 = -0.267;
const CLAVE_T60: f32 = 0.044_6;
const CLAVE_LOW_PASS_HZ: f32 = 2_164.0;
/// The same closing switch in the clave articulation: the recording decays at 1 dB/ms to about
/// −45 dB at 33 ms, then falls about 8 dB per millisecond. MEASURED in the 2026-09-19 A/B pass; a
/// measurement of the switch's control pulse replaces it.
const CLAVE_GATE_HOLD_SECONDS: f32 = 0.033;
const CLAVE_GATE_RELEASE_SECONDS: f32 = 0.001_2;
/// The shared clipping stage's drive in the clave articulation (`tanh(g·x)/g`), nearly linear at
/// the reference, and the output level. FITTED in the 2026-09-19 A/B pass; a transfer measurement
/// of the switching stage replaces the drive, and the catalogue trim owns the level.
const CLAVE_CLIP_DRIVE: f32 = 0.326;
const CLAVE_OUTPUT_GAIN: f32 = 0.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Rim,
    Clave,
}

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub pitch_semitones: f32,
    pub decay: f32,
    pub attack: f32,
    pub tone: f32,
    pub body: f32,
    pub noise: f32,
    pub character: f32,
    pub dynamics: f32,
}

#[derive(Debug, Clone)]
pub struct RimClave {
    sample_rate: f32,
    /// The rim output high-pass, which follows the sample rate alone.
    rim_output_pole: Coefficient<f32, f64>,
    high: PhasorResonator,
    low: PhasorResonator,
    /// Scaled strike waiting for the rim's strike delay.
    pending_strike: f32,
    strike_in: u32,
    feed_level: f32,
    feed_remaining: u32,
    gate: f32,
    age_since_strike: u32,
    high_pass_input: f64,
    high_pass_output: f64,
    tone_state: f32,
    age: u32,
    active: bool,
}

impl Default for RimClave {
    fn default() -> Self {
        Self::new()
    }
}

impl RimClave {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            rim_output_pole: Coefficient::new(),
            high: PhasorResonator::new(),
            low: PhasorResonator::new(),
            pending_strike: 0.0,
            strike_in: 0,
            feed_level: 0.0,
            feed_remaining: 0,
            gate: 0.0,
            age_since_strike: 0,
            high_pass_input: 0.0,
            high_pass_output: 0.0,
            tone_state: 0.0,
            age: 0,
            active: false,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = if sample_rate.is_finite() {
            sample_rate.max(MIN_SAMPLE_RATE)
        } else {
            48_000.0
        };
    }

    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let velocity = finite_or(velocity, 0.0).clamp(0.0, 1.0);
        let dynamics = finite_bipolar(patch.dynamics);
        let exponent = crate::velocity::exponent(dynamics);
        let accent = (0.35 + 0.65 * velocity.powf(exponent)) / REFERENCE_ACCENT;
        self.pending_strike += accent * (1.0 + 0.4 * finite_bipolar(patch.attack));
        self.strike_in = (RIM_STRIKE_DELAY_SECONDS * self.sample_rate).round() as u32;
        self.feed_level = accent * RIM_TRIGGER_FEED;
        self.feed_remaining = (TRIGGER_SECONDS * self.sample_rate).round().max(1.0) as u32;
        self.age = 0;
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, kind: Kind, patch: Patch) -> f32 {
        if !self.active {
            return 0.0;
        }
        let ratio = 2.0_f32.powf(finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0) / 12.0);
        let decay = finite_bipolar(patch.decay);
        let decay_scale = if decay < 0.0 {
            0.35_f32.powf(-decay)
        } else {
            8.0_f32.powf(decay)
        };
        let tone = finite_bipolar(patch.tone);
        let body = finite_bipolar(patch.body);
        let character = finite_bipolar(patch.character);
        // Noise is unsupported for both articulations and is an exact no-op.
        let _unsupported_noise = finite_bipolar(patch.noise);

        // The clave articulation strikes at once; the rim's switch path strikes after its delay.
        let delay_done = kind == Kind::Clave || self.strike_in == 0;
        if !delay_done {
            self.strike_in -= 1;
        }
        if delay_done && self.pending_strike != 0.0 {
            let strike = f64::from(std::mem::take(&mut self.pending_strike));
            if kind == Kind::Rim {
                // CHOSEN Tone and Body laws: Tone tilts the two modes against each other, Body
                // raises the lower mode that carries the rim's ring.
                self.high.strike(
                    strike * RIM_HIGH_AMPLITUDE * f64::from(1.0 + 0.5 * tone),
                    RIM_HIGH_PHASE,
                );
                self.low.strike(
                    strike * RIM_LOW_AMPLITUDE * f64::from((1.0 - 0.5 * tone) * (1.0 + 0.5 * body)),
                    RIM_LOW_PHASE,
                );
            } else {
                self.high.strike(
                    strike * CLAVE_AMPLITUDE * f64::from(1.0 + 0.25 * body),
                    CLAVE_PHASE,
                );
            }
            self.gate = 1.0;
            self.age_since_strike = 0;
        }

        let output = if kind == Kind::Rim {
            let high = self.high.step(
                self.sample_rate,
                RIM_HIGH_HZ * ratio,
                RIM_HIGH_T60 * decay_scale,
            );
            let low = self.low.step(
                self.sample_rate,
                RIM_LOW_HZ * ratio,
                RIM_LOW_T60 * decay_scale,
            );
            // Character opens the headroom around the reference clipping point. CHOSEN law.
            let drive = RIM_CLIP_DRIVE * 2.0_f32.powf(-character);
            let clipped =
                f64::from((drive * (high + low) + RIM_CLIP_BIAS).tanh() - RIM_CLIP_BIAS.tanh());
            let pole = self.rim_output_pole.get(self.sample_rate, |sample_rate| {
                let fs = f64::from(sample_rate);
                (-std::f64::consts::TAU * RIM_HIGH_PASS_HZ / fs).exp()
            });
            let differentiated = pole * (self.high_pass_output + clipped - self.high_pass_input);
            self.high_pass_input = clipped;
            self.high_pass_output = flush_f64(differentiated);
            self.advance_gate(RIM_GATE_HOLD_SECONDS, RIM_GATE_RELEASE_SECONDS, decay_scale);
            let cutoff = RIM_LOW_PASS_HZ * 3.5_f32.powf(0.5 * tone);
            let gated = self.high_pass_output as f32 * self.gate;
            let feed = if self.feed_remaining > 0 {
                self.feed_remaining -= 1;
                self.feed_level
            } else {
                0.0
            };
            self.low_pass(gated, cutoff) * RIM_OUTPUT_GAIN + feed
        } else {
            let ring = self
                .high
                .step(self.sample_rate, CLAVE_HZ * ratio, CLAVE_T60 * decay_scale);
            let cutoff = CLAVE_LOW_PASS_HZ * 3.5_f32.powf(0.5 * tone);
            self.advance_gate(
                CLAVE_GATE_HOLD_SECONDS,
                CLAVE_GATE_RELEASE_SECONDS,
                decay_scale,
            );
            let filtered = self.low_pass(ring * self.gate, cutoff);
            // The shared clipping stage, nearly linear here; Character opens its headroom.
            let drive = CLAVE_CLIP_DRIVE * 4.0_f32.powf(-character);
            (drive * filtered).tanh() / drive * CLAVE_OUTPUT_GAIN
        }
        .clamp(-OUTPUT_BOUND, OUTPUT_BOUND);

        self.age = self.age.saturating_add(1);
        if self.age > (MAX_TAIL_SECONDS * self.sample_rate) as u32
            && self.pending_strike == 0.0
            && self.high.magnitude() < 1.0e-8
            && self.low.magnitude() < 1.0e-8
            && self.high_pass_output.abs() < 1.0e-8
            && self.tone_state.abs() < 1.0e-8
        {
            self.reset();
            return 0.0;
        }
        output
    }

    /// The section's closing switch: open for `hold` after the strike, then an exponential release.
    /// Decay scales both, so the switch keeps its place in the ring.
    fn advance_gate(&mut self, hold: f32, release: f32, decay_scale: f32) {
        if self.age_since_strike >= (hold * decay_scale * self.sample_rate) as u32 {
            let factor = (-1.0 / (release * decay_scale * self.sample_rate)).exp();
            self.gate = flush(self.gate * factor);
        }
        self.age_since_strike = self.age_since_strike.saturating_add(1);
    }

    fn low_pass(&mut self, input: f32, cutoff: f32) -> f32 {
        let coefficient = 1.0
            - (-std::f32::consts::TAU * cutoff.min(0.45 * self.sample_rate) / self.sample_rate)
                .exp();
        self.tone_state = flush(self.tone_state + coefficient * (input - self.tone_state));
        self.tone_state
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        self.high.reset();
        self.low.reset();
        self.pending_strike = 0.0;
        self.strike_in = 0;
        self.feed_level = 0.0;
        self.feed_remaining = 0;
        self.gate = 0.0;
        self.age_since_strike = 0;
        self.high_pass_input = 0.0;
        self.high_pass_output = 0.0;
        self.tone_state = 0.0;
        self.age = 0;
        self.active = false;
    }
}

fn finite_bipolar(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(-1.0, 1.0)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn flush_f64(value: f64) -> f64 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> Patch {
        Patch {
            pitch_semitones: 0.0,
            decay: 0.0,
            attack: 0.0,
            tone: 0.0,
            body: 0.0,
            noise: 0.0,
            character: 0.0,
            dynamics: 0.0,
        }
    }

    fn projection(samples: &[f32], frequency: f32) -> f32 {
        let (sin, cos) =
            samples
                .iter()
                .enumerate()
                .fold((0.0_f32, 0.0_f32), |(s, c), (n, sample)| {
                    let angle = std::f32::consts::TAU * frequency * n as f32 / 48_000.0;
                    (s + sample * angle.sin(), c + sample * angle.cos())
                });
        sin.hypot(cos)
    }

    fn render(kind: Kind, patch: Patch, count: usize) -> Vec<f32> {
        let mut voice = RimClave::new();
        voice.trigger(0.82, patch);
        (0..count).map(|_| voice.process(kind, patch)).collect()
    }

    #[test]
    fn rim_contains_both_fitted_modes_and_clave_uses_the_high_mode() {
        let rim = render(Kind::Rim, patch(), 2_400);
        assert!(projection(&rim[..192], RIM_HIGH_HZ) > projection(&rim[..192], RIM_LOW_HZ));
        assert!(projection(&rim[240..480], RIM_LOW_HZ) > 1.0);
        let clave = render(Kind::Clave, patch(), 2_400);
        assert!(projection(&clave, CLAVE_HZ) > projection(&clave, 455.0) * 4.0);
    }

    #[test]
    fn the_rim_switch_closes_the_section_after_about_ten_milliseconds() {
        let rim = render(Kind::Rim, patch(), 1_200);
        let rms = |range: std::ops::Range<usize>| {
            let slice = &rim[range];
            (slice.iter().map(|x| x * x).sum::<f32>() / slice.len() as f32).sqrt()
        };
        let body = rms(96..432);
        let closed = rms(624..720);
        assert!(
            closed < 0.01 * body,
            "body {body}, after the switch {closed}"
        );
    }

    #[test]
    fn the_rim_waits_for_its_strike_and_then_clips() {
        let rim = render(Kind::Rim, patch(), 480);
        let peak = rim.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        let before = rim[..16].iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(
            before < 0.05 * peak,
            "before the strike {before}, peak {peak}"
        );
    }

    #[test]
    fn unsupported_noise_is_an_exact_no_op() {
        let render_noise = |noise| {
            let mut p = patch();
            p.noise = noise;
            render(Kind::Rim, p, 512)
        };
        assert_eq!(render_noise(-1.0), render_noise(1.0));
    }

    #[test]
    fn reset_is_exact_silence() {
        let mut voice = RimClave::new();
        voice.trigger(1.0, patch());
        let _ = voice.process(Kind::Clave, patch());
        voice.reset();
        assert_eq!(
            voice.process(Kind::Clave, patch()).to_bits(),
            0.0_f32.to_bits()
        );
    }
}
