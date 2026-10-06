//! Remaining analogue TR-909 family: triple bridged-T rim and four-cell noise clap.
//! Topology follows `research:instruments/analogue-drum-machines.md` §§4.6–4.7. Neither primary
//! calibration targets nor hardware measurements close the exact constants; the reference values
//! below were fitted to the acquired comparison recordings in the 2026-09-19 A/B pass and each names
//! the measurement that could replace it.
use crate::{engine::SlotPatch, resonator::Resonator};
const LN_001: f32 = -6.907_755_4;
/// Rim constants fitted to the acquired comparison recording (unaccented, fixed voice) in the
/// 2026-09-19 A/B pass, refitted after listening: a least-squares fit of this topology against the
/// recording's band levels (150–300 Hz, 400–650 Hz, 800–1300 Hz and above 2 kHz in 3 ms windows over
/// 57 ms), its 0.5 ms level over the first 8 ms, and its rendered spectra over 0–10, 0–25 and
/// 12–50 ms. A strike from both edges of a 3.6 ms trigger combed the spectrum, putting its peaks
/// 6–14 % low (187/448/1017 Hz against the recording's 215/476/1133 Hz); one strike under the VCA's
/// recovery fits better on every term. Each constant names the measurement that could replace it.
mod rim {
    /// Network frequencies in Hz: the recording's ring after 15 ms reads 220 and 480 Hz, and its
    /// first 10 ms peak at 215, 476 and 1133 Hz with the clipper's products at 861, 1438 and
    /// 1884 Hz. These render at 222/493/1135 Hz with products at 853/1427/1789 Hz. Replace with
    /// each bridged-T's measured centre.
    pub const FREQUENCIES: [f32; 3] = [220.0, 480.0, 1_132.0];
    /// Each network's T60 in seconds: the three bands fall about 1, 2 and 4.6 dB/ms. Replace with
    /// each network's measured Q.
    pub const T60: [f32; 3] = [0.052, 0.0285, 0.0129];
    /// Each network's weight into the summing clipper. Replace with the summing resistors.
    pub const WEIGHTS: [f32; 3] = [0.90, 2.29, 8.80];
    /// Clipper drive and output high-pass corner in Hz. Replace with the diode clipper's transfer
    /// curve and the high-pass network.
    pub const DRIVE: f32 = 3.83;
    pub const HIGH_PASS_HZ: f32 = 26.8;
    /// VCA recovery after the trigger's cut-off: gain `(1 − e^(−t/τ))²`. The recording rises from
    /// 27 dB under its peak in its first half-millisecond to within 7 dB by 1.5 ms, the low band
    /// building for 6–9 ms. Replace with the VCA control voltage.
    pub const VCA_OPEN_TIME: f32 = 0.00173;
    /// VCA T60 in seconds: the recording's decay is the networks' own, so the VCA stays nearly
    /// open over the hit. Replace with the VCA's envelope voltage.
    pub const VCA_T60: f32 = 1.0;
}

#[derive(Debug, Clone)]
pub struct TripleRim {
    fs: f32,
    modes: [Resonator; 3],
    strike: f32,
    vca: f32,
    vca_open: f32,
    lp: f32,
    active: bool,
    age: u32,
}
impl Default for TripleRim {
    fn default() -> Self {
        Self::new()
    }
}
impl TripleRim {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fs: 48_000.0,
            modes: [Resonator::new(); 3],
            strike: 0.0,
            vca: 0.0,
            vca_open: 0.0,
            lp: 0.0,
            active: false,
            age: 0,
        }
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.fs = rate(v)
    }
    pub fn trigger(&mut self, velocity: f32, p: SlotPatch) {
        let level = accent(velocity, p.dynamics);
        // The trigger strikes all three networks together.
        self.strike = (self.strike + level).min(2.0);
        self.vca = (self.vca + level).min(2.0);
        self.vca_open = 0.0;
        self.age = 0;
        self.active = true
    }
    #[must_use]
    pub fn process(&mut self, p: SlotPatch) -> f32 {
        if !self.active {
            return 0.0;
        }
        let ratio = 2.0_f32.powf(finite(p.pitch_semitones, 0.0).clamp(-24.0, 24.0) / 12.0);
        let decay = extended_decay_scale(bipolar(p.decay));
        let tone = bipolar(p.tone);
        let excitation = self.strike;
        self.strike = 0.0;
        let mut sum = 0.0;
        for i in 0..3 {
            self.modes[i].configure(self.fs, rim::FREQUENCIES[i] * ratio, rim::T60[i] * decay);
            let weight = rim::WEIGHTS[i] * [1.0 - 0.2 * tone, 1.0, 1.0 + 0.3 * tone][i];
            sum += weight * self.modes[i].process_strike(excitation)
        }
        let drive = rim::DRIVE + 1.2 * bipolar(p.character);
        let clipped = (sum * drive).tanh();
        let cutoff = (rim::HIGH_PASS_HZ * 2.0_f32.powf(2.0 * tone)).clamp(20.0, 0.4 * self.fs);
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / self.fs).exp();
        self.lp = flush(self.lp + a * (clipped - self.lp));
        let hp = clipped - self.lp;
        let attack = 1.0 + 0.35 * bipolar(p.attack);
        let body = 1.0 + 0.3 * bipolar(p.body);
        self.vca_open +=
            (1.0 - self.vca_open) * (1.0 - (-1.0 / (rim::VCA_OPEN_TIME * self.fs)).exp());
        let open = self.vca_open * self.vca_open;
        let out = (hp * open * self.vca * body * attack).clamp(-1.5, 1.5);
        self.vca = flush(self.vca * (LN_001 / (rim::VCA_T60 * decay * self.fs)).exp());
        self.age = self.age.saturating_add(1);
        if self.age > (self.fs * 1.0) as u32 && self.vca < 1e-8 {
            self.reset();
            return 0.0;
        }
        out
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        for m in &mut self.modes {
            m.reset()
        }
        self.strike = 0.0;
        self.vca = 0.0;
        self.vca_open = 0.0;
        self.lp = 0.0;
        self.active = false;
        self.age = 0
    }
}
/// Clap constants fitted to the acquired comparison recording (unaccented, fixed voice) in the
/// 2026-09-19 A/B pass and refitted after listening, against the recording's level in 1 ms and
/// 5 ms windows and its octave balance over the bursts (8–30 ms) and the tail (50–200 ms).
/// Replace with the four cells' and the tail's control voltages and each path's response.
mod clap {
    /// The tail VCA opens at the trigger; the first cell fires 8.9 ms later and each cell hands on
    /// to the next 10.9 ms after it starts, where the recording's first two bursts break. Its
    /// first 9 ms carry only the tail, about 20 dB under the first burst.
    pub const FIRST_CELL_MS: f32 = 8.9;
    pub const SPACING_MS: f32 = 10.9;
    /// The first two cells snap shut (τ = 2.5 ms): each of the recording's first two bursts falls
    /// about 3.8 dB/ms over its first 5 ms (1 ms windows), where 2.1 ms fell 4–5 dB/ms and left
    /// 10–30 ms 2 dB under it at equal loudness. The last two are 12 dB quieter and slow
    /// (τ = 14 ms), holding the recording's 18–28 dB level from 30 to 75 ms within 2 dB.
    pub const EARLY_CELL_MS: f32 = 2.5;
    pub const LATE_CELL_LEVEL: f32 = 0.26;
    pub const LATE_CELL_MS: f32 = 14.0;
    /// Tail VCA level against a cell, and its time constant in milliseconds: the recording's tail
    /// falls about 0.09 dB/ms from 60 to 390 ms.
    pub const TAIL_LEVEL: f32 = 0.16;
    pub const TAIL_MS: f32 = 99.0;
    /// The cells and the tail carry different colours. The bursts are broad, peaking near 1 kHz
    /// and flat to 4 kHz, with a steep cut below 400 Hz; the tail centres near 1 kHz and falls
    /// steeply above it. A first fit put the cell and tail bands at 1246 and 801 Hz; listening
    /// found the model lower in pitch, its bursts 7–10 dB heavy from 200 to 800 Hz and its tail
    /// 3.6 dB heavy from 400 to 800 Hz, so both bands moved up (cells 1450 Hz with a 400 Hz cut,
    /// tail 1040 Hz with a 1.5 kHz top). The tail's octave balance then lands within 1.1 dB of the
    /// recording's. Both paths filter the one machine-shared noise sample.
    pub const CELL_CENTRE_HZ: f32 = 1_450.0;
    pub const CELL_DAMPING: f32 = 0.65;
    pub const CELL_HIGH_PASS_HZ: f32 = 400.0;
    pub const CELL_LOW_PASS_HZ: f32 = 9_000.0;
    pub const TAIL_CENTRE_HZ: f32 = 1_040.0;
    pub const TAIL_DAMPING: f32 = 0.50;
    pub const TAIL_LOW_PASS_HZ: f32 = 1_500.0;
    /// The recording's tail after the cells, from about 150 ms, is darker than the colour this
    /// filter gives the gaps and the tail under the cells: with one fixed tail filter the model's
    /// 150–350 ms tail carried about 3 dB too little 200–800 Hz and 1–1.5 dB too much above
    /// 1.6 kHz against its 800–1600 Hz octave. Darkening the whole tail would also fill the gaps
    /// before each burst, which already carry more below 500 Hz than the recording's. So once
    /// the tail VCA's control voltage falls under 30 % of its start (about 120 ms at reference),
    /// the band centre and the top corner glide with it to 0.88 and 0.55 of their values at
    /// zero, and the level rises to 1.33 to hold the band's power. The 150–350 ms octaves from
    /// 200 Hz to 6.4 kHz then land within 0.6 dB rms of the recording's (2.0 dB before), each
    /// 50 ms of the tail within 0.3 dB of its former level, and 0–120 ms is unchanged. Replace
    /// with the tail path's response at low control voltage.
    pub const TAIL_GLIDE_KNEE: f32 = 0.3;
    pub const TAIL_LATE_CENTRE: f32 = 0.88;
    pub const TAIL_LATE_TOP: f32 = 0.55;
    pub const TAIL_LATE_GAIN: f32 = 1.33;
    /// Output coupling: two one-pole high-passes. Below 300 Hz the recording falls about 15 dB per
    /// octave in both the bursts and the tail.
    pub const OUTPUT_HIGH_PASS_HZ: f32 = 150.0;
    /// Gain into the bounded output stage, low enough that the stage stays within 2 % of linear
    /// on the reference hit; Character's drive is left to the band-pass damping.
    pub const DRIVE: f32 = 0.4;
}

#[derive(Debug, Clone)]
pub struct FourCellClap {
    fs: f32,
    age: u32,
    level: f32,
    tail: f32,
    cell_filter: [f32; 2],
    cell_high_pass: [f32; 2],
    cell_low_pass: f32,
    tail_filter: [f32; 2],
    tail_low_pass: f32,
    output_high_pass: [f32; 2],
    active: bool,
}
impl Default for FourCellClap {
    fn default() -> Self {
        Self::new()
    }
}
/// The clap tail's VCA control at `ms`, which is also how much of a previous hit's tail is left
/// when the next one arrives.
fn clap_tail_envelope(ms: f32, decay: f32) -> f32 {
    (-ms / (clap::TAIL_MS * decay)).exp()
}

/// The four-cell burst chain's firing pattern at `ms`: two early cells and two quieter late ones.
/// Zero once every cell has finished, which is what says the cluster is over.
fn clap_cluster(ms: f32, spacing: f32, noise_time: f32) -> f32 {
    let mut cluster = 0.0;
    for i in 0..4 {
        let t = ms - clap::FIRST_CELL_MS - i as f32 * spacing;
        let (level, time) = if i < 2 {
            (1.0, clap::EARLY_CELL_MS)
        } else {
            (clap::LATE_CELL_LEVEL, clap::LATE_CELL_MS)
        };
        if (0.0..8.0 * time * noise_time).contains(&t) {
            cluster += level * (-t / (time * noise_time)).exp()
        }
    }
    cluster
}

impl FourCellClap {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fs: 48_000.0,
            age: 0,
            level: 0.0,
            tail: 0.0,
            cell_filter: [0.0; 2],
            cell_high_pass: [0.0; 2],
            cell_low_pass: 0.0,
            tail_filter: [0.0; 2],
            tail_low_pass: 0.0,
            output_high_pass: [0.0; 2],
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.fs = rate(v)
    }
    pub fn trigger(&mut self, velocity: f32, p: SlotPatch) {
        let x = accent(velocity, p.dynamics);
        // Add to what is still sounding, not to the level the previous hit started from. Both
        // amplitudes are static and scaled by an age-derived envelope, so adding to the stored
        // value let a hit long after the last one inherit its full initial charge — a retrigger
        // near the end of the tail came out roughly twice a fresh hit with nothing audible left.
        let ms = self.age as f32 * 1000.0 / self.fs;
        let (cells, tail) = if self.active {
            let spacing = clap::SPACING_MS * 2.0_f32.powf(0.35 * bipolar(p.attack));
            (
                clap_cluster(ms, spacing, envelope_time_scale(bipolar(p.noise_decay))).min(1.0),
                clap_tail_envelope(ms, extended_decay_scale(bipolar(p.decay))),
            )
        } else {
            (0.0, 0.0)
        };
        self.level = (self.level * cells + x).min(2.0);
        self.tail = (self.tail * tail + x).min(2.0);
        self.age = 0;
        self.active = true
    }
    #[must_use]
    pub fn process(&mut self, p: SlotPatch, noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let tone = 2.0_f32.powf(0.55 * bipolar(p.tone));
        let character = bipolar(p.character);
        let noise = finite(noise, 0.0);
        let fs = self.fs;
        let cell_band = svf_band(
            &mut self.cell_filter,
            noise,
            clap::CELL_CENTRE_HZ * tone,
            clap::CELL_DAMPING + 0.3 * character,
            fs,
        );
        let mut cells = cell_band;
        // Two cascaded one-pole high-passes: the bursts' steep cut below 400 Hz.
        for state in &mut self.cell_high_pass {
            *state = flush(*state + one_pole(clap::CELL_HIGH_PASS_HZ, fs) * (cells - *state));
            cells -= *state;
        }
        self.cell_low_pass = flush(
            self.cell_low_pass
                + one_pole(clap::CELL_LOW_PASS_HZ, fs) * (cells - self.cell_low_pass),
        );
        let ms = self.age as f32 * 1000.0 / self.fs;
        let decay = extended_decay_scale(bipolar(p.decay));
        let tail_envelope = clap_tail_envelope(ms, decay);
        // Below the knee the tail's colour follows its VCA's control voltage down.
        let glide =
            ((clap::TAIL_GLIDE_KNEE - tail_envelope) / clap::TAIL_GLIDE_KNEE).clamp(0.0, 1.0);
        let tail_band = svf_band(
            &mut self.tail_filter,
            noise,
            clap::TAIL_CENTRE_HZ * tone * (1.0 - (1.0 - clap::TAIL_LATE_CENTRE) * glide),
            clap::TAIL_DAMPING + 0.12 * character,
            fs,
        );
        let top = clap::TAIL_LOW_PASS_HZ * (1.0 - (1.0 - clap::TAIL_LATE_TOP) * glide);
        self.tail_low_pass =
            flush(self.tail_low_pass + one_pole(top, fs) * (tail_band - self.tail_low_pass));
        let spacing = clap::SPACING_MS * 2.0_f32.powf(0.35 * bipolar(p.attack));
        let noise_time = envelope_time_scale(bipolar(p.noise_decay));
        let cluster = clap_cluster(ms, spacing, noise_time);
        let tail = self.tail * tail_envelope * (1.0 + (clap::TAIL_LATE_GAIN - 1.0) * glide);
        let noise_mix = 0.65 + 0.35 * bipolar(p.noise);
        let mut sum = self.cell_low_pass * cluster * self.level
            + clap::TAIL_LEVEL * self.tail_low_pass * tail;
        for state in &mut self.output_high_pass {
            *state = flush(*state + one_pole(clap::OUTPUT_HIGH_PASS_HZ, fs) * (sum - *state));
            sum -= *state;
        }
        let out = (clap::DRIVE * sum * noise_mix).tanh();
        self.age = self.age.saturating_add(1);
        // Retire on the envelopes as well as the sample. The output is noise-driven and crosses
        // zero constantly, so one near-zero sample says nothing about whether the voice is over:
        // at maximum Decay the tail is still well up at 800 ms and a crossing could truncate it.
        if ms > 800.0 && cluster == 0.0 && tail.abs() < 1.0e-6 && out.abs() < 1e-7 {
            self.reset();
            return 0.0;
        }
        out
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        self.age = 0;
        self.level = 0.0;
        self.tail = 0.0;
        self.cell_filter = [0.0; 2];
        self.cell_high_pass = [0.0; 2];
        self.cell_low_pass = 0.0;
        self.tail_filter = [0.0; 2];
        self.tail_low_pass = 0.0;
        self.output_high_pass = [0.0; 2];
        self.active = false
    }
}
/// One Chamberlin state-variable step; returns the band-pass output. `state` is `[low, band]`.
fn svf_band(state: &mut [f32; 2], input: f32, centre: f32, damping: f32, fs: f32) -> f32 {
    let centre = centre.clamp(20.0, 0.2 * fs);
    let f = (2.0 * (std::f32::consts::PI * centre / fs).sin()).min(0.95);
    let damping = damping.clamp(0.05, 1.5);
    state[0] = flush(state[0] + f * state[1]);
    let high = input - state[0] - damping * state[1];
    state[1] = flush(state[1] + f * high);
    state[1]
}
/// One-pole smoothing coefficient for a corner in Hz.
fn one_pole(hz: f32, fs: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz.min(0.4 * fs) / fs).exp()
}
/// The hit level: linear in velocity, Dynamics bending the curve (`crate::velocity`).
fn accent(v: f32, d: f32) -> f32 {
    crate::velocity::linear(v, d)
}
fn rate(v: f32) -> f32 {
    finite(v, 48_000.0).clamp(1000.0, 384_000.0)
}
fn extended_decay_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.35_f32.powf(-value)
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
fn bipolar(v: f32) -> f32 {
    finite(v, 0.0).clamp(-1.0, 1.0)
}
fn finite(v: f32, f: f32) -> f32 {
    if v.is_finite() { v } else { f }
}
fn flush(v: f32) -> f32 {
    if v.abs() < 1e-20 { 0.0 } else { v }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic noise stream, so a clap's lifetime is a property of its envelopes rather
    /// than of which sample happened to land near zero.
    fn noise_stream() -> impl FnMut() -> f32 {
        let mut state: u32 = 0x2545_f491;
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    #[test]
    fn a_late_retrigger_does_not_inherit_the_previous_hits_level() {
        // Both clap amplitudes are static values scaled by an age-derived envelope, so a hit
        // arriving once the previous one is inaudible must not add to that hit's initial charge.
        let p = SlotPatch::default();
        let peak_of = |warm_up_ms: Option<f32>| {
            let mut noise = noise_stream();
            let mut clap = FourCellClap::new();
            if let Some(ms) = warm_up_ms {
                clap.trigger(1.0, p);
                for _ in 0..((ms / 1000.0) * 48_000.0) as usize {
                    let _ = clap.process(p, noise());
                }
            }
            clap.trigger(1.0, p);
            (0..96_000).fold(0.0_f32, |peak, _| peak.max(clap.process(p, noise()).abs()))
        };
        let fresh = peak_of(None);
        let after_a_finished_hit = peak_of(Some(900.0));
        assert!(fresh > 0.0, "the clap must sound");
        assert!(
            after_a_finished_hit < fresh * 1.2,
            "a retrigger 900 ms on peaked at {after_a_finished_hit:.6} against a fresh {fresh:.6};              it inherited the previous hit's level"
        );
    }

    #[test]
    fn the_clap_tail_outlives_800_ms_at_maximum_decay() {
        // Retirement used to read one output sample, and the output is noise-driven: at maximum
        // Decay the tail is still well up at 800 ms, so a zero crossing could end the voice.
        let mut noise = noise_stream();
        let p = SlotPatch {
            decay: 1.0,
            ..SlotPatch::default()
        };
        let mut clap = FourCellClap::new();
        clap.trigger(1.0, p);
        let mut energy_after = 0.0_f32;
        for sample in 0..(2 * 48_000) {
            let out = clap.process(p, noise());
            if sample > (48_000 * 9) / 10 {
                energy_after += out.abs();
            }
        }
        assert!(
            clap.is_active(),
            "the clap retired before its extended tail was over"
        );
        assert!(
            energy_after > 0.0,
            "the clap fell silent after 900 ms at maximum Decay"
        );
    }

    #[test]
    fn both_voices_sound_remain_finite_and_reset() {
        let p = SlotPatch::default();
        let mut rim = TripleRim::new();
        rim.trigger(1.0, p);
        let e: f32 = (0..24000).map(|_| rim.process(p).abs()).sum();
        assert!(e > 1.0);
        rim.reset();
        assert_eq!(rim.process(p), 0.0);
        let mut clap = FourCellClap::new();
        clap.trigger(1.0, p);
        let e: f32 = (0..48000)
            .map(|i| clap.process(p, if i & 1 == 0 { 1.0 } else { -1.0 }).abs())
            .sum();
        assert!(e > 1.0);
        clap.reset();
        assert_eq!(clap.process(p, 1.0), 0.0)
    }
    #[test]
    fn rim_rises_through_its_vca_rather_than_starting_at_full_level() {
        // The acquired comparison recording is 27 dB under its peak for its first half
        // millisecond and peaks near 3.6 ms; the VCA's recovery gives that rise.
        let p = SlotPatch::default();
        let mut rim = TripleRim::new();
        rim.trigger(0.82, p);
        let x: Vec<f32> = (0..2_400).map(|_| rim.process(p)).collect();
        let peak = x
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let first = x[..24].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!(
            first < 0.1 * x[peak].abs(),
            "first half-millisecond {first}"
        );
        assert!(peak > 48, "peak at sample {peak}");
    }

    #[test]
    fn clap_cells_fire_after_the_tail_opens() {
        // The tail VCA opens at the trigger; the first cell fires 8.9 ms later and at least 12 dB
        // over it (the recording: about 19 dB).
        let p = SlotPatch::default();
        let mut clap = FourCellClap::new();
        clap.trigger(0.82, p);
        let mut noise = crate::reset_vco_909::HardwareNoise::new();
        let x: Vec<f32> = (0..2_400)
            .map(|_| clap.process(p, noise.tick(48_000.0)))
            .collect();
        let rms = |r: std::ops::Range<usize>| {
            (x[r.clone()].iter().map(|v| v * v).sum::<f32>() / r.len() as f32).sqrt()
        };
        let first = (clap::FIRST_CELL_MS * 48.0) as usize;
        assert!(
            rms(48..first - 24) < 0.25 * rms(first..first + 96),
            "tail {} against first cell {}",
            rms(48..first - 24),
            rms(first..first + 96)
        );
        assert!(rms(48..first - 24) > 0.0);
    }

    #[test]
    fn clap_tail_darkens_as_it_fades() {
        // Tilt of the tail, power above a two-pole 2 kHz split against power below a two-pole
        // 600 Hz one: 115–150 ms, long after the cells and as the glide starts, reads brighter
        // than 350–450 ms, where the glide has nearly finished. A fixed tail filter holds it.
        let p = SlotPatch::default();
        let mut clap = FourCellClap::new();
        clap.trigger(0.82, p);
        let mut noise = crate::reset_vco_909::HardwareNoise::new();
        let x: Vec<f32> = (0..24_000)
            .map(|_| clap.process(p, noise.tick(48_000.0)))
            .collect();
        let split = |hz: f32, high: bool| -> Vec<f32> {
            let a = 1.0 - (-std::f32::consts::TAU * hz / 48_000.0).exp();
            let mut states = [0.0_f32; 2];
            x.iter()
                .map(|v| {
                    let mut y = *v;
                    for state in &mut states {
                        *state += a * (y - *state);
                        y = if high { y - *state } else { *state };
                    }
                    y
                })
                .collect()
        };
        let (high, low) = (split(2_000.0, true), split(600.0, false));
        let tilt = |r: std::ops::Range<usize>| {
            let h: f32 = high[r.clone()].iter().map(|v| v * v).sum();
            let l: f32 = low[r].iter().map(|v| v * v).sum();
            10.0 * (h / l).log10()
        };
        let (early, late) = (tilt(5_520..7_200), tilt(16_800..21_600));
        assert!(late < early - 1.0, "tilt {early} dB early, {late} dB late");
    }

    #[test]
    fn unsupported_axes_are_exact_no_ops() {
        let render = |model: u8, p: SlotPatch| {
            if model == 22 {
                let mut v = TripleRim::new();
                v.trigger(1.0, p);
                (0..1024).map(|_| v.process(p)).collect::<Vec<_>>()
            } else {
                let mut v = FourCellClap::new();
                v.trigger(1.0, p);
                (0..1024)
                    .map(|i| v.process(p, if i & 1 == 0 { 1.0 } else { -1.0 }))
                    .collect()
            }
        };
        let p = SlotPatch::default();
        let mut x = p;
        x.noise = 1.0;
        assert_eq!(render(22, p), render(22, x));
        let mut x = p;
        x.pitch_semitones = 12.0;
        x.body = 1.0;
        assert_eq!(render(23, p), render(23, x))
    }
}
