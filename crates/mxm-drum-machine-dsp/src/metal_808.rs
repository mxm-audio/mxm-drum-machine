//! Free-running six-square metallic bank and its four TR-808-family consumers.
//!
//! `research:instruments/analogue-drum-machines.md` §§3.7–3.9 establishes six nominal oscillators
//! at 205.3, 369.6, 304.4, 522.7, 800 and 540 Hz with about 47.98% duty. Cowbell reads oscillators
//! 5/6 directly; cymbal and hats share filtered combinations, and closed hat chokes only the live
//! open envelope while bank phase continues.
//!
//! The oscillator frequencies and duties, filters, swing-VCA laws, mixes and envelopes below were
//! fitted to the acquired comparison recordings (the unaccented takes, §11.2) in the 2026-09-19 A/B
//! pass: this unit's oscillators from the cymbal tails' spectral lines, filters by third-octave
//! spectra of this bank rendered through each candidate chain, VCA laws by envelope periodicity,
//! envelopes from eight-take band-power averages. The squares are band-limited, as the analogue
//! oscillators are. The cymbal's three paths are the shared 7.1 kHz band through two different
//! VCAs plus the lower 3.2 kHz band, each under its own envelope, as §3.8 describes. VCA curves
//! remain behavioural and hardware-unverified.

use crate::noise_percussion::BandPass;
use crate::resonator::MIN_SAMPLE_RATE;
use crate::{Coefficient, flush};

/// Oscillator frequencies. Oscillators 5 and 6 are the two trimmed ones, MEASURED from the acquired
/// comparison recording's cowbell in the 2026-09-19 A/B pass (factory nominal 800 and 540 Hz).
/// Oscillators 1–4 are this unit's untrimmed values, FITTED in the same pass to the spectral lines of
/// three long cymbal tails (1–9 kHz, 2 s windows): these four fundamentals and the two trimmed ones
/// explain 96% of the tails' line power against 48% for §3.8's nominal 205.3, 369.6, 304.4 and
/// 522.7 Hz, and they reproduce the recordings' 3.6 and 5.4 ms envelope periodicity. The cymbal
/// takes run 0.3% sharp of the cowbell take, so they are scaled to the cowbell's temperature. A
/// frequency count of a hardware bank replaces them.
const FREQUENCIES: [f64; 6] = [245.17, 420.76, 339.79, 565.30, 824.65, 552.12];
/// Duty cycles, FITTED in the 2026-09-19 A/B pass: 44% for oscillators 1–4 from the cymbal tails'
/// odd/even line balance, 46% for 5 and 6 from the cowbell's, where they are heard alone. §3.8
/// calculates 47.98% for nominal thresholds; this unit's Schmitt thresholds evidently differ. A
/// scope measurement of the oscillators replaces them.
/// Kept as the source the fixed-point table below is derived from and checked against, so the
/// evidence above stays attached to a number a reader can verify rather than to a bare integer.
#[cfg(test)]
const DUTY: [f64; 6] = [0.44, 0.44, 0.44, 0.44, 0.46, 0.46];
/// One cycle in the fixed-point phase, held wide so a wrap is a value rather than a lost carry.
const CYCLE_WIDE: u128 = 1_u128 << 64;
/// One cycle as a float, for converting a frequency into a fixed-point increment.
const CYCLE_FLOAT: f64 = 18_446_744_073_709_551_616.0;
/// The duty points in fixed point, so the per-sample comparison is an integer one.
///
/// 44% for oscillators 1–4 and 46% for 5 and 6, FITTED in the 2026-09-19 A/B pass — the evidence
/// is on `DUTY`, which these are computed from rather than transcribed. Hand-typing them was
/// tried and got two of the six wrong in the low digits, which is exactly the kind of error that
/// moves an edge without looking like anything.
const DUTY_FIXED: [u64; 6] = [
    fixed(0.44),
    fixed(0.44),
    fixed(0.44),
    fixed(0.44),
    fixed(0.46),
    fixed(0.46),
];
/// The startup spread in fixed point: the same CHOSEN fractions as `MetalBank::START_PHASES`.
const START_PHASES_FIXED: [u64; 6] = [
    fixed(0.03),
    fixed(0.19),
    fixed(0.37),
    fixed(0.53),
    fixed(0.71),
    fixed(0.89),
];

/// A fraction of a cycle as the fixed-point phase counts it.
const fn fixed(fraction: f64) -> u64 {
    (fraction * CYCLE_FLOAT) as u64
}

/// Metal accent at the catalogue reference velocity (0.82) with zero Dynamics.
const REFERENCE_ACCENT: f32 = 0.48 + 0.52 * 0.82;

/// Cowbell: oscillator 6's share of the mix, the band-pass after it, and the envelope. FITTED to the
/// acquired comparison recording in the 2026-09-19 A/B pass: the line spectrum (825 Hz dominant,
/// 552 Hz 16 dB down, harmonics 20–35 dB down) within 2.4 dB RMS, and the eight-take envelope within
/// 0.6 dB RMS: a 1.4 ms rise, the "deliberately abrupt first drop" as an 82% component with a
/// 6.7 ms time constant, and a 78 ms body. Measurements of the cowbell VCAs, filter and envelope
/// replace them; the gain is a level convention the catalogue trim overrides.
const COWBELL_LOW_SHARE: f32 = 0.45;
const COWBELL_BAND_HZ: f32 = 1_062.0;
const COWBELL_BAND_Q: f32 = 2.0;
const COWBELL_RISE_SECONDS: f32 = 0.001_43;
const COWBELL_FAST_SHARE: f32 = 0.819;
const COWBELL_FAST_SECONDS: f32 = 0.006_7;
const COWBELL_BODY_SECONDS: f32 = 0.077_5;
const COWBELL_GAIN: f32 = 0.9;

/// Cymbal paths (§3.8): the lower 3.2 kHz band, linear, and two paths from the shared 7.1 kHz band,
/// one through a saturating swing VCA and one through a half-wave one, each followed by its own
/// high-pass. The filters are FITTED to the recording's late sixth-octave spectrum and to the hats
/// below; the upper path's third-order high-pass is §3.8's.
const CYMBAL_LOW_BAND: BandSpec = BandSpec {
    centre: 3_200.0,
    q: 4.0,
    vca: Vca::Linear,
    high_pass: 1_200.0,
    order: 3,
};
const CYMBAL_MIDDLE_BAND: BandSpec = OPEN_BAND;
const CYMBAL_HIGH_BAND: BandSpec = BandSpec {
    centre: 7_100.0,
    q: 5.0,
    vca: Vca::HalfWave,
    high_pass: 8_000.0,
    order: 3,
};
/// Cymbal path gains and envelopes. FITTED jointly in the 2026-09-19 A/B pass: the three paths'
/// band powers through this bank, summed, against the recording's eight-take band powers in five
/// bands from 0.5 to 14 kHz at eighteen times from 1 ms to 2.2 s, within 0.9 dB RMS. The lower path
/// rises over about 80 ms and closes at a VCA threshold near 2.2 s; the middle and upper paths start
/// quickly and decay in two slopes, the upper fastest. Each envelope is
/// `rise · (share·e₁ + (1 − share)·e₂) − threshold`, with times as amplitude T60s. Measurements of
/// the three envelope generators and swing VCAs replace them.
const CYMBAL_LOW: PathEnvelope = PathEnvelope {
    gain: 10.8,
    fast_rise: 0.000_1,
    slow_rise: 0.076_6,
    fast_rise_share: 0.014,
    first_share: 0.811,
    first_t60: 0.042_3,
    second_t60: 5.934,
    threshold: 0.011_5,
};
const CYMBAL_MIDDLE: PathEnvelope = PathEnvelope {
    gain: 0.884,
    fast_rise: 0.050,
    slow_rise: 0.011_4,
    fast_rise_share: 0.388,
    first_share: 0.860,
    first_t60: 0.519,
    second_t60: 2.729,
    threshold: 0.0,
};
const CYMBAL_HIGH: PathEnvelope = PathEnvelope {
    gain: 1.517,
    fast_rise: 0.000_77,
    slow_rise: 0.178,
    fast_rise_share: 1.0,
    first_share: 0.772,
    first_t60: 0.192_8,
    second_t60: 1.594,
    threshold: 0.0,
};
const CYMBAL_OUTPUT_GAIN: f32 = 0.5;

/// Hat paths. Both hats take §3.9's shared 7.1 kHz band, at Q 5; the open hat's swing VCA
/// saturates softly and the closed hat's passes one polarity, whose rectified edges put its energy
/// above 10 kHz; each then has its own high-pass. FITTED in the 2026-09-19 A/B pass to the
/// recordings' third-octave spectra through this bank (1.3–1.6 dB mean error) and to their envelope
/// statistics: the linear filters the first pass used left the closed hat in dense bursts between
/// near-silent gaps (envelope autocorrelation 0.37, variation 0.65, against 0.18 and 0.43 in the
/// recording), which the rectifying VCA brings to 0.24 and 0.49. Transfer measurements of the hat
/// VCAs and filters replace them.
const OPEN_BAND: BandSpec = BandSpec {
    centre: 7_100.0,
    q: 5.0,
    vca: Vca::Saturating,
    high_pass: 6_000.0,
    order: 2,
};
const CLOSED_BAND: BandSpec = BandSpec {
    centre: 7_100.0,
    q: 5.0,
    vca: Vca::HalfWave,
    high_pass: 7_500.0,
    order: 3,
};
/// RMS of the shared 7.1 kHz band of this bank's sum, which the nonlinear VCAs see as their unit
/// input level. MEASURED from this bank rendered at 48 kHz; the VCA laws above were fitted at it.
const SHARED_BAND_RMS: f32 = 0.031_1;
/// Drive of the saturating swing VCA at that unit level, `tanh(g·x)`. FITTED with the hat paths.
const SATURATING_VCA_DRIVE: f32 = 0.7;
/// Closed-hat amplitude T60. MEASURED from the recording's eight-take envelope (133 ms early,
/// 112 ms after 20 ms) in the 2026-09-19 A/B pass; a hardware envelope capture replaces it.
const CLOSED_T60: f32 = 0.115;
/// Open-hat envelope: amplitude T60, and the VCA's closing threshold and the width over which it
/// fades, both relative to the strike level. MEASURED from the recording's eight-take envelope in
/// the 2026-09-19 A/B pass: about 6 dB lost over 250 ms, then a linear-amplitude fade from 320 ms
/// to silence at about 360 ms rather than the exponential's tail. The Decay control scales the T60,
/// so the whole shape keeps its proportions across the measured 110–760 ms endpoints. A measurement
/// of the open-hat envelope and VCA threshold replaces them; the gain is a level convention.
const OPEN_T60: f32 = 2.7;
const OPEN_CLOSE_THRESHOLD: f32 = 0.4002;
const OPEN_CLOSE_WIDTH: f32 = 0.065;
const HAT_GAIN: f32 = 4.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    oscillators: [f32; 6],
}

impl Frame {
    /// Linear crossfade toward `other`, oscillator by oscillator.
    ///
    /// Used where a slot moves between this machine's shared bank and its own private one: the two
    /// banks are the same six oscillators on unrelated free phases, so switching between them on
    /// one sample is a step in every partial at once.
    #[must_use]
    pub fn blend(self, other: Self, amount: f32) -> Self {
        let mut oscillators = self.oscillators;
        for (slot, target) in oscillators.iter_mut().zip(other.oscillators) {
            *slot += (target - *slot) * amount;
        }
        Self { oscillators }
    }

    fn sum(self) -> f32 {
        self.oscillators.iter().sum::<f32>() / 6.0
    }

    fn cowbell(self) -> f32 {
        self.oscillators[4] + COWBELL_LOW_SHARE * self.oscillators[5]
    }
}

#[derive(Debug, Clone)]
pub struct MetalBank {
    /// Phase as a fraction of a cycle in fixed point: the whole `u64` range is one cycle.
    ///
    /// **Integer, not `f64`, so a dormant bank can be advanced exactly in one multiply.** Wrapping
    /// *is* the cycle, so `phase + increment × samples` is the same arithmetic the per-sample loop
    /// does and needs no rounding — where an `f64` phase rounds once per sample and cannot be
    /// jumped without drifting from what stepping would have produced (`jump.rs`). It is also the
    /// finer representation: 2⁻⁶⁴ of a cycle against `f64`'s 2⁻⁵³ near 1.0.
    ///
    /// Measured against the previous `f64` accumulation over four seconds of the whole bank, the
    /// difference is −179 dBFS at worst and −201 dBFS RMS: 35 dB below a 24-bit LSB, so it is
    /// inaudible and unrepresentable in the format a pack is written in. The PolyBLEP is what
    /// keeps it that small — an edge moves continuously with the phase, so a phase error produces
    /// an error of the same order rather than flipping a sample between +1 and −1.
    phases: [u64; 6],
    /// Per oscillator, samples n−2 … n+2 of the band-limited square under construction.
    pending: [[f64; 5]; 6],
    /// The six phase increments, held against the rate and deviation they were built from. This
    /// bank is the machine-shared one and `Engine::process_routed` ticks it on every rendered
    /// sample whatever is loaded, so it is an always-on path: a `powf` and six divides here are
    /// paid by an empty patch.
    increments: Coefficient<(f32, f32), [u64; 6]>,
}

impl Default for MetalBank {
    fn default() -> Self {
        Self::new()
    }
}

impl MetalBank {
    /// CHOSEN deterministic startup spread; the hardware is free-running and has no reset phase.
    ///
    /// Kept beside `START_PHASES_FIXED`, which is what the bank actually runs on, and checked
    /// against it so the readable fractions remain the source of truth.
    #[cfg(test)]
    const START_PHASES: [f64; 6] = [0.03, 0.19, 0.37, 0.53, 0.71, 0.89];

    /// The six phase increments in fixed point, for this rate and deviation.
    ///
    /// Held against the values they were built from, as before: the conversion is a `powf` and
    /// six divides, which an always-on bus must not repeat every sample.
    fn increments(&mut self, sample_rate: f32, pitch_semitones: f32) -> [u64; 6] {
        self.increments
            .get((sample_rate, pitch_semitones), |(sample_rate, pitch)| {
                let fs = f64::from(valid_sample_rate(sample_rate));
                let ratio =
                    f64::from(2.0_f32.powf(finite_or(pitch, 0.0).clamp(-24.0, 24.0) / 12.0));
                std::array::from_fn(|index| {
                    let cycles = (FREQUENCIES[index] * ratio / fs).min(0.25);
                    (cycles * CYCLE_FLOAT) as u64
                })
            })
    }

    #[must_use]
    pub const fn new() -> Self {
        Self {
            phases: START_PHASES_FIXED,
            pending: [[0.0; 5]; 6],
            increments: Coefficient::new(),
        }
    }

    /// Advances the six squares one sample and returns their band-limited values.
    ///
    /// Each square is the trivial ±1 pulse at its duty with both edges repaired by the four-point
    /// (third-order B-spline) PolyBLEP residual of Välimäki, Pekonen and Nam, "Perceptually informed
    /// synthesis of bandlimited classical waveforms using integrated polynomial interpolation"
    /// (JASA 2012, Table VII), as mxm-kit's `docs/oscillators/02-antialiasing.md` §2.6.2 gives it.
    /// The trivial squares folded partials back into the hats' 5–14 kHz band only 9.6 dB under
    /// the true lines; the four-point residual puts them 45 dB under, at 1.7 dB of top-octave
    /// droop. Correcting two samples ahead of each edge delays the bank by two samples, which a
    /// free-running source cannot show.
    #[inline]
    #[must_use]
    pub fn tick(&mut self, sample_rate: f32, pitch_semitones: f32) -> Frame {
        let increments = self.increments(sample_rate, pitch_semitones);
        let mut oscillators = [0.0; 6];
        for index in 0..6 {
            let duty = DUTY_FIXED[index];
            let increment = increments[index];
            let phase = self.phases[index];
            let pending = &mut self.pending[index];
            pending[2] += if phase < duty { 1.0 } else { -1.0 };
            // Held wide so the wrap is a value to compare against rather than a lost carry.
            let next = u128::from(phase) + u128::from(increment);
            // The rising edge at the wrap and the falling edge at the duty point, each a step of 2.
            for (edge, height) in [(u128::from(duty), -2.0), (CYCLE_WIDE, 2.0)] {
                if u128::from(phase) < edge && edge <= next {
                    let residual = blep4((next - edge) as f64 / increment as f64);
                    for (slot, value) in pending[1..].iter_mut().zip(residual) {
                        *slot += height * value;
                    }
                }
            }
            oscillators[index] = pending[0] as f32;
            pending.rotate_left(1);
            pending[4] = 0.0;
            self.phases[index] = phase.wrapping_add(increment);
        }
        Frame { oscillators }
    }

    /// Moves the bank to where running would have left it, without rendering (plan §4.4).
    ///
    /// A gated bus must resume free-running, not restart: the hardware's oscillators keep going
    /// while powered and a trigger opens an envelope rather than resetting them. Reseeding would
    /// lose exactly what makes the bank worth sharing — the phase relationship between the
    /// cowbell's two squares and the cymbal's six, and the correlation between voices struck
    /// together.
    ///
    /// **Bit-identical to ticking, and that is the whole point.** The phase runs the same
    /// `fract(phase + increment)` in the same order, so it lands on the same bits; a closed-form
    /// `phase + increment × n` would not, which `jump.rs` documents and tests. What is skipped is
    /// the band-limiting work — the edge residuals and the rolling window — which is most of the
    /// cost and none of the state that survives.
    ///
    /// The window is rebuilt rather than carried: a residual lives at most four samples before it
    /// rotates out, so a gap of five or more leaves nothing of it, and replaying the last few
    /// samples in full restores it exactly.
    pub fn advance(&mut self, samples: u64, sample_rate: f32, pitch_semitones: f32) {
        /// The rolling window's width: after this many samples nothing written before the gap
        /// survives, which is what lets the window be cleared and rebuilt.
        const WINDOW: u64 = 5;

        if samples == 0 {
            return;
        }
        // Short gaps carry live residuals, so there is nothing to skip and nothing to rebuild.
        if samples < WINDOW {
            for _ in 0..samples {
                let _ = self.tick(sample_rate, pitch_semitones);
            }
            return;
        }

        let increments = self.increments(sample_rate, pitch_semitones);
        // **One multiply per oscillator, whatever the gap.** Wrapping is the cycle, so this is
        // the same arithmetic the per-sample loop performs, just all at once — which is exactly
        // what an `f64` phase could not offer.
        let skipped = samples - WINDOW;
        for (phase, increment) in self.phases.iter_mut().zip(increments) {
            *phase = phase.wrapping_add(increment.wrapping_mul(skipped));
        }
        self.pending = [[0.0; 5]; 6];
        for _ in 0..WINDOW {
            let _ = self.tick(sample_rate, pitch_semitones);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// The four-point B-spline PolyBLEP residual for a step whose next sample trails it by `d` of a
/// sample: corrections for the samples two before to one after that sample.
#[inline]
fn blep4(d: f64) -> [f64; 4] {
    let d2 = d * d;
    let d3 = d2 * d;
    let d4 = d2 * d2;
    [
        d4 / 24.0,
        -d4 / 8.0 + d3 / 6.0 + d2 / 4.0 + d / 6.0 + 1.0 / 24.0,
        d4 / 8.0 - d3 / 3.0 + 2.0 * d / 3.0 - 1.0 / 2.0,
        -d4 / 24.0 + d3 / 6.0 - d2 / 4.0 + d / 6.0 - 1.0 / 24.0,
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub decay: f32,
    pub attack: f32,
    pub tone: f32,
    pub body: f32,
    pub noise: f32,
    pub character: f32,
    pub dynamics: f32,
}

/// The swing VCA's transfer on its band (§2.6): the audio chops or steers the envelope, so it is not
/// a transparent multiplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vca {
    Linear,
    /// Soft saturation of the band at `SATURATING_VCA_DRIVE` per unit band level.
    Saturating,
    /// Conducts on one polarity of the band only.
    HalfWave,
}

/// A metal band as a hat or cymbal path takes it: band-pass, swing VCA, then its own high-pass.
#[derive(Debug, Clone, Copy)]
struct BandSpec {
    centre: f32,
    q: f32,
    vca: Vca,
    high_pass: f64,
    order: usize,
}

#[derive(Debug, Clone, Copy)]
struct MetalBand {
    band: BandPass,
    high_pass: [(f64, f64); 3],
}

impl MetalBand {
    const fn new() -> Self {
        Self {
            band: BandPass::new(),
            high_pass: [(0.0, 0.0); 3],
        }
    }

    /// Band-passes `input`, applies the VCA law, multiplies by `gain` (the envelope) and
    /// high-passes the product, so the high-pass also removes the envelope the rectifier leaks.
    fn process(
        &mut self,
        input: f32,
        sample_rate: f32,
        spec: BandSpec,
        colour: f32,
        gain: f32,
    ) -> f32 {
        let banded = self
            .band
            .process(input, sample_rate, spec.centre * colour, spec.q);
        let shaped = match spec.vca {
            Vca::Linear => banded,
            Vca::Saturating => {
                (SATURATING_VCA_DRIVE * banded / SHARED_BAND_RMS).tanh() / SATURATING_VCA_DRIVE
            }
            Vca::HalfWave => (banded / SHARED_BAND_RMS).max(0.0),
        };
        let fs = f64::from(sample_rate);
        let corner = (spec.high_pass * f64::from(colour)).min(0.45 * fs);
        let pole = (-std::f64::consts::TAU * corner / fs).exp();
        let mut signal = f64::from(shaped * gain);
        for (previous_input, previous_output) in self.high_pass.iter_mut().take(spec.order) {
            let output = pole * *previous_output + 0.5 * (1.0 + pole) * (signal - *previous_input);
            *previous_input = signal;
            *previous_output = flush_f64(output);
            signal = *previous_output;
        }
        signal as f32
    }

    fn reset(&mut self) {
        self.band.reset();
        self.high_pass = [(0.0, 0.0); 3];
    }
}

/// Fitted shape of one cymbal path's envelope; see [`CYMBAL_LOW`].
#[derive(Debug, Clone, Copy)]
struct PathEnvelope {
    gain: f32,
    fast_rise: f32,
    slow_rise: f32,
    fast_rise_share: f32,
    first_share: f32,
    first_t60: f32,
    second_t60: f32,
    threshold: f32,
}

/// Running state of one [`PathEnvelope`]: two rising and two decaying one-pole states.
#[derive(Debug, Clone, Copy)]
struct PathState {
    rise: [f32; 2],
    decay: [f32; 2],
}

impl PathState {
    const fn new() -> Self {
        Self {
            rise: [0.0; 2],
            decay: [0.0; 2],
        }
    }

    fn strike(&mut self, level: f32) {
        self.decay[0] = (self.decay[0] + level).min(2.0);
        self.decay[1] = (self.decay[1] + level).min(2.0);
    }

    /// Returns the path gain for this sample and advances the envelope.
    fn advance(&mut self, shape: PathEnvelope, sample_rate: f32, time_scale: f32) -> f32 {
        let rise =
            shape.fast_rise_share * self.rise[0] + (1.0 - shape.fast_rise_share) * self.rise[1];
        let decay = shape.first_share * self.decay[0] + (1.0 - shape.first_share) * self.decay[1];
        let gain = rise * (decay - shape.threshold).max(0.0) / (1.0 - shape.threshold);
        for (state, seconds) in self.rise.iter_mut().zip([shape.fast_rise, shape.slow_rise]) {
            let coefficient = 1.0 - (-1.0 / (seconds * sample_rate)).exp();
            *state = flush(*state + coefficient * (1.0 - *state));
        }
        for (state, t60) in self
            .decay
            .iter_mut()
            .zip([shape.first_t60, shape.second_t60])
        {
            *state = flush(*state * (0.001_f32.ln() / (t60 * time_scale * sample_rate)).exp());
        }
        gain
    }

    fn is_closed(&self, shape: PathEnvelope) -> bool {
        shape.first_share * self.decay[0] + (1.0 - shape.first_share) * self.decay[1]
            <= shape.threshold.max(1.0e-8)
    }
}

#[derive(Debug, Clone)]
pub struct Cowbell {
    sample_rate: f32,
    band: BandPass,
    rise: f32,
    fast_envelope: f32,
    body_envelope: f32,
    age: u32,
    active: bool,
}

impl Default for Cowbell {
    fn default() -> Self {
        Self::new()
    }
}

impl Cowbell {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            band: BandPass::new(),
            rise: 0.0,
            fast_envelope: 0.0,
            body_envelope: 0.0,
            age: 0,
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, value: f32) {
        self.sample_rate = valid_sample_rate(value);
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        self.fast_envelope = (self.fast_envelope + level).min(2.0);
        self.body_envelope = (self.body_envelope + level).min(2.0);
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, patch: Patch, frame: Frame) -> f32 {
        if !self.active {
            return 0.0;
        }
        // CHOSEN Tone law: the band-pass centre moves by 1.5 either way.
        let tone = finite_bipolar(patch.tone);
        let banded = self.band.process(
            frame.cowbell(),
            self.sample_rate,
            COWBELL_BAND_HZ * 1.5_f32.powf(tone),
            COWBELL_BAND_Q,
        );
        let envelope = self.rise
            * (COWBELL_FAST_SHARE * self.fast_envelope
                + (1.0 - COWBELL_FAST_SHARE) * self.body_envelope);
        let rise = 1.0 - (-1.0 / (COWBELL_RISE_SECONDS * self.sample_rate)).exp();
        self.rise = flush(self.rise + rise * (1.0 - self.rise));
        // CHOSEN Attack and Decay laws: Attack lengthens the first drop, Decay scales the body.
        let fast = COWBELL_FAST_SECONDS * 1.8_f32.powf(finite_bipolar(patch.attack));
        let body = scale_decay(COWBELL_BODY_SECONDS, finite_bipolar(patch.decay), 0.35, 8.0);
        self.fast_envelope = flush(self.fast_envelope * (-1.0 / (fast * self.sample_rate)).exp());
        self.body_envelope = flush(self.body_envelope * (-1.0 / (body * self.sample_rate)).exp());
        // The swing VCA saturates; Character raises its drive from a nearly clean reference.
        let drive = 0.15 * 4.0_f32.powf(finite_bipolar(patch.character));
        let output = COWBELL_GAIN * (drive * banded * envelope).tanh() / drive;
        let _unsupported = (finite_bipolar(patch.body), finite_bipolar(patch.noise));
        self.age = self.age.saturating_add(1);
        if self.age > (0.7 * self.sample_rate) as u32 && envelope < 1.0e-8 {
            self.reset();
            return 0.0;
        }
        output
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        self.band.reset();
        self.rise = 0.0;
        self.fast_envelope = 0.0;
        self.body_envelope = 0.0;
        self.age = 0;
        self.active = false;
    }
}

#[derive(Debug, Clone)]
pub struct Cymbal {
    sample_rate: f32,
    bands: [MetalBand; 3],
    envelopes: [PathState; 3],
    age: u32,
    active: bool,
}

impl Default for Cymbal {
    fn default() -> Self {
        Self::new()
    }
}
impl Cymbal {
    const PATHS: [PathEnvelope; 3] = [CYMBAL_LOW, CYMBAL_MIDDLE, CYMBAL_HIGH];
    const BANDS: [BandSpec; 3] = [CYMBAL_LOW_BAND, CYMBAL_MIDDLE_BAND, CYMBAL_HIGH_BAND];

    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            bands: [MetalBand::new(); 3],
            envelopes: [PathState::new(); 3],
            age: 0,
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, value: f32) {
        self.sample_rate = valid_sample_rate(value);
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        for envelope in &mut self.envelopes {
            envelope.strike(level);
        }
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, patch: Patch, frame: Frame) -> f32 {
        if !self.active {
            return 0.0;
        }
        let raw = frame.sum();
        let tone = finite_bipolar(patch.tone);
        // CHOSEN Decay law, as the service table's 350/800/1200 ms short/mid/long spread.
        let time_scale = scale_decay(1.0, finite_bipolar(patch.decay), 0.35, 8.0);
        // CHOSEN Tone law: Tone trades the lower path against the upper, as §3.8's coupled passive
        // TONE network mostly moves the third band. CHOSEN Attack law: Attack raises the upper
        // path, whose envelope carries the strike.
        let weights = [
            1.0 - 0.4 * tone,
            1.0,
            (1.0 + 0.6 * tone) * (1.0 + 0.5 * finite_bipolar(patch.attack)),
        ];
        let mut mixed = 0.0;
        let paths = self
            .bands
            .iter_mut()
            .zip(&mut self.envelopes)
            .zip(Self::BANDS.into_iter().zip(Self::PATHS))
            .zip(weights);
        for (((band, envelope), (filter, shape)), weight) in paths {
            let gain = envelope.advance(shape, self.sample_rate, time_scale);
            mixed += band.process(
                raw,
                self.sample_rate,
                filter,
                1.0,
                shape.gain * weight * gain,
            );
        }
        // The swing VCAs clip by design; Character raises their drive from a nearly clean reference.
        let drive = 0.3 * 4.0_f32.powf(finite_bipolar(patch.character));
        let output = CYMBAL_OUTPUT_GAIN * (drive * mixed).tanh() / drive;
        let _unsupported = (finite_bipolar(patch.body), finite_bipolar(patch.noise));
        self.age = self.age.saturating_add(1);
        if self.age > (2.0 * self.sample_rate) as u32
            && self
                .envelopes
                .iter()
                .zip(Self::PATHS)
                .all(|(state, shape)| state.is_closed(shape))
        {
            self.reset();
            return 0.0;
        }
        output
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.envelopes = [PathState::new(); 3];
        self.age = 0;
        self.active = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HatKind {
    Closed,
    Open,
}
#[derive(Debug, Clone)]
pub struct Hat {
    sample_rate: f32,
    band: MetalBand,
    envelope: f32,
    strike_level: f32,
    click: f32,
    /// The click's fixed 2 ms decay, which follows the sample rate alone.
    click_decay: Coefficient<f32, f32>,
    age: u32,
    active: bool,
}
impl Default for Hat {
    fn default() -> Self {
        Self::new()
    }
}
impl Hat {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            band: MetalBand::new(),
            envelope: 0.0,
            strike_level: 0.0,
            click: 0.0,
            click_decay: Coefficient::new(),
            age: 0,
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, value: f32) {
        self.sample_rate = valid_sample_rate(value);
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        self.envelope = (self.envelope + level).min(2.0);
        self.strike_level = self.envelope;
        self.click = 1.0;
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, kind: HatKind, patch: Patch, frame: Frame) -> f32 {
        if !self.active {
            return 0.0;
        }
        // CHOSEN Tone law: the hat's band moves by 1.5 either way.
        let colour = 1.5_f32.powf(finite_bipolar(patch.tone));
        let filter = if kind == HatKind::Closed {
            CLOSED_BAND
        } else {
            OPEN_BAND
        };
        let decay = finite_bipolar(patch.decay);
        let gain = if kind == HatKind::Closed {
            let t60 = scale_decay(CLOSED_T60, decay, 0.4, 8.0);
            let gain = self.envelope;
            self.envelope =
                flush(self.envelope * (0.001_f32.ln() / (t60 * self.sample_rate)).exp());
            gain
        } else {
            // The open-hat swing VCA passes the envelope until it nears its closing threshold, then
            // fades linearly to nothing: the finite end the recordings show at every Decay position.
            let t60 = scale_decay(OPEN_T60, decay, 0.31, 8.0);
            let threshold = OPEN_CLOSE_THRESHOLD * self.strike_level;
            if self.envelope <= threshold {
                self.reset();
                return 0.0;
            }
            let fade =
                ((self.envelope - threshold) / (OPEN_CLOSE_WIDTH * self.strike_level)).min(1.0);
            let gain = self.envelope * fade;
            self.envelope =
                flush(self.envelope * (0.001_f32.ln() / (t60 * self.sample_rate)).exp());
            gain
        };
        // CHOSEN Attack law: a 2 ms strike emphasis that Attack raises or removes.
        let attack = 1.0 + 0.6 * finite_bipolar(patch.attack) * self.click;
        let click_decay = self
            .click_decay
            .get(self.sample_rate, |fs| (-1.0 / (0.002 * fs)).exp());
        self.click = flush(self.click * click_decay);
        let band = self
            .band
            .process(frame.sum(), self.sample_rate, filter, colour, gain * attack);
        // The swing VCA clips by design; Character raises its drive from a nearly clean reference.
        let drive = 0.5 * 4.0_f32.powf(finite_bipolar(patch.character));
        let output = HAT_GAIN * (drive * band).tanh() / drive;
        let _unsupported = (finite_bipolar(patch.body), finite_bipolar(patch.noise));
        self.age = self.age.saturating_add(1);
        if self.age > (5.0 * self.sample_rate) as u32 && self.envelope < 1.0e-8 {
            self.reset();
            return 0.0;
        }
        output
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        self.band.reset();
        self.envelope = 0.0;
        self.strike_level = 0.0;
        self.click = 0.0;
        self.age = 0;
        self.active = false;
    }
}

/// Metal accent relative to the catalogue reference, so the reference hit is the fitted level.
fn accent(velocity: f32, dynamics: f32) -> f32 {
    let velocity = finite_or(velocity, 0.0).clamp(0.0, 1.0);
    let dynamics = finite_bipolar(dynamics);
    // The collection's one Dynamics mapping (`crate::velocity`); this family's own was
    // `1 − 0.45·d` / `1 − 1.5·d`, the same at zero.
    let exponent = crate::velocity::exponent(dynamics);
    (0.48 + 0.52 * velocity.powf(exponent)) / REFERENCE_ACCENT
}
fn scale_decay(reference: f32, value: f32, short: f32, long: f32) -> f32 {
    if value < 0.0 {
        reference * short.powf(-value)
    } else {
        reference * long.powf(value)
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
            decay: 0.0,
            attack: 0.0,
            tone: 0.0,
            body: 0.0,
            noise: 0.0,
            character: 0.0,
            dynamics: 0.0,
        }
    }
    #[test]
    fn bank_is_free_running_deterministic_and_pitch_specific() {
        let mut a = MetalBank::new();
        let mut b = MetalBank::new();
        for _ in 0..10_000 {
            assert_eq!(
                a.tick(48_000.0, 0.0).oscillators,
                b.tick(48_000.0, 0.0).oscillators
            );
        }
        let normal: Vec<_> = (0..1_000)
            .map(|_| a.tick(48_000.0, 0.0).oscillators)
            .collect();
        let shifted: Vec<_> = (0..1_000)
            .map(|_| b.tick(48_000.0, 12.0).oscillators)
            .collect();
        assert_ne!(normal, shifted);
    }
    #[test]
    fn all_consumers_sound_and_reset() {
        let mut bank = MetalBank::new();
        let mut cow = Cowbell::new();
        let mut cymbal = Cymbal::new();
        let mut closed = Hat::new();
        let mut open = Hat::new();
        cow.trigger(0.8, patch());
        cymbal.trigger(0.8, patch());
        closed.trigger(0.8, patch());
        open.trigger(0.8, patch());
        let mut peaks = [0.0_f32; 4];
        for _ in 0..4_800 {
            let frame = bank.tick(48_000.0, 0.0);
            let values = [
                cow.process(patch(), frame),
                cymbal.process(patch(), frame),
                closed.process(HatKind::Closed, patch(), frame),
                open.process(HatKind::Open, patch(), frame),
            ];
            for (i, value) in values.into_iter().enumerate() {
                assert!(value.is_finite());
                peaks[i] = peaks[i].max(value.abs());
            }
        }
        assert!(peaks.into_iter().all(|peak| peak > 0.01), "{peaks:?}");
        // The slot-level bounded choke ends in this reset; the circuit has no choke of its own.
        open.reset();
        assert_eq!(
            open.process(HatKind::Open, patch(), bank.tick(48_000.0, 0.0))
                .to_bits(),
            0.0_f32.to_bits()
        );
    }
    #[test]
    fn open_hat_preserves_reference_and_extends_beyond_the_measured_stock_range() {
        let duration = |decay| {
            let mut bank = MetalBank::new();
            let mut voice = Hat::new();
            let mut p = patch();
            p.decay = decay;
            voice.trigger(0.8, p);
            let mut samples = 0;
            while voice.is_active() && samples < 480_000 {
                let _ = voice.process(HatKind::Open, p, bank.tick(48_000.0, 0.0));
                samples += 1;
            }
            samples as f32 / 48_000.0
        };
        let short = duration(-1.0);
        let centre = duration(0.0);
        let long = duration(1.0);
        assert!((0.10..0.12).contains(&short), "short {short}");
        assert!((0.35..0.37).contains(&centre), "centre {centre}");
        assert!((2.85..2.88).contains(&long), "extended {long}");
    }

    #[test]
    fn open_hat_fades_to_its_end_instead_of_stopping_at_full_level() {
        // The recording loses about 3 dB between 300 and 320 ms, then fades to silence by 360 ms.
        let mut bank = MetalBank::new();
        let mut voice = Hat::new();
        voice.trigger(0.82, patch());
        let samples: Vec<f32> = (0..17_280)
            .map(|_| voice.process(HatKind::Open, patch(), bank.tick(48_000.0, 0.0)))
            .collect();
        let rms = |start: usize| {
            let slice = &samples[start..start + 480];
            (slice.iter().map(|x| x * x).sum::<f32>() / 480.0).sqrt()
        };
        let body = rms(14_400);
        let closing = rms(16_560);
        assert!(closing < 0.5 * body, "body {body}, closing {closing}");
        assert!(closing > 0.0);
    }

    #[test]
    fn cowbell_is_dominated_by_oscillator_five() {
        let mut bank = MetalBank::new();
        let mut voice = Cowbell::new();
        voice.trigger(0.82, patch());
        let samples: Vec<f32> = (0..4_800)
            .map(|_| voice.process(patch(), bank.tick(48_000.0, 0.0)))
            .collect();
        let projection = |frequency: f64| {
            let (s, c) = samples
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(s, c), (n, x)| {
                    let angle = std::f64::consts::TAU * frequency * n as f64 / 48_000.0;
                    (
                        s + f64::from(*x) * angle.sin(),
                        c + f64::from(*x) * angle.cos(),
                    )
                });
            f64::hypot(s, c)
        };
        let high = projection(FREQUENCIES[4]);
        let low = projection(FREQUENCIES[5]);
        assert!(high > 4.0 * low, "825 Hz {high}, 552 Hz {low}");
    }

    #[test]
    fn unsupported_body_and_noise_are_exact_no_ops() {
        let render = |body, noise| {
            let mut bank = MetalBank::new();
            let mut voice = Cowbell::new();
            let mut p = patch();
            p.body = body;
            p.noise = noise;
            voice.trigger(0.8, p);
            (0..512)
                .map(|_| voice.process(p, bank.tick(48_000.0, 0.0)))
                .collect::<Vec<_>>()
        };
        assert_eq!(render(-1.0, -1.0), render(1.0, 1.0));
    }
}

#[cfg(test)]
mod gating_tests {
    use super::*;

    /// Ticks a fresh bank `samples` times and returns it.
    fn ticked(samples: u64, fs: f32) -> MetalBank {
        let mut bank = MetalBank::new();
        for _ in 0..samples {
            let _ = bank.tick(fs, 0.0);
        }
        bank
    }

    #[test]
    fn advancing_lands_exactly_where_ticking_does() {
        // The justification for gating the bus at all. A bank that resumes on a different phase
        // is a different instrument, and the cymbal's six squares would no longer stand in the
        // relationship the hardware puts them in.
        let fs = 48_000.0;
        for samples in [1_u64, 4, 5, 6, 97, 1_000, 48_000] {
            let mut jumped = MetalBank::new();
            jumped.advance(samples, fs, 0.0);
            let stepped = ticked(samples, fs);
            assert_eq!(
                jumped.phases, stepped.phases,
                "phases differ after {samples} samples"
            );
            assert_eq!(
                jumped.pending, stepped.pending,
                "the band-limiting window differs after {samples} samples"
            );
        }
    }

    #[test]
    fn what_it_renders_next_is_identical_too() {
        // Equal state is the mechanism; equal audio is the promise. Rendered past the point a
        // resume happens, a gated bank and one that never stopped must agree sample for sample.
        let fs = 48_000.0;
        let gap = 2_000;
        let mut jumped = MetalBank::new();
        jumped.advance(gap, fs, 0.0);
        let mut stepped = ticked(gap, fs);
        for frame in 0..512 {
            let a = jumped.tick(fs, 0.0);
            let b = stepped.tick(fs, 0.0);
            assert_eq!(a.oscillators, b.oscillators, "frame {frame} after the gap");
        }
    }

    #[test]
    fn advancing_nothing_changes_nothing() {
        let fs = 48_000.0;
        let mut bank = ticked(64, fs);
        let before = bank.clone();
        bank.advance(0, fs, 0.0);
        assert_eq!(bank.phases, before.phases);
        assert_eq!(bank.pending, before.pending);
    }

    #[test]
    fn a_gap_in_two_halves_equals_one_gap() {
        // A bus that wakes and sleeps repeatedly must not drift away from one that slept once.
        let fs = 48_000.0;
        let mut split = MetalBank::new();
        split.advance(700, fs, 0.0);
        split.advance(1_300, fs, 0.0);
        let mut whole = MetalBank::new();
        whole.advance(2_000, fs, 0.0);
        assert_eq!(split.phases, whole.phases);
        assert_eq!(split.pending, whole.pending);
    }
}

#[cfg(test)]
mod gating_cost {
    use super::*;

    /// Not a gate: prints what dormancy costs against ticking, so plan §4.7's CPU claim rests on
    /// a measurement rather than an expectation.
    #[test]
    #[ignore = "prints a timing; run with --release -- --ignored --nocapture"]
    fn what_dormancy_saves() {
        let fs = 48_000.0;
        let samples = 480_000_u64;

        let mut ticking = MetalBank::new();
        let start = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(ticking.tick(fs, 0.0));
        }
        let ticked = start.elapsed();

        let mut dormant = MetalBank::new();
        let start = std::time::Instant::now();
        dormant.advance(samples, fs, 0.0);
        let advanced = start.elapsed();

        let per = |d: std::time::Duration| d.as_secs_f64() * 1.0e9 / samples as f64;
        println!(
            "MetalBank  ticking {:.1} ns/sample   dormant {:.1} ns/sample   {:.1}x cheaper",
            per(ticked),
            per(advanced),
            per(ticked) / per(advanced).max(1.0e-9)
        );
    }
}

#[cfg(test)]
mod fixed_point_tables {
    use super::*;

    #[test]
    fn the_fixed_tables_match_their_fractions() {
        // The bank runs on integers so a dormant bus can be advanced in one multiply, but the
        // numbers a person can check are the fractions. This keeps the two in step: a mistyped
        // digit in either table moves an edge, and an edge moved is a different cymbal.
        for (index, duty) in DUTY.iter().enumerate() {
            assert_eq!(
                DUTY_FIXED[index],
                (duty * CYCLE_FLOAT) as u64,
                "duty {index} ({duty}) does not match its fixed-point value"
            );
        }
        for (index, phase) in MetalBank::START_PHASES.iter().enumerate() {
            assert_eq!(
                START_PHASES_FIXED[index],
                (phase * CYCLE_FLOAT) as u64,
                "start phase {index} ({phase}) does not match its fixed-point value"
            );
        }
    }
}
