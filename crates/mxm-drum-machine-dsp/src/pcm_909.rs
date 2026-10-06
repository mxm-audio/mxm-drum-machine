//! Fixed captured-source PCM metal family with clock-coupled playback.
//!
//! IDs 24–27 use the owner's selected offline NMF-12 resyntheses. One coherent KL soft-mask source
//! is shared by both hats, as data; crash and ride have one source each. Closed/open articulation
//! comes from separate address and envelope paths over that shared hat source.
//!
//! The hardware runs both hats from **one** address counter, so only one can sound and either
//! restarts the other. That is deliberately not modelled: each slot advances its own address, and
//! closing one sound with another is a choke group the user assigns (owner, 2026-09-20), which
//! works across any two models rather than only this machine's own pair.
//! The mono 44.1 kHz sources are stored as little-endian signed 16-bit PCM and are played directly
//! here; applying the former procedural source's six-bit conversion and reconstruction filter again
//! would process an already complete hardware-output resynthesis twice. The asset record documents
//! the offline method and fixed representation.

use crate::engine::SlotPatch;

const SOURCE_RATE: f64 = 44_100.0;
const HAT: &[u8] = include_bytes!("../assets/reset-909-nmf/hat.pcm");
const CRASH: &[u8] = include_bytes!("../assets/reset-909-nmf/crash.pcm");
const RIDE: &[u8] = include_bytes!("../assets/reset-909-nmf/ride.pcm");
const CLOSED_ENVELOPE: &[u8] = include_bytes!("../assets/reset-909-nmf/closed-envelope-f32.table");
const CLOSED_NOISE_AMOUNT: f32 = 0.03;
const SINC_PHASES: usize = 256;
const SINC_RADIUS: isize = 8;
const SINC_TAPS: usize = SINC_RADIUS as usize * 2;
const SINC_TABLE: &[u8] = include_bytes!("../assets/reset-909-nmf/sinc8-256-f32.table");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    ClosedHat,
    OpenHat,
    Crash,
    Ride,
}

impl Kind {
    fn data(self) -> &'static [u8] {
        match self {
            Self::ClosedHat | Self::OpenHat => HAT,
            Self::Crash => CRASH,
            Self::Ride => RIDE,
        }
    }

    fn source_length(self) -> usize {
        self.data().len() / 2
    }

    fn playback_length(self) -> usize {
        match self {
            Self::ClosedHat => CLOSED_ENVELOPE.len() / 4,
            Self::OpenHat | Self::Crash | Self::Ride => self.source_length(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PcmMetal {
    fs: f64,
    position: f64,
    elapsed_samples: u64,
    gain: f32,
    tone_lowpass: f32,
    active: bool,
}

impl Default for PcmMetal {
    fn default() -> Self {
        Self::new()
    }
}

impl PcmMetal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fs: 48_000.0,
            position: 0.0,
            elapsed_samples: 0,
            gain: 0.0,
            tone_lowpass: 0.0,
            active: false,
        }
    }

    pub fn set_sample_rate(&mut self, value: f32) {
        self.fs = f64::from(finite(value, 48_000.0).clamp(1_000.0, 384_000.0));
    }

    pub fn trigger(&mut self, velocity: f32, patch: SlotPatch) {
        self.position = 0.0;
        self.elapsed_samples = 0;
        // Linear in velocity, Dynamics bending the curve (`crate::velocity`).
        self.gain = crate::velocity::linear(velocity, patch.dynamics);
        self.tone_lowpass = 0.0;
        self.active = true;
    }

    #[must_use]
    pub fn process(&mut self, kind: Kind, patch: SlotPatch) -> f32 {
        if !self.active {
            return 0.0;
        }

        let index = self.position.floor() as usize;
        if index >= kind.playback_length() {
            self.reset();
            return 0.0;
        }

        let fraction = self.position - index as f64;
        let rounded_phase = (fraction * SINC_PHASES as f64).round() as usize;
        let (base_index, phase) = if rounded_phase == SINC_PHASES {
            (index.saturating_add(1), 0)
        } else {
            (index, rounded_phase)
        };
        // A 256-phase, sixteen-tap Hann-windowed sinc keeps the approved source's top octave when
        // 44.1 kHz content is rendered by a 48 kHz host. The table is fixed project-generated data;
        // decoding it from little-endian bytes keeps the crate deterministic on every platform.
        let raw = (0..SINC_TAPS).fold(0.0, |sum, tap| {
            let source_index = base_index as isize + tap as isize - (SINC_RADIUS - 1);
            sum + sinc_weight(phase, tap) * source_sample_signed(kind, source_index)
        });

        // The closed analogue path restores a little broadband residual under its own envelope.
        // It is deterministic and addressed by the same source position, so retrigger and Pitch
        // retain the PCM clock law rather than adding host-rate random noise.
        let articulated_raw = if kind == Kind::ClosedHat {
            let next = index.saturating_add(1);
            let current_noise = source_noise(index);
            let noise = current_noise + (source_noise(next) - current_noise) * fraction as f32;
            raw + CLOSED_NOISE_AMOUNT * noise
        } else {
            raw
        };

        // Tone is an explicitly bypassed parallel colour path at zero, preserving the selected
        // source/articulation exactly. Negative values move toward a 5 kHz low-pass; positive values
        // add the complementary high band.
        let cutoff = 5_000.0_f64.min(0.44 * self.fs);
        let coefficient = (1.0 - (-std::f64::consts::TAU * cutoff / self.fs).exp()) as f32;
        self.tone_lowpass += coefficient * (articulated_raw - self.tone_lowpass);
        let tone = bipolar(patch.tone);
        let coloured = if tone < 0.0 {
            articulated_raw + (-tone) * (self.tone_lowpass - articulated_raw)
        } else {
            articulated_raw + 0.85 * tone * (articulated_raw - self.tone_lowpass)
        };

        let progress =
            (self.position / (kind.playback_length().saturating_sub(1).max(1) as f64)) as f32;
        let decay = bipolar(patch.decay);
        let decay_gain = if decay < 0.0 {
            (0.001_f32.ln() * (-decay) * progress).exp()
        } else {
            (4.0_f32.ln() * decay * progress).exp()
        };

        // At zero this is a strict bypass. Positive Attack rounds the captured onset; negative
        // values emphasize it without moving the source counter or changing its pitch law.
        let attack = bipolar(patch.attack);
        let elapsed = self.elapsed_samples as f32 / self.fs as f32;
        let attack_gain = if attack > 0.0 {
            let tau = 0.001 + 0.029 * attack;
            1.0 - (-elapsed / tau).exp()
        } else if attack < 0.0 {
            1.0 + 0.75 * (-attack) * (-elapsed / 0.008).exp()
        } else {
            1.0
        };

        let character = bipolar(patch.character);
        let shaped = if character > 0.0 {
            let drive = 1.0 + 5.0 * character;
            (coloured * drive).tanh() / drive.tanh()
        } else if character < 0.0 {
            let levels = (32.0 + 224.0 * (1.0 + character)).round().max(2.0);
            (coloured.clamp(-1.0, 1.0) * levels).round() / levels
        } else {
            coloured
        };

        // Both hats read one source table and diverge only in articulation. They do not share an
        // address counter: each slot advances its own, and pairing a closed hat to an open one is
        // a user-assigned choke group rather than hardware wiring. The measured
        // closed gain table is smooth at 44.1 kHz; linear interpolation keeps it clock-coupled to
        // Pitch and reaches its exact address stop without a second sample source.
        let articulation_gain = if kind == Kind::ClosedHat {
            closed_envelope(self.position)
        } else {
            1.0
        };

        let ratio =
            2.0_f64.powf(f64::from(finite(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0)) / 12.0);
        self.position += SOURCE_RATE * ratio / self.fs;
        self.elapsed_samples = self.elapsed_samples.saturating_add(1);

        let output = shaped * self.gain * decay_gain * attack_gain * articulation_gain;
        if !output.is_finite() {
            self.reset();
            return 0.0;
        }
        output.clamp(-4.0, 4.0)
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset(&mut self) {
        self.position = 0.0;
        self.elapsed_samples = 0;
        self.gain = 0.0;
        self.tone_lowpass = 0.0;
        self.active = false;
    }
}

#[cfg(test)]
fn source_sample(kind: Kind, index: usize) -> f32 {
    source_sample_signed(kind, index as isize)
}

fn source_sample_signed(kind: Kind, index: isize) -> f32 {
    if index < 0 {
        return 0.0;
    }
    let data = kind.data();
    let byte = index as usize * 2;
    if byte + 1 >= data.len() {
        return 0.0;
    }
    f32::from(i16::from_le_bytes([data[byte], data[byte + 1]])) / 32_768.0
}

fn closed_envelope(position: f64) -> f32 {
    let index = position.floor() as usize;
    let fraction = (position - index as f64) as f32;
    let current = closed_envelope_sample(index);
    current + (closed_envelope_sample(index.saturating_add(1)) - current) * fraction
}

fn closed_envelope_sample(index: usize) -> f32 {
    let byte = index * 4;
    if byte + 3 >= CLOSED_ENVELOPE.len() {
        return 0.0;
    }
    f32::from_le_bytes([
        CLOSED_ENVELOPE[byte],
        CLOSED_ENVELOPE[byte + 1],
        CLOSED_ENVELOPE[byte + 2],
        CLOSED_ENVELOPE[byte + 3],
    ])
}

fn source_noise(index: usize) -> f32 {
    let mut value = index as u64 ^ 0x9e37_79b9_7f4a_7c15;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let unit = ((value ^ (value >> 31)) >> 40) as f32 * (1.0 / 16_777_216.0);
    2.0 * unit - 1.0
}

fn sinc_weight(phase: usize, tap: usize) -> f32 {
    let byte = (phase * SINC_TAPS + tap) * 4;
    f32::from_le_bytes([
        SINC_TABLE[byte],
        SINC_TABLE[byte + 1],
        SINC_TABLE[byte + 2],
        SINC_TABLE[byte + 3],
    ])
}

fn bipolar(value: f32) -> f32 {
    finite(value, 0.0).clamp(-1.0, 1.0)
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [Kind; 4] = [Kind::ClosedHat, Kind::OpenHat, Kind::Crash, Kind::Ride];

    #[test]
    fn embedded_sources_are_distinct_bounded_and_well_formed() {
        let expected_source_lengths = [24_784, 24_784, 23_257, 21_737];
        let expected_playback_lengths = [13_920, 24_784, 23_257, 21_737];
        for ((kind, expected_source), expected_playback) in KINDS
            .into_iter()
            .zip(expected_source_lengths)
            .zip(expected_playback_lengths)
        {
            assert_eq!(kind.data().len() % 2, 0);
            assert_eq!(kind.source_length(), expected_source);
            assert_eq!(kind.playback_length(), expected_playback);
            let peak = (0..kind.source_length())
                .map(|index| source_sample(kind, index).abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.1 && peak <= 1.0, "{kind:?}: {peak}");
        }
        assert_eq!(CLOSED_ENVELOPE.len() % 4, 0);
        assert!((0..CLOSED_ENVELOPE.len() / 4).all(|index| {
            let gain = closed_envelope_sample(index);
            gain.is_finite() && (0.0..=1.0).contains(&gain)
        }));
        assert_eq!(Kind::ClosedHat.data(), Kind::OpenHat.data());
        assert_ne!(HAT, CRASH);
        assert_ne!(CRASH, RIDE);
    }

    #[test]
    fn every_source_renders_finite_audio_and_reset_is_exact() {
        let patch = SlotPatch::default();
        for kind in KINDS {
            let mut voice = PcmMetal::new();
            voice.trigger(1.0, patch);
            let output: Vec<_> = (0..4_096).map(|_| voice.process(kind, patch)).collect();
            assert!(
                output
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 4.0)
            );
            assert!(output.iter().any(|sample| sample.abs() > 0.01));
            voice.reset();
            assert_eq!(voice.process(kind, patch), 0.0);
        }
    }

    #[test]
    fn zero_controls_play_the_embedded_source_without_a_second_colour_stage() {
        let patch = SlotPatch::default();
        let mut voice = PcmMetal::new();
        voice.set_sample_rate(SOURCE_RATE as f32);
        voice.trigger(1.0, patch);
        let expected_gain = 0.65 + 0.35 * 0.5;
        for index in 0..512 {
            let actual = voice.process(Kind::Crash, patch);
            let expected = source_sample(Kind::Crash, index) * expected_gain;
            assert!(
                (actual - expected).abs() < 1.0e-6,
                "sample {index}: {actual} != {expected}"
            );
        }
    }

    #[test]
    fn both_hats_read_one_source_but_take_distinct_articulation_paths() {
        let patch = SlotPatch::default();
        let render = |kind| {
            let mut voice = PcmMetal::new();
            voice.set_sample_rate(SOURCE_RATE as f32);
            voice.trigger(1.0, patch);
            (0..16_000)
                .map(|_| voice.process(kind, patch))
                .collect::<Vec<_>>()
        };
        let closed = render(Kind::ClosedHat);
        let open = render(Kind::OpenHat);
        let closed_late: f32 = closed[4_410..6_615]
            .iter()
            .map(|sample| sample * sample)
            .sum();
        let open_late: f32 = open[4_410..6_615]
            .iter()
            .map(|sample| sample * sample)
            .sum();
        assert!(closed_late < 0.05 * open_late);
        assert!(closed[14_000..].iter().all(|sample| *sample == 0.0));
        assert!(open[14_000..].iter().any(|sample| sample.abs() > 1.0e-4));
    }

    #[test]
    fn pitch_clock_couples_pitch_and_duration() {
        let render = |pitch| {
            let patch = SlotPatch {
                pitch_semitones: pitch,
                ..SlotPatch::default()
            };
            let mut voice = PcmMetal::new();
            voice.trigger(1.0, patch);
            let mut frames = 0;
            while voice.is_active() && frames < 200_000 {
                let _ = voice.process(Kind::Crash, patch);
                frames += 1;
            }
            frames
        };
        assert!(render(12.0) < render(0.0));
    }
}
