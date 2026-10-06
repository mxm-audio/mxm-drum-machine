//! Reset-VCO bass and snare models for the TR-909 analogue family.
//!
//! `research:instruments/analogue-drum-machines.md` §§2.3, 4.2–4.5 establishes hard-reset triangle
//! VCOs, pitch envelopes, diode-to-sine shaping and a shared roughly 300 kHz hardware-noise
//! generator. Exact production tuning spans and envelope curves remain an evidence gap: the
//! reference constants below were fitted to the acquired comparison recordings (a sample pack at
//! centre settings, unaccented) in the 2026-09-19 A/B pass. They are recording fits, not
//! measurements of a trimmed unit, and each names the measurement that could replace it.

use crate::flush;
use crate::resonator::{MIN_SAMPLE_RATE, Resonator};

#[derive(Debug, Clone)]
pub struct HardwareNoise {
    state: u32,
    /// The 300 kHz clock's fractional position, in fixed point: the whole `u64` range is one
    /// clock period.
    ///
    /// Integer so a dormant bus can work out exactly how many shift steps it missed. An `f64`
    /// accumulator rounds once per sample and could only be caught up by iterating — the same
    /// reason `metal_808.rs` counts phase in integers.
    clock_phase: u64,
}
impl Default for HardwareNoise {
    fn default() -> Self {
        Self::new()
    }
}
impl HardwareNoise {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: 0x5a17_3d2b & 0x7fff_ffff,
            clock_phase: 0,
        }
    }
    #[must_use]
    pub fn tick(&mut self, sample_rate: f32) -> f32 {
        let (whole, fraction) = clock_increment(sample_rate);
        let carried = u128::from(self.clock_phase) + u128::from(fraction);
        self.clock_phase = carried as u64;
        let steps = whole + (carried >> 64) as u32;
        for _ in 0..steps {
            self.state = shift(self.state);
        }
        if self.state & 1 == 0 { -1.0 } else { 1.0 }
    }

    /// Moves the register to where running would have left it, without rendering (plan §4.4).
    ///
    /// Two exact pieces: how many times the 300 kHz clock would have fired over the gap, which
    /// the fixed-point accumulator gives in one multiply, and where that many shifts leave the
    /// register, which `jump.rs` gives in constant time because the shift is linear over GF(2).
    pub fn advance(&mut self, samples: u64, sample_rate: f32) {
        if samples == 0 {
            return;
        }
        let (whole, fraction) = clock_increment(sample_rate);
        let carried = u128::from(self.clock_phase) + u128::from(fraction) * u128::from(samples);
        self.clock_phase = carried as u64;
        let steps = u64::from(whole) * samples + (carried >> 64) as u64;
        self.state = crate::jump::advance_lfsr31(self.state, steps);
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// One step of the traced 31-stage register, taps 31 and 13.
///
/// The zero guard the engine carried is gone from the step itself and asserted instead: a maximal
/// register never reaches zero from a nonzero seed, and keeping the branch would have made the
/// map non-linear and so unjumpable.
pub(crate) const fn shift(state: u32) -> u32 {
    let feedback = ((state >> 30) ^ (state >> 12)) & 1;
    ((state << 1) | feedback) & 0x7fff_ffff
}

/// The 300 kHz clock's per-sample advance, split into whole periods and a fixed-point remainder.
fn clock_increment(sample_rate: f32) -> (u32, u64) {
    let fs = f64::from(valid_sample_rate(sample_rate));
    let per_sample = 300_000.0 / fs;
    let whole = per_sample.floor();
    (
        whole as u32,
        ((per_sample - whole) * 18_446_744_073_709_551_616.0) as u64,
    )
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
/// The shape one reset triangle VCO leaves its diode clipper with.
#[derive(Debug, Clone, Copy)]
struct VcoShape {
    /// Fraction of each period spent rising. Unequal integrator charge and discharge currents skew
    /// the triangle; 0.5 is a symmetric triangle.
    rise: f32,
    /// Symmetric hard clip of the triangle before the curve, as a fraction of its amplitude and
    /// renormalised to full scale; 1 is none.
    clip: f32,
    /// How far the diode clipper curves the positive half toward sine: 0 is the raw triangle, 1
    /// the full `sin(π/2·x)` curve.
    rounding: f32,
    /// The same for the negative half.
    negative_rounding: f32,
    /// Hard clip of the negative half, as a fraction of the triangle's amplitude; 1 is none.
    floor: f32,
}

#[derive(Debug, Clone, Copy)]
struct ResetTriangle {
    phase: f64,
}
impl ResetTriangle {
    const fn new() -> Self {
        Self { phase: 0.0 }
    }
    /// A trigger discharges the integrator capacitor, so the triangle restarts from its centre —
    /// a zero crossing — with the comparator forced to the rising state
    /// (`research:instruments/analogue-drum-machines.md` §2.3).
    fn reset_to_centre(&mut self, shape: VcoShape) {
        self.reset_to(0.5 * shape.rise);
    }
    fn reset_to(&mut self, phase: f32) {
        self.phase = f64::from(phase).rem_euclid(1.0);
    }
    fn tick(&mut self, fs: f32, hz: f32, shape: VcoShape) -> f32 {
        let phase = self.phase as f32;
        let rise = shape.rise;
        let triangle = if phase < rise {
            -1.0 + 2.0 * phase / rise
        } else {
            1.0 - 2.0 * (phase - rise) / (1.0 - rise)
        };
        self.phase = (self.phase + f64::from(hz.clamp(1.0, 0.45 * fs)) / f64::from(fs)).fract();
        let triangle = triangle.clamp(-shape.clip, shape.clip) / shape.clip;
        let rounding = if triangle >= 0.0 {
            shape.rounding
        } else {
            shape.negative_rounding
        };
        let curved =
            (1.0 - rounding) * triangle + rounding * (std::f32::consts::FRAC_PI_2 * triangle).sin();
        curved.max(-shape.floor)
    }
}

/// Trigger length in seconds. The about-2 ms trigger holds a reset VCO's integrator discharged and
/// its VCA cut off (`research:instruments/analogue-drum-machines.md` §4.4, §4.10); in the acquired
/// comparison recordings the kick's tone starts 1.9 ms after its first rise and the mid tom steps
/// in 1.9 ms after its onset (2026-09-19 A/B pass). Replace with the trigger pulse on a scope.
const TRIGGER_HOLD: f32 = 0.0019;

/// Kick constants fitted to the acquired comparison recording (Mid Tone / Mid Attack / Mid Decay,
/// unaccented) in the 2026-09-19 A/B pass. Each names the measurement that could replace it.
mod kick {
    use super::VcoShape;
    /// Rest pitch in Hz: the settled cycle length over 60–140 ms of the recording. Replace with a
    /// trimmed unit's TUNE-centre frequency.
    pub const REST_HZ: f32 = 54.0;
    /// Linear-frequency sweep depth: the swept part of the frequency is `REST_HZ × SWEEP_DEPTH` at
    /// the trigger. The VCO's frequency follows its control current linearly, so the exponential
    /// control transient decays in hertz, not in semitones: `54·(1 + 4.95·e^(−t/12.4 ms))` Hz from
    /// the trigger, with the tone released 1.9 ms later, places the recording's first eighteen
    /// zero crossings within 0.15 ms. Replace with a control-voltage trace at the VCO.
    pub const SWEEP_DEPTH: f32 = 4.95;
    /// Sweep time constant in seconds (not a T60): the same fit.
    pub const SWEEP_TIME: f32 = 0.0124;
    /// VCA recovery time constant after the trigger releases it, in seconds: the recording's first
    /// half-cycle reaches 0.6 of the next. Replace with the VCA control voltage.
    pub const VCA_OPEN_TIME: f32 = 0.0008;
    /// Envelope time constant in seconds and the VCA threshold, as a fraction of the start level.
    /// The recording's decay accelerates — −6 dB at 64 ms, −21 dB at 128 ms, −37 dB at 165 ms and
    /// gone by about 175 ms — which an exponential capacitor voltage crossing a VCA conduction
    /// threshold reproduces: gain `(1 + c)·e^(−t/τ) − c`, zero after `τ·ln((1 + c)/c)`. Least
    /// squares over the half-cycle peaks gave τ = 75 ms, c = 0.11. Replace with the envelope
    /// voltage and the VCA's transfer curve.
    pub const DECAY_TIME: f32 = 0.075;
    pub const VCA_THRESHOLD: f32 = 0.11;
    /// Rise fraction and clipper rounding. The recording's settled cycle keeps the triangle's third
    /// harmonic (−20 dB) and a −28 dB second: a 53 % rise with 30 % sine rounding landed within
    /// 4 dB on harmonics 2–7 of the settled ring, but over the loud body (0–68 ms, A-weighted) left
    /// the sixth to eighth harmonics 4–7 dB short; 15 % rounding closes that. Replace with the VCO
    /// and clipper outputs on a scope.
    pub const SHAPE: VcoShape = VcoShape {
        rise: 0.53,
        clip: 1.0,
        rounding: 0.15,
        negative_rounding: 0.15,
        floor: 1.0,
    };
    /// Tone-path low-pass corner in Hz. The recording's first 20 ms fall 8–10 dB below a raw
    /// triangle's corners from 1.6 to 5 kHz and its cycles have rounded corners; one pole at 1 kHz
    /// reproduces both. Replace with the voice's measured output response.
    pub const TONE_CORNER_HZ: f32 = 1_000.0;
    /// Attack-path pulse: the trigger through two low-pass poles
    /// (`research:instruments/analogue-drum-machines.md` §4.3). During the trigger the recording
    /// rises smoothly from nothing, through 1 % of its peak at 0.8 ms, to 4.2 % at 1.9 ms; two
    /// 2.3 ms poles give that curve. Replace with the attack-path output at Mid Attack.
    pub const CLICK_TIME: f32 = 0.0023;
    pub const CLICK_LEVEL: f32 = 0.10;
    /// Low-passed shared noise, mixed into the pulse before the attack VCA. The recording's ramp is
    /// smooth to about 1 % of its height, so the noise is small at Mid Attack. Replace with the
    /// attack path's noise level and corner.
    pub const NOISE_LEVEL: f32 = 0.02;
    pub const NOISE_CORNER_HZ: f32 = 3_000.0;
}

#[derive(Debug, Clone)]
pub struct ResetKick {
    sample_rate: f32,
    osc: ResetTriangle,
    pitch_env: f32,
    amp_env: f32,
    level: f32,
    vca_open: f32,
    click: [f32; 2],
    noise_lp: f32,
    tone_lp: f32,
    age: u32,
    active: bool,
}
impl Default for ResetKick {
    fn default() -> Self {
        Self::new()
    }
}
impl ResetKick {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            osc: ResetTriangle::new(),
            pitch_env: 0.0,
            amp_env: 0.0,
            level: 0.0,
            vca_open: 0.0,
            click: [0.0; 2],
            noise_lp: 0.0,
            tone_lp: 0.0,
            age: 0,
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.sample_rate = valid_sample_rate(v);
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        self.osc.reset_to_centre(kick::SHAPE);
        self.pitch_env = 1.0;
        self.amp_env = (self.amp_env + level).min(2.0);
        self.level = level;
        self.vca_open = 0.0;
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, patch: Patch, shared_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        // The shared bus is sanitised here rather than trusted: `flush` passes a non-finite
        // value through, so one bad sample would otherwise sit in the recursive state for good.
        let shared_noise = finite_or(shared_noise, 0.0);
        let fs = self.sample_rate;
        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let body = finite_bipolar(patch.body);
        let base = kick::REST_HZ * 2.0_f32.powf(pitch / 12.0);
        let pitch_depth = envelope_depth(finite_bipolar(patch.pitch_envelope));
        let frequency =
            base * (1.0 + kick::SWEEP_DEPTH * (1.0 + 0.5 * body) * pitch_depth * self.pitch_env);
        // Reset-VCO strike phase: the trigger holds the integrator at its centre, then releases it.
        let held = self.age < (TRIGGER_HOLD * fs) as u32;
        let oscillator = if held {
            0.0
        } else {
            self.vca_open +=
                (1.0 - self.vca_open) * (1.0 - (-1.0 / (kick::VCA_OPEN_TIME * fs)).exp());
            self.osc.tick(fs, frequency, kick::SHAPE)
        };
        let pitch_time = kick::SWEEP_TIME * envelope_time_scale(finite_bipolar(patch.pitch_decay));
        self.pitch_env = flush(self.pitch_env * (-1.0 / (pitch_time * fs)).exp());
        // Extended C8 modifications establish the direction and multi-second behavior; 12× is a
        // CHOSEN bounded endpoint, not a measured unit (`research:instruments/analogue-drum-machines.md` §4.11).
        let decay = scale_decay(kick::DECAY_TIME, finite_bipolar(patch.decay), 0.25, 12.0);
        self.amp_env = flush(self.amp_env * (-1.0 / (decay * fs)).exp());
        let gain = ((1.0 + kick::VCA_THRESHOLD) * self.amp_env - kick::VCA_THRESHOLD * self.level)
            .max(0.0)
            * self.vca_open;
        let gate = if held { self.level } else { 0.0 };
        let click_time = kick::CLICK_TIME * envelope_time_scale(finite_bipolar(patch.noise_decay));
        let click_pole = 1.0 - (-1.0 / (click_time * fs)).exp();
        self.click[0] = flush(self.click[0] + click_pole * (gate - self.click[0]));
        self.click[1] = flush(self.click[1] + click_pole * (self.click[0] - self.click[1]));
        let lp = 1.0 - (-std::f32::consts::TAU * kick::NOISE_CORNER_HZ.min(0.4 * fs) / fs).exp();
        self.noise_lp = flush(self.noise_lp + lp * (shared_noise - self.noise_lp));
        // ATTACK sets the attack path's amount before it joins the tone (§4.3); −1 removes it.
        let attack_axis = finite_bipolar(patch.attack);
        let attack_amount = if attack_axis < 0.0 {
            1.0 + attack_axis
        } else {
            1.0 + 2.0 * attack_axis
        };
        let attack = attack_amount
            * self.click[1]
            * (kick::CLICK_LEVEL
                + kick::NOISE_LEVEL * self.noise_lp * (1.0 + finite_bipolar(patch.noise)));
        let tone = finite_bipolar(patch.tone);
        let tone_pole =
            1.0 - (-std::f32::consts::TAU * kick::TONE_CORNER_HZ.min(0.4 * fs) / fs).exp();
        self.tone_lp = flush(self.tone_lp + tone_pole * (oscillator * gain - self.tone_lp));
        // 0.5: level into the bounded output stage, keeping the fitted wave nearly linear there.
        let body_signal = 0.5 * self.tone_lp * (1.0 + 0.25 * body);
        let output = (body_signal * (1.0 + 0.45 * tone) + attack).tanh();
        let _unsupported = finite_bipolar(patch.character);
        self.age = self.age.saturating_add(1);
        if self.age > (2.5 * fs) as u32 && self.amp_env < 1.0e-8 && self.click[1] < 1.0e-8 {
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
        *self = Self {
            sample_rate: self.sample_rate,
            ..Self::new()
        };
    }
}

/// Snare constants fitted to the acquired comparison recording (Mid Tuning / Mid Tone / Mid Snap,
/// unaccented) in the 2026-09-19 A/B pass. Each names the measurement that could replace it.
mod snare {
    use super::VcoShape;
    /// The two VCOs' rest frequencies in Hz. A two-sine least-squares fit to the first 45 ms, low-
    /// passed at 700 Hz, settles at 177 and 262 Hz; the late ring's strongest partial reads
    /// 178–180 Hz. Replace with each VCO's frequency at the TUNE centre.
    pub const REST_HZ: [f32; 2] = [178.0, 262.0];
    /// Linear-frequency sweep shared by both VCOs, as the kick's: `f·(1 + 1.14·e^(−t/6.2 ms))`
    /// from the same fit. Replace with the control-voltage trace.
    pub const SWEEP_DEPTH: f32 = 1.14;
    pub const SWEEP_TIME: f32 = 0.0062;
    /// VCO 2's level against VCO 1: −12.6 dB in the same fit. Replace with the two VCA outputs.
    pub const SECOND_LEVEL: f32 = 0.234;
    /// VCA recovery after the trigger's cut-off: gain `(1 − e^(−t/τ))²`, τ = 1.5 ms from the fit;
    /// the recording peaks 5.5 ms after its onset. Replace with the VCA control voltage.
    pub const VCA_OPEN_TIME: f32 = 0.0015;
    /// VCO 1's envelope decays in two slopes over 35–250 ms: τ = 22.8 ms, and a residue 30 dB
    /// down with τ = 98 ms. A two-exponential fit to its band level lands within 0.6 dB rms.
    /// Replace with the envelope voltage and the VCA's transfer curve.
    pub const FIRST_TIME: f32 = 0.0228;
    pub const FIRST_RESIDUE: f32 = 0.031;
    pub const RESIDUE_TIME: f32 = 0.098;
    /// VCO 2's envelope accelerates away, as a capacitor voltage crossing a VCA conduction
    /// threshold: gain `(1 + c)·e^(−t/τ) − c`, τ = 31 ms, c = 0.06, silent after 89 ms. Fitted to
    /// its 245–280 Hz band level over 30–80 ms. Its band then holds about 24 dB under its 35 ms
    /// level and falls with VCO 1's residue time, so it carries the same residue at 0.05. Replace
    /// as above.
    pub const SECOND_TIME: f32 = 0.031;
    pub const SECOND_THRESHOLD: f32 = 0.06;
    pub const SECOND_RESIDUE: f32 = 0.05;
    /// Both VCOs' shape: the recording's averaged settled cycle has a flat negative floor and a
    /// rounded top (second harmonic −15 dB, third −27 dB). A 60 % rise, a fully rounded positive
    /// half and a raw negative half clipped at half amplitude fit that cycle best of the shapes
    /// tried. Replace with each clipper's output on a scope.
    pub const SHAPE: VcoShape = VcoShape {
        rise: 0.60,
        clip: 1.0,
        rounding: 1.0,
        negative_rounding: 0.0,
        floor: 0.5,
    };
    /// Coupling-capacitor corner that removes the clipped wave's DC, in Hz. A chosen value below
    /// the band; replace with the output coupling network.
    pub const COUPLING_HZ: f32 = 20.0;
    /// SNAPPY noise low-pass: the recording's snappy band is flat from 3 to 8 kHz and falls
    /// about 5/12 dB in the ⅓-octave bands at 10 and 13 kHz, a fourth-order Butterworth slope near
    /// 11 kHz. A first fit at 8.5 kHz left the octave above 12.8 kHz 10 dB short of the
    /// recording's air. Replace with the snappy filter's measured response.
    pub const NOISE_LOW_PASS_HZ: f64 = 11_000.0;
    /// The high-passed half of the split: a second-order corner at 2 kHz, which holds the
    /// 1.6–3.2 kHz octave to the recording's level through the tail. The direct half fills the
    /// band below: at 0.55 of the high-passed half, the 800–1600 Hz octave over 30–300 ms sat
    /// 6–8 dB under the recording's, whose snappy noise there is broadband between VCO 1's lines
    /// and read as missing upper harmonics; at 1.3 each octave from 200 Hz to 12.8 kHz over
    /// 30–300 ms lands within 2.2 dB.
    pub const NOISE_HIGH_PASS_HZ: f64 = 2_000.0;
    pub const DIRECT_LEVEL: f32 = 1.3;
    /// Both SNAPPY envelopes fall as `(1 + c)·e^(−t/τ) − c`, τ = 52 ms, c = 0.032, silent after
    /// 182 ms: fitted to the level above 3.2 kHz over 12–180 ms within 1.8 dB rms, down through
    /// −60 dB at 170 ms rather than stopping short. Their VCA recovers with a 6 ms time constant:
    /// with the stronger direct half, a 3 ms recovery put the first 20 ms' spectral roll-off
    /// (85 % of the energy above 40 Hz) five semitones over the recording's; 6 ms keeps it within
    /// two (376 Hz against 342 Hz).
    pub const NOISE_TIME: f32 = 0.0523;
    pub const NOISE_THRESHOLD: f32 = 0.032;
    pub const NOISE_OPEN_TIME: f32 = 0.006;
    /// Tone and SNAPPY levels: VCO 1's band peaks about 15 dB under the hit's peak and the snappy
    /// band about 16 dB under it; 0.14 holds the octaves above 3.2 kHz within 1 dB of the
    /// recording's over 30–300 ms once the direct half carries the band below. Replace with the
    /// two paths' output levels.
    pub const TONE_LEVEL: f32 = 0.90;
    pub const NOISE_LEVEL: f32 = 0.14;
}

/// One RBJ biquad section with `f64` coefficients and state.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}
impl Biquad {
    const fn new() -> Self {
        Self {
            b: [0.0; 3],
            a: [0.0; 2],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }
    fn design(&mut self, fs: f32, hz: f64, q: f64, high_pass: bool) {
        let fs = f64::from(fs);
        let w = std::f64::consts::TAU * hz.min(0.45 * fs) / fs;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high_pass {
            (0.5 * (1.0 + cos), -(1.0 + cos))
        } else {
            (0.5 * (1.0 - cos), 1.0 - cos)
        };
        self.b = [b0 / a0, b1 / a0, b0 / a0];
        self.a = [-2.0 * cos / a0, (1.0 - alpha) / a0];
    }
    fn process(&mut self, input: f32) -> f32 {
        let x = f64::from(input);
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        let y = if y.is_finite() && y.abs() > 1.0e-30 {
            y
        } else {
            0.0
        };
        self.y = [y, self.y[0]];
        y as f32
    }
    fn clear(&mut self) {
        self.x = [0.0; 2];
        self.y = [0.0; 2];
    }
}

/// Fourth-order Butterworth pole quality factors.
const BUTTERWORTH_4: [f64; 2] = [0.541_196_1, 1.306_563];

#[derive(Debug, Clone)]
pub struct ResetSnare {
    sample_rate: f32,
    osc: [ResetTriangle; 2],
    pitch_env: f32,
    body_env: [f32; 3],
    noise_env: f32,
    level: f32,
    vca_open: f32,
    noise_open: f32,
    coupling: f32,
    noise_low_pass: [Biquad; 2],
    noise_high_pass: Biquad,
    age: u32,
    active: bool,
}
impl Default for ResetSnare {
    fn default() -> Self {
        Self::new()
    }
}
impl ResetSnare {
    #[must_use]
    pub fn new() -> Self {
        let mut voice = Self {
            sample_rate: 48_000.0,
            osc: [ResetTriangle::new(); 2],
            pitch_env: 0.0,
            body_env: [0.0; 3],
            noise_env: 0.0,
            level: 0.0,
            vca_open: 0.0,
            noise_open: 0.0,
            coupling: 0.0,
            noise_low_pass: [Biquad::new(); 2],
            noise_high_pass: Biquad::new(),
            age: 0,
            active: false,
        };
        voice.design_filters();
        voice
    }
    fn design_filters(&mut self) {
        for (section, q) in self.noise_low_pass.iter_mut().zip(BUTTERWORTH_4) {
            section.design(self.sample_rate, snare::NOISE_LOW_PASS_HZ, q, false);
        }
        self.noise_high_pass.design(
            self.sample_rate,
            snare::NOISE_HIGH_PASS_HZ,
            std::f64::consts::FRAC_1_SQRT_2,
            true,
        );
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.sample_rate = valid_sample_rate(v);
        self.design_filters();
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        for osc in &mut self.osc {
            osc.reset_to_centre(snare::SHAPE);
        }
        self.pitch_env = 1.0;
        for env in &mut self.body_env {
            *env = (*env + level).min(2.0);
        }
        self.noise_env = (self.noise_env + level).min(2.0);
        self.level = level;
        self.vca_open = 0.0;
        self.noise_open = 0.0;
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, patch: Patch, shared_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        // The shared bus is sanitised here rather than trusted: `flush` passes a non-finite
        // value through, so one bad sample would otherwise sit in the recursive state for good.
        let shared_noise = finite_or(shared_noise, 0.0);
        let fs = self.sample_rate;
        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let pitch_depth = envelope_depth(finite_bipolar(patch.pitch_envelope));
        let ratio =
            2.0_f32.powf(pitch / 12.0) * (1.0 + snare::SWEEP_DEPTH * pitch_depth * self.pitch_env);
        let tones = [
            self.osc[0].tick(fs, snare::REST_HZ[0] * ratio, snare::SHAPE),
            self.osc[1].tick(fs, snare::REST_HZ[1] * ratio, snare::SHAPE),
        ];
        let pitch_time = snare::SWEEP_TIME * envelope_time_scale(finite_bipolar(patch.pitch_decay));
        self.pitch_env = flush(self.pitch_env * (-1.0 / (pitch_time * fs)).exp());
        let decay = scale_decay(1.0, finite_bipolar(patch.decay), 0.35, 8.0);
        let times = [snare::FIRST_TIME, snare::RESIDUE_TIME, snare::SECOND_TIME];
        for (env, time) in self.body_env.iter_mut().zip(times) {
            *env = flush(*env * (-1.0 / (time * decay * fs)).exp());
        }
        self.vca_open += (1.0 - self.vca_open) * (1.0 - (-1.0 / (snare::VCA_OPEN_TIME * fs)).exp());
        let open = self.vca_open * self.vca_open;
        let first = self.body_env[0] + snare::FIRST_RESIDUE * self.body_env[1];
        let second = ((1.0 + snare::SECOND_THRESHOLD) * self.body_env[2]
            - snare::SECOND_THRESHOLD * self.level)
            .max(0.0)
            + snare::SECOND_RESIDUE * self.body_env[1];
        let tone = finite_bipolar(patch.tone);
        // TONE balances the two VCAs (`research:instruments/analogue-drum-machines.md` §4.4).
        let body = open
            * (tones[0] * first * (1.0 - 0.4 * tone)
                + snare::SECOND_LEVEL * tones[1] * second * (1.0 + 1.5 * tone))
            * (1.0 + 0.3 * finite_bipolar(patch.body));
        let coupling = 1.0 - (-std::f32::consts::TAU * snare::COUPLING_HZ / fs).exp();
        self.coupling = flush(self.coupling + coupling * (body - self.coupling));
        let body = body - self.coupling;
        // SNAPPY: low-passed common noise split into a direct and a high-passed path (§4.4).
        let first_section = self.noise_low_pass[0].process(shared_noise);
        let low_passed = self.noise_low_pass[1].process(first_section);
        let high_passed = self.noise_high_pass.process(low_passed);
        let attack = finite_bipolar(patch.attack);
        let noise_time = snare::NOISE_TIME
            * 1.8_f32.powf(attack)
            * envelope_time_scale(finite_bipolar(patch.noise_decay));
        self.noise_env = flush(self.noise_env * (-1.0 / (noise_time * fs)).exp());
        self.noise_open +=
            (1.0 - self.noise_open) * (1.0 - (-1.0 / (snare::NOISE_OPEN_TIME * fs)).exp());
        let noise_gain = ((1.0 + snare::NOISE_THRESHOLD) * self.noise_env
            - snare::NOISE_THRESHOLD * self.level)
            .max(0.0)
            * self.noise_open;
        let snappy = snare::NOISE_LEVEL
            * noise_gain
            * (high_passed + snare::DIRECT_LEVEL * low_passed)
            * (1.0 + finite_bipolar(patch.noise));
        let output = (snare::TONE_LEVEL * body + snappy).tanh();
        let _unsupported = finite_bipolar(patch.character);
        self.age = self.age.saturating_add(1);
        if self.age > (1.5 * fs) as u32
            && self
                .body_env
                .iter()
                .chain(std::iter::once(&self.noise_env))
                .all(|x| *x < 1.0e-8)
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
        self.osc = [ResetTriangle::new(); 2];
        self.pitch_env = 0.0;
        self.body_env = [0.0; 3];
        self.noise_env = 0.0;
        self.level = 0.0;
        self.vca_open = 0.0;
        self.noise_open = 0.0;
        self.coupling = 0.0;
        for section in &mut self.noise_low_pass {
            section.clear();
        }
        self.noise_high_pass.clear();
        self.age = 0;
        self.active = false;
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TomKind {
    Low,
    Mid,
    High,
}

/// One tom's three-VCO calibration, fitted to the acquired comparison recordings (Mid Tune / Mid
/// Decay, unaccented) in the 2026-09-19 A/B pass. Each field names the measurement that could
/// replace it.
#[derive(Debug, Clone, Copy)]
struct TomCalibration {
    /// Rest frequency of the dominant VCO in Hz: the asymptote of a linear-frequency exponential
    /// fitted to its cycle lengths over 25–300 ms. Replace with the VCO's TUNE-centre frequency.
    rest_hz: f32,
    /// The other two VCOs against the dominant one: the late ring's second tone (its odd
    /// harmonics confirm a separate oscillator), and the short upper tone of the first 60 ms.
    /// Replace with each VCO's frequency.
    ratios: [f32; 3],
    /// Linear-frequency sweep `rest·(1 + depth·e^(−t/τ))`, from the same fit: the recording's pitch
    /// sags for about a hundred milliseconds rather than snapping to rest. Replace with the
    /// control-voltage trace.
    sweep_depth: f32,
    sweep_time: f32,
    /// The phase each triangle leaves the trigger at, as a fraction of its period from the trough
    /// (0.275 is the rising centre of the 55 % rise). Fitted to each VCO's heterodyned phase in the
    /// recording. Replace with a scope capture of the reset.
    phases: [f32; 3],
    /// Each VCO's envelope time constant in seconds and VCA threshold: gain
    /// `(1 + c)·e^(−t/τ) − c`, fitted to each VCO's heterodyned or band level. Replace with the
    /// envelope voltages and the VCAs' transfer curves.
    times: [f32; 3],
    thresholds: [f32; 3],
    /// Each VCO's level before the output coupling, against the dominant VCO.
    levels: [f32; 3],
    /// Signed trigger feedthrough, against the dominant VCO's level.
    pedestal: f32,
    /// Attack transient rung by the trigger's release: level against the dominant VCO, centre in
    /// Hz and T60 in seconds. The mid and high recordings carry a strong positive pulse 1–2.5 ms
    /// after the release — their loudest peak — that the three VCOs do not make; a waveform fit of
    /// the first 30 ms (low-passed at 3 kHz) places it. The low tom shows none. Replace with the
    /// tom's attack network's output on a scope.
    attack: [f32; 3],
    /// VCO 3's VCA recovery time constant in seconds, or zero where it opens with the release.
    /// VCO 3 mixes with tom noise before its own VCA (§4.5); in the mid and high recordings its
    /// edges click from the release — the mid tom's sharp negative step at 1.9 ms and the
    /// 1.6–3.2 kHz ticks of the first cycles, whose 0–4 ms octave levels an instant opening
    /// matches within 2 dB — where the low tom's rises with its other two VCOs.
    upper_open_time: f32,
    /// Signed click at the trigger's release, against the dominant VCO's level. The low
    /// recording spikes to a quarter of its peak at the release and stays positive until its
    /// first swing, where the mid and high recordings' release click is VCO 3's own edge.
    release_click: f32,
}

impl TomKind {
    const fn calibration(self) -> TomCalibration {
        match self {
            // Second tone 60.5 Hz, upper tone 160 Hz (171 Hz while swept); sub level −11 dB after
            // the coupling, the upper tone within 3 dB of the dominant one at 10–30 ms. The sub
            // renders at the recording's 60.7 Hz over 100–400 ms. The recording falls from −20 to
            // −40 dB (1 ms windows) in 148 ms: sub and dominant envelopes of 110 and 106 ms with
            // thresholds 0.03 and 0.02 took 72 ms, these take 105 ms and hold 60–400 ms within
            // 2.4 dB of it at equal loudness.
            Self::Low => TomCalibration {
                rest_hz: 88.5,
                ratios: [0.684, 1.0, 1.825],
                sweep_depth: 0.315,
                sweep_time: 0.0395,
                phases: [0.197, 0.281, 0.691],
                times: [0.166, 0.116, 0.214],
                thresholds: [0.126, 0.033, 2.98],
                levels: [0.37, 1.0, 0.62],
                pedestal: 0.07,
                attack: [0.0, 340.0, 0.02],
                upper_open_time: 0.0012,
                release_click: 0.34,
            },
            // Second tone 74.5 Hz, upper tone 199 Hz (213 Hz while swept).
            Self::Mid => TomCalibration {
                rest_hz: 120.0,
                ratios: [0.621, 1.0, 1.663],
                sweep_depth: 0.344,
                sweep_time: 0.0392,
                phases: [0.857, 0.863, 0.873],
                times: [0.062, 0.076, 0.040],
                thresholds: [0.01, 0.02, 0.07],
                levels: [0.61, 1.0, 0.62],
                pedestal: -0.07,
                attack: [1.26, 208.5, 0.0165],
                upper_open_time: 0.0,
                release_click: 0.0,
            },
            // Second tone 91.7 Hz, upper tone 250 Hz (268 Hz while swept); the second tone renders
            // at the recording's 92.0 Hz over 100–400 ms.
            Self::High => TomCalibration {
                rest_hz: 138.7,
                ratios: [0.661, 1.0, 1.822],
                sweep_depth: 0.340,
                sweep_time: 0.0382,
                phases: [0.880, 0.979, 0.854],
                times: [0.066, 0.074, 0.029],
                thresholds: [0.01, 0.01, 0.02],
                levels: [0.50, 1.0, 0.55],
                pedestal: -0.07,
                attack: [1.18, 320.3, 0.0346],
                upper_open_time: 0.0,
                release_click: 0.0,
            },
        }
    }
    #[cfg(test)]
    const fn rest_hz(self) -> f32 {
        self.calibration().rest_hz
    }
}

/// Tom constants shared by all three calibrations, fitted in the same pass.
mod tom {
    use super::VcoShape;
    /// All three VCOs' shape: a 53 % rise clipped at half the triangle and 30 % curved toward
    /// sine, then the output coupling below. Chosen jointly on the recordings' 60–180 ms waveforms
    /// and the harmonic levels of their dominant tones: 0.20 rms waveform error and 4.7 dB mean
    /// error over harmonics 2–12 across the three toms, with the 20–150 ms octaves from 100 Hz to
    /// 1.6 kHz within about 3 dB. A first fit to averaged cycles alone (clip 0.6, fully curved)
    /// notched the fifth harmonic 20 dB low and dulled the ring's mids; a harmonic fit alone (clip
    /// 0.9, uncurved) doubled the waveform error. Replace with the clipper outputs on a scope.
    pub const SHAPE: VcoShape = VcoShape {
        rise: 0.53,
        clip: 0.5,
        rounding: 0.3,
        negative_rounding: 0.3,
        floor: 1.0,
    };
    /// Clipper hardness at the trigger and its relaxation time in seconds. VCO 1's clipper starts
    /// harder, nearer square, and relaxes toward sine under a trigger-derived envelope (§4.5); in
    /// all three recordings the first cycles' edges rise in about 0.4 ms against the settled
    /// cycle's, so every VCO here starts clipped at 0.12 of its triangle and relaxes to
    /// `SHAPE.clip`. Replace with the clipper control voltage.
    pub const CLIP_START: f32 = 0.12;
    pub const CLIP_TIME: f32 = 0.012;
    /// Output coupling high-pass corner in Hz. The droop that best fits each tom's cycle corresponds
    /// to one fixed corner near 130 Hz in all three, not to one ratio of the tom's pitch. Replace
    /// with the output coupling network.
    pub const COUPLING_HZ: f32 = 130.0;
    /// VCO 1 and VCO 2's VCA recovery after the trigger's cut-off: gain `(1 − e^(−t/τ))²`; their
    /// tones build over several milliseconds, as the snare's do. VCO 3's recovery is per
    /// calibration. Replace with the VCA control voltage.
    pub const VCA_OPEN_TIME: f32 = 0.0012;
    /// Trigger feedthrough: during the trigger each recording sits a few per cent of its peak off
    /// zero — below it for the mid and high toms, above it for the low one, the same sense as the
    /// dominant VCO's reset phase. A 0.3 ms pole on the trigger, scaled by each calibration's
    /// signed `pedestal` and joining after the output coupling, reproduces it. Replace with the tom
    /// output during the trigger.
    pub const PEDESTAL_TIME: f32 = 0.0003;
    /// Rise and fall time constants of the release click, in seconds: the low recording's spike
    /// peaks about 0.2 ms after the release and is back under a fifth of its height by 0.8 ms.
    /// Replace with the tom output at the trigger's end on a scope.
    pub const RELEASE_CLICK_RISE: f32 = 0.0001;
    pub const RELEASE_CLICK_FALL: f32 = 0.0004;
    /// Delay from the trigger's release to the attack transient, in seconds, from the transient's
    /// waveform fit.
    pub const ATTACK_DELAY: f32 = 0.0008;
    /// Tom-noise level, corner and T60. In the recordings the band above 4 kHz falls about
    /// 0.28 dB/ms after 20 ms, outlasting the upper tone that shares its VCA, so the noise keeps
    /// its own fall. Listening found the first pass's level (0.055, 6 kHz corner) audible as hiss
    /// on the low tom: its 20–150 ms octaves above 3.2 kHz sat 6–11 dB over the recording's. At
    /// this level and corner they land within 2 dB. Replace with the tom-noise path's output.
    pub const NOISE_LEVEL: f32 = 0.025;
    pub const NOISE_CORNER_HZ: f32 = 3_500.0;
    pub const NOISE_T60: f32 = 0.210;
    /// Mix gain before the bounded output stage, low enough that the stage stays within about
    /// 2 % of linear on the reference hit: at 0.55 it flattened the first peak that the mid and
    /// high recordings lead with.
    pub const OUTPUT_GAIN: f32 = 0.25;
}

#[derive(Debug, Clone)]
pub struct ResetTom {
    sample_rate: f32,
    oscillators: [ResetTriangle; 3],
    pitch_envelope: f32,
    envelopes: [f32; 3],
    level: f32,
    vca_open: f32,
    upper_open: f32,
    noise_envelope: f32,
    noise_low_pass: f32,
    coupling: f32,
    pedestal: f32,
    release_click: [f32; 2],
    attack: Resonator,
    clip_envelope: f32,
    restart: bool,
    age: u32,
    active: bool,
}

impl Default for ResetTom {
    fn default() -> Self {
        Self::new()
    }
}

impl ResetTom {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            oscillators: [ResetTriangle::new(); 3],
            pitch_envelope: 0.0,
            envelopes: [0.0; 3],
            level: 0.0,
            vca_open: 0.0,
            upper_open: 0.0,
            noise_envelope: 0.0,
            noise_low_pass: 0.0,
            coupling: 0.0,
            pedestal: 0.0,
            release_click: [0.0; 2],
            attack: Resonator::new(),
            clip_envelope: 0.0,
            restart: false,
            age: 0,
            active: false,
        }
    }
    pub fn set_sample_rate(&mut self, value: f32) {
        self.sample_rate = valid_sample_rate(value);
    }
    pub fn trigger(&mut self, velocity: f32, patch: Patch) {
        let level = accent(velocity, patch.dynamics);
        // All three timing capacitors discharge together (§4.5); the calibration's reset phases
        // apply on the next sample, where the tom's kind is known.
        self.restart = true;
        self.clip_envelope = 1.0;
        self.pitch_envelope = 1.0;
        for envelope in &mut self.envelopes {
            *envelope = (*envelope + level).min(2.0);
        }
        self.level = level;
        self.vca_open = 0.0;
        self.upper_open = 0.0;
        self.noise_envelope = (self.noise_envelope + level).min(2.0);
        self.age = 0;
        self.active = true;
    }
    #[must_use]
    pub fn process(&mut self, kind: TomKind, patch: Patch, shared_noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        // The shared bus is sanitised here rather than trusted: `flush` passes a non-finite
        // value through, so one bad sample would otherwise sit in the recursive state for good.
        let shared_noise = finite_or(shared_noise, 0.0);
        let fs = self.sample_rate;
        let calibration = kind.calibration();
        if self.restart {
            for (oscillator, phase) in self.oscillators.iter_mut().zip(calibration.phases) {
                oscillator.reset_to(phase);
            }
            self.restart = false;
        }
        let pitch = finite_or(patch.pitch_semitones, 0.0).clamp(-24.0, 24.0);
        let pitch_depth = envelope_depth(finite_bipolar(patch.pitch_envelope));
        // One control envelope bends all three VCOs; TUNE adds the manual offset (§4.5).
        let hz = calibration.rest_hz
            * 2.0_f32.powf(pitch / 12.0)
            * (1.0 + calibration.sweep_depth * pitch_depth * self.pitch_envelope);
        // CHARACTER moves the clipper: harder toward square, softer toward the plain triangle.
        let character = finite_bipolar(patch.character);
        let clip = tom::SHAPE.clip - (tom::SHAPE.clip - tom::CLIP_START) * self.clip_envelope;
        let shape = VcoShape {
            clip: (clip * 2.0_f32.powf(-character)).clamp(0.05, 1.0),
            ..tom::SHAPE
        };
        self.clip_envelope = flush(self.clip_envelope * (-1.0 / (tom::CLIP_TIME * fs)).exp());
        // Reset-VCO strike phase: the trigger holds all three integrators, then releases them.
        let held = self.age < (TRIGGER_HOLD * fs) as u32;
        let waves: [f32; 3] = std::array::from_fn(|index| {
            if held {
                0.0
            } else {
                self.oscillators[index].tick(fs, hz * calibration.ratios[index], shape)
            }
        });
        let pitch_time =
            calibration.sweep_time * envelope_time_scale(finite_bipolar(patch.pitch_decay));
        self.pitch_envelope = flush(self.pitch_envelope * (-1.0 / (pitch_time * fs)).exp());
        // Only VCO 2, the dominant tone, has the user-adjustable DECAY path; the other two keep
        // their fixed envelopes and fall away under it (§4.5).
        let decay = scale_decay(1.0, finite_bipolar(patch.decay), 0.35, 8.0);
        let mut gains = [0.0; 3];
        for (index, (envelope, gain)) in self.envelopes.iter_mut().zip(&mut gains).enumerate() {
            let time = calibration.times[index] * if index == 1 { decay } else { 1.0 };
            *envelope = flush(*envelope * (-1.0 / (time * fs)).exp());
            let threshold = calibration.thresholds[index];
            *gain = ((1.0 + threshold) * *envelope - threshold * self.level).max(0.0);
        }
        if !held {
            self.vca_open +=
                (1.0 - self.vca_open) * (1.0 - (-1.0 / (tom::VCA_OPEN_TIME * fs)).exp());
            self.upper_open = if calibration.upper_open_time > 0.0 {
                self.upper_open
                    + (1.0 - self.upper_open)
                        * (1.0 - (-1.0 / (calibration.upper_open_time * fs)).exp())
            } else {
                1.0
            };
        }
        let open = self.vca_open * self.vca_open;
        let attack = finite_bipolar(patch.attack);
        let noise_t60 = tom::NOISE_T60
            * 1.8_f32.powf(attack)
            * envelope_time_scale(finite_bipolar(patch.noise_decay));
        self.noise_envelope =
            flush(self.noise_envelope * (0.001_f32.ln() / (noise_t60 * fs)).exp());
        let lp = 1.0 - (-std::f32::consts::TAU * tom::NOISE_CORNER_HZ.min(0.4 * fs) / fs).exp();
        self.noise_low_pass =
            flush(self.noise_low_pass + lp * (shared_noise - self.noise_low_pass));
        // Tom noise roughens the attack beside VCO 3 and opens with VCO 3's VCA (§4.5), silent
        // through the trigger; it keeps its own, longer fall.
        let noise = tom::NOISE_LEVEL
            * self.upper_open
            * self.upper_open
            * self.noise_low_pass
            * self.noise_envelope
            * (1.0 + finite_bipolar(patch.noise));
        let gate = if held { self.level } else { 0.0 };
        self.pedestal = flush(
            self.pedestal
                + (1.0 - (-1.0 / (tom::PEDESTAL_TIME * fs)).exp()) * (gate - self.pedestal),
        );
        let tone = finite_bipolar(patch.tone);
        let tones = open
            * (calibration.levels[0] * (1.0 - 0.3 * tone) * waves[0] * gains[0]
                + calibration.levels[1] * waves[1] * gains[1])
            + self.upper_open
                * self.upper_open
                * calibration.levels[2]
                * (1.0 + 0.3 * tone)
                * waves[2]
                * gains[2];
        let gain = tom::OUTPUT_GAIN * (1.0 + 0.3 * finite_bipolar(patch.body));
        let signal = gain * (tones + noise);
        let coupling = 1.0 - (-std::f32::consts::TAU * tom::COUPLING_HZ.min(0.4 * fs) / fs).exp();
        self.coupling = flush(self.coupling + coupling * (signal - self.coupling));
        // ATTACK scales the attack transient; −1 removes it.
        let [attack_level, attack_hz, attack_t60] = calibration.attack;
        self.attack
            .configure(fs, attack_hz * 2.0_f32.powf(pitch / 12.0), attack_t60);
        let hold = (TRIGGER_HOLD * fs) as u32;
        if self.age == hold {
            for state in &mut self.release_click {
                *state += self.level;
            }
        }
        // A difference of two exponentials: a rounded spike rather than a step. With 0.1 and
        // 0.4 ms the difference peaks at 0.47 of the step, 0.19 ms after it; 2.1 brings that
        // peak to the step's height.
        let click = 2.1 * (self.release_click[1] - self.release_click[0]);
        for (state, time) in self
            .release_click
            .iter_mut()
            .zip([tom::RELEASE_CLICK_RISE, tom::RELEASE_CLICK_FALL])
        {
            *state = flush(*state * (-1.0 / (time * fs)).exp());
        }
        let strike = if self.age == hold + (tom::ATTACK_DELAY * fs) as u32 {
            self.level
        } else {
            0.0
        };
        let transient = attack_level * (1.0 + attack) * self.attack.process_strike(strike);
        let output = (signal - self.coupling
            + gain
                * (calibration.pedestal * self.pedestal
                    + calibration.release_click * click
                    + transient))
            .tanh();
        self.age = self.age.saturating_add(1);
        if self.age > (2.0 * fs) as u32
            && self.envelopes.iter().all(|x| *x < 1.0e-8)
            && self.noise_envelope < 1.0e-8
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
        *self = Self {
            sample_rate: self.sample_rate,
            ..Self::new()
        };
    }
}

fn accent(v: f32, d: f32) -> f32 {
    let v = finite_or(v, 0.0).clamp(0.0, 1.0);
    let d = finite_bipolar(d);
    // The collection's one Dynamics mapping (`crate::velocity`); this family's own was
    // `1 − 0.6·d` / `1 − 1.8·d`, the same at zero.
    let exponent = crate::velocity::exponent(d);
    0.3 + 0.7 * v.powf(exponent)
}
fn scale_decay(r: f32, v: f32, s: f32, l: f32) -> f32 {
    if v < 0.0 {
        r * s.powf(-v)
    } else {
        r * l.powf(v)
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
fn valid_sample_rate(v: f32) -> f32 {
    if v.is_finite() {
        v.max(MIN_SAMPLE_RATE)
    } else {
        48_000.0
    }
}
fn finite_bipolar(v: f32) -> f32 {
    finite_or(v, 0.0).clamp(-1.0, 1.0)
}
fn finite_or(v: f32, f: f32) -> f32 {
    if v.is_finite() { v } else { f }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_finite_shared_noise_sample_does_not_poison_a_voice() {
        // The noise bus is machine-shared, and `flush` passes a non-finite value straight through,
        // so one bad sample would otherwise sit in a recursive state that never recovers.
        let patch = patch();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut kick = ResetKick::new();
            kick.trigger(0.82, patch);
            for _ in 0..64 {
                let _ = kick.process(patch, 0.1);
            }
            let _ = kick.process(patch, bad);
            let after: Vec<f32> = (0..512).map(|_| kick.process(patch, 0.1)).collect();
            assert!(
                after.iter().all(|sample| sample.is_finite()),
                "the kick never recovered from a {bad} on the shared bus"
            );

            let mut tom = ResetTom::new();
            tom.trigger(0.82, patch);
            for _ in 0..64 {
                let _ = tom.process(TomKind::Low, patch, 0.1);
            }
            let _ = tom.process(TomKind::Low, patch, bad);
            let after: Vec<f32> = (0..512)
                .map(|_| tom.process(TomKind::Low, patch, 0.1))
                .collect();
            assert!(
                after.iter().all(|sample| sample.is_finite()),
                "the tom never recovered from a {bad} on the shared bus"
            );
        }
    }

    fn patch() -> Patch {
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
    fn lfsr_is_deterministic_nonconstant_and_resettable() {
        let mut a = HardwareNoise::new();
        let mut b = HardwareNoise::new();
        let x: Vec<_> = (0..1000).map(|_| a.tick(48_000.0)).collect();
        let y: Vec<_> = (0..1000).map(|_| b.tick(48_000.0)).collect();
        assert_eq!(x, y);
        assert!(x.windows(2).any(|w| w[0] != w[1]));
        a.reset();
        assert_eq!(a.tick(48_000.0), HardwareNoise::new().tick(48_000.0));
    }
    #[test]
    fn reset_vcos_begin_deterministically_but_noise_changes_attack() {
        let render = |noise| {
            let mut k = ResetKick::new();
            k.trigger(0.8, patch());
            (0..512)
                .map(|_| k.process(patch(), noise))
                .collect::<Vec<_>>()
        };
        assert_eq!(render(0.0), render(0.0));
        assert_ne!(render(-1.0), render(1.0));
    }
    #[test]
    fn kick_pitch_envelope_is_audible_beyond_the_click_window() {
        let mut voice = ResetKick::new();
        let mut p = patch();
        p.noise = -1.0;
        voice.trigger(1.0, p);
        let samples: Vec<_> = (0..9_600).map(|_| voice.process(p, 0.0)).collect();
        let crossings = |range: std::ops::Range<usize>| {
            samples[range]
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count()
        };
        let early_hz = crossings(0..960) as f32 / 0.020;
        // The VCA threshold silences the reference hit by about 175 ms, so the settled window
        // ends before it.
        let settled_hz = crossings(2_880..7_200) as f32 / 0.090;
        assert!(
            early_hz > settled_hz * 1.5,
            "early {early_hz} Hz, settled {settled_hz} Hz"
        );
        assert!(
            (45.0..65.0).contains(&settled_hz),
            "settled {settled_hz} Hz"
        );
    }

    fn rising_crossings(x: &[f32], fs: f32) -> Vec<f32> {
        x.windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, w)| (i as f32 + w[0] / (w[0] - w[1])) / fs)
            .collect()
    }

    #[test]
    fn kick_sweep_follows_the_fitted_linear_frequency_law() {
        // The acquired comparison recording's cycle lengths fit 54·(1 + 4.95·e^(−t/12.4 ms)) Hz
        // from the trigger, with the tone released from its centre 1.9 ms later (2026-09-19).
        let fs = 48_000.0;
        let mut voice = ResetKick::new();
        let mut p = patch();
        p.noise = -1.0;
        p.attack = -1.0;
        voice.trigger(0.82, p);
        let x: Vec<f32> = (0..7_200).map(|_| voice.process(p, 0.0)).collect();
        let rising = rising_crossings(&x, fs);
        assert!(
            (rising[0] - TRIGGER_HOLD).abs() < 2.0 / fs,
            "tone released at {} s",
            rising[0]
        );
        let law = |t: f32| 54.0 * (1.0 + 4.95 * (-t / 0.0124).exp());
        let mut checked = 0;
        for pair in rising.windows(2).filter(|pair| pair[1] < 0.140) {
            let measured = 1.0 / (pair[1] - pair[0]);
            let steps = 64;
            let expected = (0..steps)
                .map(|k| law(pair[0] + (pair[1] - pair[0]) * (k as f32 + 0.5) / steps as f32))
                .sum::<f32>()
                / steps as f32;
            assert!(
                (measured / expected - 1.0).abs() < 0.05,
                "cycle at {:.4} s: {measured} Hz against {expected} Hz",
                pair[0]
            );
            checked += 1;
        }
        assert!(checked >= 8, "only {checked} cycles");
    }

    #[test]
    fn kick_vca_threshold_ends_the_reference_hit_in_finite_time() {
        // The recording's decay accelerates and is gone by about 175 ms: a capacitor voltage
        // crossing the VCA's conduction threshold, not an exponential tail.
        let mut voice = ResetKick::new();
        let p = patch();
        voice.trigger(0.82, p);
        let x: Vec<f32> = (0..14_400).map(|_| voice.process(p, 0.0)).collect();
        let peak = x[..2_400].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        let at = |from: usize| {
            x[from..from + 480]
                .iter()
                .fold(0.0_f32, |m, v| m.max(v.abs()))
        };
        assert!(at(3_072) > 0.35 * peak, "64 ms: {}", at(3_072) / peak);
        assert!(at(7_680) < 0.05 * peak, "160 ms: {}", at(7_680) / peak);
        assert!(x[9_600..].iter().all(|v| *v == 0.0), "sound after 200 ms");
    }

    #[test]
    fn toms_hold_their_vcos_through_the_trigger() {
        for kind in [TomKind::Low, TomKind::Mid, TomKind::High] {
            let mut voice = ResetTom::new();
            let mut p = patch();
            p.noise = -1.0;
            voice.trigger(0.82, p);
            let x: Vec<f32> = (0..4_800).map(|_| voice.process(kind, p, 0.0)).collect();
            let hold = (TRIGGER_HOLD * 48_000.0) as usize;
            let during = x[..hold].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
            let after = x[hold..].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
            assert!(
                during < 0.1 * after,
                "{kind:?}: {during} during the trigger, {after} after"
            );
            // Only the trigger feedthrough sounds during the hold, in the calibration's sense.
            assert_eq!(
                x[hold / 2].signum(),
                kind.calibration().pedestal.signum(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn low_tom_clicks_positive_at_the_trigger_release() {
        // The low recording spikes by about a quarter of its peak right after the release, and
        // the spike falls away within a millisecond while the VCOs are still rising.
        let mut voice = ResetTom::new();
        let mut p = patch();
        p.noise = -1.0;
        voice.trigger(0.82, p);
        let x: Vec<f32> = (0..9_600)
            .map(|_| voice.process(TomKind::Low, p, 0.0))
            .collect();
        let whole = x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        let release = (TRIGGER_HOLD * 48_000.0) as usize;
        let spike = x[release..release + 24]
            .iter()
            .fold(f32::MIN, |m, v| m.max(*v));
        let settled = x[release + 48];
        assert!(
            spike - x[release - 1] > 0.2 * whole,
            "spike {spike} of {whole}"
        );
        assert!(
            spike - settled > 0.12 * whole,
            "spike {spike}, 1 ms later {settled}"
        );
    }

    #[test]
    fn mid_and_high_toms_lead_with_their_attack_peak() {
        // The mid and high recordings' loudest peak comes 1.5–3 ms after the trigger's release,
        // so their first 3 ms after it reach at least 80 % of the hit's peak; the low tom's
        // builds later, with its VCOs.
        for (kind, leads) in [
            (TomKind::Low, false),
            (TomKind::Mid, true),
            (TomKind::High, true),
        ] {
            let mut voice = ResetTom::new();
            let p = patch();
            voice.trigger(0.82, p);
            let x: Vec<f32> = (0..9_600).map(|_| voice.process(kind, p, 0.0)).collect();
            let peak = |r: std::ops::Range<usize>| x[r].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
            let release = (TRIGGER_HOLD * 48_000.0) as usize;
            let early = peak(release..release + 144);
            let whole = peak(0..9_600);
            assert_eq!(
                early >= 0.8 * whole,
                leads,
                "{kind:?}: {early} against {whole}"
            );
        }
    }

    #[test]
    fn reset_tom_centres_follow_the_available_hardware_comparisons() {
        // The dominant VCO's fitted rest in the acquired comparison recordings (2026-09-19).
        assert_eq!(TomKind::Low.rest_hz(), 88.5);
        assert_eq!(TomKind::Mid.rest_hz(), 120.0);
        assert_eq!(TomKind::High.rest_hz(), 138.7);
    }

    #[test]
    fn every_reset_tom_calibration_sounds_and_resets() {
        for kind in [TomKind::Low, TomKind::Mid, TomKind::High] {
            let mut voice = ResetTom::new();
            voice.trigger(1.0, patch());
            let energy: f32 = (0..24_000)
                .map(|_| voice.process(kind, patch(), 0.25).abs())
                .sum();
            assert!(energy > 10.0, "silent {kind:?}: {energy}");
            voice.reset();
            assert_eq!(
                voice.process(kind, patch(), 0.0).to_bits(),
                0.0_f32.to_bits()
            );
        }
    }

    #[test]
    fn reset_tom_third_vco_reads_shared_hardware_noise() {
        let render = |noise| {
            let mut voice = ResetTom::new();
            voice.trigger(1.0, patch());
            (0..512)
                .map(|_| voice.process(TomKind::Mid, patch(), noise))
                .collect::<Vec<_>>()
        };
        assert_ne!(render(-1.0), render(1.0));
    }

    #[test]
    fn kick_and_snare_stay_finite_and_reset_exactly() {
        let mut k = ResetKick::new();
        let mut s = ResetSnare::new();
        k.trigger(1.0, patch());
        s.trigger(1.0, patch());
        for _ in 0..96_000 {
            assert!(k.process(patch(), 0.5).is_finite());
            assert!(s.process(patch(), -0.5).is_finite());
        }
        k.reset();
        s.reset();
        assert_eq!(k.process(patch(), 0.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(s.process(patch(), 0.0).to_bits(), 0.0_f32.to_bits());
    }
}
