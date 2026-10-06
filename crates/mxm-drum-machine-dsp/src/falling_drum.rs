//! Diode-loaded struck resonators for the six low/mid/high tom and conga articulations.
//!
//! `research:instruments/analogue-drum-machines.md` §3.4 establishes the six service frequencies
//! and decay figures, and the diode path that raises pitch most strongly at the strike before
//! falling to the named centre. The reference constants below were fitted to the acquired
//! comparison recordings (the centre-position unaccented takes, §11.2) in the 2026-09-19 A/B pass,
//! by least-squares fitting of this structure to each voice's first 30–60 ms and by half-cycle decay
//! slopes: a cosine-phase strike (the bridged-T band-pass's impulse response), a brief large diode
//! excursion, the 1 ms common trigger reaching the output, and a one-pole output low-pass.
//!
//! The recordings show a band-limited noise tail in all three toms, about 26 dB under the ring and
//! outlasting it, and none in the congas: the tom/conga switch removes it. §3.4 names it only for
//! the low tom. Low tom alone exposes it to Noise and Noise decay; mid and high tom play it fixed.
//! Every tom reads the one machine-shared coloured noise sample through its own filter states.

use crate::deep_bridge_kick::PhasorResonator;
use crate::resonator::MIN_SAMPLE_RATE;
use crate::{Coefficient, flush};

const MAX_TAIL_SECONDS: f32 = 1.5;
/// Final emergency bound. Ordinary reference hits peak near 0.6 and never reach it, so the clean
/// ring the recordings show is not shaped by it.
const OUTPUT_BOUND: f32 = 1.0;
/// CHOSEN conversion from the fitted recording scale (peak about 1) to this renderer's level; the
/// catalogue trim in `engine.rs` owns the absolute level.
const OUTPUT_GAIN: f32 = 0.58;
/// Accent at the catalogue reference velocity (0.82) with zero Dynamics; fitted amplitudes are the
/// recording's scale at this accent.
const REFERENCE_ACCENT: f32 = 0.35 + 0.65 * 0.82;
/// The common trigger's width, `research:instruments/analogue-drum-machines.md` §2.1.
const TRIGGER_SECONDS: f32 = 0.001;
/// Tom noise band: one-pole high-pass and two one-pole low-passes applied locally to the shared
/// coloured sample. FITTED to the third-octave spectrum of the acquired comparison recordings' take
/// differences in the 2026-09-19 A/B pass (flat 125–400 Hz, falling above 500 Hz); a measurement of
/// the 808's pink-noise branch replaces it.
const NOISE_HIGH_PASS_HZ: f64 = 120.0;
const NOISE_LOW_PASS_HZ: f64 = 250.0;
/// Tom noise envelope rise. FITTED in the 2026-09-19 A/B pass: the take-difference noise is about
/// 7 dB down in the first 10 ms and at its plateau by 15–30 ms.
const NOISE_ATTACK_SECONDS: f32 = 0.012;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    LowTom,
    LowConga,
    MidTom,
    MidConga,
    HighTom,
    HighConga,
}

impl Kind {
    /// Rest pitch. MEASURED from the acquired comparison recordings' take averages in the
    /// 2026-09-19 A/B pass (late-window spectral peak); every value lies inside the service bounds
    /// of `research:instruments/analogue-drum-machines.md` §3.4. A trimmed hardware unit replaces
    /// them.
    #[must_use]
    pub const fn frequency_hz(self) -> f32 {
        match self {
            Self::LowTom => 85.5,
            Self::LowConga => 186.0,
            Self::MidTom => 132.7,
            Self::MidConga => 275.0,
            Self::HighTom => 181.1,
            Self::HighConga => 391.2,
        }
    }

    /// Service-note decay figure (§3.4). The recordings place it near the −20 dB point, not the
    /// −60 dB point, so it is not the resonator's T60.
    #[must_use]
    pub const fn service_decay_seconds(self) -> f32 {
        match self {
            Self::LowTom => 0.200,
            Self::LowConga => 0.180,
            Self::MidTom => 0.130,
            Self::MidConga => 0.100,
            Self::HighTom => 0.100,
            Self::HighConga => 0.080,
        }
    }

    /// Amplitude T60 of the ring. MEASURED from the acquired comparison recordings in the
    /// 2026-09-19 A/B pass (half-cycle peak slopes over the first 15 dB, checked by a 100–150 ms
    /// waveform fit); a hardware decay measurement replaces them.
    #[must_use]
    const fn reference_t60_seconds(self) -> f32 {
        match self {
            Self::LowTom => 0.640,
            Self::LowConga => 0.505,
            Self::MidTom => 0.305,
            Self::MidConga => 0.248,
            Self::HighTom => 0.288,
            Self::HighConga => 0.226,
        }
    }

    /// Relative lift of the diode excursion at the strike, and its exponential time constant.
    /// FITTED to the acquired comparison recordings in the 2026-09-19 A/B pass: the conducting
    /// diodes more than double the frequency for a fraction of a millisecond, which advances the
    /// ring's phase by 45–90° before it settles. Instantaneous-frequency measurement of the first
    /// cycle replaces them.
    const fn reference_pitch_jump(self) -> (f32, f32) {
        match self {
            Self::LowTom => (1.565, 0.001_881),
            Self::LowConga => (1.234, 0.000_708),
            Self::MidTom => (1.379, 0.000_963),
            Self::MidConga => (2.321, 0.000_213),
            Self::HighTom => (1.166, 0.000_724),
            Self::HighConga => (1.573, 0.000_200),
        }
    }

    /// Strike amplitude and the 1 ms trigger's level at the output, as a share of the strike.
    /// FITTED to the acquired comparison recordings in the 2026-09-19 A/B pass; the trigger is the
    /// sharp positive spike before the first negative half-cycle, ending in a kink at 1 ms. A
    /// measurement of the trigger's path through each bridged-T network replaces them.
    const fn strike(self) -> (f32, f32) {
        match self {
            Self::LowTom => (0.796, 0.243),
            Self::LowConga => (1.058, 0.143),
            Self::MidTom => (0.853, 0.161),
            Self::MidConga => (1.139, 0.255),
            Self::HighTom => (0.968, 0.120),
            Self::HighConga => (1.208, 0.153),
        }
    }

    /// One-pole equivalent of the output network's low-pass at the reference Tone. FITTED to the
    /// acquired comparison recordings in the 2026-09-19 A/B pass from the spike's rise and the ring's
    /// phase; the toms' network is about three times wider than the congas'. The output network's
    /// measured response replaces them.
    const fn tone_hz(self) -> f32 {
        match self {
            Self::LowTom => 1_527.0,
            Self::LowConga => 652.0,
            Self::MidTom => 2_359.0,
            Self::MidConga => 713.0,
            Self::HighTom => 1_768.0,
            Self::HighConga => 713.0,
        }
    }

    /// Noise tail level (RMS gain on the band-limited shared sample) and its T60, or `None` for the
    /// congas. FITTED to the acquired comparison recordings' eight-take differences in the
    /// 2026-09-19 A/B pass: plateaus near −32/−33/−34 dB of the hit's peak, then 0.85/0.75/0.72 s
    /// T60, longer than each ring. A measurement of the tom noise VCA replaces them.
    const fn noise_tail(self) -> Option<(f32, f32)> {
        match self {
            Self::LowTom => Some((0.47, 0.85)),
            Self::MidTom => Some((0.42, 0.75)),
            Self::HighTom => Some((0.38, 0.72)),
            Self::LowConga | Self::MidConga | Self::HighConga => None,
        }
    }

    /// Low tom alone exposes its noise tail to the Noise and Noise decay controls.
    const fn noise_is_controlled(self) -> bool {
        matches!(self, Self::LowTom)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub pitch_semitones: f32,
    pub pitch_envelope: f32,
    pub pitch_decay: f32,
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
pub struct FallingDrum {
    sample_rate: f32,
    /// Held against their inputs. The noise attack and the tail band follow the sample rate alone;
    /// the pitch and noise decays follow their control positions with it.
    pitch_decay: Coefficient<(f32, f32), f32>,
    noise_rise_pole: Coefficient<f32, f32>,
    noise_band_poles: Coefficient<f32, (f64, f64)>,
    noise_decay: Coefficient<(f32, f32, f32), f32>,
    resonator: PhasorResonator,
    pitch_envelope: f32,
    feed_remaining: u32,
    feed_level: f32,
    noise_envelope: f32,
    noise_rise: f32,
    noise_high_input: f64,
    noise_high_output: f64,
    noise_low: [f64; 2],
    tone_state: f32,
    age: u32,
    active: bool,
}

impl Default for FallingDrum {
    fn default() -> Self {
        Self::new()
    }
}

impl FallingDrum {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            pitch_decay: Coefficient::new(),
            noise_rise_pole: Coefficient::new(),
            noise_band_poles: Coefficient::new(),
            noise_decay: Coefficient::new(),
            resonator: PhasorResonator::new(),
            pitch_envelope: 0.0,
            feed_remaining: 0,
            feed_level: 0.0,
            noise_envelope: 0.0,
            noise_rise: 0.0,
            noise_high_input: 0.0,
            noise_high_output: 0.0,
            noise_low: [0.0; 2],
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

    /// Strikes the live resonator; a retrigger adds to the ring according to its current phase.
    pub fn trigger(&mut self, kind: Kind, velocity: f32, patch: Patch) {
        let velocity = finite_unit(velocity);
        let dynamics = finite_bipolar(patch.dynamics);
        let attack = finite_bipolar(patch.attack);
        let exponent = crate::velocity::exponent(dynamics);
        let accent = (0.35 + 0.65 * velocity.powf(exponent)) / REFERENCE_ACCENT;
        let (amplitude, feed_share) = kind.strike();
        let strike = accent * amplitude * (1.0 + 0.35 * attack);
        // Cosine phase: the band-pass impulse response starts at its peak.
        self.resonator.strike(f64::from(strike), 0.0);
        // CHOSEN Attack law: the trigger feed-through is the click, so Attack raises it too.
        self.feed_level = accent * amplitude * feed_share * (1.0 + 0.6 * attack);
        self.feed_remaining = (TRIGGER_SECONDS * self.sample_rate).round().max(1.0) as u32;
        self.pitch_envelope = (self.pitch_envelope + accent * (1.0 + 0.5 * attack)).min(2.0);
        if kind.noise_tail().is_some() {
            let noise = if kind.noise_is_controlled() {
                1.0 + finite_bipolar(patch.noise)
            } else {
                1.0
            };
            self.noise_envelope = (self.noise_envelope + accent * noise).min(3.0);
        }
        self.age = 0;
        self.active = true;
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, kind: Kind, patch: Patch, shared_pink_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let body = finite_bipolar(patch.body);
        let pitch_depth = envelope_depth(finite_bipolar(patch.pitch_envelope));
        let (jump, jump_seconds) = kind.reference_pitch_jump();
        let jump = jump * (1.0 + 0.75 * body) * pitch_depth;
        let frequency =
            kind.frequency_hz() * 2.0_f32.powf(pitch / 12.0) * (1.0 + jump * self.pitch_envelope);
        let decay = decay_seconds(kind.reference_t60_seconds(), finite_bipolar(patch.decay));
        let struck = self.resonator.step(self.sample_rate, frequency, decay);

        // Pitch decay spans common capacitor substitutions without moving the reference position.
        let pitch_time = jump_seconds * envelope_time_scale(finite_bipolar(patch.pitch_decay));
        let pitch_decay = self
            .pitch_decay
            .get((pitch_time, self.sample_rate), |(pitch_time, fs)| {
                (-1.0 / (pitch_time * fs)).exp()
            });
        self.pitch_envelope = flush(self.pitch_envelope * pitch_decay);

        let feed = if self.feed_remaining > 0 {
            self.feed_remaining -= 1;
            self.feed_level
        } else {
            0.0
        };
        let tone = finite_bipolar(patch.tone);
        // CHOSEN Tone law: ±1 moves the fitted network corner by a factor of about 2.1 either way.
        let cutoff = kind.tone_hz() * 4.5_f32.powf(0.5 * tone);
        let coefficient = 1.0
            - (-std::f32::consts::TAU * cutoff.min(0.45 * self.sample_rate) / self.sample_rate)
                .exp();
        self.tone_state = flush(self.tone_state + coefficient * (struck + feed - self.tone_state));

        let mut noise_tail = 0.0;
        if let Some((level, reference_t60)) = kind.noise_tail() {
            let band = self.noise_band(finite_bipolar(shared_pink_noise));
            let rise = self.noise_rise_pole.get(self.sample_rate, |fs| {
                1.0 - (-1.0 / (NOISE_ATTACK_SECONDS * fs)).exp()
            });
            self.noise_rise = flush(self.noise_rise + rise * (1.0 - self.noise_rise));
            noise_tail = band * level * self.noise_envelope * self.noise_rise;
            let time_scale = if kind.noise_is_controlled() {
                envelope_time_scale(finite_bipolar(patch.noise_decay))
            } else {
                1.0
            };
            let noise_decay = self.noise_decay.get(
                (reference_t60, time_scale, self.sample_rate),
                |(reference_t60, time_scale, fs)| {
                    (0.001_f32.ln() / (reference_t60 * time_scale * fs)).exp()
                },
            );
            self.noise_envelope = flush(self.noise_envelope * noise_decay);
        }
        // Character is unsupported for these six rows and is deliberately unread by the sound law.
        let _unsupported_character = finite_bipolar(patch.character);
        let output = (OUTPUT_GAIN * ((1.0 + 0.25 * body) * self.tone_state + noise_tail))
            .clamp(-OUTPUT_BOUND, OUTPUT_BOUND);

        self.age = self.age.saturating_add(1);
        if self.age > (MAX_TAIL_SECONDS * self.sample_rate) as u32
            && self.resonator.magnitude() < 1.0e-8
            && self.noise_envelope.abs() < 1.0e-8
        {
            self.reset();
            return 0.0;
        }
        output
    }

    /// The tom's own band on the shared coloured sample: one-pole high-pass, two one-pole
    /// low-passes. `f64` because the 120 Hz high-pass pole sits close to one.
    fn noise_band(&mut self, input: f32) -> f32 {
        let (high_pole, low) = self.noise_band_poles.get(self.sample_rate, |sample_rate| {
            let fs = f64::from(sample_rate);
            (
                (-std::f64::consts::TAU * NOISE_HIGH_PASS_HZ / fs).exp(),
                1.0 - (-std::f64::consts::TAU * NOISE_LOW_PASS_HZ / fs).exp(),
            )
        });
        let input = f64::from(input);
        let high = high_pole * self.noise_high_output
            + 0.5 * (1.0 + high_pole) * (input - self.noise_high_input);
        self.noise_high_input = input;
        self.noise_high_output = flush_f64(high);
        self.noise_low[0] = flush_f64(self.noise_low[0] + low * (high - self.noise_low[0]));
        self.noise_low[1] =
            flush_f64(self.noise_low[1] + low * (self.noise_low[0] - self.noise_low[1]));
        self.noise_low[1] as f32
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        self.resonator.reset();
        self.pitch_envelope = 0.0;
        self.feed_remaining = 0;
        self.feed_level = 0.0;
        self.noise_envelope = 0.0;
        self.noise_rise = 0.0;
        self.noise_high_input = 0.0;
        self.noise_high_output = 0.0;
        self.noise_low = [0.0; 2];
        self.tone_state = 0.0;
        self.age = 0;
        self.active = false;
    }
}

fn decay_seconds(reference: f32, value: f32) -> f32 {
    if value < 0.0 {
        reference * 0.35_f32.powf(-value)
    } else {
        reference * 8.0_f32.powf(value)
    }
}

fn envelope_depth(value: f32) -> f32 {
    if value < 0.0 {
        1.0 + value
    } else {
        1.0 + 3.0 * value
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

    const KINDS: [Kind; 6] = [
        Kind::LowTom,
        Kind::LowConga,
        Kind::MidTom,
        Kind::MidConga,
        Kind::HighTom,
        Kind::HighConga,
    ];

    fn default_patch() -> Patch {
        Patch {
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
        }
    }

    #[test]
    fn service_decay_figures_are_not_mislabeled_as_t60() {
        // The measured T60s sit two to three and a half times the service figures, which describe
        // a point near −20 dB rather than the end of the ring.
        for kind in KINDS {
            let ratio = kind.reference_t60_seconds() / kind.service_decay_seconds();
            assert!((2.2..3.5).contains(&ratio), "{kind:?}: T60/service {ratio}");
        }
    }

    #[test]
    fn all_service_rows_sound_and_stay_bounded() {
        for kind in KINDS {
            let mut voice = FallingDrum::new();
            voice.trigger(kind, 0.8, default_patch());
            let mut peak = 0.0_f32;
            for _ in 0..48_000 {
                let sample = voice.process(kind, default_patch(), 0.25);
                assert!(sample.is_finite());
                peak = peak.max(sample.abs());
            }
            assert!(peak > 0.05 && peak < OUTPUT_BOUND, "{kind:?}: {peak}");
        }
    }

    #[test]
    fn the_trigger_spike_precedes_a_negative_half_cycle() {
        // The acquired comparison recordings open with a positive spike inside the 1 ms trigger;
        // the ring's first extreme after it is negative.
        for kind in KINDS {
            let mut voice = FallingDrum::new();
            voice.trigger(kind, 0.82, default_patch());
            let samples: Vec<_> = (0..4_800)
                .map(|_| voice.process(kind, default_patch(), 0.0))
                .collect();
            let spike_at = samples[..48]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(index, _)| index)
                .expect("samples");
            let first_negative = samples.iter().position(|s| *s < 0.0).expect("a swing");
            assert!(spike_at < 24, "{kind:?}: spike at sample {spike_at}");
            assert!(
                first_negative < 96,
                "{kind:?}: first negative sample {first_negative}"
            );
        }
    }

    #[test]
    fn pitch_envelope_falls_toward_the_measured_rest() {
        let mut voice = FallingDrum::new();
        let patch = default_patch();
        voice.trigger(Kind::HighConga, 1.0, patch);
        let samples: Vec<_> = (0..4_800)
            .map(|_| voice.process(Kind::HighConga, patch, 0.0))
            .collect();
        fn crossings(samples: &[f32]) -> usize {
            samples
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count()
        }
        let late = crossings(&samples[960..4_800]);
        let late_hz = late as f32 / (3_840.0 / 48_000.0);
        assert!((375.0..405.0).contains(&late_hz), "late {late_hz} Hz");
        // The diode excursion advances the first cycle: the first upward crossing comes before a
        // quarter-plus-half period of the rest frequency would put it.
        let first_up = samples
            .windows(2)
            .position(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .expect("an upward crossing") as f32;
        let unswept = 0.75 * 48_000.0 / Kind::HighConga.frequency_hz();
        assert!(first_up < unswept, "first upward crossing {first_up}");
    }

    #[test]
    fn noise_is_a_no_op_except_on_low_tom() {
        let render = |kind, noise| {
            let mut voice = FallingDrum::new();
            let mut patch = default_patch();
            patch.noise = noise;
            patch.noise_decay = noise;
            voice.trigger(kind, 0.8, patch);
            (0..512)
                .map(|_| voice.process(kind, patch, 0.5))
                .collect::<Vec<_>>()
        };
        for kind in [Kind::LowConga, Kind::MidTom, Kind::HighTom] {
            assert_eq!(render(kind, -1.0), render(kind, 1.0), "{kind:?}");
        }
        assert_ne!(render(Kind::LowTom, -1.0), render(Kind::LowTom, 1.0));
    }

    #[test]
    fn toms_carry_the_shared_noise_tail_and_congas_do_not() {
        let render = |kind, noise| {
            let mut voice = FallingDrum::new();
            voice.trigger(kind, 0.8, default_patch());
            (0..4_800)
                .map(|_| voice.process(kind, default_patch(), noise))
                .collect::<Vec<_>>()
        };
        for kind in KINDS {
            let changed = render(kind, 0.0) != render(kind, 0.5);
            assert_eq!(changed, kind.noise_tail().is_some(), "{kind:?}");
        }
    }

    #[test]
    fn reset_is_exact_silence() {
        let mut voice = FallingDrum::new();
        voice.trigger(Kind::LowTom, 1.0, default_patch());
        let _ = voice.process(Kind::LowTom, default_patch(), 0.5);
        voice.reset();
        assert_eq!(
            voice.process(Kind::LowTom, default_patch(), 0.5).to_bits(),
            0.0_f32.to_bits()
        );
    }
}
