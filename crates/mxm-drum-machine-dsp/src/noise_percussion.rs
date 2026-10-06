//! Shared-white-noise percussion: the short maraca gate and multi-burst handclap.
//!
//! `research:instruments/analogue-drum-machines.md` §3.6 establishes a 25–35 ms high-passed maraca,
//! and a clap with a roughly 1 kHz noise band, clustered comparator-driven attacks and a separate
//! long "reverb" envelope. Both voices consume the one machine-shared white-noise sample supplied by
//! the engine; their filters and envelopes are local.
//!
//! The reference constants were fitted to the acquired comparison recordings (the unaccented
//! takes, §11.2) in the 2026-09-19 A/B pass: envelopes from eight-take power averages at 0.5–20 ms
//! resolution, noise colours from third-octave spectra. The maraca's envelope charges through its
//! whole gate and is cut by a VCA threshold; the clap's bursts arrive at 0, 8.0, 19.5 and 29.5 ms,
//! each longer than the last, through a brighter path than its 1 kHz reverb tail.

use crate::resonator::MIN_SAMPLE_RATE;
use crate::{Coefficient, flush};

/// Accent at the catalogue reference velocity (0.82) with zero Dynamics; fitted levels are the
/// recording's scale at this accent.
const REFERENCE_ACCENT: f32 = 0.35 + 0.65 * 0.82;
/// Final emergency bound for both voices; reference hits stay well inside it.
const OUTPUT_BOUND: f32 = 1.0;

/// Maraca gate length, envelope rise and release time constants, and the VCA threshold on the
/// released envelope. FITTED to the acquired comparison recording in the 2026-09-19 A/B pass: the
/// level climbs as `1 − e^(−t/7 ms)` to a peak at 18.5 ms (the service note's 25–35 ms figure
/// spans the audible event), then falls ever faster to silence 11.5 ms later. A measurement of the
/// maraca envelope capacitor and VCA offset replaces them.
const MARACA_GATE_SECONDS: f32 = 0.018_5;
const MARACA_RISE_SECONDS: f32 = 0.007;
const MARACA_RELEASE_SECONDS: f32 = 0.004;
const MARACA_THRESHOLD: f32 = 0.056;
/// Maraca noise colour: three one-pole high-passes and two trapezoidal one-pole low-passes. FITTED
/// to the recording's third-octave spectrum (peak 10–13 kHz, −50 dB at 1 kHz, −8 dB at 16 kHz) in
/// the 2026-09-19 A/B pass; the maraca filter's measured response replaces them. The gain is a level
/// convention; the catalogue trim owns the absolute level.
const MARACA_HIGH_PASS_HZ: f64 = 8_000.0;
const MARACA_LOW_PASS_HZ: f64 = 11_000.0;
const MARACA_GAIN: f32 = 3.2;

/// Clap burst schedule, per-burst peak level and decay time constant. FITTED to the acquired
/// comparison recording's eight-take envelope in the 2026-09-19 A/B pass. The fourth burst is the
/// final differentiated pulse and lasts about ten times longer. Isolated comparator-envelope
/// measurement replaces them.
const BURST_SECONDS: [f32; 4] = [0.0, 0.008_0, 0.019_5, 0.029_5];
const BURST_LEVELS: [f32; 4] = [2.182, 3.253, 2.907, 2.393];
const BURST_DECAY_SECONDS: [f32; 4] = [0.002_14, 0.002_92, 0.003_72, 0.022_95];
/// Clap reverb path: rise time constant, amplitude T60 of the envelope `R`, and the gains of its
/// `R²` and `√R` terms. FITTED in the 2026-09-19 A/B pass (1 dB RMS error from 0 to 1.8 s): the
/// swing VCA's nonlinear transfer makes the decay slow continuously, about 7.5 dB per 100 ms near
/// 150 ms and 1.7 dB per 100 ms after a second. A measurement of the reverb envelope and its VCA
/// transfer replaces them.
const REVERB_RISE_SECONDS: f32 = 0.023;
const REVERB_T60: f32 = 2.133;
const REVERB_SQUARE_GAIN: f32 = 0.244;
const REVERB_ROOT_GAIN: f32 = 0.022_8;
/// Clap colours. Bursts: three one-pole high-passes at 700 Hz and two one-pole low-passes at 3 kHz,
/// the broad bright spectrum of the switched VCA. Reverb: two cascaded 1.1 kHz band-passes at Q 0.8.
/// FITTED to third-octave spectra of the recording's bursts (0–4 and 20–60 ms) and of 0.06–0.3 s in
/// the 2026-09-19 A/B pass; Q 0.5 matched the tail alone, 0.8 matches it with the fourth burst's
/// decay on top. The clap band-pass's measured response replaces them.
const BURST_HIGH_PASS_HZ: f64 = 700.0;
const BURST_LOW_PASS_HZ: f64 = 3_000.0;
const REVERB_BAND_HZ: f32 = 1_100.0;
const REVERB_BAND_Q: f32 = 0.8;
/// Each burst's sawtooth edge leaks through its swing VCA as a click
/// (`research:instruments/analogue-drum-machines.md` §2.6): an impulse of this size per unit burst
/// level enters a copy of the burst band at 48 kHz, making the first half-millisecond of each burst
/// the loudest, as in the recording. FITTED by eye to the recording's burst onsets in the
/// 2026-09-19 A/B pass; a measurement of the VCA's envelope feed-through replaces it.
const BURST_CLICK: f32 = 2.5;
/// Level convention; the catalogue trim owns the absolute level.
const CLAP_GAIN: f32 = 0.6;

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

#[derive(Debug, Clone)]
pub struct Maraca {
    sample_rate: f32,
    /// The band's high-pass pole and its trapezoidal low-pass warp, held against the rate and the
    /// corner they were built from.
    band_poles: Coefficient<(f32, f64), (f64, f64)>,
    level: f32,
    envelope: f32,
    gate_remaining: u32,
    high_pass: [(f64, f64); 3],
    low_pass: [f64; 2],
    age: u32,
    active: bool,
}

impl Default for Maraca {
    fn default() -> Self {
        Self::new()
    }
}

impl Maraca {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            band_poles: Coefficient::new(),
            level: 0.0,
            envelope: 0.0,
            gate_remaining: 0,
            high_pass: [(0.0, 0.0); 3],
            low_pass: [0.0; 2],
            age: 0,
            active: false,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = valid_sample_rate(sample_rate);
    }

    /// Opens the gate; a retrigger restarts it while the envelope continues from where it is.
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        self.level = accent(velocity, patch.dynamics);
        let gate = scale_decay(MARACA_GATE_SECONDS, finite_bipolar(patch.decay), 0.5, 8.0);
        self.gate_remaining = (gate * self.sample_rate).round().max(1.0) as u32;
        self.age = 0;
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, patch: Patch, shared_white_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let tone = finite_bipolar(patch.tone);
        let decay = finite_bipolar(patch.decay);
        let attack = finite_bipolar(patch.attack);
        // CHOSEN Tone law: the high-pass corners move by a factor of about 1.55 either way.
        let cutoff = f64::from(2.4_f32.powf(0.5 * tone)) * MARACA_HIGH_PASS_HZ;
        let band = self.band(finite_bipolar(shared_white_noise), cutoff);

        if self.gate_remaining > 0 {
            self.gate_remaining -= 1;
            // CHOSEN Attack law: Attack shortens the charging time constant.
            let rise_seconds = MARACA_RISE_SECONDS * 4.0_f32.powf(-attack);
            let rise = 1.0 - (-1.0 / (rise_seconds * self.sample_rate)).exp();
            self.envelope = flush(self.envelope + rise * (self.level - self.envelope));
        } else {
            let release_seconds = scale_decay(MARACA_RELEASE_SECONDS, decay, 0.5, 8.0);
            self.envelope =
                flush(self.envelope * (-1.0 / (release_seconds * self.sample_rate)).exp());
        }
        // The VCA passes only what the envelope holds above its threshold.
        let gain = (self.envelope - MARACA_THRESHOLD).max(0.0) / (1.0 - MARACA_THRESHOLD);
        let noise_gain = 1.0 + finite_bipolar(patch.noise);
        let output = (MARACA_GAIN * band * gain * noise_gain).clamp(-OUTPUT_BOUND, OUTPUT_BOUND);
        self.age = self.age.saturating_add(1);

        // Pitch, Body and Character have no honest mapping for this row and are exact no-ops.
        let _unsupported = (
            finite_or(patch.pitch_semitones, 0.0),
            finite_bipolar(patch.body),
            finite_bipolar(patch.character),
        );
        // Below the threshold the VCA is closed and stays closed until the next trigger.
        if self.gate_remaining == 0 && self.envelope <= MARACA_THRESHOLD {
            self.reset();
            return 0.0;
        }
        output
    }

    fn band(&mut self, input: f32, high_pass_hz: f64) -> f32 {
        let (pole, warped) = self.band_poles.get(
            (self.sample_rate, high_pass_hz),
            |(sample_rate, high_pass_hz)| {
                let fs = f64::from(sample_rate);
                (
                    (-std::f64::consts::TAU * high_pass_hz.min(0.45 * fs) / fs).exp(),
                    (std::f64::consts::PI * MARACA_LOW_PASS_HZ.min(0.45 * fs) / fs).tan(),
                )
            },
        );
        let mut signal = f64::from(input);
        for (previous_input, previous_output) in &mut self.high_pass {
            let output = pole * *previous_output + 0.5 * (1.0 + pole) * (signal - *previous_input);
            *previous_input = signal;
            *previous_output = flush_f64(output);
            signal = *previous_output;
        }
        // Trapezoidal (TPT) one-poles: their zero at Nyquist keeps the top octave down at 48 kHz,
        // which a matched-pole one-pole cannot.
        let gain = warped / (1.0 + warped);
        for state in &mut self.low_pass {
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
        self.level = 0.0;
        self.envelope = 0.0;
        self.gate_remaining = 0;
        self.high_pass = [(0.0, 0.0); 3];
        self.low_pass = [0.0; 2];
        self.age = 0;
        self.active = false;
    }
}

/// `(sample rate, centre, Q)` to the band-pass's `(b0, b2, a1, a2)`.
type HeldBiquad = Coefficient<(f32, f32, f32), (f64, f64, f64, f64)>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BandPass {
    z1: f64,
    z2: f64,
    /// The biquad's coefficients, held against the tuning they were built from. A band whose
    /// centre and Q are steady stops rebuilding a `sin`, a `cos` and four divides per sample.
    coefficients: HeldBiquad,
}

impl BandPass {
    pub(crate) const fn new() -> Self {
        Self {
            z1: 0.0,
            z2: 0.0,
            coefficients: Coefficient::new(),
        }
    }

    /// RBJ Audio EQ Cookbook constant-skirt-gain band-pass, direct form II transposed.
    pub(crate) fn process(
        &mut self,
        input: f32,
        sample_rate: f32,
        frequency_hz: f32,
        q: f32,
    ) -> f32 {
        let (b0, b2, a1, a2) = self.coefficients.get(
            (sample_rate, frequency_hz, q),
            |(sample_rate, frequency_hz, q)| {
                let fs = f64::from(sample_rate);
                let frequency = f64::from(frequency_hz.clamp(20.0, 0.4 * sample_rate));
                let omega = std::f64::consts::TAU * frequency / fs;
                let alpha = omega.sin() / (2.0 * f64::from(q.clamp(0.2, 8.0)));
                let a0 = 1.0 + alpha;
                (
                    alpha / a0,
                    -alpha / a0,
                    -2.0 * omega.cos() / a0,
                    (1.0 - alpha) / a0,
                )
            },
        );
        let output = b0 * f64::from(input) + self.z1;
        self.z1 = -a1 * output + self.z2;
        self.z2 = b2 * f64::from(input) - a2 * output;
        if output.is_finite() && self.z1.is_finite() && self.z2.is_finite() {
            self.z1 = flush_f64(self.z1);
            self.z2 = flush_f64(self.z2);
            output as f32
        } else {
            self.reset();
            0.0
        }
    }

    pub(crate) fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

#[derive(Debug, Clone)]
pub struct PulseClap {
    sample_rate: f32,
    reverb_band: [BandPass; 2],
    burst_high_pass: [(f64, f64); 3],
    burst_low_pass: [f64; 2],
    pending_click: f32,
    click_high_pass: [(f64, f64); 3],
    click_low_pass: [f64; 2],
    burst_envelope: f32,
    burst_decay_seconds: f32,
    reverb_envelope: f32,
    reverb_rise: f32,
    burst_level: f32,
    burst_index: usize,
    age: u32,
    active: bool,
}

impl Default for PulseClap {
    fn default() -> Self {
        Self::new()
    }
}

impl PulseClap {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            reverb_band: [BandPass::new(); 2],
            burst_high_pass: [(0.0, 0.0); 3],
            burst_low_pass: [0.0; 2],
            pending_click: 0.0,
            click_high_pass: [(0.0, 0.0); 3],
            click_low_pass: [0.0; 2],
            burst_envelope: 0.0,
            burst_decay_seconds: BURST_DECAY_SECONDS[0],
            reverb_envelope: 0.0,
            reverb_rise: 0.0,
            burst_level: 0.0,
            burst_index: 0,
            age: 0,
            active: false,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = valid_sample_rate(sample_rate);
    }

    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        // Retrigger restarts the comparator sequence while the prior reverb/noise-filter state lives.
        self.burst_index = 0;
        self.age = 0;
        self.burst_level = level;
        self.reverb_envelope = (self.reverb_envelope + level).min(2.0);
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, patch: Patch, shared_white_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        while self.burst_index < BURST_SECONDS.len()
            && self.age >= (BURST_SECONDS[self.burst_index] * self.sample_rate) as u32
        {
            let level = BURST_LEVELS[self.burst_index] * self.burst_level;
            self.burst_envelope = self.burst_envelope.max(level).min(6.0);
            self.pending_click += BURST_CLICK * level * 48_000.0 / self.sample_rate;
            self.burst_decay_seconds = BURST_DECAY_SECONDS[self.burst_index];
            self.burst_index += 1;
        }

        let tone = finite_bipolar(patch.tone);
        let character = finite_bipolar(patch.character);
        let white = finite_bipolar(shared_white_noise);
        // CHOSEN Tone law: both colours move by 1.8 either way. CHOSEN Character law: it narrows or
        // widens the reverb band.
        let colour = 1.8_f32.powf(tone);
        let burst = self.burst_band(white, f64::from(colour));
        let impulse = std::mem::take(&mut self.pending_click);
        let click = self.burst_click(impulse, f64::from(colour));
        let q = REVERB_BAND_Q * 1.8_f32.powf(character);
        let mut reverb_noise = white;
        for band in &mut self.reverb_band {
            reverb_noise = band.process(reverb_noise, self.sample_rate, REVERB_BAND_HZ * colour, q);
        }

        let attack = finite_bipolar(patch.attack);
        let burst_seconds = self.burst_decay_seconds
            * 1.8_f32.powf(attack)
            * envelope_time_scale(finite_bipolar(patch.noise_decay));
        let burst_decay = (-1.0 / (burst_seconds * self.sample_rate)).exp();
        let reverb_t60 = scale_decay(REVERB_T60, finite_bipolar(patch.decay), 0.35, 8.0);
        let reverb_decay = (0.001_f32.ln() / (reverb_t60 * self.sample_rate)).exp();
        let rise = 1.0 - (-1.0 / (REVERB_RISE_SECONDS * self.sample_rate)).exp();
        self.reverb_rise = flush(self.reverb_rise + rise * (1.0 - self.reverb_rise));
        let reverb = self.reverb_envelope.max(0.0);
        let tail = self.reverb_rise
            * (REVERB_SQUARE_GAIN * reverb * reverb + REVERB_ROOT_GAIN * reverb.sqrt());
        let noise_gain = 1.0 + finite_bipolar(patch.noise);
        let output =
            (CLAP_GAIN * noise_gain * (burst * self.burst_envelope + click + reverb_noise * tail))
                .clamp(-OUTPUT_BOUND, OUTPUT_BOUND);
        self.burst_envelope = flush(self.burst_envelope * burst_decay);
        self.reverb_envelope = flush(self.reverb_envelope * reverb_decay);
        self.age = self.age.saturating_add(1);

        // Pitch and Body are unsupported and exact no-ops.
        let _unsupported = (
            finite_or(patch.pitch_semitones, 0.0),
            finite_bipolar(patch.body),
        );
        if self.age > (1.5 * self.sample_rate) as u32
            && self.burst_envelope.abs() < 1.0e-8
            && self.reverb_envelope.abs() < 1.0e-8
        {
            self.reset();
            return 0.0;
        }
        output
    }

    /// Three one-pole high-passes and two one-pole low-passes, all scaled by the Tone colour.
    fn burst_band(&mut self, input: f32, colour: f64) -> f32 {
        let sample_rate = self.sample_rate;
        colour_band(
            &mut self.burst_high_pass,
            &mut self.burst_low_pass,
            f64::from(input),
            sample_rate,
            colour,
        )
    }

    /// The edge click through its own copy of the burst band, so it stays a separate linear path.
    fn burst_click(&mut self, impulse: f32, colour: f64) -> f32 {
        let sample_rate = self.sample_rate;
        colour_band(
            &mut self.click_high_pass,
            &mut self.click_low_pass,
            f64::from(impulse),
            sample_rate,
            colour,
        )
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        for band in &mut self.reverb_band {
            band.reset();
        }
        self.burst_high_pass = [(0.0, 0.0); 3];
        self.burst_low_pass = [0.0; 2];
        self.pending_click = 0.0;
        self.click_high_pass = [(0.0, 0.0); 3];
        self.click_low_pass = [0.0; 2];
        self.burst_envelope = 0.0;
        self.burst_decay_seconds = BURST_DECAY_SECONDS[0];
        self.reverb_envelope = 0.0;
        self.reverb_rise = 0.0;
        self.burst_level = 0.0;
        self.burst_index = 0;
        self.age = 0;
        self.active = false;
    }
}

/// The clap burst colour: cascaded one-pole high-passes then one-pole low-passes, in `f64`.
fn colour_band(
    high_pass: &mut [(f64, f64); 3],
    low_pass: &mut [f64; 2],
    input: f64,
    sample_rate: f32,
    colour: f64,
) -> f32 {
    let fs = f64::from(sample_rate);
    let pole = (-std::f64::consts::TAU * (BURST_HIGH_PASS_HZ * colour).min(0.45 * fs) / fs).exp();
    let mut signal = input;
    for (previous_input, previous_output) in high_pass.iter_mut() {
        let output = pole * *previous_output + 0.5 * (1.0 + pole) * (signal - *previous_input);
        *previous_input = signal;
        *previous_output = flush_f64(output);
        signal = *previous_output;
    }
    let low =
        1.0 - (-std::f64::consts::TAU * (BURST_LOW_PASS_HZ * colour).min(0.45 * fs) / fs).exp();
    for state in low_pass.iter_mut() {
        *state = flush_f64(*state + low * (signal - *state));
        signal = *state;
    }
    signal as f32
}

/// Accent relative to the catalogue reference, so the reference hit is the fitted level.
fn accent(velocity: f32, dynamics: f32) -> f32 {
    let velocity = finite_or(velocity, 0.0).clamp(0.0, 1.0);
    let dynamics = finite_bipolar(dynamics);
    let exponent = crate::velocity::exponent(dynamics);
    (0.35 + 0.65 * velocity.powf(exponent)) / REFERENCE_ACCENT
}

fn scale_decay(reference: f32, value: f32, short: f32, long: f32) -> f32 {
    if value < 0.0 {
        reference * short.powf(-value)
    } else {
        reference * long.powf(value)
    }
}

fn envelope_time_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.25_f32.powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}

fn valid_sample_rate(value: f32) -> f32 {
    if value.is_finite() {
        value.max(MIN_SAMPLE_RATE)
    } else {
        48_000.0
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
            noise_decay: 0.0,
            character: 0.0,
            dynamics: 0.0,
        }
    }

    fn deterministic_noise(state: &mut u64) -> f32 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        2.0 * ((*state >> 40) as u32) as f32 / ((1_u32 << 24) - 1) as f32 - 1.0
    }

    fn window_energy(samples: &[f32], start: usize, end: usize) -> f32 {
        samples[start..end].iter().map(|x| x * x).sum::<f32>()
    }

    #[test]
    fn maraca_rises_through_its_gate_and_unsupported_axes_are_no_ops() {
        let render = |pitch, body, character| {
            let mut voice = Maraca::new();
            let mut p = patch();
            p.pitch_semitones = pitch;
            p.body = body;
            p.character = character;
            voice.trigger(0.82, p);
            let mut state = 1_u64;
            (0..2_400)
                .map(|_| voice.process(p, deterministic_noise(&mut state)))
                .collect::<Vec<_>>()
        };
        let reference = render(-24.0, -1.0, -1.0);
        assert_eq!(reference, render(24.0, 1.0, 1.0));
        // The recording's level climbs through the gate: the millisecond before 18 ms is far
        // louder than the first, and the hit is silent by 30 ms.
        let first = window_energy(&reference, 0, 48);
        let late_gate = window_energy(&reference, 816, 864);
        let after = window_energy(&reference, 1_488, 2_400);
        assert!(
            late_gate > 20.0 * first,
            "first {first}, late gate {late_gate}"
        );
        assert_eq!(after, 0.0);
    }

    #[test]
    fn clap_bursts_follow_the_fitted_schedule_and_the_reverb_lasts() {
        let mut voice = PulseClap::new();
        voice.trigger(0.82, patch());
        let mut state = 1_u64;
        let samples: Vec<_> = (0..9_600)
            .map(|_| voice.process(patch(), deterministic_noise(&mut state)))
            .collect();
        // Each burst's first millisecond is far louder than the millisecond before it.
        for burst in &BURST_SECONDS[1..] {
            let at = (burst * 48_000.0) as usize;
            let before = window_energy(&samples, at - 72, at - 24);
            let after = window_energy(&samples, at, at + 48);
            assert!(
                after > 10.0 * before,
                "burst at {burst} s: {before} then {after}"
            );
        }
        let continued_tail: f32 = (0..48_000)
            .map(|_| voice.process(patch(), deterministic_noise(&mut state)))
            .map(|sample| sample * sample)
            .sum();
        assert!(continued_tail > 1.0e-6, "missing long reverb path");
    }

    #[test]
    fn clap_pitch_and_body_are_exact_no_ops() {
        let render = |pitch, body| {
            let mut voice = PulseClap::new();
            let mut p = patch();
            p.pitch_semitones = pitch;
            p.body = body;
            voice.trigger(0.8, p);
            let mut state = 9_u64;
            (0..2_400)
                .map(|_| voice.process(p, deterministic_noise(&mut state)))
                .collect::<Vec<_>>()
        };
        assert_eq!(render(-24.0, -1.0), render(24.0, 1.0));
    }

    #[test]
    fn reset_is_exact_silence_and_hostile_values_are_finite() {
        let hostile = Patch {
            pitch_semitones: f32::NAN,
            decay: f32::INFINITY,
            attack: f32::NEG_INFINITY,
            tone: f32::NAN,
            body: f32::INFINITY,
            noise: f32::NEG_INFINITY,
            noise_decay: f32::NAN,
            character: f32::NAN,
            dynamics: f32::INFINITY,
        };
        let mut maraca = Maraca::new();
        maraca.trigger(f32::NAN, hostile);
        assert!(maraca.process(hostile, f32::NAN).is_finite());
        maraca.reset();
        assert_eq!(maraca.process(patch(), 0.5).to_bits(), 0.0_f32.to_bits());
        let mut clap = PulseClap::new();
        clap.trigger(f32::NAN, hostile);
        assert!(clap.process(hostile, f32::NAN).is_finite());
        clap.reset();
        assert_eq!(clap.process(patch(), 0.5).to_bits(), 0.0_f32.to_bits());
    }
}
