//! Four economical DR-55-derived voices with one machine-shared transistor-noise source.
//! Published targets: kick 62 Hz, snare body 320 Hz/~75 ms, rim 1.33 kHz/7 ms, hat ~50 ms
//! (`research:instruments/analogue-drum-machines.md` §5.5). The constants below were fitted to the
//! acquired comparison recordings (single hits from a private DR-55 sample folder) in the
//! 2026-09-19 A/B pass; each names the measurement that could replace it.
use crate::{engine::SlotPatch, resonator::Resonator};

/// The level a velocity-0.82, zero-Dynamics hit strikes with: the fitted kick's unit strike, so
/// its clipper sees the recording's drive at the reference setting and harder or softer hits
/// clip more or less.
const REFERENCE_STRIKE: f32 = 0.82 * 0.825;

/// Kick: a transistor resonant network struck by both edges of the trigger, the trigger itself
/// leaking into the output, a clipper that flattens the positive swing and an output coupling
/// capacitor. A least-squares fit of that chain to the recording's first 70 ms lands within 0.047
/// of its peak rms: the negative first swing, the drooping positive hump and the quick tail.
mod kick {
    /// Network centre in Hz (fit 62.5; the published 62 Hz) and T60 in seconds. Replace with the
    /// network's measured ring at small amplitude.
    pub const HZ: f32 = 62.0;
    pub const T60: f32 = 0.0402;
    /// Trigger width in seconds: its trailing edge strikes the network again. Replace with the
    /// trigger pulse on a scope.
    pub const PULSE_WIDTH: f32 = 0.008;
    /// Trigger feedthrough: the pulse through a 2.76 ms pole at 1.71 of the network's unit swing.
    pub const FEED: f32 = 1.71;
    pub const FEED_TIME: f32 = 0.00276;
    /// Network gain into the clipper, and the clipper's positive and negative knees.
    pub const GAIN: f32 = 4.47;
    pub const KNEE_POSITIVE: f32 = 0.32;
    pub const KNEE_NEGATIVE: f32 = 1.67;
    /// Output coupling high-pass corner in Hz: the positive hump's droop.
    pub const COUPLING_HZ: f32 = 28.0;
    /// Level into the bounded output stage, keeping the fitted shape nearly linear there.
    pub const OUTPUT: f32 = 0.45;
}

/// Snare: a 350 Hz body network struck by both trigger edges, and shared noise through a broad
/// band-pass under a delayed VCA.
mod snare {
    /// Body centre in Hz and T60 in seconds. Band-passed 220–520 Hz, the recording's first three
    /// cycles read 338, 339 and 356 Hz and its 4–30 ms partial 351 Hz; the feedthrough and the
    /// trailing strike pull the rendered ring about 3 % under the network, so 350 Hz renders
    /// 341, 335 and 341 Hz and 351 Hz, where 340 Hz rendered 333, 322 and 322 Hz and 343 Hz.
    /// With both trigger edges striking, a 30 ms T60 places its 150–600 Hz band over the first
    /// 40 ms (5 ms windows) within 2.3 dB mean; the first pass's 38 ms held the ring 4 dB too long
    /// against its opening. Replace with the network's measured ring.
    pub const HZ: f32 = 350.0;
    pub const T60: f32 = 0.030;
    /// Trigger width in seconds: the body rings again, inverted, 9.7 ms after the hit, 3 dB under
    /// its first ring. Replace with the trigger pulse on a scope.
    pub const PULSE_WIDTH: f32 = 0.0097;
    pub const TRAILING_STRIKE: f32 = 0.7;
    /// Trigger feedthrough, as the kick's: the pulse through the kick's 2.76 ms pole and a
    /// second-order coupling high-pass at 7.8 Hz, Q 0.91, at 0.22 of the body's level. Below
    /// 150 Hz the recording swings to −9 % of its peak by 20 ms, overshoots to +2 % near 85 ms
    /// and settles by 180 ms; this places that sub-audio excursion within 0.7 % of its peak rms.
    /// It is most of the recording's energy after 50 ms, so without it the page's loudness match
    /// (body RMS down to −40 dB) played the recording 4 dB louder. Replace with the snare output
    /// below 150 Hz on a scope.
    pub const FEED: f32 = 0.222;
    pub const FEED_COUPLING_HZ: f64 = 7.8;
    pub const FEED_COUPLING_Q: f64 = 0.91;
    /// Noise band-pass centre in Hz and damping: the recording's noise is flat from 3 to 13 kHz
    /// and 18 dB down at 1 kHz; a band-pass fitted to its ⅓-octave spectrum over 15–50 ms lands
    /// within 1.3 dB rms. Replace with the noise path's measured response.
    pub const NOISE_HZ: f32 = 6_068.0;
    pub const NOISE_DAMPING: f32 = 1.66;
    /// Noise VCA: closed for 2.7 ms after the trigger, then opening with a 6 ms time constant;
    /// its level falls as `(1 − t/T)^p` with T = 91 ms and p = 3.0, a constant-current discharge
    /// through a curved VCA, fitted to the level above 2.5 kHz in 1 ms windows over 3–70 ms within
    /// 1.4 dB rms. A 0.7 ms opening after 3.5 ms put 3–8 ms 4 dB over the recording and its
    /// early spectrum 9 semitones bright. Replace with the VCA control voltage.
    pub const NOISE_DELAY: f32 = 0.0027;
    pub const NOISE_OPEN_TIME: f32 = 0.006;
    pub const NOISE_FALL: f32 = 0.091;
    pub const NOISE_POWER: f32 = 3.0;
    /// Body and noise levels. Listening found the recording louder at the page's equal body
    /// energy: its body opens 5–6 dB over the first pass's and its noise above 2 kHz sits 3–4 dB
    /// over it through 15–75 ms.
    pub const BODY_LEVEL: f32 = 1.4;
    pub const NOISE_LEVEL: f32 = 1.12;
}

/// Rim: a 1.35 kHz network struck negative, first swing down as in the recording, and 20 dB down
/// in 6 ms (the published "7 ms" is close to that point, not a T60): its band falls 4.5 dB/ms.
/// Replace with the network's measured ring.
mod rim {
    pub const HZ: f32 = 1_350.0;
    pub const T60: f32 = 0.0138;
}

/// Hat: shared noise through the LC band-pass and a coupling high-pass, under a VCA that opens in
/// about a millisecond, holds, then falls away as `(1 − t/T)^p`.
mod hat {
    /// The LC's resonance, which is the only tone this voice has, and the high-pass corner.
    /// `tools/ab_resonance.py` reports, per window, the A-weighted median frequency and the peak
    /// of the spectrum smoothed over ±8 %. Over 0–10, 10–30 and 30–60 ms the recording peaks at
    /// 9232 → 8480 → 8802 Hz: it opens high, dips as the strike passes and rises again as it
    /// fades. One fixed centre cannot do that, and a single curve fitted to all three windows
    /// pinned the body high, at 9475 and 9497 Hz — a fifth of an octave over the recording where
    /// the hat is loudest after the strike.
    ///
    /// So the centre carries three terms with their own time constants, over a base that stays
    /// near the recording's body: a strike lift of `STRIKE_RISE` that falls away with
    /// `STRIKE_TIME`, and a late rise of `LATE_RISE` that only arrives once the envelope is well
    /// down, as `(1 − envelope)²`. The damping opens from `DAMPING` at full envelope to
    /// `DAMPING_LATE` at the end. It renders 9716 → 8382 → 8643 Hz, with medians
    /// 9049 → 8793 → 8803 against the recording's 8967 → 8136 → 9024, −3 dB widths of Q
    /// 2.6/4.4/3.2 against 2.8/3.7/5.1, and window levels of −11.8/−14.3/−23.3 dB against
    /// −11.0/−15.0/−23.8.
    ///
    /// Two residues stay. The 0–10 ms peak reads 485 Hz high because the smoothed spectrum there
    /// has two humps and the strike tips the balance between them; its median is within 82 Hz.
    /// The 10–30 ms median stays about 650 Hz high because the recording carries more
    /// 800 Hz–3 kHz in that window than this path can pass — dropping the high-pass corner from
    /// 4 kHz to 2.6 kHz moves that median by 20 Hz, so it is where the mass sits, not the skirt.
    /// Replace with the LC filter's response under its VCA.
    pub const HZ: f32 = 8_500.0;
    pub const STRIKE_RISE: f32 = 0.14;
    pub const STRIKE_TIME: f32 = 0.005;
    pub const LATE_RISE: f32 = 0.10;
    pub const DAMPING: f32 = 0.28;
    pub const DAMPING_LATE: f32 = 0.345;
    /// Samples between coefficient updates: the drift moves by under 0.2 % across 32 samples at
    /// 48 kHz, and less at higher rates.
    pub const DRIFT_BLOCK: u32 = 32;
    pub const HIGH_PASS_HZ: f32 = 4_000.0;
    /// Broadband leakage around the resonant high-pass. The acquired recording keeps an audible
    /// 0.2–2 kHz bed through the hit where the filtered path is nearly empty; the owner hears the
    /// missing band as less low end. Fitted from the 2026-09-20 spectrogram and octave levels.
    pub const LOW_FLOOR_LEVEL: f32 = 0.08;
    pub const LOW_FLOOR_T60: f32 = 0.08;
    pub const LOW_FLOOR_HIGH_PASS_HZ: f32 = 150.0;
    pub const LOW_FLOOR_LOW_PASS_HZ: f32 = 2_500.0;
    /// T = 57 ms, p = 1.3, fitted with the resonance above to the recording's level above 3 kHz
    /// in 2.5 ms windows over 3–58 ms (3.4 dB rms) and to the three windows' levels against the
    /// body’s rms: +3.4/+1.0/−8.0 dB against the recording’s +4.3/+0.3/−8.4. T = 59 ms with
    /// p = 1.9 read −9.7 dB in the last window, the 3 dB of missing tail. The published hat is
    /// about 50 ms. Replace with the VCA control voltage.
    pub const FALL: f32 = 0.057;
    pub const POWER: f32 = 1.3;
    pub const OPEN_TIME: f32 = 0.001;
    /// The noise driving the LC arrives in bursts, and they are slow. Over the first 60 ms the
    /// recording's 4–16 kHz envelope carries 8.0 % modulation at 60–150 Hz and 6.9 % and 4.0 % at
    /// 150–300 and 300–600 (Hilbert envelope, mean-normalised, windowed). A noise-driven
    /// modulator smoothed at 600 Hz gave 4.6, 10.5 and 10.0: about three times too fast, and the
    /// owner heard the recording's slower one as "lower frequency modulation" that the model
    /// lacks. The band's own noise accounts for 3.7, 4.0 and 5.1 of that before any modulator,
    /// so the 300–600 row can only be brought down so far.
    ///
    /// Burst noise in a transistor is a two-level random telegraph, so that is what carries the
    /// slow part here: two poles smooth the machine-shared sample, a comparator with hysteresis
    /// at `BURST_THRESHOLD` turns it into a telegraph that flips about every 14 ms,
    /// `BURST_EDGE_TIME` rounds the flips, and one pole at `BURST_FLOOR_HZ` takes the slowest
    /// part back out so what is left lands in the recording's band rather than under it. With the
    /// fast wander below it reads 6.8, 6.3 and 6.8 %, and the decay's own 15–60 Hz row stays at
    /// 32.9 against 32.7. What the telegraph cannot hold is the 1 ms envelope swing and the
    /// envelope autocorrelation, 0.28 and 0.83 against 0.33 and 0.77 before: both measure fast
    /// modulation, which is what this pass took away. Replace with the noise transistor's burst
    /// statistics.
    pub const BURST_DEPTH: f32 = 0.11;
    pub const BURST_RATE_HZ: f32 = 200.0;
    pub const BURST_THRESHOLD: f32 = 1.3;
    pub const BURST_EDGE_TIME: f32 = 0.0005;
    pub const BURST_FLOOR_HZ: f32 = 70.0;
    /// Rms of the flutter that the telegraph and its floor pole leave, measured on white noise at
    /// 44.1, 48 and 96 kHz (0.472, 0.471, 0.470), so `BURST_DEPTH` means the same at any of them.
    pub const BURST_FLOOR_GAIN: f32 = 2.12;
    /// The fast wander under the telegraph, smoothed at 600 Hz: the recording's peak stands
    /// 15.3 dB over its body and falls in its first 10 ms, and the telegraph alone left the model
    /// at 12.9 dB with its loudest millisecond at 15 ms. Replace as above.
    pub const WANDER_DEPTH: f32 = 0.16;
    pub const WANDER_HZ: f32 = 600.0;
    /// Rms of the machine-shared sample, uniform over ±1, used to scale `u`.
    pub const NOISE_RMS: f32 = 0.577_350_3;
    /// Level into the bounded output stage.
    pub const LEVEL: f32 = 0.9;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Kick,
    Snare,
    Rim,
    Hat,
}
#[derive(Debug, Clone)]
pub struct Noise {
    state: u32,
}
impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}
impl Noise {
    #[must_use]
    pub const fn new() -> Self {
        Self { state: 0x55d3_1a7b }
    }
    #[must_use]
    pub fn tick(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = if x == 0 { 1 } else { x };
        (self.state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    /// Moves the register to where running would have left it, without rendering (plan §4.4).
    ///
    /// The same GF(2) jump the other shift registers take. Small enough that gating it saves
    /// little on its own — it is here so the rule holds for every shared bus without exception,
    /// which is worth more than the nanoseconds.
    pub fn advance(&mut self, samples: u64) {
        self.state = crate::jump::advance_xorshift32(self.state, samples);
    }

    pub fn reset(&mut self) {
        *self = Self::new()
    }
}

/// Topology-preserving (trapezoidal) state-variable filter: accurate to the band edge, where the
/// hat's 9.3 kHz centre sits too high for the Chamberlin form.
#[derive(Debug, Clone, Copy)]
struct Svf {
    low: f32,
    band: f32,
}
impl Svf {
    const fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
        }
    }
    /// Returns (band-pass, high-pass).
    fn process(&mut self, input: f32, centre: f32, damping: f32, fs: f32) -> (f32, f32) {
        let g = f64::from(std::f32::consts::PI * centre.clamp(20.0, 0.45 * fs) / fs).tan() as f32;
        let k = damping.clamp(0.05, 4.0);
        let high = (input - (k + g) * self.band - self.low) / (1.0 + g * (k + g));
        let band = g * high + self.band;
        let low = g * band + self.low;
        self.band = flush(band + g * high);
        self.low = flush(low + g * band);
        (band, high)
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    fs: f32,
    res: Resonator,
    strike: f32,
    level: f32,
    env: f32,
    pitch_env: f32,
    feed: f32,
    coupling: f32,
    feed_coupling: [f64; 4],
    filter: Svf,
    high_pass: [f32; 2],
    low_floor: [f32; 2],
    drift: [f32; 2],
    burst: [f32; 6],
    telegraph: f32,
    open: f32,
    tone_low_pass: f32,
    active: bool,
    age: u32,
}
impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}
impl Voice {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fs: 48000.0,
            res: Resonator::new(),
            strike: 0.0,
            level: 0.0,
            env: 0.0,
            pitch_env: 0.0,
            feed: 0.0,
            coupling: 0.0,
            feed_coupling: [0.0; 4],
            filter: Svf::new(),
            high_pass: [0.0; 2],
            low_floor: [0.0; 2],
            drift: [hat::HZ, hat::DAMPING],
            burst: [0.0; 6],
            telegraph: 1.0,
            open: 0.0,
            tone_low_pass: 0.0,
            active: false,
            age: 0,
        }
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.fs = finite(v, 48000.0).clamp(1000.0, 384000.0)
    }
    pub fn trigger(&mut self, velocity: f32, p: SlotPatch) {
        // Linear in velocity, Dynamics bending the curve (`crate::velocity`).
        let x = crate::velocity::linear(velocity, p.dynamics);
        self.strike = (self.strike + x).min(2.0);
        self.level = x;
        self.env = (self.env + x).min(2.0);
        self.pitch_env = (self.pitch_env + x).min(2.0);
        self.open = 0.0;
        self.age = 0;
        self.active = true
    }
    #[must_use]
    pub fn process(&mut self, kind: Kind, p: SlotPatch, noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let fs = self.fs;
        let t = self.age as f32 / fs;
        let ratio = 2.0_f32.powf(finite(p.pitch_semitones, 0.0).clamp(-24.0, 24.0) / 12.0);
        let decay_scale = extended_decay_scale(bipolar(p.decay));
        let tone = bipolar(p.tone);
        let body = bipolar(p.body);
        let attack = bipolar(p.attack);
        let noise = finite(noise, 0.0);
        // Both edges of the trigger strike a network: the leading edge now, the trailing edge,
        // inverted, one pulse width later.
        let pulse_width = match kind {
            Kind::Kick => kick::PULSE_WIDTH,
            Kind::Snare => snare::PULSE_WIDTH,
            Kind::Rim | Kind::Hat => 0.0,
        };
        let mut excitation = self.strike;
        self.strike = 0.0;
        if pulse_width > 0.0 && self.age == ((pulse_width * fs) as u32).max(1) {
            let trailing = if kind == Kind::Snare {
                snare::TRAILING_STRIKE
            } else {
                1.0
            };
            excitation -= trailing * self.level;
        }
        let gate = if t < pulse_width { self.level } else { 0.0 };
        let (mut out, longest) = match kind {
            Kind::Kick => {
                let sweep = 2.0_f32.powf(18.0 * bipolar(p.pitch_envelope) * self.pitch_env / 12.0);
                self.res
                    .configure(fs, kick::HZ * ratio * sweep, kick::T60 * decay_scale);
                let ring = self.res.process_strike(-excitation / REFERENCE_STRIKE);
                // ATTACK sets how much of the trigger leaks through.
                let feed_pole = 1.0 - (-1.0 / (kick::FEED_TIME * fs)).exp();
                self.feed = flush(self.feed + feed_pole * (gate / REFERENCE_STRIKE - self.feed));
                let drive = kick::GAIN * (1.0 + 0.25 * body) * ring
                    + kick::FEED * (1.0 + 0.8 * attack) * self.feed;
                let knee = if drive > 0.0 {
                    kick::KNEE_POSITIVE
                } else {
                    kick::KNEE_NEGATIVE
                };
                let clipped = knee * (drive / knee).tanh();
                let coupling = 1.0 - (-std::f32::consts::TAU * kick::COUPLING_HZ / fs).exp();
                self.coupling = flush(self.coupling + coupling * (clipped - self.coupling));
                (kick::OUTPUT * (clipped - self.coupling), kick::T60)
            }
            Kind::Snare => {
                self.res
                    .configure(fs, snare::HZ * ratio, snare::T60 * decay_scale);
                let ring = self.res.process_strike(excitation);
                let (band, _) = self.filter.process(
                    noise,
                    snare::NOISE_HZ * 2.0_f32.powf(0.5 * tone),
                    snare::NOISE_DAMPING,
                    fs,
                );
                if t >= snare::NOISE_DELAY {
                    self.open +=
                        (1.0 - self.open) * (1.0 - (-1.0 / (snare::NOISE_OPEN_TIME * fs)).exp());
                }
                // The wires follow Noise decay alone, so Decay lengthens the body without
                // stretching the noise over the ring.
                let fall = snare::NOISE_FALL * envelope_time_scale(bipolar(p.noise_decay));
                let noise_gain =
                    self.level * self.open * (1.0 - t / fall).max(0.0).powf(snare::NOISE_POWER);
                let feed_pole = 1.0 - (-1.0 / (kick::FEED_TIME * fs)).exp();
                self.feed = flush(self.feed + feed_pole * (gate - self.feed));
                let feed = snare::FEED
                    * snare::BODY_LEVEL
                    * high_pass_biquad(
                        &mut self.feed_coupling,
                        self.feed,
                        snare::FEED_COUPLING_HZ,
                        snare::FEED_COUPLING_Q,
                        fs,
                    );
                (
                    feed + snare::BODY_LEVEL * (1.0 + 0.3 * body) * ring
                        + snare::NOISE_LEVEL * (0.55 + 0.4 * (0.5 + 0.5 * bipolar(p.noise))) / 0.75
                            * noise_gain
                            * band,
                    snare::T60.max(snare::NOISE_FALL),
                )
            }
            Kind::Rim => {
                self.res
                    .configure(fs, rim::HZ * ratio, rim::T60 * decay_scale);
                (
                    -(0.85 + 0.2 * body) * self.res.process_strike(excitation),
                    rim::T60,
                )
            }
            Kind::Hat => {
                // Burst noise: the source's transistor jumps between two levels, so a comparator
                // on the smoothed machine-shared sample drives a telegraph, its edges are rounded,
                // and a floor pole leaves only the part that flutters in the audible band.
                let smoothing =
                    1.0 - (-std::f32::consts::TAU * hat::BURST_RATE_HZ.min(0.4 * fs) / fs).exp();
                let mut smoothed = noise;
                for state in &mut self.burst[..2] {
                    *state = flush(*state + smoothing * (smoothed - *state));
                    smoothed = *state;
                }
                // `(2 − a)/√a` is the two-pole cascade's rms gain for white noise, to within
                // 2 % over the supported rates, so the comparator sees the same level at any.
                let slow = smoothed / hat::NOISE_RMS * (2.0 - smoothing) / smoothing.sqrt();
                if self.telegraph > 0.0 {
                    if slow < -hat::BURST_THRESHOLD {
                        self.telegraph = -1.0;
                    }
                } else if slow > hat::BURST_THRESHOLD {
                    self.telegraph = 1.0;
                }
                let edge = 1.0 - (-1.0 / (hat::BURST_EDGE_TIME * fs)).exp();
                self.burst[2] = flush(self.burst[2] + edge * (self.telegraph - self.burst[2]));
                let floor =
                    1.0 - (-std::f32::consts::TAU * hat::BURST_FLOOR_HZ.min(0.4 * fs) / fs).exp();
                self.burst[3] = flush(self.burst[3] + floor * (self.burst[2] - self.burst[3]));
                let flutter = (self.burst[2] - self.burst[3]) * hat::BURST_FLOOR_GAIN;
                // The fast wander the source keeps under the telegraph, which carries the hit's
                // crest: without it the loudest millisecond moves off the strike.
                let wander_smoothing =
                    1.0 - (-std::f32::consts::TAU * hat::WANDER_HZ.min(0.4 * fs) / fs).exp();
                let mut wander = noise;
                for state in &mut self.burst[4..] {
                    *state = flush(*state + wander_smoothing * (wander - *state));
                    wander = *state;
                }
                let wander =
                    wander / hat::NOISE_RMS * (2.0 - wander_smoothing) / wander_smoothing.sqrt();
                let burst = (1.0 + hat::BURST_DEPTH * flutter + hat::WANDER_DEPTH * wander)
                    / (1.0
                        + hat::BURST_DEPTH * hat::BURST_DEPTH
                        + hat::WANDER_DEPTH * hat::WANDER_DEPTH)
                        .sqrt();
                // The LC's centre and damping follow the voice's envelope, recomputed on a block
                // boundary because the drift is far slower than the audio it filters.
                if self.age.is_multiple_of(hat::DRIFT_BLOCK) {
                    let envelope = (1.0 - t / (hat::FALL * decay_scale))
                        .max(0.0)
                        .powf(hat::POWER);
                    let strike = (-t / hat::STRIKE_TIME).exp();
                    self.drift = [
                        hat::HZ
                            * (1.0 + hat::STRIKE_RISE * strike)
                            * (1.0 + hat::LATE_RISE * (1.0 - envelope) * (1.0 - envelope)),
                        hat::DAMPING_LATE + (hat::DAMPING - hat::DAMPING_LATE) * envelope,
                    ];
                }
                let (band, _) = self.filter.process(
                    burst * noise,
                    self.drift[0] * 2.0_f32.powf(0.5 * tone),
                    self.drift[1],
                    fs,
                );
                let hp =
                    1.0 - (-std::f32::consts::TAU * hat::HIGH_PASS_HZ.min(0.4 * fs) / fs).exp();
                // Two one-pole high-passes carry the resonant top. A small band-limited share of
                // the same machine noise leaks around them, filling the recording's low bed
                // without copying its 50/100 Hz mains hum.
                let mut high = band;
                for state in &mut self.high_pass {
                    *state = flush(*state + hp * (high - *state));
                    high -= *state;
                }
                let low_pole = 1.0
                    - (-std::f32::consts::TAU * hat::LOW_FLOOR_LOW_PASS_HZ.min(0.4 * fs) / fs)
                        .exp();
                self.low_floor[0] =
                    flush(self.low_floor[0] + low_pole * (noise - self.low_floor[0]));
                let high_pole = 1.0
                    - (-std::f32::consts::TAU * hat::LOW_FLOOR_HIGH_PASS_HZ.min(0.4 * fs) / fs)
                        .exp();
                self.low_floor[1] =
                    flush(self.low_floor[1] + high_pole * (self.low_floor[0] - self.low_floor[1]));
                let low_floor = (self.low_floor[0] - self.low_floor[1])
                    * (-6.9077554 * t / hat::LOW_FLOOR_T60).exp();
                self.open += (1.0 - self.open) * (1.0 - (-1.0 / (hat::OPEN_TIME * fs)).exp());
                let gain = self.level
                    * self.open
                    * (1.0 - t / (hat::FALL * decay_scale))
                        .max(0.0)
                        .powf(hat::POWER);
                (
                    hat::LEVEL
                        * (0.7 + 0.3 * (0.5 + 0.5 * bipolar(p.noise)))
                        * gain
                        * (high + hat::LOW_FLOOR_LEVEL * low_floor),
                    hat::FALL,
                )
            }
        };
        if matches!(kind, Kind::Kick | Kind::Rim) {
            // One output tone control shapes the machine (§5.5).
            let cutoff = (1_800.0 * 3.0_f32.powf(tone)).clamp(250.0, 0.4 * fs);
            let coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / fs).exp();
            self.tone_low_pass =
                flush(self.tone_low_pass + coefficient * (out - self.tone_low_pass));
            out = self.tone_low_pass;
        }
        if kind != Kind::Kick {
            out *= 1.0 + 0.2 * attack;
        }
        let character = if kind == Kind::Hat {
            bipolar(p.character)
        } else {
            0.0
        };
        out = (out * (1.0 + 0.5 * character)).tanh();
        // Activity tracker for parking: the voice's longest time constant, scaled like its decay.
        self.env *= (-6.9077554 / (longest * decay_scale.max(1.0) * fs)).exp();
        if kind == Kind::Kick {
            self.pitch_env *= (-6.9077554
                / (kick::T60 * decay_scale * envelope_time_scale(bipolar(p.pitch_decay)) * fs))
                .exp();
        } else {
            self.pitch_env = 0.0;
        }
        self.age = self.age.saturating_add(1);
        if self.age > (self.fs * 20.0) as u32
            || (self.age > (self.fs * 1.5) as u32 && self.env < 1e-8 && self.pitch_env < 1e-8)
        {
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
        let fs = self.fs;
        *self = Self::new();
        self.fs = fs;
    }
}
/// One RBJ high-pass biquad step in `f64`; `state` is `[x1, x2, y1, y2]`.
fn high_pass_biquad(state: &mut [f64; 4], input: f32, hz: f64, q: f64, fs: f32) -> f32 {
    let fs = f64::from(fs);
    let w = std::f64::consts::TAU * hz.min(0.45 * fs) / fs;
    let (sin, cos) = w.sin_cos();
    let alpha = sin / (2.0 * q);
    let a0 = 1.0 + alpha;
    let b0 = 0.5 * (1.0 + cos) / a0;
    let x = f64::from(input);
    let y = b0 * x - 2.0 * b0 * state[0] + b0 * state[1] + (2.0 * cos / a0) * state[2]
        - ((1.0 - alpha) / a0) * state[3];
    let y = if y.is_finite() && y.abs() > 1.0e-30 {
        y
    } else {
        0.0
    };
    *state = [x, state[0], y, state[2]];
    y as f32
}
fn envelope_time_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.25_f32.powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}
fn extended_decay_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.35_f32.powf(-value)
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
    #[test]
    fn four_calibrations_sound_and_reset() {
        for k in [Kind::Kick, Kind::Snare, Kind::Rim, Kind::Hat] {
            let p = SlotPatch::default();
            let mut v = Voice::new();
            // The machine's own noise: an alternating ±1 input sits at Nyquist, where the hat's
            // band-pass has a zero.
            let mut noise = Noise::new();
            v.trigger(1.0, p);
            let e: f32 = (0..24000)
                .map(|_| v.process(k, p, noise.tick()).abs())
                .sum();
            assert!(e > 0.1, "{k:?} {e}");
            v.reset();
            assert_eq!(v.process(k, p, 0.0), 0.0)
        }
    }
    fn render(kind: Kind, frames: usize) -> Vec<f32> {
        let p = SlotPatch::default();
        let mut v = Voice::new();
        let mut noise = Noise::new();
        v.trigger(0.82, p);
        (0..frames)
            .map(|_| v.process(kind, p, noise.tick()))
            .collect()
    }

    #[test]
    fn kick_swings_negative_first_and_rings_again_on_the_trailing_edge() {
        // The acquired comparison recording's first swing is negative, and the trailing edge of
        // the 8 ms trigger turns the network over again.
        let x = render(Kind::Kick, 2_400);
        let first = x.iter().find(|v| v.abs() > 0.05).copied().unwrap_or(0.0);
        assert!(first < 0.0, "first swing {first}");
        let swing = |r: std::ops::Range<usize>| x[r].iter().fold(0.0_f32, |m, v| m.min(*v));
        let edge = (kick::PULSE_WIDTH * 48_000.0) as usize;
        let (opening, second) = (swing(0..edge), swing(edge + 288..edge + 768));
        assert!(
            second < 0.4 * opening,
            "second swing {second} against {opening}"
        );
    }

    #[test]
    fn hat_and_snare_noise_end_in_finite_time() {
        // `(1 − t/T)^p` VCAs: the hat is silent after 59 ms at reference.
        let hat = render(Kind::Hat, 4_800);
        let end = (hat::FALL * 48_000.0) as usize + 1;
        assert!(hat[end..].iter().all(|v| *v == 0.0));
        assert!(hat[480..960].iter().any(|v| *v != 0.0));
        let snare = render(Kind::Snare, 4_800);
        let delay = (snare::NOISE_DELAY * 48_000.0) as usize;
        // Before the noise VCA opens only the body sounds: its first samples are smooth.
        assert!(snare[..delay].windows(2).all(|w| (w[1] - w[0]).abs() < 0.1));
    }

    #[test]
    fn the_hats_resonance_opens_high_and_settles() {
        // The recording's spectrum peaks at 9.2 kHz over its first 10 ms and 8.5 kHz over
        // 10–30 ms; the LC follows the envelope, so the voice's own centroid above 2 kHz falls
        // by at least 300 Hz between those windows and its coefficients hold within a block.
        let x = render(Kind::Hat, 4_800);
        // A zero-crossing rate stands in for the centroid: the band is narrow either way.
        let centroid = |r: std::ops::Range<usize>| {
            let seg = &x[r];
            let crossings = seg
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count() as f32;
            crossings * 48_000.0 / (2.0 * seg.len() as f32)
        };
        let (early, later) = (centroid(0..480), centroid(480..1_440));
        assert!(
            early > later + 300.0,
            "zero-crossing rate {early} Hz early against {later} Hz later"
        );
    }

    #[test]
    fn the_hats_noise_arrives_in_bursts() {
        // Its 1 ms envelope over 3–50 ms swings by a quarter of its trend on this voice's own
        // output, where the same path without the burst depth gives 0.20.
        let x = render(Kind::Hat, 4_800);
        let window = 48;
        let envelope: Vec<f32> = x[144..2_400]
            .chunks(window)
            .map(|w| (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt())
            .collect();
        let trend: Vec<f32> = (0..envelope.len())
            .map(|i| {
                let lo = i.saturating_sub(10);
                let hi = (i + 11).min(envelope.len());
                envelope[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
            })
            .collect();
        let ratios: Vec<f32> = envelope
            .iter()
            .zip(&trend)
            .map(|(e, t)| e / t.max(1e-9))
            .collect();
        let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
        let swing = (ratios.iter().map(|r| (r - mean) * (r - mean)).sum::<f32>()
            / ratios.len() as f32)
            .sqrt();
        assert!(swing > 0.25, "envelope swing {swing}");
    }

    #[test]
    fn noise_is_machine_shared_deterministic() {
        let mut a = Noise::new();
        let mut b = Noise::new();
        for _ in 0..100 {
            assert_eq!(a.tick(), b.tick())
        }
    }
}
