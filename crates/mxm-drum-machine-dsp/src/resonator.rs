//! Struck two-pole resonator substrate.
//!
//! This is not a generic "analogue drum" voice. It is the exact discrete-time primitive several
//! admitted circuits can parameterise after their own strike, nonlinear and output networks are
//! derived. A model owns those laws and constants; this module owns only the stable pole pair.

use std::f64::consts::TAU;

/// Lowest sample rate accepted by the DSP crate.
pub const MIN_SAMPLE_RATE: f32 = 1_000.0;
/// Frequencies stay below this fraction of sample rate so the pole angle remains away from Nyquist.
pub const MAX_FREQUENCY_FRACTION: f32 = 0.45;
/// Recursive radius below one by this margin, even when an extreme decay is requested.
const RADIUS_CEILING: f64 = 1.0 - 1.0e-9;

/// A real second-order resonator represented by a complex-conjugate pole pair.
///
/// The recurrence is
/// `y[n] = x[n] + 2 r cos(ω) y[n-1] - r² y[n-2]`.
/// Frequency and exponential T60 become the pole angle and radius directly, avoiding a coefficient
/// fit. A model still supplies its actual excitation and output scaling.
#[derive(Debug, Clone, Copy)]
pub struct Resonator {
    // Recursive coefficients and state are f64: at low drum frequencies an f32 cosine rounds
    // enough pole angle away to move tuning measurably, and the error compounds for the whole tail.
    a1: f64,
    a2: f64,
    /// `sin(ω)`: numerator for a unit-scale damped sinus rather than the raw all-pole gain.
    strike_scale: f64,
    y1: f64,
    y2: f64,
}

impl Default for Resonator {
    fn default() -> Self {
        Self::new()
    }
}

impl Resonator {
    /// Silent resonator with inert coefficients.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a1: 0.0,
            a2: 0.0,
            strike_scale: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Sets the pole pair from centre frequency and exponential T60.
    ///
    /// `radius^(T60·fs) = 0.001`, so `radius = exp(ln(0.001)/(T60·fs))`. Invalid values recover to
    /// bounded defaults rather than poisoning recursive state. Per-model ranges are narrower and
    /// remain the model's responsibility.
    pub fn configure(&mut self, sample_rate: f32, frequency_hz: f32, t60_seconds: f32) {
        let fs = finite_or(sample_rate, 48_000.0).max(MIN_SAMPLE_RATE) as f64;
        let frequency =
            finite_or(frequency_hz, 100.0).clamp(0.0, MAX_FREQUENCY_FRACTION * fs as f32) as f64;
        let t60 = finite_or(t60_seconds, 0.1).clamp(1.0 / fs as f32, 60.0) as f64;
        let radius = (0.001_f64.ln() / (t60 * fs)).exp().min(RADIUS_CEILING);
        let angle = TAU * frequency / fs;
        self.a1 = 2.0 * radius * angle.cos();
        self.a2 = -(radius * radius);
        self.strike_scale = angle.sin().abs();
    }

    /// Processes one physical strike input with pole-angle-normalised gain.
    ///
    /// The raw all-pole recurrence's impulse response is
    /// `rⁿ sin((n+1)ω) / sin(ω)`: at a 56 Hz drum pole its gain is about 136 before the circuit has
    /// supplied any gain at all. Multiplying the input by `sin(ω)` removes that numerical artefact,
    /// leaving a unit-scale damped sinus for the model's pulse and output networks to calibrate.
    /// This is the numerator of the resonator primitive, not output normalisation: retriggers still
    /// add to whatever state is live and can overload a model's later stages.
    #[inline]
    #[must_use]
    pub fn process_strike(&mut self, excitation: f32) -> f32 {
        let scaled = f64::from(excitation) * self.strike_scale;
        self.process_f64(scaled)
    }

    /// Processes one raw all-pole excitation sample.
    ///
    /// Models should ordinarily use [`Self::process_strike`]. This raw form remains public because
    /// its analytic impulse response is the direct pole-pair control used by the substrate tests.
    /// It is not a level-calibrated circuit output.
    #[inline]
    #[must_use]
    pub fn process(&mut self, excitation: f32) -> f32 {
        let input = if excitation.is_finite() {
            f64::from(excitation)
        } else {
            0.0
        };
        self.process_f64(input)
    }

    #[inline]
    fn process_f64(&mut self, input: f64) -> f32 {
        let input = if input.is_finite() { input } else { 0.0 };
        let output = input + self.a1 * self.y1 + self.a2 * self.y2;
        if !output.is_finite() {
            // A non-finite recursive state must not persist into the host's next block.
            self.reset();
            return 0.0;
        }
        let output = if output.abs() < 1.0e-20 { 0.0 } else { output };
        self.y2 = self.y1;
        self.y1 = output;
        output as f32
    }

    /// Clears all energy while preserving coefficients.
    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    /// Whether the recursive state is exact digital silence.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.y1 == 0.0 && self.y2 == 0.0
    }
}

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossing_frequency(samples: &[f32], sample_rate: f32) -> f32 {
        let mut crossings = Vec::new();
        for index in 1..samples.len() {
            if samples[index - 1] <= 0.0 && samples[index] > 0.0 {
                let before = samples[index - 1];
                let after = samples[index];
                let fraction = -before / (after - before);
                crossings.push(index as f32 - 1.0 + fraction);
            }
        }
        let periods = crossings.len().saturating_sub(1);
        sample_rate * periods as f32 / (crossings[periods] - crossings[0])
    }

    #[test]
    fn configured_poles_render_the_requested_frequency() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for frequency in [55.0, 100.0, 440.0, 4_000.0] {
                let mut resonator = Resonator::new();
                resonator.configure(sample_rate, frequency, 2.0);
                let mut rendered = vec![0.0; (sample_rate * 2.1) as usize];
                rendered[0] = resonator.process(1.0);
                for sample in &mut rendered[1..] {
                    *sample = resonator.process(0.0);
                }
                let measured = zero_crossing_frequency(&rendered[32..], sample_rate);
                let cents = 1200.0 * (measured / frequency).log2().abs();
                assert!(
                    cents < 0.02,
                    "{sample_rate} Hz, {frequency} Hz: {cents} cents"
                );
            }
        }
    }

    #[test]
    fn strike_input_has_unit_scale_instead_of_raw_all_pole_gain() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for frequency in [55.0, 100.0, 440.0, 4_000.0] {
                let mut resonator = Resonator::new();
                resonator.configure(sample_rate, frequency, 2.0);
                let mut peak = resonator.process_strike(1.0).abs();
                for _ in 1..(sample_rate / frequency * 2.0) as usize {
                    peak = peak.max(resonator.process_strike(0.0).abs());
                }
                assert!(
                    (0.98..=1.001).contains(&peak),
                    "{sample_rate} Hz, {frequency} Hz: normalized peak {peak}"
                );
            }
        }
    }

    #[test]
    fn t60_is_the_requested_sixty_decibel_decay() {
        let sample_rate = 48_000.0;
        let t60 = 0.5;
        let mut resonator = Resonator::new();
        resonator.configure(sample_rate, 100.0, t60);
        let window = (0.05 * sample_rate) as usize;
        let start_at = (0.05 * sample_rate) as usize;
        let end_at = start_at + (t60 * sample_rate) as usize;
        let mut start_energy = 0.0_f32;
        let mut end_energy = 0.0_f32;
        for index in 0..end_at + window {
            let sample = resonator.process(if index == 0 { 1.0 } else { 0.0 });
            if (start_at..start_at + window).contains(&index) {
                start_energy += sample * sample;
            }
            if (end_at..end_at + window).contains(&index) {
                end_energy += sample * sample;
            }
        }
        let decay_db = 10.0 * (end_energy / start_energy).log10();
        assert!((decay_db + 60.0).abs() < 0.2, "{decay_db} dB");
    }

    #[test]
    fn reset_and_invalid_inputs_recover_to_exact_silence() {
        let mut resonator = Resonator::new();
        resonator.configure(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);
        assert!(resonator.process(f32::NAN).is_finite());
        for _ in 0..128 {
            assert!(resonator.process(0.0).is_finite());
        }
        resonator.reset();
        assert!(resonator.is_silent());
        assert_eq!(resonator.process(0.0).to_bits(), 0.0_f32.to_bits());
    }
}
