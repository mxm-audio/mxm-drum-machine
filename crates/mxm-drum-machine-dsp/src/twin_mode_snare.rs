//! Two struck body modes in parallel with a shared-noise wire path.
//!
//! `research:instruments/analogue-drum-machines.md` §3.3 and the service notes establish two
//! bridged-T modes an octave apart, a Tone-controlled mode ratio and a Snappy-controlled noise
//! envelope. The service table names 238 and 476 Hz; the acquired comparison recording (the
//! centre-position unaccented takes, §11.2) rings at 168 and 332 Hz on every Tone position, as a
//! second 808 capture does at 172 and 338 Hz, while a third sits near 255 Hz. The reference follows
//! the recording it is compared against. Constants below were fitted to that recording in the
//! 2026-09-19 A/B pass: body modes by least-squares waveform fitting of the eight-take average,
//! wire noise from the eight takes' differences. Fidelity is unverified against authenticated
//! hardware.

use crate::deep_bridge_kick::PhasorResonator;
use crate::resonator::MIN_SAMPLE_RATE;
use crate::{Coefficient, flush};

/// Body modes: frequency, strike amplitude, strike phase (`amplitude · cos(ωt + phase)`) and
/// amplitude T60. FITTED to the acquired comparison recording in the 2026-09-19 A/B pass; the
/// upper mode starts slightly louder but dies three times faster, so the lower one names the note.
/// A hardware mode-by-mode measurement replaces them.
const LOW_HZ: f32 = 168.3;
const LOW_AMPLITUDE: f64 = 0.470;
const LOW_PHASE: f64 = -1.963;
const LOW_T60: f32 = 0.196;
const HIGH_HZ: f32 = 331.9;
const HIGH_AMPLITUDE: f64 = 0.562;
const HIGH_PHASE: f64 = 3.116;
const HIGH_T60: f32 = 0.062;
/// Level of the 1 ms common trigger at the output, inverted in this voice: the negative step the
/// recording opens with. FITTED to the acquired comparison recording in the 2026-09-19 A/B pass; a
/// measurement of the trigger's path to the output replaces it.
const TRIGGER_FEED: f32 = -0.340;
const TRIGGER_SECONDS: f32 = 0.001;
/// One-pole equivalent of the body's output network. FITTED with the modes in the 2026-09-19 A/B
/// pass; its phase lag is part of the fitted onset. The network's measured response replaces it.
const BODY_LOW_PASS_HZ: f32 = 332.0;
/// Wire noise band: two one-pole high-passes and two trapezoidal one-pole low-passes on the shared
/// white sample. FITTED to the third-octave spectra of the acquired comparison recording's take
/// differences and 20–120 ms window in the 2026-09-19 A/B pass (steep below 2 kHz, broad 2.5–8 kHz,
/// −20 dB at 16 kHz); the snappy filter's measured response replaces them.
const WIRE_HIGH_PASS_HZ: f64 = 1_600.0;
const WIRE_LOW_PASS_HZ: f64 = 8_000.0;
/// Wire envelope: rise time constant, amplitude T60 and level (RMS gain on the band-limited
/// sample). FITTED to the take-difference envelope in the 2026-09-19 A/B pass: at its peak 5–10 ms
/// after the strike, about 7 dB under the body, then about 20 dB per 50 ms. A measurement of the
/// snappy envelope replaces them.
const WIRE_RISE_SECONDS: f32 = 0.002;
const WIRE_T60: f32 = 0.230;
const WIRE_LEVEL: f32 = 0.82;
/// Accent at the catalogue reference velocity (0.82) with zero Dynamics; fitted amplitudes are the
/// recording's scale at this accent.
const REFERENCE_ACCENT: f32 = 0.35 + 0.65 * 0.82;
/// CHOSEN conversion from the fitted recording scale to this renderer's level; the catalogue trim
/// in `engine.rs` owns the absolute level.
const OUTPUT_GAIN: f32 = 0.6;
/// Final emergency bound; reference hits peak near 0.5 and never reach it.
const OUTPUT_BOUND: f32 = 1.0;
const MAX_TAIL_SECONDS: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub pitch_semitones: f32,
    pub decay: f32,
    pub attack: f32,
    pub tone: f32,
    pub body: f32,
    pub noise: f32,
    pub noise_decay: f32,
    pub character: f32,
    pub dynamics: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            decay: 0.0,
            attack: 0.0,
            tone: 0.0,
            body: 0.0,
            noise: 0.0,
            noise_decay: 0.0,
            character: 0.0,
            dynamics: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TwinModeSnare {
    sample_rate: f32,
    /// Held against their inputs rather than rebuilt per sample. The body pole, the wire rise and
    /// the wire band's two coefficients follow the sample rate alone; the wire decay follows the
    /// Noise decay control with it.
    body_pole: Coefficient<f32, f32>,
    wire_rise: Coefficient<f32, f32>,
    wire_band_poles: Coefficient<f32, (f64, f64)>,
    wire_decay: Coefficient<(f32, f32), f32>,
    low: PhasorResonator,
    high: PhasorResonator,
    feed_remaining: u32,
    feed_level: f32,
    body_low_pass: f32,
    noise_envelope: f32,
    noise_rise: f32,
    wire_high: [(f64, f64); 2],
    wire_low: [f64; 2],
    age: u32,
    active: bool,
}

impl Default for TwinModeSnare {
    fn default() -> Self {
        Self::new()
    }
}

impl TwinModeSnare {
    #[must_use]
    pub fn new() -> Self {
        let mut voice = Self {
            sample_rate: 48_000.0,
            body_pole: Coefficient::new(),
            wire_rise: Coefficient::new(),
            wire_band_poles: Coefficient::new(),
            wire_decay: Coefficient::new(),
            low: PhasorResonator::new(),
            high: PhasorResonator::new(),
            feed_remaining: 0,
            feed_level: 0.0,
            body_low_pass: 0.0,
            noise_envelope: 0.0,
            noise_rise: 0.0,
            wire_high: [(0.0, 0.0); 2],
            wire_low: [0.0; 2],
            age: 0,
            active: false,
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

    /// Strikes live resonators; repeated notes add to their ring rather than resetting it.
    pub fn trigger(&mut self, velocity: f32, dynamics: f32, attack: f32, noise: f32) {
        let velocity = finite_unit(velocity);
        let dynamics = finite_bipolar(dynamics);
        let attack = finite_bipolar(attack);
        let exponent = crate::velocity::exponent(dynamics);
        let accent = (0.35 + 0.65 * velocity.powf(exponent)) / REFERENCE_ACCENT;
        let strike = f64::from(accent * (1.0 + 0.35 * attack));
        self.low.strike(strike * LOW_AMPLITUDE, LOW_PHASE);
        self.high.strike(strike * HIGH_AMPLITUDE, HIGH_PHASE);
        // CHOSEN Attack law: Attack raises the strike and the trigger's click together.
        self.feed_level = accent * TRIGGER_FEED * (1.0 + 0.6 * attack);
        self.feed_remaining = (TRIGGER_SECONDS * self.sample_rate).round().max(1.0) as u32;
        self.noise_envelope += accent * (1.0 + finite_bipolar(noise));
        self.noise_envelope = self.noise_envelope.min(3.0);
        self.age = 0;
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, patch: Patch, shared_white_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let ratio = 2.0_f32.powf(pitch / 12.0);
        let decay = body_decay_scale(finite_bipolar(patch.decay));
        let low = self
            .low
            .step(self.sample_rate, LOW_HZ * ratio, LOW_T60 * decay);
        let high = self
            .high
            .step(self.sample_rate, HIGH_HZ * ratio, HIGH_T60 * decay);

        // TONE sets the ratio of the two modes; at ±1 each side moves about 15 dB against the
        // other, matching the recordings' Tone 1 and Tone 9 captures. CHOSEN law.
        let tone = finite_bipolar(patch.tone);
        let body_level = 1.0 + finite_bipolar(patch.body);
        let feed = if self.feed_remaining > 0 {
            self.feed_remaining -= 1;
            self.feed_level
        } else {
            0.0
        };
        let body = body_level * ((1.0 - 0.7 * tone) * low + (1.0 + 0.7 * tone) * high) + feed;
        let body_coefficient = self.body_pole.get(self.sample_rate, |fs| {
            1.0 - (-std::f32::consts::TAU * BODY_LOW_PASS_HZ.min(0.45 * fs) / fs).exp()
        });
        self.body_low_pass =
            flush(self.body_low_pass + body_coefficient * (body - self.body_low_pass));

        // The source sample is shared; these filter and envelope states remain local.
        let band = self.wire_band(finite_bipolar(shared_white_noise));
        let rise = self.wire_rise.get(self.sample_rate, |fs| {
            1.0 - (-1.0 / (WIRE_RISE_SECONDS * fs)).exp()
        });
        self.noise_rise = flush(self.noise_rise + rise * (1.0 - self.noise_rise));
        let wire = band * WIRE_LEVEL * self.noise_envelope * self.noise_rise;
        let wire_time = envelope_time_scale(finite_bipolar(patch.noise_decay));
        let wire_decay = self
            .wire_decay
            .get((wire_time, self.sample_rate), |(wire_time, fs)| {
                (0.001_f32.ln() / (WIRE_T60 * wire_time * fs)).exp()
            });
        self.noise_envelope = flush(self.noise_envelope * wire_decay);

        // Character is unsupported for this catalogue row and must remain an exact no-op.
        let _unsupported_character = finite_bipolar(patch.character);
        let output = (OUTPUT_GAIN * (self.body_low_pass + wire)).clamp(-OUTPUT_BOUND, OUTPUT_BOUND);
        self.age = self.age.saturating_add(1);
        if self.age > (MAX_TAIL_SECONDS * self.sample_rate) as u32
            && self.noise_envelope.abs() < 1.0e-8
            && self.low.magnitude() < 1.0e-8
            && self.high.magnitude() < 1.0e-8
        {
            self.reset();
            return 0.0;
        }
        output
    }

    /// Two one-pole high-passes then two trapezoidal one-pole low-passes, in `f64` for the
    /// high-pass poles. The trapezoidal low-passes' zero at Nyquist keeps the top octave down.
    fn wire_band(&mut self, input: f32) -> f32 {
        let (pole, warped) = self.wire_band_poles.get(self.sample_rate, |sample_rate| {
            let fs = f64::from(sample_rate);
            (
                (-std::f64::consts::TAU * WIRE_HIGH_PASS_HZ.min(0.45 * fs) / fs).exp(),
                (std::f64::consts::PI * WIRE_LOW_PASS_HZ.min(0.45 * fs) / fs).tan(),
            )
        });
        let mut signal = f64::from(input);
        for (previous_input, previous_output) in &mut self.wire_high {
            let output = pole * *previous_output + 0.5 * (1.0 + pole) * (signal - *previous_input);
            *previous_input = signal;
            *previous_output = flush_f64(output);
            signal = *previous_output;
        }
        let gain = warped / (1.0 + warped);
        for state in &mut self.wire_low {
            let v = (signal - *state) * gain;
            let output = v + *state;
            *state = flush_f64(output + v);
            signal = output;
        }
        signal as f32
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.high.reset();
        self.feed_remaining = 0;
        self.feed_level = 0.0;
        self.body_low_pass = 0.0;
        self.noise_envelope = 0.0;
        self.noise_rise = 0.0;
        self.wire_high = [(0.0, 0.0); 2];
        self.wire_low = [0.0; 2];
        self.age = 0;
        self.active = false;
    }
}

/// Decay scales both mode T60s together. The negative end keeps the former 75/180 ms short
/// setting's ratio; the positive range continues to eight times reference, following common
/// extended-decay modifications (`research:instruments/analogue-drum-machines.md` §4.11).
fn body_decay_scale(value: f32) -> f32 {
    if value < 0.0 {
        (0.075_f32 / 0.180).powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}

fn envelope_time_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.25_f32.powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}

fn finite_unit(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
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

    fn body_only() -> Vec<f32> {
        let mut voice = TwinModeSnare::new();
        voice.trigger(0.82, 0.0, 0.0, -1.0);
        (0..9_600)
            .map(|_| {
                voice.process(
                    Patch {
                        noise: -1.0,
                        ..Patch::default()
                    },
                    0.0,
                )
            })
            .collect()
    }

    #[test]
    fn reference_body_contains_both_fitted_modes() {
        let samples = body_only();
        assert!(projection(&samples, LOW_HZ) > 10.0);
        assert!(projection(&samples, HIGH_HZ) > 2.0);
    }

    #[test]
    fn the_upper_mode_dies_first_so_the_lower_names_the_note() {
        let samples = body_only();
        let early = &samples[..960];
        let late = &samples[4_800..9_600];
        let early_ratio = projection(early, HIGH_HZ) / projection(early, LOW_HZ);
        let late_ratio = projection(late, HIGH_HZ) / projection(late, LOW_HZ);
        assert!(early_ratio > 0.3, "early upper/lower {early_ratio}");
        assert!(late_ratio < 0.1, "late upper/lower {late_ratio}");
    }

    #[test]
    fn the_hit_opens_with_the_inverted_trigger_step() {
        let samples = body_only();
        let first = samples[..24].iter().copied().fold(f32::MAX, f32::min);
        let later = samples[48..144].iter().copied().fold(f32::MIN, f32::max);
        assert!(first < 0.0 && later > 0.0, "first {first}, later {later}");
    }

    #[test]
    fn shared_noise_sample_changes_the_wire_path() {
        let render = |noise| {
            let mut voice = TwinModeSnare::new();
            voice.trigger(0.8, 0.0, 0.0, 0.0);
            (0..512)
                .map(|_| voice.process(Patch::default(), noise))
                .collect::<Vec<_>>()
        };
        assert_ne!(render(0.0), render(0.75));
    }

    #[test]
    fn unsupported_character_is_an_exact_no_op() {
        let render = |character| {
            let mut voice = TwinModeSnare::new();
            voice.trigger(0.8, 0.0, 0.0, 0.0);
            (0..512)
                .map(|_| {
                    voice.process(
                        Patch {
                            character,
                            ..Patch::default()
                        },
                        0.25,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(render(-1.0), render(1.0));
    }

    #[test]
    fn reset_is_exact_silence_and_hostile_values_stay_finite() {
        let mut voice = TwinModeSnare::new();
        voice.trigger(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN);
        for _ in 0..96_000 {
            assert!(
                voice
                    .process(
                        Patch {
                            pitch_semitones: f32::INFINITY,
                            ..Patch::default()
                        },
                        f32::NAN
                    )
                    .is_finite()
            );
        }
        voice.reset();
        assert_eq!(
            voice.process(Patch::default(), 0.0).to_bits(),
            0.0_f32.to_bits()
        );
    }
}
