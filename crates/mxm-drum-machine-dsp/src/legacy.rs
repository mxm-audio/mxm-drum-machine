//! Evidence-aligned supporting-machine families (catalogue IDs 32–94).
//! One implementation owns the repeated electrical primitives, while every row below selects an
//! explicit machine source, topology and calibration. This is not a universal drum preset engine.
//!
//! Calibration status: the per-voice constants marked "fitted" were fitted to the acquired
//! comparison recording of that voice in the 2026-09-19 A/B pass (zero deviation, velocity 0.82,
//! 48 kHz), from the recordings indexed in `research:instruments/analogue-drum-machines.md` §11.2.
//! They are recording fits, not hardware verification: an authenticated capture of the voice at
//! its centre trim (§12.4) replaces each one. Voices without a recording (38, 39, 65, 66, 73,
//! 75–78, 83, 84, 86–89, 93, 94) keep their schematic-nominal tuning and take only the systemic
//! renderer changes and their measured siblings' circuit shapes.
use crate::{
    engine::SlotPatch,
    model::ModelId,
    resonator::{MAX_FREQUENCY_FRACTION, MIN_SAMPLE_RATE, Resonator},
};

/// Width of the machines' trigger pulse, seconds. The TR-808 widens its sequencer pulse to about
/// 1 ms (`research:instruments/analogue-drum-machines.md` §2.1); the CR-8000 closed hat's
/// recording puts its falling-edge click 0.95 ms after the metal opens. MEASURED from that
/// recording in the 2026-09-19 review pass; a trigger-line capture replaces it.
const TRIGGER_WIDTH: f32 = 0.000_95;

/// Level at which the output saturator sees the reference signal. A voice's output is therefore
/// bounded by `1 / OUTPUT_HEADROOM`.
const OUTPUT_HEADROOM: f32 = 0.25;

/// Oscillators per machine frame: the six-square metal bank plus, on the CR-8000 only, the
/// cowbell's own pair.
const OSCILLATORS: usize = 8;
/// The first six entries are the cymbal/hat bank that `Frame::metal` sums.
const BANK: usize = 6;

#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    pub noise: f32,
    pub metal: f32,
    oscillators: [f32; OSCILLATORS],
}
#[derive(Debug, Clone)]
pub struct Sources {
    noise: [u32; 6],
    squares: [[BlepSquare; OSCILLATORS]; 6],
}
impl Default for Sources {
    fn default() -> Self {
        Self::new()
    }
}
impl Sources {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            noise: [0x8001, 0x18003, 0x24007, 0x31009, 0x4100b, 0x5200d],
            squares: [start_bank(); 6],
        }
    }
    pub fn tick(&mut self, fs: f32) -> [Frame; 6] {
        let rate = f64::from(finite(fs, 48_000.0).max(MIN_SAMPLE_RATE));
        std::array::from_fn(|m| {
            let frequencies = metal_frequencies(m);
            let mut x = self.noise[m];
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.noise[m] = if x == 0 { 1 } else { x };
            let noise = (self.noise[m] as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let mut metal = 0.0;
            let mut n = 0.0;
            let mut oscillators = [0.0; OSCILLATORS];
            for (i, frequency) in frequencies.iter().copied().enumerate() {
                if frequency > 0.0 {
                    oscillators[i] = self.squares[m][i].tick(
                        f64::from(frequency),
                        rate,
                        f64::from(metal_duty(m, i)),
                    );
                    if i < BANK {
                        metal += oscillators[i];
                        n += 1.0
                    }
                }
            }
            Frame {
                noise,
                metal: metal / n,
                oscillators,
            }
        })
    }
    /// Moves all six machines to where running would have left them, without rendering
    /// (plan §4.4).
    ///
    /// The largest of the shared buses: six machines, up to eight squares each, plus a noise
    /// source apiece — measured at roughly 205 ns a sample, ticked whether or not any supporting
    /// model is loaded. Each square's phase is one multiply and each noise is a `O(1)` jump over
    /// GF(2), so the whole thing costs the same whether the gap was a millisecond or an hour.
    ///
    /// Not a reseed: the squares keep the phase relationships their machine's recording was
    /// fitted against, and the noise keeps its place in a sequence two voices may be reading
    /// together.
    pub fn advance(&mut self, samples: u64, fs: f32) {
        if samples == 0 {
            return;
        }
        let rate = f64::from(finite(fs, 48_000.0).max(MIN_SAMPLE_RATE));
        for m in 0..6 {
            self.noise[m] = crate::jump::advance_xorshift32(self.noise[m], samples);
            for (i, frequency) in metal_frequencies(m).iter().copied().enumerate() {
                if frequency > 0.0 {
                    self.squares[m][i].advance(
                        samples,
                        f64::from(frequency),
                        rate,
                        f64::from(metal_duty(m, i)),
                    );
                }
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new()
    }
}
/// Start phases of a machine frame's oscillators. CHOSEN deterministic spread: the hardware is
/// free-running and has no reset phase, and squares all starting at zero put one coincident edge
/// cluster on the first hit after a reset. The CR-8000 cowbell pair's two (0.85, 0.5) are chosen so
/// the reference hit's first cycles read near the 795 Hz its recording opens with.
const START_PHASES: [f64; OSCILLATORS] = [0.03, 0.19, 0.37, 0.53, 0.71, 0.89, 0.85, 0.5];

/// One cycle, held wide so a wrap is a value rather than a lost carry.
const CYCLE_WIDE: u128 = 1_u128 << 64;
/// One cycle as a float, for turning a fraction or a frequency into fixed point.
const CYCLE_FLOAT: f64 = 18_446_744_073_709_551_616.0;

/// A fraction of a cycle as the fixed-point phase counts it.
const fn fixed(fraction: f64) -> u64 {
    (fraction * CYCLE_FLOAT) as u64
}

/// The per-sample phase increment for a frequency, in fixed point.
fn increment_fixed(frequency: f64, sample_rate: f64) -> u64 {
    ((frequency / sample_rate).clamp(0.0, 0.25) * CYCLE_FLOAT) as u64
}

const fn start_bank() -> [BlepSquare; OSCILLATORS] {
    let mut bank = [BlepSquare::new(0.0); OSCILLATORS];
    let mut index = 0;
    while index < OSCILLATORS {
        bank[index] = BlepSquare::new(START_PHASES[index]);
        index += 1;
    }
    bank
}

/// One free-running square, the trivial ±1 pulse at its duty with both edges repaired by the
/// four-point (third-order B-spline) PolyBLEP residual of Välimäki, Pekonen and Nam, "Perceptually
/// informed synthesis of bandlimited classical waveforms using integrated polynomial
/// interpolation" (JASA 2012, Table VII), as mxm-kit's `docs/oscillators/02-antialiasing.md`
/// §2.6.2 gives it — the technique `metal_808.rs` uses, kept local here. The trivial squares
/// folded partials back into the hats' band; correcting two samples ahead of each edge delays a
/// free-running source by two samples, which it cannot show.
#[derive(Debug, Clone, Copy)]
struct BlepSquare {
    /// Phase as a fraction of a cycle in fixed point: the whole `u64` range is one cycle.
    ///
    /// Integer for the reason `metal_808.rs` gives at length — wrapping *is* the cycle, so a
    /// dormant square can be advanced in one multiply, where an `f64` phase rounds once per
    /// sample and cannot be jumped without drifting from what stepping would produce. Measured
    /// there at −179 dBFS worst against the previous accumulation: inaudible, and below a 24-bit
    /// LSB.
    phase: u64,
    /// Samples n−2 … n+2 of the band-limited square under construction.
    pending: [f64; 5],
}
impl BlepSquare {
    const fn new(phase: f64) -> Self {
        Self {
            phase: fixed(phase),
            pending: [0.0; 5],
        }
    }

    /// Moves the square to where running would have left it, without rendering (plan §4.4).
    ///
    /// One multiply for the phase, then the last few samples replayed so the band-limiting window
    /// is the one ticking would have built. A residual lives at most four samples, so a gap of
    /// five or more leaves nothing of the old window to carry.
    fn advance(&mut self, samples: u64, frequency: f64, sample_rate: f64, duty: f64) {
        const WINDOW: u64 = 5;
        if samples == 0 {
            return;
        }
        if samples < WINDOW {
            for _ in 0..samples {
                let _ = self.tick(frequency, sample_rate, duty);
            }
            return;
        }
        let increment = increment_fixed(frequency, sample_rate);
        self.phase = self
            .phase
            .wrapping_add(increment.wrapping_mul(samples - WINDOW));
        self.pending = [0.0; 5];
        for _ in 0..WINDOW {
            let _ = self.tick(frequency, sample_rate, duty);
        }
    }
    fn tick(&mut self, frequency: f64, sample_rate: f64, duty: f64) -> f32 {
        let increment = increment_fixed(frequency, sample_rate);
        let duty_fixed = fixed(duty);
        let phase = self.phase;
        let pending = &mut self.pending;
        pending[2] += if phase < duty_fixed { 1.0 } else { -1.0 };
        // Held wide so the wrap is a value to compare against rather than a lost carry.
        let next = u128::from(phase) + u128::from(increment);
        // The falling edge at the duty point and the rising edge at the wrap, each a step of 2.
        for (edge, height) in [(u128::from(duty_fixed), -2.0), (CYCLE_WIDE, 2.0)] {
            if increment > 0 && u128::from(phase) < edge && edge <= next {
                let residual = blep4((next - edge) as f64 / increment as f64);
                for (slot, value) in pending[1..].iter_mut().zip(residual) {
                    *slot += height * value;
                }
            }
        }
        let out = pending[0] as f32;
        pending.rotate_left(1);
        pending[4] = 0.0;
        self.phase = phase.wrapping_add(increment);
        out
    }
}

/// The four-point B-spline PolyBLEP residual for a step whose next sample trails it by `d` of a
/// sample: corrections for the samples two before to one after that sample.
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

const fn metal_frequencies(machine: usize) -> [f32; OSCILLATORS] {
    match machine {
        // CR-8000: this unit's six metal oscillators, then the cowbell's own pair. FITTED in the
        // 2026-09-19 review pass to the spectral lines of the acquired cymbal and open-hat
        // recordings' tails (a 2²¹-point FFT of each): each square's harmonic series, searched
        // frequency by frequency and kept where its low harmonics form a complete series. These
        // six explain 91 % of the cymbal's line power above 1 kHz; the service-note nominals
        // (283, 368, 444, 524, 647 and 794 Hz, `research:instruments/analogue-drum-machines.md`
        // §5.6) explain 1 %. The 902/918 Hz pair beats at 15 Hz, the kind of adjacent ratio the
        // service note warns against, and it is in both recordings. The cowbell's 571.1/838.2 Hz
        // are its recording's two partials; 838 Hz is not in the bank's lines, and 571 Hz sits
        // 10 cents from the bank's 574 Hz, so the pair stays its own. A frequency-counter
        // measurement of each oscillator replaces all eight.
        0 => [311.55, 574.35, 780.93, 902.3, 917.55, 1149.3, 571.1, 838.15],
        // TR-606: FITTED in the 2026-09-19 review pass the same way, to the cymbal and open-hat
        // recordings' lines. The six explain 69 % of their odd-harmonic line power above 1 kHz; the
        // printed-value nominals (246, 308, 367, 418, 440 and 627 Hz,
        // `research:instruments/analogue-drum-machines.md` §5.7) explain 3 %. Only 440 Hz sits on
        // its nominal.
        1 => [401.44, 439.6, 480.5, 553.32, 679.74, 971.58, 0., 0.],
        // DR-110: FITTED in the review pass to the cymbal and open-hat recordings' lines, whose
        // mixed noise leaves them the least clear, and refitted in the owner-notes pass. Two of
        // the review's four were other squares' harmonics: 607.84 Hz restated the 304 Hz square's
        // even ones, and 371.44 Hz a third of 1114.3 Hz, whose own harmonics 3–9 (3343, 5571,
        // 6686, 7800, 8914 and 10028 Hz) are the recordings' strongest lines. The open hat's
        // strongest unexplained lines — 4040, 4939, 5388, 5837, 7184, 8081 and 9877 Hz — are
        // harmonics 9, 11, 12, 13, 16, 18 and 22 of 448.97 Hz. All four now sit within 2 % of
        // §5.8's approximate 305/444/794/1136 Hz, and explain 88–93 % of the three recordings'
        // line power above 2 kHz (the review's four: 81–83 %).
        2 => [304.26, 448.97, 789.28, 1114.3, 0., 0., 0., 0.],
        // CR-78: cowbell pair, then the metal-beat bell group. The pair is the acquired comparison
        // recording's 545/775 Hz (2026-09-19 A/B pass), 1.8 and 3.1 % below the service note's
        // 555/800 Hz — within trim tolerance; a centre-trim capture replaces it.
        3 => [545., 775., 4080., 5620., 6170., 0., 0., 0.],
        4 => [317., 509., 733., 1019., 0., 0., 0., 0.],
        _ => [271., 421., 613., 887., 0., 0., 0., 0.],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Topology {
    Resonator,
    Dual,
    Noise,
    Metal,
    Snare,
    Diode,
    Clap,
    Guiro,
}
#[derive(Debug, Clone, Copy)]
struct Config {
    machine: usize,
    topology: Topology,
    /// Principal pole or filter frequency, Hz.
    f1: f32,
    /// Second pole (Dual, Snare second body filter, Resonator second mode) or guiro rate, Hz.
    f2: f32,
    /// Main T60, seconds: the pole's decay for tonal voices, the release for gated ones.
    decay: f32,
}
fn config(id: u8) -> Config {
    use Topology::*;
    let c = |machine, topology, f1, f2, decay| Config {
        machine,
        topology,
        f1,
        f2,
        decay,
    };
    // Frequencies and T60s of the recorded voices are fitted to their acquired comparison
    // recordings in the 2026-09-19 A/B pass: the rest pitch from the settled late ring, the T60
    // from the RMS envelope's slope (release T60 for gated voices, see `shape`). A centre-trim
    // hardware capture of each voice replaces them. Unrecorded rows keep their nominal values.
    match id {
        32 => c(0, Dual, 61.0, 83.0, 0.250),
        33 => c(0, Snare, 222.4, 333., 0.100),
        34 => c(0, Diode, 103., 140., 0.420),
        35 => c(0, Diode, 140., 103., 0.330),
        36 => c(0, Diode, 197., 0., 0.370),
        // The low conga follows its acquired recording, which rings at 299.4 Hz (owner's ruling in
        // the 2026-09-19 review pass; the first pass had kept 175 Hz as doubtful). The unrecorded
        // mid and high congas keep the circuit's intervals: CHOSEN inference, 255 and 370 Hz scaled
        // by the same 299.4/175. Recordings of the other two replace them.
        37 => c(0, Diode, 299.4, 0., 0.235),
        38 => c(0, Diode, 436.3, 0., 0.12),
        39 => c(0, Diode, 633.0, 0., 0.08),
        40 => c(0, Resonator, 1300., 0., 0.008),
        41 => c(0, Metal, 0., 0., 1.700),
        42 => c(0, Metal, 0., 0., 0.150),
        43 => c(0, Metal, 0., 0., 0.850),
        44 => c(0, Clap, 1400., 0., 0.150),
        45 => c(0, Resonator, 2246., 0., 0.040),
        46 => c(0, Metal, 0., 0., 0.660),
        47 => c(1, Dual, 61.4, 103., 0.260),
        48 => c(1, Snare, 208., 0., 0.130),
        49 => c(1, Resonator, 131., 0., 0.313),
        50 => c(1, Resonator, 207., 0., 0.210),
        51 => c(1, Metal, 0., 0., 0.700),
        52 => c(1, Metal, 0., 0., 0.110),
        53 => c(1, Metal, 0., 0., 0.300),
        54 => c(2, Resonator, 50.4, 0., 0.160),
        55 => c(2, Snare, 243., 0., 0.080),
        56 => c(2, Metal, 0., 0., 1.900),
        57 => c(2, Metal, 0., 0., 0.160),
        58 => c(2, Metal, 0., 0., 0.040),
        59 => c(2, Clap, 1100., 0., 0.700),
        60 => c(3, Resonator, 59.4, 0., 0.270),
        61 => c(3, Snare, 302., 0., 0.055),
        62 => c(3, Resonator, 1405., 0., 0.007),
        63 => c(3, Noise, 8000., 0., 0.013),
        64 => c(3, Noise, 8000., 0., 0.330),
        65 => c(3, Noise, 10000., 0., 0.020),
        66 => c(3, Resonator, 2630., 0., 0.018),
        67 => c(3, Resonator, 591., 0., 0.095),
        68 => c(3, Resonator, 387., 2511., 0.058),
        69 => c(3, Resonator, 196.5, 0., 0.380),
        70 => c(3, Metal, 0., 0., 0.060),
        71 => c(3, Noise, 4600., 0., 0.090),
        72 => c(3, Guiro, 125., 77., 0.030),
        73 => c(3, Dual, 6170., 4080., 0.050),
        74 => c(4, Resonator, 62.7, 0., 0.250),
        75 => c(4, Resonator, 208., 0., 0.160),
        76 => c(4, Resonator, 400., 0., 0.040),
        77 => c(4, Resonator, 600., 0., 0.040),
        78 => c(4, Resonator, 830., 0., 0.035),
        79 => c(4, Resonator, 1471., 0., 0.014),
        80 => c(4, Resonator, 2341., 0., 0.033),
        81 => c(4, Snare, 273., 398., 0.150),
        82 => c(4, Noise, 7800., 0., 0.12),
        83 => c(4, Noise, 9000., 0., 0.040),
        84 => c(4, Noise, 7000., 0., 0.400),
        85 => c(5, Resonator, 51., 0., 0.770),
        86 => c(5, Resonator, 190., 0., 0.15),
        87 => c(5, Resonator, 290., 0., 0.11),
        88 => c(5, Resonator, 520., 0., 0.060),
        89 => c(5, Dual, 780., 540., 0.060),
        90 => c(5, Resonator, 353., 1116., 0.050),
        91 => c(5, Snare, 274., 0., 0.025),
        92 => c(5, Noise, 11100., 0., 0.650),
        93 => c(5, Noise, 9000., 0., 0.035),
        94 => c(5, Noise, 5200., 0., 0.18),
        _ => c(0, Noise, 0., 0., 0.01),
    }
}
#[must_use]
pub const fn machine(id: u8) -> usize {
    match id {
        32..=46 => 0,
        47..=53 => 1,
        54..=59 => 2,
        60..=73 => 3,
        74..=84 => 4,
        _ => 5,
    }
}

/// Where the struck ring starts, in degrees: 0 is a sine from zero (the pole driven at its
/// low-pass port), 90 a cosine from its peak (the trigger pulse entering at the network's
/// band-pass port, as in a bridged-T or twin-T feedback path); 180 is a sine on the negative
/// swing. Fitted by least squares to the first cycles of each acquired comparison recording in the
/// 2026-09-19 A/B pass, jointly with `strike_rc`; a probe on the network's output at the trigger
/// edge replaces it. Unrecorded voices outside the CR-8000 tom loop keep a sine start.
const fn strike_phase(id: u8) -> f32 {
    match id {
        32 => 105.0,
        33 => 95.0,
        // The CR-8000 low and mid toms' recordings reach their first peak 9.4 and 7.6 ms in, after a
        // small first lobe and a flat trough (`restrikes`, `diode_clip`).
        34 => -4.0,
        35 => 11.0,
        36 => 105.0,
        37..=39 => 135.0,
        45 => 111.0,
        47 => 119.0,
        // The TR-606, CR-78 and TR-66 snare recordings start their body on the negative swing.
        48 => 180.0,
        // The TR-66 snare's recording opens on a short positive lobe, its ring near cosine phase.
        81 => 110.0,
        61 => 51.0,
        // The TR-606 low tom's ring, under its trigger step (`trigger_pulse`), starts falling.
        49 => 159.0,
        50 => 137.0,
        54 => 69.0,
        55 => -60.0,
        60 => 139.0,
        62 => 90.0,
        67 => 105.0,
        68 => 84.0,
        69 => 86.0,
        74 => 101.0,
        79 => 57.0,
        80 => 180.0,
        85 => 97.0,
        // The FR-2L clave's body starts near its negative peak (with `third_mode`); its snare's
        // body lobe peaks at 0.5 ms.
        90 => -122.0,
        91 => 50.0,
        _ => 0.0,
    }
}

/// Time constant of the trigger pulse's RC shaping, seconds: the network is struck by an
/// exponential pulse rather than one sample, so the ring rises over a fraction of a millisecond
/// as the recordings do (the TR-808 widens its trigger to about 1 ms,
/// `research:instruments/analogue-drum-machines.md` §2.1). 0.4 ms was fitted jointly with
/// `strike_phase` to the tom, conga and snare recordings in the 2026-09-19 A/B pass; three kicks
/// keep the sharper edge spike their recordings show, and the rims and claves start sharply on a
/// one-sample strike. A trigger-line capture replaces it.
const fn strike_rc(id: u8) -> f32 {
    match id {
        // The CR-8000 and TR-606 kicks and the CR-78 kick keep a sharper edge spike.
        32 => 0.00014,
        47 => 0.00017,
        60 => 0.0001,
        // The CR-8000 snare's first half-cycle rises over 0.7 ms (review pass).
        33 => 0.0007,
        37..=39 => 0.0002,
        // The FR-2L clave's first lobe rises over about 0.1 ms, its snare's body over 0.2 ms.
        90 => 0.0001,
        91 => 0.0002,
        34..=36 | 48..=50 | 54 | 55 | 61 | 67 | 74 | 81 | 85 => 0.0004,
        69 => 0.0002,
        _ => 0.0,
    }
}

/// A native pitch sweep: `[depth, T60 seconds]`, where the pole starts `depth × strike` above rest
/// (below it for a negative depth) and relaxes exponentially. Diode rows are the CR-8000 tom/conga loop, whose two
/// diodes conduct harder at high amplitude (`research:instruments/analogue-drum-machines.md`
/// §5.6); the kick and tom rows are the swept bridged-T/transistor networks visible in their
/// recordings. Depths and times are least-squares fits of a swept damped sinusoid to each acquired
/// comparison recording's first 150 ms (2026-09-19 A/B pass); an instantaneous-frequency capture
/// of the hardware replaces them. Pitch envelope scales the depth (−1 removes it), Pitch decay the time.
const fn native_sweep(id: u8) -> [f32; 2] {
    match id {
        32 => [0.56, 0.002],
        // The low tom's recording starts its first cycles at 124 Hz against its 104 Hz rest.
        34 => [0.3, 0.15],
        35 => [0.14, 0.440],
        36 => [1.17, 0.0068],
        // The low conga's recording settles within a cycle once its second strike has passed: its
        // diodes' pull is under 25 cents and gone in about 10 ms.
        37..=39 => [0.015, 0.04],
        // The CR-8000 rimshot runs flat while loud: its recording's first cycles sit near 1.08 kHz
        // and its later, quiet rings near 1.25–1.36 kHz. A negative depth.
        40 => [-0.25, 0.006],
        // The phase-shift clave starts near 2.38 kHz and settles to 2.25 kHz over about 10 ms.
        45 => [0.0857, 0.023],
        47 => [1.35, 0.023],
        // The TR-606 snare body falls from about 330 Hz to its 208 Hz rest within 20 ms (its
        // recording's strongest early partial sits at 259 Hz). It is a fixed property of the body
        // network: the model declares no Pitch envelope control.
        48 => [1.2, 0.069],
        49 => [0.60, 0.250],
        50 => [1.18, 0.0079],
        54 => [1.97, 0.0155],
        60 => [1.68, 0.031],
        // The CR-78 snare's body starts at about 317 Hz and settles to 302 Hz within 30 ms.
        61 => [0.06, 0.04],
        // The CR-78 low conga's first half-cycle is short: it starts a few per cent sharp.
        69 => [0.08, 0.005],
        85 => [0.45, 0.013],
        _ => [0.0, 0.0],
    }
}

/// Rise of a released pole's damping, per second per second: after the gate's hold the decay rate
/// grows linearly from `6.9 / config().decay`, so the ring falls along a Gaussian-like curve and
/// looks linear before its exponential tail. The CR-8000 phase-shift clave's recording holds for
/// 6.9 ms, then its cycle peaks fall 0.83 → 0.20 over 7 ms in near-straight steps, which a
/// constant T60 cannot give; the fit's rate is 173 s⁻¹ rising by 17 300 s⁻². The loop gain
/// falling with the gate's discharge is the likely mechanism. Fitted to the acquired comparison
/// recording's cycle peaks in the 2026-09-19 review pass; a capture of the oscillator's gate
/// replaces it. Decay scales the whole curve in time.
const fn damping_ramp(id: u8) -> f32 {
    match id {
        45 => 17_300.0,
        _ => 0.0,
    }
}

/// Amplitude-dependent damping of the CR-8000 diode loop: the pole's T60 is divided by
/// `1 + damping × diode conduction`, so the first cycles fall faster than the tail (the recordings'
/// early drop). Fitted to the acquired comparison recordings' first 50 ms in the 2026-09-19 A/B
/// pass; an amplitude-stepped hardware measurement of the loop replaces it.
const fn diode_damping(id: u8) -> f32 {
    match id {
        34 | 35 => 0.0,
        _ => 0.0,
    }
}

/// A second pole struck with the first: `[gain, T60 seconds]`, at `config().f2`, including the
/// CR-8000 toms' sympathetic neighbours. Fitted to the
/// acquired comparison recordings in the 2026-09-19 A/B pass: 68's recording carries a strong
/// 2.51 kHz partial beside its 390 Hz body, 90's an inharmonic 1.12 kHz partial, 33's the CR-8000
/// snare's second bridged-T at 333 Hz (`research:instruments/analogue-drum-machines.md` §5.6),
/// nearly silent in its recording, and 81's the TR-66 snare's coupled low-bongo body at 400 Hz
/// (§5.3). A per-partial hardware decay measurement replaces them.
const fn second_mode(id: u8) -> [f32; 2] {
    match id {
        // The CR-8000 snare's second, 333 Hz body sits more than 55 dB under its first in its
        // recording.
        33 => [0.004, 0.100],
        // The CR-8000 toms ring in each other: the low tom's recording carries the mid tom's
        // 140 Hz 22 dB down, the mid tom's the low tom's 103 Hz 9 dB down. The mid tom keeps it
        // 16.5 dB down so the rest-pitch measurement still names its own 140 Hz note. Read as
        // crosstalk through the shared trigger and reverb-noise wiring; a capture of one tom with
        // the others disconnected would confirm it.
        34 => [0.08, 0.440],
        35 => [0.15, 0.320],
        68 => [0.3, 0.005],
        81 => [0.10, 0.050],
        90 => [0.46, 0.034],
        _ => [0.0, 0.0],
    }
}

/// Milliseconds before a snare's wires sound. The FR-2L snare's recording is one smooth body lobe
/// for its first 1.4 ms before any wire noise, the CR-78 snare's for 0.8 ms and the TR-66
/// snare's for 1.8 ms. Fitted in the 2026-09-19 sweep pass; a capture of the wire gate replaces
/// it.
const fn wire_delay(id: u8) -> f32 {
    match id {
        61 => 0.8,
        81 => 1.8,
        91 => 1.4,
        _ => 0.0,
    }
}

/// A third pole struck with the first: `[Hz, gain, T60 seconds, phase degrees]`. The FR-2L
/// clave's recording is double-humped: beside its 353 Hz body it carries a 1.12 kHz partial
/// (`second_mode`, 0.46 of the body) and a 1.30 kHz one at 0.82 that dies in 20 ms, and together
/// they make its first millisecond two sharp negative lobes. Fitted by least squares to that
/// recording's first 12 ms in the 2026-09-19 sweep pass (the three modes leave 22 % of its RMS);
/// a per-partial hardware measurement replaces it.
const fn third_mode(id: u8) -> [f32; 4] {
    match id {
        // The CR-78 low bongo's 2.51 kHz mode rings on after its opening burst (`second_mode`).
        68 => [2_511.0, 0.43, 0.07, -136.0],
        90 => [1_301.0, 0.82, 0.020, -91.0],
        _ => [0.0, 0.0, 0.1, 0.0],
    }
}

/// The start phase of pole `index` (1 the second mode, 2 the third), where it is not the voice's
/// own `strike_phase`. Fitted with `third_mode`.
fn mode_phase(id: u8, index: usize, voice: [f32; 2]) -> [f32; 2] {
    let degrees = match (id, index) {
        (90, 1) => -97.0,
        // The CR-78 low bongo's 2.51 kHz mode opens on two positive half-waves, 0.25 and 0.65 ms
        // in.
        (68, 1) | (68, 2) => -136.0,
        (90, 2) => third_mode(90)[3],
        _ => return voice,
    };
    let radians = f32::to_radians(degrees);
    [radians.cos(), radians.sin()]
}

/// The trigger pulse itself reaching a tonal voice's output: `[level, width ms, droop ms, rise
/// ms]`, a step of `level` at the trigger that rises and droops with the given time constants and
/// stops when the trigger ends. The CR-78 kick's recording opens on a spike almost twice its first
/// ring peak, falling within about a millisecond; the TR-606 low tom's opens on a positive step
/// that droops for 1.2 ms before the trigger's falling edge drops it; the CR-8000 low and mid
/// toms' open on a step of about a sixth of their peak that their ring then carries; the CR-78
/// low conga's first peak is a sharp spike on its ring, 0.4 ms in. Fitted in the
/// 2026-09-19 sweep pass to those recordings' first milliseconds; a trigger-line capture replaces
/// them.
const fn trigger_pulse(id: u8) -> [f32; 4] {
    match id {
        34 => [0.5, 1.9, 1.0, 0.02],
        35 => [0.6, 1.9, 1.0, 0.02],
        49 => [0.9, 1.2, 3.0, 0.04],
        60 => [2.2, 1.6, 0.45, 0.08],
        62 => [1.3, 0.25, 0.06, 0.02],
        69 => [0.4, 1.2, 0.5, 0.05],
        _ => [0.0, 0.0, 1.0, 1.0],
    }
}

/// The output's polarity. The CR-78 rim and cowbell and the TR-66 rim recordings start on the
/// opposite polarity to the models' struck networks, with the rest of their waveforms matching;
/// `research:instruments/analogue-drum-machines.md` §5.3–5.4 says nothing about the output
/// stages' inversion, so the recordings decide (2026-09-19 sweep pass). A probe on each output
/// replaces it.
const fn output_sign(id: u8) -> f32 {
    match id {
        62 | 70 | 79 => -1.0,
        _ => 1.0,
    }
}

/// A ring's later decay: `[ms after the trigger, T60 seconds at reference]`; Decay scales it as it
/// scales the ring. The CR-8000 rimshot's recording keeps ringing quietly after its strike train
/// (`strike_train`) ends, falling to −40 dB at about 35 ms and ending near 50 ms, where its 8 ms
/// ring alone stopped at 27 ms. Fitted in the 2026-09-19 sweep pass to that recording's envelope;
/// the rimshot's schematic, once read, replaces it.
const fn late_ring(id: u8) -> [f32; 2] {
    match id {
        40 => [25.0, 0.055],
        _ => [0.0, 0.0],
    }
}

/// A train of later strikes, jittered by the machine's shared noise: `[period ms, jitter share,
/// charge, start ms, end ms]`. The train's span and fade are the trigger circuit's, so Decay,
/// which lengthens the ring, lengthens neither. After its first strikes the CR-8000 rimshot's
/// recording keeps being re-struck about every 2 ms at irregular intervals — spikes at about 6,
/// 7.5 and 9 ms, then small rings from 12.7 ms on — a trigger that keeps bouncing. Each interval
/// is the period times `1 + jitter × noise`; the strike takes the noise's sign and fades at the
/// second envelope stage's reference T60 (`shape().stage_t60`, 70 ms). Past 25 ms its strikes are
/// under −40 dB and the train ends; the last rings carry on at `late_ring`. Fitted in the
/// 2026-09-19 review and sweep passes to that recording's spike spacing and envelope; the
/// rimshot's schematic, once read, replaces this reading.
const fn strike_train(id: u8) -> [f32; 5] {
    match id {
        40 => [2.1, 0.3, 0.8, 5.5, 25.0],
        _ => [0.0, 0.0, 0.0, 0.0, 0.0],
    }
}

/// Later strikes of the same trigger, `(ms after the trigger, charge relative to it)`; a zero
/// charge ends the list. Each adds to the trigger's RC pulse, so it both rings the pole and passes
/// the output's edge click.
/// - The CR-8000 toms and congas: the trigger's falling edge, 2.1 ms in (1.65 ms on the congas),
///   strikes the diode loop
///   again with the opposite sign, which is the recordings' kinked first cycle (the conga's trough
///   at 1.6 ms after the page's onset). A single strike left the least-squares residual of their
///   first 15 ms at 0.105–0.128; the second strike brings the high tom and low conga to
///   0.034–0.066 (`docs/drum-model-fitting.md` §6, "A long trigger strikes twice"). The low and
///   mid toms' recordings fall steeply from a small first lobe 1.6–1.7 ms in: their edge comes at
///   1.9 ms with a charge of −1.5 (2026-09-19 sweep pass, with `strike_phase` and
///   `trigger_pulse`).
/// - The CR-8000 rimshot, one of the service note's exceptions to the TR-808 method
///   (`research:instruments/analogue-drum-machines.md` §5.6): its recording is a smooth 1.1 kHz
///   ring, T60 about 8 ms, struck again by sharp alternating pulses about 2.2, 2.8 and 3.9 ms in;
///   a trigger that bounces. The times and charges are read off that one hit.
///
/// Fitted to the acquired comparison recordings in the 2026-09-19 review pass; a trigger-line
/// capture replaces them.
const fn restrikes(id: u8) -> [[f32; 2]; 3] {
    match id {
        34 | 35 => [[1.9, -1.5], [0.0, 0.0], [0.0, 0.0]],
        36 => [[2.1, -0.7], [0.0, 0.0], [0.0, 0.0]],
        37..=39 => [[1.65, -0.7], [0.0, 0.0], [0.0, 0.0]],
        40 => [[2.29, -0.8], [2.9, 0.35], [4.02, -0.6]],
        // The CR-78 snare's trigger falls at 0.6 ms: the deep negative swing to 0.9 ms in its
        // recording.
        61 => [[0.6, -1.4], [0.0, 0.0], [0.0, 0.0]],
        _ => [[0.0, 0.0]; 3],
    }
}

/// A two-band metal voice's lower band: `[band-pass Hz, Q, level, T60 seconds, release T60
/// seconds]`, fed from the same source as the main colour through a two-pole high-pass an octave
/// below it (the squares' fundamentals stay out, as in the recordings) and decaying on its own:
/// at its T60 while a gate holds or where there is no gate, then at its release T60. The TR-606
/// cymbal takes both of the metal bank's 3.44 and 7.10 kHz bands
/// (`research:instruments/analogue-drum-machines.md` §5.7), and its recording keeps the 3.4 kHz
/// line for half a second after the top releases; the DR-110 cymbal's low-metal body outlasts its
/// high-metal body (§5.8); the CR-8000 cymbal's recording holds a 0.5–2.5 kHz band after its top
/// has fallen. Fitted to the recordings' band-split envelopes in the 2026-09-19 A/B pass; a
/// per-band hardware capture replaces them.
const fn low_band(id: u8) -> [f32; 5] {
    match id {
        41 => [1_500.0, 1.0, 0.01, 1.8, 1.8],
        // Halved on 2026-09-20 when the cymbal moved onto the hats' narrow high band: the trim
        // that restores its peak lifts everything under that band, and at the old level the
        // 3.4 kHz line led the tail's six loudest, which the recording's does not.
        51 => [3_440.0, 10.0, 0.06, 3.6, 1.5],
        56 => [3_450.0, 6.0, 0.08, 2.5, 2.5],
        _ => [0.0, 1.0, 0.0, 1.0, 1.0],
    }
}

/// Output-stage nonlinearity of a tonal voice, after its roll-off:
/// `[square, cube, tick, threshold]`. The bend `y = x + square·x² + cube·x³` is exactly zero at
/// rest and gives the recordings' harmonic series. `tick` is a switching threshold the ring crosses
/// twice a cycle: each crossing injects a step of `±2·tick·envelope` through a 3 kHz one-pole
/// high-pass. That is the broadband tick the TR-606 kick and toms' recordings show at their own
/// half-period until the ring falls below `threshold`, the swing-type VCA chopping the audio
/// (`research:instruments/analogue-drum-machines.md` §2.6). The CR-8000 toms' diode loop bends and
/// faintly ticks (§5.6); the phase-shift clave limits (§5.6). Fitted to each recording's harmonic
/// levels over its first 150 ms and its 4–20 kHz band in the 2026-09-19 A/B pass (the CR-8000
/// toms' rows refitted with `diode_clip` in the sweep pass); a transfer-curve and switching
/// measurement of each output stage replaces them.
const fn shaper(id: u8) -> [f32; 4] {
    match id {
        34 => [0.0, 0.45, 0.005, 0.07],
        35 => [0.0, 0.28, 0.004, 0.07],
        45 => [0.0, 0.0, 0.0, 0.0],
        54 => [0.08, 0.3, 0.0, 0.0],
        47 => [0.0, 0.1, 0.024, 0.07],
        49 => [0.1, 0.7, 0.03, 0.07],
        50 => [0.15, 0.2, 0.036, 0.07],
        _ => [0.0, 0.0, 0.0, 0.0],
    }
}

/// A body's own distortion: `[H2, H3, H4, H5, H6, H7, H8, off, on]`. While the ring's amplitude
/// is above `on` it carries a fixed harmonic series, each harmonic's amplitude relative to the
/// ring's own, which fades out as the amplitude falls to `off`. Each harmonic is `cos(kθ)` on the
/// ring's phase, by the Chebyshev polynomial of the ring normalised by its amplitude, so the series
/// is exact and band-limited; the signs are those of a trough flattened by the circuit.
/// - The CR-8000 toms: the recordings' H2–H8 hold the same levels relative to the fundamental
///   while the ring is loud (from 30 to about 110–140 ms) and then fall 20–30 dB within 20 ms. A
///   polynomial bend's series falls steadily with the ring instead; this one stops where the two
///   diodes in the loop stop conducting (`research:instruments/analogue-drum-machines.md` §5.6).
/// - The CR-8000 snare: its recording carries H2–H4 of the 223 Hz body 45–60 dB down at every
///   level of the ring, so the series never gates.
/// - The DR-110 kick: its recording's 50 Hz ring carries H2 about 42 dB down and H3–H8 55–65 dB
///   down (H5 nearer 45) through its first 110 ms, where the model's ring was a pure sine (owner:
///   "the waveform on the original is less pure sine").
/// - The CR-8000 phase-shift clave: its recording holds H2–H8 at the same levels, −21 to −69 dB,
///   from its first milliseconds to −35 dB (§5.6's limiting oscillator), where a square-and-cube
///   bend had given H2/H3 that fell with the ring and nothing above.
///
/// Fitted in the 2026-09-19 owner-notes pass to the recordings' Blackman-Harris harmonic series
/// over time (owner: "the original has more overtones"); a capture of the circuit's node replaces
/// it.
const fn body_harmonics(id: u8) -> [f32; 9] {
    match id {
        54 => [
            0.0079, -0.0014, 0.0018, -0.0045, 0.0008, -0.0010, 0.0005, 0.0, 1.0e-6,
        ],
        45 => [
            0.071, -0.020, 0.0035, -0.0012, 0.0008, -0.0010, 0.0004, 0.0, 1.0e-6,
        ],
        33 => [0.0018, -0.005, 0.0020, -0.0008, 0.0, 0.0, 0.0, 0.0, 1.0e-6],
        34 => [
            0.020, -0.016, 0.0056, -0.0045, 0.0028, -0.0022, 0.0011, 0.135, 0.175,
        ],
        35 => [
            0.028, -0.014, 0.0045, -0.0028, 0.0022, -0.0020, 0.0008, 0.12, 0.16,
        ],
        36 => [
            0.002, -0.0028, 0.0014, -0.0009, 0.0007, -0.0006, 0.0005, 0.065, 0.09,
        ],
        _ => [0.0; 9],
    }
}

/// The distortion `body_harmonics` adds to a ring of `value` at `amplitude`.
fn harmonic_series(series: [f32; 9], value: f32, amplitude: f32) -> f32 {
    let [off, on] = [series[7], series[8]];
    if amplitude <= off || amplitude <= 1.0e-9 {
        return 0.0;
    }
    let x = ((amplitude - off) / (on - off)).clamp(0.0, 1.0);
    let gate = x * x * (3.0 - 2.0 * x);
    let u = (value / amplitude).clamp(-1.0, 1.0);
    let (mut previous, mut current) = (1.0_f32, u);
    let mut sum = 0.0;
    for harmonic in &series[..7] {
        let next = 2.0 * u * current - previous;
        previous = current;
        current = next;
        sum += harmonic * current;
    }
    gate * amplitude * sum
}

/// Knee of the CR-8000 toms' negative clip, relative to the trigger's level and applied after the
/// output bend (`shaper`): below it the excess passes at 8 %. Their recordings' first trough is
/// cut flat for about 1.5 ms (the diodes in the RC loop,
/// `research:instruments/analogue-drum-machines.md` §5.6); only that first, largest trough reaches
/// the knee. Fitted in the 2026-09-19 review pass and refitted in the sweep pass, with `shaper`, to
/// the recordings' first cycles and harmonic levels; a transfer measurement of the loop replaces
/// it.
const fn diode_clip(id: u8) -> f32 {
    match id {
        34 => 1.67,
        35 => 2.1,
        _ => 0.0,
    }
}

/// A low-passed rectangular trigger pulse mixed into the output: `[level × envelope, width s]`.
/// The DR-110 bass drum mixes a low-passed trigger "whack"
/// (`research:instruments/analogue-drum-machines.md` §5.8); its recording steps down for 1.3 ms
/// at the strike and back up. Fitted in the 2026-09-19 A/B pass; a trigger-line capture replaces
/// it.
const fn whack(id: u8) -> [f32; 2] {
    match id {
        54 => [-0.3, 0.0013],
        _ => [0.0, 0.0],
    }
}

/// Dual-network balance: `[gain A, T60 ratio A, gain B, T60 ratio B]` for the poles at f1 and f2.
/// The kicks' recordings settle on one low pole (f1) with a faster upper network (f2) shaping only
/// the first cycles, fitted by least squares in the 2026-09-19 A/B pass; a separate capture of
/// each network replaces it. The unrecorded bell (73) and early cowbell (89) keep the former
/// balance.
const fn dual_modes(id: u8) -> [f32; 4] {
    match id {
        32 => [1.0, 1.0, 0.59, 0.50],
        47 => [1.0, 1.0, 0.79, 0.21],
        _ => [0.58, 1.0, 0.48, 0.8],
    }
}

/// Direct trigger-edge feed into a tonal voice's output, relative to the strike. A struck
/// transistor network passes a little of the edge as well as ringing; without it every rim/block
/// is a pure laboratory sine. The recorded rims' and claves' levels were chosen in the 2026-09-19
/// A/B pass against their recordings' first-sample jump behind the 8 kHz corner (`tone_cutoff`);
/// a capture of the trigger edge at each output replaces them.
const fn click_level(id: u8) -> f32 {
    match id {
        // The CR-8000 rimshot's recording puts a spike of the opposite sign, 1.7 times its ring's
        // first peak, on each strike.
        40 => -2.8,
        // The CR-8000 closed hat's recording has one spike twice its metal's peaks about 1 ms in:
        // the trigger's falling edge through the hat VCA's high-pass (`TRIGGER_WIDTH`).
        42 => 0.12,
        43 => 0.0,
        // The CR-78 snare's recording opens on a 0.8 ms positive edge pulse louder than its body.
        61 => 30.0,
        // The CR-78 hat and cymbal recordings open on a 0.5–3 kHz edge click, over their attack
        // noise (`attack_noise`).
        63 => 0.12,
        64 => 0.3,
        65 | 71 | 82..=84 | 92..=94 => 0.0,
        // The recorded rims and claves: a sharp edge that the 8 kHz corner passes.
        45 | 62 | 66 | 79 | 80 | 90 => 0.05,
        76..=78 | 88 => 0.16,
        _ => 0.025,
    }
}

/// Corner of the tonal voices' output low-pass (the output stage's roll-off), Hz, before Tone.
/// The rims and claves pass their edge and upper partials: their recordings keep energy to
/// 5–8 kHz that the drums' 2 kHz corner removed. Chosen in the 2026-09-19 A/B pass from the
/// recordings' third-octave spectra; a swept measurement of each output stage replaces it.
const fn tone_cutoff(id: u8) -> f32 {
    match id {
        40 => 12_000.0,
        // The phase-shift clave's recording keeps its harmonic series to 16 kHz
        // (`body_harmonics`).
        45 => 16_000.0,
        62 | 66 | 79 | 80 | 90 => 8_000.0,
        // The CR-78 low bongo's 2.51 kHz mode (`second_mode`) is 6 dB under its body early in
        // its recording: the voice's own, since it starts with the strike and decays with the
        // body, where bleed from the 2.63 kHz, 18 ms clave would not.
        68 => 6_000.0,
        // The CR-78 snare's edge pulse rises over about 0.4 ms in its recording.
        61 => 600.0,
        _ => 2_000.0,
    }
}

/// Snare body and wire balance: `[body level, wire level, wire T60 seconds]`. The body pole uses
/// `config().decay`. Fitted to the acquired comparison recordings' band-split envelopes (body band
/// around the rest pitch, wire band above 1.5 kHz) in the 2026-09-19 A/B pass; a separate capture
/// of each path replaces them.
const fn snare_mix(id: u8) -> [f32; 3] {
    match id {
        // The CR-8000 recording is almost pure 223 Hz body: the wire sits 33 dB down.
        33 => [1.0, 0.008, 0.030],
        48 => [0.30, 1.0, 0.160],
        55 => [0.90, 1.0, 0.150],
        61 => [1.05, 1.0, 0.050],
        81 => [0.90, 0.45, 0.045],
        // The FR-2L snare's recording opens on one large smooth body lobe before its wires.
        91 => [1.5, 1.0, 0.065],
        _ => [0.45, 1.0, 0.100],
    }
}

/// Noise/metal colour: `[high-pass Hz, high-pass Q, band-pass Hz, band-pass Q, low-pass Hz]`; a
/// zero frequency bypasses that stage. Noise and Clap rows take their band-pass centre from
/// `config().f1` and write 0 here. Every stage is a topology-preserving-transform state-variable
/// filter (Zavalishin, *The Art of VA Filter Design*, ch. 4), stable to Nyquist; the former single
/// Chamberlin
/// band-pass clamped every centre above about 7.3 kHz at 48 kHz, which darkened every hat. Fitted
/// by a grid search against each acquired comparison recording's third-octave body spectrum in the
/// 2026-09-19 A/B pass; a swept-sine measurement of each voice's filter replaces it. Unrecorded
/// rows take their recorded machine siblings' shape.
const fn colour(id: u8) -> [f32; 5] {
    match id {
        // Snare wires.
        33 => [1_500.0, 0.707, 0.0, 1.0, 0.0],
        48 => [800.0, 0.707, 3_500.0, 1.0, 0.0],
        55 => [0.0, 0.707, 3_500.0, 0.5, 0.0],
        61 => [0.0, 0.707, 4_500.0, 1.0, 0.0],
        81 => [2_000.0, 2.0, 10_000.0, 4.0, 0.0],
        91 => [2_000.0, 1.2, 5_500.0, 1.0, 0.0],
        // CR-8000 metal: a steep high-pass lifts the six squares' top octaves.
        41 => [5_800.0, 1.2, 8_500.0, 0.5, 0.0],
        42 => [6_000.0, 0.707, 12_000.0, 0.7, 0.0],
        43 => [6_000.0, 2.0, 12_000.0, 1.5, 0.0],
        // Cowbell pairs.
        46 => [0.0, 0.707, 1_000.0, 2.5, 0.0],
        // The CR-78 cowbell's recording keeps its second harmonics and little above 3 kHz.
        70 => [0.0, 0.707, 750.0, 1.2, 1_500.0],
        // TR-606 metal: the high band's resonant filter and the hats' high-pass
        // (`research:instruments/analogue-drum-machines.md` §5.7).
        // The cymbal takes the hats' high band (§5.7: it uses both, they use the high one) and
        // adds its own 3.44 kHz low band. The band sits at 7.2 kHz, not the hats' 6.85: the
        // recording's smoothed resonance is anchored at 7.20–7.23 kHz for its whole life, and a
        // band on the 6802 Hz line alone pulled ours down to 6.1–6.5 kHz through the body.
        51 => [4_500.0, 0.707, 7_300.0, 7.0, 0.0],
        // The 606's two hats share one audio path (§5.7), so the closed hat takes the open
        // hat's band (owner, 2026-09-20: "give the 606 closed hat the same as the open").
        52 | 53 => [4_500.0, 0.707, 7_250.0, 4.0, 0.0],
        // DR-110 mixed metal (§5.8).
        56 => [4_500.0, 0.707, 7_100.0, 5.0, 0.0],
        // The DR-110 hats keep a flat 0.2–1.6 kHz floor of mixed noise 30–36 dB under their top.
        57 | 58 => [3_000.0, 0.707, 7_100.0, 5.0, 0.0],
        // The guiro's recording keeps its scrape in 4–7 kHz, each pulse ringing on for about 6 ms
        // at 5.5 kHz: a high-pass removes the gate edges' 0.3–3 kHz spread, a Q 10 band rings.
        72 => [3_500.0, 0.707, 5_500.0, 10.0, 0.0],
        // Noise voices and claps: band-pass centre from `config().f1`.
        44 => [0.0, 0.707, 0.0, 1.5, 0.0],
        // The DR-110 clap's bursts carry little above 5 kHz in its recording.
        59 => [0.0, 0.707, 0.0, 2.0, 0.0],
        63 => [7_000.0, 1.2, 0.0, 5.0, 0.0],
        // The CR-78 cymbal's recording fills 2–5 kHz more than the hat's (owner: "a more full
        // spectrum"); a wider band.
        // The recording carries noise through 1.6–3.2 kHz under the whole hit: a 7 kHz corner
        // cut the cymbal's own body, leaving that octave 11–16 dB light in every window while
        // 6.4 kHz and up matched (owner, 2026-09-20: "original sounds like it has a bit more low
        // band noise"). The corner is the voice's body, not a colour: resonant at 7 kHz it also
        // stood a bump where the recording is flat.
        64 => [4_000.0, 0.707, 0.0, 3.5, 0.0],
        65 => [7_000.0, 1.2, 0.0, 4.0, 0.0],
        71 => [3_400.0, 1.8, 0.0, 9.0, 0.0],
        // The TR-66 hat's recording carries 1.6–5 kHz a few dB over the model's narrower band
        // (owner: "a little more bottom").
        82 => [6_000.0, 0.707, 0.0, 10.0, 0.0],
        83 => [8_000.0, 1.2, 0.0, 4.0, 0.0],
        84 => [6_000.0, 1.2, 0.0, 4.0, 0.0],
        92..=94 => [2_000.0, 0.707, 0.0, 4.0, 0.0],
        _ => [0.0, 0.707, 4_000.0, 2.0, 0.0],
    }
}

/// Share of a metal or clap voice's unfiltered source summed with its colour. The CR-8000 cowbell's
/// recording keeps its upper harmonics falling at about 6 dB per octave from 3 to 16 kHz, the
/// squares' own slope, 30–45 dB under the band-passed body; 8 % of the raw pair gives that. Fitted
/// to the recording's line levels in the 2026-09-19 review pass; a swept measurement of the
/// cowbell's output network replaces it.
const fn colour_direct(id: u8) -> f32 {
    match id {
        // The CR-8000 clap's recording stays within 12 dB of its 1.3 kHz peak up to 20 kHz: white
        // noise at 8 % beside the band-pass.
        44 => 0.08,
        46 => 0.08,
        59 => 0.012,
        _ => 0.0,
    }
}

/// The DR-110 mixes its LFSR noise into the four squares before the metal filters
/// (`research:instruments/analogue-drum-machines.md` §5.8):
/// `[level beside the square sum, that noise's low-pass Hz, level of a high-passed top beside the
/// band, that top's order in poles]`.
/// Fitted to the acquired comparison recordings' low-band floor in the 2026-09-19 A/B pass; the
/// top's order was fitted on 2026-09-20. A band an octave under the 6 kHz corner needs two poles:
/// the TR-606 hats' one-pole skirt filled their own band, +11…+14 dB from 0.8 to 2.5 kHz, while
/// the DR-110's higher band clears it.
const fn metal_noise(id: u8) -> [f32; 4] {
    match id {
        41 => [0.15, 9_000.0, 0.0, 1.0],
        // The cymbal carries the top too: its Q 4 band alone left the recording's 10–16 kHz
        // 5–18 dB short. The two hats were approved without it (owner, 2026-09-20) and keep
        // their merged sound.
        51 => [0.12, 5_500.0, 0.05, 2.0],
        52 | 53 => [0.15, 4_000.0, 0.05, 2.0],
        56 => [0.03, 0.0, 0.06, 1.0],
        57 | 58 => [0.4, 0.0, 0.08, 1.0],
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// How a voice's resonance moves as its own envelope falls: `[centre x at the strike, centre x
/// when it has gone, Q x at the strike, Q x when it has gone, curve of the fall]`, applied to the
/// voice's band-pass colour and to its `noise_resonance`, recomputed every 64 samples (`drift`).
/// A sixth entry gives the drift its own time constant in seconds, for a voice whose envelope
/// holds too flat to drive it; zero rides the envelope's fall.
/// Measured on the acquired comparison recordings on 2026-09-20, model against recording, over
/// 0–10 / 10–30 / 30–60 / 60–120 / 120–250 / 250–500 ms windows: the recordings' resonance climbs
/// through the tail (the CR-78 cymbal's 7.6 kHz at the strike to 8.7 kHz at 250–500 ms) and holds
/// its width, where one fixed band sits flat and broadens. The owner hears the difference as
/// "the original changes resonance over time... the model does not".
const fn resonance_drift(id: u8) -> [f32; 6] {
    match id {
        // CR-78 cymbal: the strike sits 330 Hz under the body and the tail 760 Hz over it.
        64 => [0.95, 1.09, 1.0, 2.0, 0.3, 0.0],
        // CR-78 tambourine: the recording's strike is a definite whistle, not a band. Owner-
        // approved on 2026-09-20 as it stands, so this row is frozen. A later measurement wanted
        // its tail 5 % higher — `[0.98, 1.03, 1.5, 1.0, 4.0, 0.03]` with an `attack_noise` row of
        // `[0.02, 0.0, 8_000.0, 16_000.0]` fits it — but the owner had already judged the sound.
        71 => [0.97, 1.05, 1.5, 1.0, 1.3, 0.0],
        // TR-606 hats: both recordings strike narrow (Q 10.4 and 7.9) and settle to Q 2.6–6.3,
        // and they share one path, so they share one row.
        52 | 53 => [1.0, 1.0, 1.8, 1.0, 0.6, 0.0],
        // TR-66 hat: its width holds through the tail where ours collapses.
        82 => [0.99, 1.02, 1.0, 1.25, 0.5, 0.0],
        _ => [1.0, 1.0, 1.0, 1.0, 1.0, 0.0],
    }
}

/// A voice's own measured amplitude modulation: `[rate Hz, depth]`. The TR-606 cymbal's recording
/// carries a 24.5 Hz ripple on its 4–10 kHz envelope, 4.5 % of the mean at 20–150 ms and 9 % from
/// 150 ms on, coherent across the whole tail where the model's envelope has only the incoherent
/// 130–270 Hz beating of its own bank (measured 2026-09-20 on the acquired recording's envelope,
/// third-order detrended, 100–500 ms). The owner hears it as "LF modulation going on on the
/// harmonics".
const fn body_modulation(id: u8) -> [f32; 2] {
    match id {
        51 => [24.5, 0.08],
        _ => [0.0, 0.0],
    }
}

/// A metal voice's output resonance: `[Hz, Q, gain]`, a band-pass added to its own signal. The
/// CR-8000 cymbal's recording leads on its 5746 Hz line and falls away upward, 3 dB to its
/// neighbours and 7 dB by 8 kHz, where the model's band led at 8044 Hz (owner: "different ring,
/// original sounds higher"; the owner's higher and lower are the perceived tone, so what matters
/// is which line leads). The TR-606 cymbal's 6.80 kHz line likewise leads each audible window,
/// while the model's same line sat under its 7.2 and 7.75 kHz neighbours. Fitted in the 2026-09-20
/// tone rounds to those recordings' line tables; a swept measurement of either output network
/// replaces its row.
const fn metal_peak(id: u8) -> [f32; 3] {
    match id {
        41 => [5_750.0, 8.0, 0.3],
        // The TR-606 cymbal recording's 6.80 kHz line leads every audible window. The model has
        // the same bank line, but its 7.2 and 7.75 kHz neighbours lead through the first 60 ms.
        51 => [6_800.0, 30.0, 1.0],
        _ => [0.0, 1.0, 0.0],
    }
}

/// A metal voice's noise floor beside its band: `[level re the trigger, T60 s, low-pass Hz]`, a
/// zero T60 riding the voice's envelope and a zero corner leaving the noise white. The TR-606
/// hats' recordings carry 200–800 Hz for their first 20 ms, falling faster than their own band;
/// the DR-110 hats' keep a flat floor under theirs. Fitted in the 2026-09-19 and 2026-09-20
/// owner-notes rounds to those recordings' octave levels over time; a capture of each voice's
/// pre-filter node replaces them.
const fn metal_floor(id: u8) -> [f32; 3] {
    match id {
        51 => [0.010, 0.0, 2_500.0],
        52 => [0.025, 0.05, 500.0],
        53 => [0.025, 0.45, 500.0],
        57 | 58 => [0.03, 0.0, 0.0],
        _ => [0.0, 0.0, 0.0],
    }
}

/// Gated-VCA shape for the noise, metal, clap and wire paths, and the hold of a gated oscillator
/// (a Resonator row's pole sustains at `hold_t60` while held).
#[derive(Debug, Clone, Copy)]
struct Shape {
    /// One-pole attack time constant of the VCA control, seconds; 0 is instant.
    attack: f32,
    /// Seconds the trigger gate holds the envelope before its release T60 (`config().decay`).
    hold: f32,
    /// T60 while the gate holds, seconds; 0 holds flat.
    hold_t60: f32,
    /// Share of a second decay stage, summed with the main envelope and not held by the gate:
    /// a cymbal's or cowbell's fast first fall, or a clap's slow floor.
    stage_share: f32,
    /// T60 of that stage, seconds.
    stage_t60: f32,
}

/// The recordings' metal and noise voices rise over 1–15 ms through the VCA's attack and the
/// filters' build-up rather than starting at their peak; several older machines hold a gate and
/// then release (the CR-78, TR-66 and FR-2L voices), and the cymbals and cowbells fall in two
/// stages. Fitted to each acquired comparison recording's 1 ms and 10 ms RMS envelopes in the
/// 2026-09-19 A/B pass; an envelope-probe capture of the hardware replaces each. Unrecorded rows
/// take their recorded machine siblings' shape.
const fn shape(id: u8) -> Shape {
    const fn s(attack: f32, hold: f32, hold_t60: f32, stage_share: f32, stage_t60: f32) -> Shape {
        Shape {
            attack,
            hold,
            hold_t60,
            stage_share,
            stage_t60,
        }
    }
    match id {
        41 => s(0.004, 0.0, 0.0, 0.75, 0.35),
        42 => s(0.003, 0.025, 0.4, 0.0, 0.0),
        43 => s(0.0035, 0.25, 1.0, 0.0, 0.0),
        44 => s(0.004, 0.075, 0.4, 0.06, 1.0),
        // The RC phase-shift clave oscillates while gated, then decays (§5.6).
        // The rimshot's strike train (`strike_train`) fades on the second stage.
        40 => s(0.0, 0.0, 0.0, 0.0, 0.07),
        45 => s(0.0, 0.0069, 0.86, 0.0, 0.0),
        46 => s(0.001, 0.0, 0.0, 0.92, 0.060),
        // The TR-606 cymbal holds, then releases after about a second.
        51 => s(0.001, 1.0, 2.4, 0.60, 0.12),
        52 => s(0.0009, 0.0, 0.0, 0.0, 0.0),
        53 => s(0.0003, 1.15, 4.5, 0.0, 0.0),
        56 => s(0.0002, 0.0, 0.0, 0.75, 0.10),
        57 => s(0.0002, 0.0, 0.0, 0.0, 0.0),
        // The DR-110 open hat's recording holds, then stops at about 830 ms: a gate.
        58 => s(0.0002, 0.82, 2.0, 0.0, 0.0),
        59 => s(0.001, 0.0, 0.0, 0.8, 0.12),
        61 => s(0.0005, 0.055, 0.5, 0.0, 0.0),
        63 => s(0.0002, 0.0175, 0.12, 0.0, 0.0),
        64 => s(0.0002, 0.29, 2.0, 0.0, 0.0),
        65 => s(0.0005, 0.0, 0.0, 0.0, 0.0),
        70 => s(0.0005, 0.052, 0.9, 0.0, 0.0),
        71 => s(0.0005, 0.2, 2.5, 0.0, 0.0),
        72 => s(0.0005, 0.82, 8.0, 0.0, 0.0),
        81 => s(0.001, 0.055, 0.0, 0.0, 0.0),
        82 => s(0.003, 0.045, 0.4, 0.0, 0.0),
        83 | 84 => s(0.002, 0.035, 0.0, 0.0, 0.0),
        91 => s(0.0015, 0.045, 0.0, 0.0, 0.0),
        // The FR-2L cymbal's recording is within 5 dB of its peak in its first millisecond
        // (owner: "slightly more attack").
        92 => s(0.0008, 0.0, 0.0, 0.4, 0.2),
        93 | 94 => s(0.003, 0.0, 0.0, 0.4, 0.2),
        // The CR-8000 snare's wires hold level for 50 ms, then close within about 15 ms.
        33 => s(0.001, 0.050, 0.4, 0.0, 0.0),
        48 => s(0.001, 0.0, 0.0, 0.0, 0.0),
        // The DR-110 snare's recording falls fast, then keeps a faint 0.4 s floor of its wires.
        55 => s(0.001, 0.0, 0.0, 0.07, 0.4),
        _ => s(0.0, 0.0, 0.0, 0.0, 0.0),
    }
}

/// The trigger edge a metal voice passes at its attack: `[level re the trigger, one-pole corner
/// Hz]`. The TR-606 hats' recordings open with a click 9 dB over the model's below 400 Hz in their
/// first 5 ms (owner: "the attack is more pronounced on the original"), and rise into it rather
/// than starting at it. Fitted in the 2026-09-20 owner-notes round; a trigger-line capture
/// replaces it.
const fn metal_edge(id: u8) -> [f32; 2] {
    match id {
        // The shared path's click, at each hat's own level: the closed hat's band is short, so
        // the same click stood a third of its peak.
        52 => [0.12, 700.0],
        53 => [0.25, 700.0],
        _ => [0.0, 1_000.0],
    }
}

/// A metal voice that darkens as it falls: `[one-pole corner Hz at the trigger's level,
/// exponent, make-up exponent]`, the corner at `full × (envelope / level)^exponent` and the
/// band's loss made up by `(full / corner)^make-up`. The CR-8000 cymbal's recording
/// keeps its spectral peak near 6.1 kHz while its centroid falls from 8.3 to 5.9 kHz over its
/// first 400 ms; one fixed band cannot be both (owner: "slightly higher pitched. Perhaps just more
/// HF noise"). Fitted in the 2026-09-19 owner-notes round to that recording's centroid per window;
/// a capture of the cymbal's output stage replaces it.
const fn metal_darkening(id: u8) -> [f32; 3] {
    match id {
        41 => [20_000.0, 0.5, 0.35],
        _ => [0.0, 0.0, 0.0],
    }
}

/// A metal voice's rectifying VCA: `[threshold, high-pass Hz]`. The CR-8000 hats' recordings are
/// sparse negative spikes (their envelope's coefficient of variation 0.33–0.38) whose spectra carry
/// even harmonics and sum lines a linear path lacks: the VCA conducts on one polarity of the band
/// only, above a threshold of half the band's RMS (0.031 and 0.035 through the two hats' colours at
/// reference), and a second-order 6 kHz high-pass after it removes the envelope it leaks
/// (`research:instruments/analogue-drum-machines.md` §2.6 on swing VCAs). The cymbal, one of the
/// service note's exceptions (§5.6), stays linear. Fitted in the 2026-09-19 review pass to the
/// recordings' third-octave spectra, line overlap and envelope statistics; a transfer measurement
/// of the hat VCA replaces it.
/// The CR-8000 hats' output network after their rectifying VCA: `[one-pole high-pass Hz, peak Hz,
/// peak Q, peak gain]`.
const fn hat_output(id: u8) -> [f32; 4] {
    match id {
        42 | 43 => [0.0, 6_200.0, 3.0, 1.0],
        _ => [0.0, 1.0, 1.0, 0.0],
    }
}

const fn rectifying_vca(id: u8) -> [f32; 2] {
    match id {
        42 => [0.031, 6_000.0],
        43 => [0.012, 6_000.0],
        _ => [0.0, 0.0],
    }
}

/// A narrow resonance beside a noise voice's own band: `[Hz, Q, level, rise s]`, under the voice's
/// own envelope and opening over its own time, so it emerges as the wash falls rather than
/// crowding the attack. The CR-78 tambourine's
/// recording carries a line near 4.7 kHz that stands 23–33 dB over the noise beside it and outlasts
/// the wash — the owner's "clear whistle" — where the model's own band left it 10–15 dB. Fitted in
/// the 2026-09-19 owner-notes pass to that recording's 100–260 ms spectra; a capture of the
/// tambourine's resonant network replaces it.
const fn noise_resonance(id: u8) -> [f32; 4] {
    match id {
        // The CR-78 cymbal's recording rings at 8.87 kHz, its loudest line, 11–13 dB over the
        // noise beside it; the tambourine's whistles at 4.7 kHz from its first millisecond.
        64 => [8_420.0, 45.0, 1.6, 0.0],
        71 => [4_660.0, 28.0, 3.5, 0.0],
        // The FR-2L cymbal's recording rings at 7.95 kHz beside its 11.1 kHz band.
        92 => [7_950.0, 30.0, 1.1, 0.0],
        _ => [0.0, 1.0, 0.0, 0.01],
    }
}

/// A burst of the machine's noise at a noise voice's attack, before the voice's own colour:
/// `[level re the trigger, T60 s, high-pass Hz, low-pass Hz]`; a zero T60 rides the voice's own
/// envelope instead of fading, which is a floor under the whole hit rather than a burst. The CR-78 hat's and cymbal's recordings open on
/// noise reaching down to 200 Hz, 20–45 dB over the band-passed voice below 2 kHz for their first
/// 3–10 ms and gone by 20 ms: the attack the owner hears ("more low frequency noise"), and a
/// transient of the voice, not the chain's floor, since it falls with the hit. Fitted in the
/// 2026-09-19 owner-notes pass to those recordings' third-octave levels over their first 20 ms; a
/// capture of the voice's pre-filter node replaces it.
const fn attack_noise(id: u8) -> [f32; 4] {
    match id {
        63 => [0.16, 0.015, 150.0, 800.0],
        // The TR-66 hat's recording carries 200–800 Hz through the whole hit, 20–60 dB over the
        // band-passed voice, from its first millisecond and falling with it (owner: "lacking some
        // low end noise").
        82 => [0.0007, 0.0, 120.0, 900.0],
        92 => [0.008, 0.0, 150.0, 800.0],
        64 => [0.14, 0.02, 150.0, 800.0],
        _ => [0.0, 0.1, 100.0, 1_000.0],
    }
}

/// Transistor VCAs leak their control voltage into the audio as a slow thump
/// (`research:instruments/analogue-drum-machines.md` §2.6 names the wart for swing-type VCAs):
/// `[level, smoothing Hz, coupling high-pass Hz]`. The FR-2L snare's and cymbal's recordings swing
/// by a tenth of their peak; the TR-606 cymbal's and hats' recordings, sharing one metal VCA
/// (§5.7), dip and recover by 2–3 % of their peak within 50 ms. Fitted to each acquired comparison
/// recording's content below 150 Hz in the 2026-09-19 A/B pass; a DC-coupled hardware capture
/// replaces it.
const fn feedthrough(id: u8) -> [f32; 3] {
    match id {
        51 => [0.019, 400.0, 22.0],
        53 => [0.021, 400.0, 22.0],
        52 => [0.006, 400.0, 30.0],
        91 => [1.6, 15.0, 1.9],
        // The FR-2L cymbal's recording swells by a tenth of its peak over 20 ms and undershoots.
        92 => [-0.6, 5.0, 5.0],
        _ => [0.0, 0.0, 0.0],
    }
}

/// A VCA's conduction threshold: `[threshold, width]` relative to the trigger's level. When the
/// envelope capacitor falls below the amplifier's conduction point the decay steepens and stops
/// (`docs/drum-model-fitting.md` §6). The CR-8000 clap's recording ends at about −55 dB 340 ms
/// in, where an exponential tail runs on another 100 ms. Fitted to that recording's dB curve in the
/// 2026-09-19 review pass; a measurement of the clap VCA's threshold replaces it.
const fn vca_threshold(id: u8) -> [f32; 2] {
    match id {
        44 => [0.01, 0.005],
        // The TR-66-family hat holds its measured level through 60 ms, then its VCA closes around
        // 80–95 ms; shortening the exponential alone makes the 30–60 ms body too quiet.
        82 => [0.09, 0.05],
        _ => [0.0, 0.0],
    }
}

/// Clap voicing: `[burst attack ms, tail low-pass Hz, tail T60 s, slow share, slow T60 s]`, 0 Hz
/// for no low-pass and 0 s for a tail that rides the voice envelope from the trigger; a tail with
/// its own time adds a slower share under it. Each burst rises over a fraction of
/// a millisecond rather than stepping; the DR-110's room tail is darker than its bursts. The
/// CR-8000 clap's tail keeps the bursts' full colour: its recording stays within 12 dB of the
/// 1.3 kHz peak to 20 kHz through the tail (review pass). The DR-110's recording opens its tail as
/// loud as its bursts and lets it fall about 17 dB over the next 65 ms, then about 7 dB per
/// 150 ms from 30 dB down, so its tail decays from its own opening in two slopes; its bursts keep white noise to 16 kHz (`colour_direct`; owner: "the original
/// has more high pitched noise"). Fitted in the 2026-09-19 A/B, review and owner-notes passes; a
/// capture of each path replaces them.
const fn clap_voicing(id: u8) -> [f32; 5] {
    match id {
        44 => [0.3, 0.0, 0.0, 0.0, 1.0],
        59 => [1.0, 3_000.0, 0.28, 0.05, 1.4],
        _ => [0.3, 8_000.0, 0.0, 0.0, 1.0],
    }
}

/// Clap bursts: `[count, spacing ms, burst T60 ms, tail level, edge click, tail opens ms, tail
/// rise ms]`. The tail is the `shape` envelope under `config().decay`. The CR-8000
/// sawtooth-modulates its noise and passes the trigger edge (§5.6), and its tail opens with the
/// last of four bursts; the DR-110's CPU times three bursts (§5.8), and its recording's tail opens
/// one spacing after the last, rising over about 2 ms to 3 dB under the bursts — the tail, not a
/// fourth burst, is what the first pass made loudest. Fitted to the acquired comparison
/// recordings' 1 ms envelopes in the 2026-09-19 A/B and review passes; a trigger-line capture
/// replaces them.
const fn clap_bursts(id: u8) -> [f32; 7] {
    match id {
        44 => [4.0, 9.5, 40.0, 2.0, 0.8, 28.5, 0.0],
        59 => [3.0, 11.0, 16.0, 0.6, 0.0, 28.0, 4.0],
        _ => [4.0, 14.0, 41.0, 1.0, 0.0, 42.0, 0.0],
    }
}

/// Guiro scrape: `[seconds at the slow rate, pulse ms]`. The CR-78's long guiro scrapes at the
/// slow pulse rate (`config().f2`, 77 Hz) and then the fast one (`config().f1`, 125 Hz) until its
/// gate closes (`research:instruments/analogue-drum-machines.md` §5.4); each pulse gates the noise
/// into a resonant band-pass. Fitted to the acquired comparison recording's pulse times in the
/// 2026-09-19 A/B pass; a trigger-line capture replaces them.
const fn guiro_timing(id: u8) -> [f32; 2] {
    match id {
        72 => [0.545, 3.0],
        _ => [0.5, 1.0],
    }
}

const fn cowbell_pair_indices(id: u8) -> [usize; 2] {
    match id {
        46 => [6, 7], // the CR-8000 cowbell's own 571/838 Hz pair in the shared frame
        70 => [0, 1], // 545/775 Hz in the shared CR-78 frame
        _ => [0, 1],
    }
}

/// Mix weights of a cowbell's two oscillators. The CR-8000 recording keeps its 571 Hz partial
/// 14 dB under the 838 Hz one, and fitting all thirteen of its lines through the 1 kHz, Q 2.5
/// band-pass puts the lower oscillator at 0.45, which also makes 838 Hz the rest pitch by the catalogue's
/// lowest-partial-within-12-dB rule, as it is in the recording. Fitted in the 2026-09-19 A/B pass;
/// a measurement of the summing resistors replaces it.
const fn cowbell_pair_weights(id: u8) -> [f32; 2] {
    match id {
        46 => [0.45, 1.0],
        _ => [1.0, 1.0],
    }
}

/// Mix weights of a cowbell's two oscillators in its direct path (`colour_direct`). The CR-8000
/// recording's 571 Hz odd harmonics from the 7th to the 17th fall at the square's own 1/n from its
/// fundamental, 6–8 dB above what the band-pass path's 0.45 weight leaves them; its 838 Hz series
/// already matched, so the direct path takes both squares whole (2026-09-19 owner-notes pass).
const fn cowbell_direct_weights(id: u8) -> [f32; 2] {
    match id {
        46 => [1.0, 1.0],
        _ => cowbell_pair_weights(id),
    }
}

fn cowbell_pair(id: u8, source: Frame) -> f32 {
    let pair = cowbell_pair_indices(id);
    let weight = cowbell_pair_weights(id);
    0.5 * (weight[0] * source.oscillators[pair[0]] + weight[1] * source.oscillators[pair[1]])
}

/// Share of each period a machine's oscillator `index` spends high. The Schmitt banks run near
/// 48 % (the former universal value). The CR-78 cowbell's recording carries second harmonics only
/// 11–16 dB down, which a 40 % duty gives (`|sin 2πd| / 2|sin πd|`). The CR-8000 cowbell's
/// recording's thirteen lines fit a 46 % duty for its lower oscillator (second harmonic 1142 Hz
/// 12 dB under it) and 49 % for its upper one (1676 Hz 36 dB under). Fitted in the 2026-09-19 A/B and review
/// passes; an oscilloscope capture of the oscillators replaces them.
const fn metal_duty(machine: usize, index: usize) -> f32 {
    match (machine, index) {
        (3, _) => 0.40,
        (0, 6) => 0.47,
        (0, 7) => 0.49,
        _ => 0.48,
    }
}

/// Topology-preserving-transform state-variable filter (Zavalishin, *The Art of VA Filter
/// Design*, ch. 4). f64 state; coefficients recomputed only when the tuning changes.
#[derive(Debug, Clone, Copy)]
struct Svf {
    s1: f64,
    s2: f64,
    key: [f32; 3],
    g: f64,
    k: f64,
    h: f64,
}
impl Svf {
    const fn new() -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            key: [0.0; 3],
            g: 0.0,
            k: 0.0,
            h: 0.0,
        }
    }
    fn tune(&mut self, fs: f32, hz: f32, q: f32) {
        if self.key == [fs, hz, q] {
            return;
        }
        self.key = [fs, hz, q];
        let fs = f64::from(fs);
        let hz = f64::from(finite(hz, 1_000.0)).clamp(10.0, 0.45 * fs);
        self.g = (std::f64::consts::PI * hz / fs).tan();
        self.k = 1.0 / f64::from(finite(q, 0.707)).max(0.1);
        self.h = 1.0 / (1.0 + self.k * self.g + self.g * self.g);
    }
    /// Returns `(high-pass, unity-peak band-pass, low-pass)`.
    fn process(&mut self, x: f32) -> (f32, f32, f32) {
        let x = f64::from(x);
        let hp = (x - (self.k + self.g) * self.s1 - self.s2) * self.h;
        let v1 = self.g * hp;
        let bp = v1 + self.s1;
        self.s1 = bp + v1;
        let v2 = self.g * bp;
        let lp = v2 + self.s2;
        self.s2 = lp + v2;
        if !(self.s1.is_finite() && self.s2.is_finite()) {
            self.reset();
            return (0.0, 0.0, 0.0);
        }
        self.s1 = flush(self.s1);
        self.s2 = flush(self.s2);
        (hp as f32, (self.k * bp) as f32, lp as f32)
    }
    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    fs: f32,
    modes: [Resonator; 3],
    /// Each mode's previous sine-phase output, for its quadrature (cosine-phase) output.
    previous: [f32; 3],
    strike: f32,
    /// The latest trigger's accent level, held for its falling edge.
    level: f32,
    /// Sample of the next strike in a voice's strike train (`strike_train`).
    next_strike: u32,
    /// Charge left in the RC-shaped trigger pulse.
    pulse: f32,
    env: f32,
    stage_env: f32,
    pitch_env: f32,
    noise_env: f32,
    /// Attack-smoothed VCA control of the noise/metal/wire/clap paths.
    amp: f32,
    /// A two-band metal voice's lower band: its envelope and attack-smoothed VCA control.
    low_env: f32,
    amp_low: f32,
    /// High-pass, band-pass and low-pass colour stages, then the lower metal band's band-pass and
    /// high-pass.
    filters: [Svf; 5],
    metal_peak_filter: Svf,
    /// One-pole coefficients at fixed corners. They follow the sample rate alone, so they are held
    /// here rather than rebuilt with an `exp()` on every rendered sample.
    pole_600: f32,
    pole_1000: f32,
    pole_6000: f32,
    decay_3000: f32,
    /// The strike RC's charge share, with the model and rate it was built for.
    strike_share: f32,
    strike_share_for: (u8, f32),
    /// Each mode's last configured tuning, so an unchanged pole is not rebuilt every sample.
    /// `NaN` forces a rebuild, which is how reset and a rate change invalidate it.
    mode_tuning: [(f32, f32); 3],
    /// The `pole_terms` of that held tuning, which `ring_and_amplitude` needs for its quadrature
    /// output. Rebuilt with the tuning, never per sample.
    mode_pole: [(f64, f64); 3],
    /// One-pole coefficient at 400 Hz, a fixed corner.
    pole_400: f32,
    /// The feedthrough pair's smoothing and coupling coefficients, with the model and rate they
    /// were built for.
    thump_poles: (f32, f32),
    thump_poles_for: (u8, f32),
    /// Where this voice reads its metallic bank: 0 the machine-shared one, 1 its own, and in
    /// between a bounded crossfade. `metal_ratio` holds the tuning the private bank is running at
    /// so an outgoing deviation fades out as itself.
    metal_blend: f32,
    metal_ratio: f32,
    /// One-pole state, by topology: a tom loop's pink noise, a clap's darker tail, a noise voice's
    /// click band.
    aux_lp: f32,
    /// One-pole state of the high-passed noise beside a metal band (`metal_noise`).
    top_hp: f32,
    /// Block-rate resonance drift (`resonance_drift`): the loudest envelope this hit has reached,
    /// and the centre and Q multipliers that its fall from that peak has produced.
    drift_peak: f32,
    drift_hz: f32,
    drift_q: f32,
    /// Phase of a voice's own measured amplitude modulation (`body_modulation`).
    mod_phase: f32,
    thump: [f32; 3],
    /// Output-stage switch: which side of its threshold the ring is on, and its decaying spike.
    switched: bool,
    spike: f32,
    /// Low-passed even-order term, subtracted to couple it through a high-pass.
    even_lp: f32,
    tone_lp: f32,
    phase: f32,
    /// The slot's private squares, used only away from reference pitch.
    metal_squares: [BlepSquare; OSCILLATORS],
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
            fs: 48000.,
            modes: [Resonator::new(); 3],
            previous: [0.0; 3],
            strike: 0.,
            level: 0.,
            next_strike: 0,
            pulse: 0.,
            env: 0.,
            stage_env: 0.,
            pitch_env: 0.,
            noise_env: 0.,
            amp: 0.,
            low_env: 0.,
            amp_low: 0.,
            filters: [Svf::new(); 5],
            metal_peak_filter: Svf::new(),
            pole_600: one_pole(48000., 600.0),
            pole_1000: one_pole(48000., 1_000.0),
            pole_6000: one_pole(48000., 6_000.0),
            decay_3000: decay_pole(48000., 3_000.0),
            strike_share: 0.0,
            strike_share_for: (0, 0.0),
            mode_tuning: [(f32::NAN, f32::NAN); 3],
            mode_pole: [(0.0, 1.0); 3],
            pole_400: one_pole(48000., 400.0),
            thump_poles: (0.0, 0.0),
            thump_poles_for: (0, 0.0),
            metal_blend: 0.0,
            metal_ratio: 1.0,
            aux_lp: 0.,
            top_hp: 0.,
            drift_peak: 0.,
            drift_hz: 1.,
            drift_q: 1.,
            mod_phase: 0.,
            thump: [0.0; 3],
            switched: false,
            spike: 0.,
            even_lp: 0.,
            tone_lp: 0.,
            phase: 0.,
            metal_squares: start_bank(),
            active: false,
            age: 0,
        }
    }
    pub fn set_sample_rate(&mut self, v: f32) {
        self.fs = finite(v, 48000.).clamp(1000., 384000.);
        self.pole_600 = one_pole(self.fs, 600.0);
        self.pole_1000 = one_pole(self.fs, 1_000.0);
        self.pole_400 = one_pole(self.fs, 400.0);
        self.pole_6000 = one_pole(self.fs, 6_000.0);
        self.decay_3000 = decay_pole(self.fs, 3_000.0);
        // Both held tunings were built against the old rate.
        self.strike_share_for = (0, 0.0);
        self.thump_poles_for = (0, 0.0);
        self.mode_tuning = [(f32::NAN, f32::NAN); 3];
    }
    pub fn trigger(&mut self, velocity: f32, p: SlotPatch) {
        // Linear in velocity, Dynamics bending the curve (`crate::velocity`).
        let x = crate::velocity::linear(velocity, p.dynamics);
        self.strike = (self.strike + x).min(2.);
        self.level = x;
        self.env = (self.env + x).min(2.);
        self.stage_env = (self.stage_env + x).min(2.);
        self.pitch_env = (self.pitch_env + x).min(2.);
        self.noise_env = (self.noise_env + x).min(2.);
        self.low_env = (self.low_env + x).min(2.);
        // A new hit restarts the resonance's drift and the modulation's phase: both are properties
        // of one strike's own fall, not of the slot.
        self.drift_peak = 0.;
        self.drift_hz = 1.;
        self.drift_q = 1.;
        self.mod_phase = 0.;
        self.age = 0;
        self.active = true
    }
    #[must_use]
    pub fn process(&mut self, id: u8, p: SlotPatch, source: Frame, tempo_bpm: f32) -> f32 {
        if !self.active {
            return 0.;
        }
        let c = config(id);
        let capabilities = ModelId::new(id).capabilities();
        self.drift(id);
        let body = if capabilities.body {
            bipolar(p.body)
        } else {
            0.0
        };
        let noise_amount = if capabilities.noise {
            bipolar(p.noise)
        } else {
            0.0
        };
        let character = if capabilities.character {
            bipolar(p.character)
        } else {
            0.0
        };
        let ratio = 2f32.powf(finite(p.pitch_semitones, 0.).clamp(-24., 24.) / 12.);
        let pitch_depth = if capabilities.pitch_envelope {
            bipolar(p.pitch_envelope)
        } else {
            0.0
        };
        let [sweep_depth, sweep_t60] = native_sweep(id);
        let pitch_mod = if sweep_depth != 0.0 {
            1.0 + sweep_depth * envelope_depth(pitch_depth) * self.pitch_env
        } else if capabilities.pitch_envelope {
            2.0_f32.powf(18.0 * pitch_depth * self.pitch_env / 12.0)
        } else {
            1.0
        };
        let tempo_scale = if id == 53 {
            (120.0 / finite(tempo_bpm, 120.0).clamp(30.0, 300.0)).clamp(0.5, 2.0)
        } else {
            1.0
        };
        let ds = extended_decay_scale(bipolar(p.decay)) * tempo_scale;
        let noise_time = if capabilities.noise_decay {
            envelope_time_scale(bipolar(p.noise_decay))
        } else {
            1.0
        };
        // The trigger arrives as one charge and leaves through the RC pulse shaper; its later
        // strikes (`restrikes`) add their charge at their own samples.
        self.pulse += self.strike;
        self.strike = 0.;
        for [at_ms, charge] in restrikes(id) {
            if charge != 0.0 && self.age == (at_ms * 0.001 * self.fs) as u32 {
                self.pulse += charge * self.level;
            }
        }
        let [
            train_period,
            train_jitter,
            train_charge,
            train_start,
            train_end,
        ] = strike_train(id);
        if train_period > 0.0 && self.age < (train_end * 0.001 * self.fs) as u32 {
            if self.age == (train_start * 0.001 * self.fs) as u32 {
                self.next_strike = self.age;
            }
            if self.age >= (train_start * 0.001 * self.fs) as u32 && self.age == self.next_strike {
                // The train fades on the second stage's reference time, which Decay does not
                // stretch either.
                let fade = (-6.9077554 * self.age as f32
                    / (shape(id).stage_t60.max(0.001) * self.fs))
                    .exp();
                self.pulse += train_charge * self.level * fade * source.noise.signum();
                let interval = train_period * 0.001 * self.fs * (1.0 + train_jitter * source.noise);
                self.next_strike = self.age + interval.max(1.0) as u32;
            }
        }
        let strike_time = strike_rc(id);
        let strike = if strike_time > 0.0 {
            let share = {
                let key = (id, self.fs);
                if self.strike_share_for != key {
                    self.strike_share = 1.0 - (-1.0 / (strike_time * self.fs)).exp();
                    self.strike_share_for = key;
                }
                self.strike_share
            };
            let out = self.pulse * share;
            self.pulse -= out;
            out
        } else {
            std::mem::take(&mut self.pulse)
        };
        self.pulse = if self.pulse.abs() < 1.0e-12 {
            0.0
        } else {
            self.pulse
        };
        let tone = bipolar(p.tone);
        let shape = shape(id);
        let holding = (self.age as f32) < shape.hold * ds * self.fs;
        let reference_time = c.decay.max(0.001) * ds;
        let phase = if matches!(
            c.topology,
            Topology::Metal | Topology::Noise | Topology::Clap | Topology::Guiro
        ) {
            [1.0, 0.0]
        } else {
            let radians = strike_phase(id).to_radians();
            [radians.cos(), radians.sin()]
        };
        let colour = match c.topology {
            Topology::Noise | Topology::Clap => {
                let [high_pass, high_q, _, band_q, low_pass] = colour(id);
                [high_pass, high_q, c.f1, band_q, low_pass]
            }
            _ => colour(id),
        };
        let mut out = match c.topology {
            Topology::Resonator | Topology::Diode => {
                let conduction = if c.topology == Topology::Diode {
                    self.pitch_env
                } else {
                    0.0
                };
                let released = (self.age as f32 / self.fs - shape.hold * ds).max(0.0);
                let [late_ms, late_t60] = late_ring(id);
                let t60 = if holding {
                    shape.hold_t60
                } else if late_t60 > 0.0 && self.age as f32 >= late_ms * 0.001 * self.fs {
                    late_t60 * ds
                } else {
                    6.9077554
                        / (6.9077554 / reference_time + damping_ramp(id) * released / (ds * ds))
                } / (1.0 + diode_damping(id) * conduction);
                // The CR-8000 toms feed pink noise into their own loop as artificial
                // reverberation (`research:instruments/analogue-drum-machines.md` §5.6), so it
                // rings at the drum's pitch rather than hissing above it. The one-pole 600 Hz
                // colour and 0.004 drive are chosen in the 2026-09-19 A/B pass to keep the
                // recordings' 1–4 kHz bands 45–70 dB down with a faint noisy ring; a capture of the
                // loop's injected noise replaces them.
                let excitation = if capabilities.noise {
                    let coefficient = self.pole_600;
                    self.aux_lp += coefficient * (source.noise - self.aux_lp);
                    strike
                        + self.aux_lp
                            * self.noise_env
                            * 0.004
                            * (0.5 + 0.5 * (0.5 + 0.5 * noise_amount))
                } else {
                    strike
                };
                let f = c.f1 * ratio * pitch_mod;
                let series = body_harmonics(id);
                let mut ring = if series[8] > 0.0 {
                    // The loop's own distortion: a fixed harmonic series on the ring's phase while
                    // its amplitude is above the diodes' conduction (`body_harmonics`).
                    let (value, amplitude) = self.ring_and_amplitude(0, f, t60, excitation, phase);
                    value + harmonic_series(series, value, amplitude)
                } else {
                    self.ring(0, f, t60, excitation, phase)
                };
                let [second_gain, second_t60] = second_mode(id);
                if second_gain > 0.0 {
                    let second_phase = mode_phase(id, 1, phase);
                    ring += second_gain
                        * self.ring(
                            1,
                            c.f2 * ratio * pitch_mod,
                            second_t60 * ds,
                            strike,
                            second_phase,
                        );
                }
                let [third_hz, third_gain, third_t60, _] = third_mode(id);
                if third_gain > 0.0 {
                    let third_phase = mode_phase(id, 2, phase);
                    ring += third_gain
                        * self.ring(
                            2,
                            third_hz * ratio * pitch_mod,
                            third_t60 * ds,
                            strike,
                            third_phase,
                        );
                }
                let [pulse_level, pulse_width, pulse_droop, pulse_rise] = trigger_pulse(id);
                let trigger =
                    if pulse_level != 0.0 && (self.age as f32) < pulse_width * 0.001 * self.fs {
                        let ms = self.age as f32 * 1000.0 / self.fs;
                        pulse_level
                            * self.level
                            * (1.0 - (-ms / pulse_rise).exp())
                            * (-ms / pulse_droop).exp()
                    } else {
                        0.0
                    };
                let [whack_level, whack_width] = whack(id);
                let whack = if (self.age as f32) < whack_width * self.fs {
                    whack_level * self.env
                } else {
                    0.0
                };
                let mut click = strike * click_level(id) + trigger;
                if id == 40 {
                    // The rimshot's spikes carry no low end in its recording: its edge is
                    // coupled through a first-order 1 kHz high-pass.
                    let coefficient = self.pole_1000;
                    self.aux_lp += coefficient * (click - self.aux_lp);
                    click -= self.aux_lp;
                }
                ring * (1.0 + 0.2 * body) + click + whack
            }
            Topology::Dual => {
                let [gain_a, time_a, gain_b, time_b] = dual_modes(id);
                let a = self.ring(
                    0,
                    c.f1 * ratio * pitch_mod,
                    reference_time * time_a,
                    strike,
                    phase,
                );
                let b = self.ring(
                    1,
                    c.f2 * ratio * pitch_mod,
                    reference_time * time_b,
                    strike,
                    phase,
                );
                (gain_a * a + gain_b * b) * (1.0 + 0.2 * body)
            }
            Topology::Snare => {
                let [body_level, wire_level, _] = snare_mix(id);
                let body_f = c.f1 * ratio * pitch_mod;
                let series = body_harmonics(id);
                let mut body_sample = if series[8] > 0.0 {
                    let (value, amplitude) =
                        self.ring_and_amplitude(0, body_f, reference_time, strike, phase);
                    value + harmonic_series(series, value, amplitude)
                } else {
                    self.ring(0, body_f, reference_time, strike, phase)
                };
                let [second_gain, second_t60] = second_mode(id);
                if second_gain > 0.0 {
                    body_sample +=
                        second_gain * self.ring(1, c.f2 * ratio, second_t60 * ds, strike, phase);
                }
                let wire = self.coloured(source.noise, colour, 2f32.powf(0.55 * tone));
                let wire = if (self.age as f32) < wire_delay(id) * 0.001 * self.fs {
                    0.0
                } else {
                    wire
                };
                body_sample * body_level * (1.0 + 0.33 * body)
                    + wire * self.amp * wire_level * (0.70 + 0.60 * (0.5 + 0.5 * noise_amount))
                    + self.feedthrough(id)
                    + self.edge(id, strike * click_level(id), tone)
            }
            Topology::Noise => {
                (self.coloured(source.noise, colour, 2f32.powf(tone))
                    * self.amp
                    * (0.7 + 0.3 * (0.5 + 0.5 * noise_amount))
                    + self.band_edge(id, strike * click_level(id), tone)
                    + self.feedthrough(id)
                    + self.attack_noise(id, source.noise)
                    + self.noise_resonance(id, source.noise))
                    * self.closing(id)
            }
            Topology::Metal => {
                let pair = matches!(id, 46 | 70);
                let at_reference_pitch = finite(p.pitch_semitones, 0.0).abs() <= 1.0e-6;
                // Two voices cannot ask one physical bank for two tunings, so a deviation takes
                // this voice's own squares. The two banks free-run on unrelated phases and the
                // private one is parked with an empty BLEP pipeline, so crossing between them is
                // bounded rather than done on one sample. Returning to the reference rejoins the
                // shared phase without resetting either bank.
                if !at_reference_pitch {
                    self.metal_ratio = ratio;
                }
                let target = if at_reference_pitch { 0.0 } else { 1.0 };
                let step = 1.0 / (self.fs * crate::engine::METAL_TRANSITION_SECONDS).max(1.0);
                self.metal_blend += (target - self.metal_blend).clamp(-step, step);
                let blend = self.metal_blend;
                let ratio = self.metal_ratio;
                // A pair's direct path takes its own mix (`cowbell_direct_weights`).
                let direct;
                let mut shared_direct = 0.0;
                let shared = if pair {
                    let indices = cowbell_pair_indices(id);
                    let weights = cowbell_direct_weights(id);
                    shared_direct = 0.5
                        * (weights[0] * source.oscillators[indices[0]]
                            + weights[1] * source.oscillators[indices[1]]);
                    cowbell_pair(id, source)
                } else {
                    // Cymbals and hats consume the continuously running machine bank. Their
                    // former f1/f2 shortcut silently replaced the six/four-source substrate with
                    // one private square oscillator.
                    source.metal
                };
                let mut private_direct = 0.0;
                // Parked on shared, the private bank is not ticked at all: its phase persists.
                let private = if blend <= 0.0 {
                    0.0
                } else if pair {
                    // Away from reference the same two squares the shared frame supplies, moved by
                    // Tune. Two free-running phases: deriving the second from the first hard-synced
                    // it to the first's period, and model 46 took another machine's pair, so any
                    // Tune away from zero replaced the cowbell with a different instrument.
                    let frequencies = metal_frequencies(c.machine);
                    let mut sum = 0.0;
                    let mut direct_sum = 0.0;
                    for ((index, weight), direct_weight) in cowbell_pair_indices(id)
                        .into_iter()
                        .zip(cowbell_pair_weights(id))
                        .zip(cowbell_direct_weights(id))
                    {
                        let square = self.metal_squares[index].tick(
                            f64::from(frequencies[index] * ratio),
                            f64::from(self.fs),
                            f64::from(metal_duty(c.machine, index)),
                        );
                        sum += weight * square;
                        direct_sum += direct_weight * square;
                    }
                    private_direct = 0.5 * direct_sum;
                    0.5 * sum
                } else {
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for (index, frequency) in metal_frequencies(c.machine)
                        .iter()
                        .copied()
                        .enumerate()
                        .take(BANK)
                    {
                        if frequency > 0.0 {
                            sum += self.metal_squares[index].tick(
                                f64::from(frequency * ratio),
                                f64::from(self.fs),
                                f64::from(metal_duty(c.machine, index)),
                            );
                            count += 1.0;
                        }
                    }
                    sum / count
                };
                // Exact at both ends, so a steady voice at either tuning is untouched by the
                // crossfade existing.
                let mut x = if blend <= 0.0 {
                    direct = shared_direct;
                    shared
                } else if blend >= 1.0 {
                    direct = private_direct;
                    private
                } else {
                    direct = shared_direct + (private_direct - shared_direct) * blend;
                    shared + (private - shared) * blend
                };
                let [noise_mix, noise_corner, _, _] = metal_noise(id);
                x += noise_mix
                    * if noise_corner > 0.0 {
                        let a = 1.0 - (-std::f32::consts::TAU * noise_corner / self.fs).exp();
                        self.aux_lp += a * (source.noise - self.aux_lp);
                        self.aux_lp
                    } else {
                        source.noise
                    };
                if capabilities.noise {
                    x += source.noise * (0.12 + 0.18 * (0.5 + 0.5 * noise_amount));
                }
                let [low_hz, low_q, low_level, _, _] = low_band(id);
                let low = if low_level > 0.0 {
                    let centre = low_hz * 2f32.powf(tone);
                    self.filters[4].tune(self.fs, 0.5 * centre, 0.707);
                    let lifted = self.filters[4].process(x).0;
                    self.filters[3].tune(self.fs, centre, low_q);
                    self.filters[3].process(lifted).1 * self.amp_low * low_level
                } else {
                    0.0
                };
                let direct = if pair { direct } else { x };
                let [_, _, top, top_poles] = metal_noise(id);
                let top_noise = if top <= 0.0 {
                    0.0
                } else if top_poles >= 2.0 {
                    // Two poles where one pole's skirt would fill the band below (the TR-606
                    // band sits an octave under this corner). Filter 2 is the colour's low-pass
                    // slot and the peak's; neither meets a two-pole top.
                    self.filters[2].tune(self.fs, 9_000.0, 0.707);
                    top * self.filters[2].process(source.noise).0
                } else {
                    // Its own state: a voice that low-passes its mixed noise already owns
                    // `aux_lp`.
                    let a = self.pole_6000;
                    self.top_hp += a * (source.noise - self.top_hp);
                    top * (source.noise - self.top_hp)
                };
                let coloured = self.coloured(x, colour, 2f32.powf(tone))
                    + colour_direct(id) * direct
                    + top_noise;
                let [threshold, post_high_pass] = rectifying_vca(id);
                let metal = if post_high_pass > 0.0 {
                    // Conducts on one polarity above a threshold; the high-pass after it removes
                    // the envelope the rectifier leaks. The trigger's falling edge, a step through
                    // that high-pass, is the hat's opening click.
                    let trigger_end = (TRIGGER_WIDTH * self.fs) as u32;
                    let edge = if self.age >= trigger_end {
                        -click_level(id) * self.level
                    } else {
                        0.0
                    };
                    let conducted = (coloured + threshold).min(0.0) * self.amp + edge;
                    let corner = (post_high_pass * 2f32.powf(tone)).min(0.45 * self.fs);
                    self.filters[2].tune(self.fs, corner, 0.707);
                    let passed = self.filters[2].process(conducted).0;
                    // A third, one-pole order, then the output network's resonance
                    // (`hat_output`).
                    let [pole_hz, peak_hz, peak_q, peak_gain] = hat_output(id);
                    let steeper = if pole_hz > 0.0 {
                        let a = 1.0
                            - (-std::f32::consts::TAU * pole_hz * 2f32.powf(tone) / self.fs).exp();
                        self.aux_lp += a * (passed - self.aux_lp);
                        passed - self.aux_lp
                    } else {
                        passed
                    };
                    if peak_gain > 0.0 {
                        self.filters[3].tune(self.fs, peak_hz * 2f32.powf(tone), peak_q);
                        steeper + peak_gain * self.filters[3].process(steeper).1
                    } else {
                        steeper
                    }
                } else {
                    coloured * self.amp
                };
                // A metal voice whose recording darkens as it falls (`metal_darkening`).
                let [full_corner, exponent, makeup] = metal_darkening(id);
                let metal = if full_corner > 0.0 && self.level > 0.0 {
                    let corner = (full_corner * (self.amp / self.level).max(0.0).powf(exponent))
                        .clamp(500.0, 0.45 * self.fs);
                    let coefficient = 1.0 - (-std::f32::consts::TAU * corner / self.fs).exp();
                    self.tone_lp += coefficient * (metal - self.tone_lp);
                    // The band it removes is level, not loudness: the recording keeps its envelope
                    // while its colour falls, so the cut is made up.
                    self.tone_lp * (full_corner / corner).powf(makeup)
                } else {
                    metal
                };
                // The trigger's own click at a metal voice's attack (`metal_edge`), low-passed by
                // the output stage. `even_lp` is the tonal branch's state and free here.
                let [edge_level, edge_corner] = metal_edge(id);
                let click = if edge_level > 0.0 {
                    let corner = edge_corner * 3.0_f32.powf(tone);
                    let coefficient = 1.0 - (-std::f32::consts::TAU * corner / self.fs).exp();
                    self.even_lp += coefficient * (strike * edge_level - self.even_lp);
                    self.even_lp
                } else {
                    0.0
                };
                // The output network's own resonance, where the recording's lines peak at one
                // place the band alone does not reach (`metal_peak`). It owns a filter because
                // the TR-606 cymbal already uses filter 2 for its two-pole high-passed top.
                let [peak_hz, peak_q, peak_gain] = metal_peak(id);
                let metal = if peak_gain > 0.0 {
                    self.metal_peak_filter
                        .tune(self.fs, peak_hz * 2f32.powf(tone), peak_q);
                    metal + peak_gain * self.metal_peak_filter.process(metal).1
                } else {
                    metal
                };
                let [floor_level, floor_t60, floor_corner] = metal_floor(id);
                let floor = if floor_level > 0.0 {
                    let envelope = if floor_t60 > 0.0 {
                        self.level * (-6.9077554 * self.age as f32 / (floor_t60 * self.fs)).exp()
                    } else {
                        self.amp
                    };
                    let shaped = if floor_corner > 0.0 {
                        let coefficient =
                            1.0 - (-std::f32::consts::TAU * floor_corner / self.fs).exp();
                        self.tone_lp += coefficient * (source.noise - self.tone_lp);
                        self.tone_lp
                    } else {
                        source.noise
                    };
                    floor_level * envelope * shaped
                } else {
                    0.0
                };
                (metal + low + self.feedthrough(id) + click + floor) * self.closing(id)
            }
            Topology::Clap => {
                let ms = self.age as f32 * 1000. / self.fs;
                let [
                    count,
                    spacing,
                    burst_t60,
                    tail_level,
                    edge,
                    tail_open,
                    tail_rise,
                ] = clap_bursts(id);
                let [attack_ms, tail_corner, tail_t60, slow_share, slow_t60] = clap_voicing(id);
                let burst_t60 = burst_t60 * noise_time;
                let mut burst = 0.0;
                let mut index = 0.0;
                while index < count {
                    let t = ms - index * spacing;
                    // Each burst is cut by the next; the last runs out.
                    if t >= 0.0 && (t < spacing || index + 1.0 >= count) {
                        burst +=
                            (1.0 - (-t / attack_ms).exp()) * (-6.9077554 * t / burst_t60).exp();
                    }
                    index += 1.0;
                }
                // The tail opens at its own time, and a VCA with a conduction threshold ends it in
                // finite time.
                let tail = if ms >= tail_open {
                    let rise = if tail_rise > 0.0 {
                        1.0 - (-(ms - tail_open) / tail_rise).exp()
                    } else {
                        1.0
                    };
                    // A tail with its own time decays from its opening; otherwise it rides the
                    // voice envelope from the trigger.
                    let envelope = if tail_t60 > 0.0 {
                        let since = -6.9077554 * (ms - tail_open) / (1000.0 * ds);
                        self.level
                            * ((since / tail_t60).exp()
                                + slow_share * (since / slow_t60.max(0.001)).exp())
                    } else {
                        self.amp
                    };
                    tail_level * rise * envelope * self.closing(id)
                } else {
                    0.0
                };
                let noise = self.coloured(source.noise, colour, 2f32.powf(0.4 * tone))
                    + colour_direct(id) * source.noise;
                // The room tail is a separate path, darker where a corner is given.
                let tail_noise = if tail_corner > 0.0 {
                    let coefficient = 1.0 - (-std::f32::consts::TAU * tail_corner / self.fs).exp();
                    self.aux_lp += coefficient * (noise - self.aux_lp);
                    self.aux_lp
                } else {
                    noise
                };
                (noise * burst + tail_noise * tail) * (0.75 + 0.25 * (0.5 + 0.5 * noise_amount))
                    + strike * edge
            }
            Topology::Guiro => {
                let [slow_seconds, pulse_ms] = guiro_timing(id);
                let rate = if (self.age as f32) < slow_seconds * ds * self.fs {
                    c.f2
                } else {
                    c.f1
                } * ratio;
                self.phase = (self.phase + rate / self.fs).fract();
                let gate = if self.phase < pulse_ms * 0.001 * rate {
                    1.0
                } else {
                    0.0
                };
                self.coloured(source.noise * gate, colour, 2f32.powf(0.35 * tone)) * self.amp
            }
        };
        let [mod_rate, mod_depth] = body_modulation(id);
        if mod_rate > 0.0 {
            self.mod_phase = (self.mod_phase + mod_rate / self.fs).fract();
            out *= 1.0 + mod_depth * (std::f32::consts::TAU * self.mod_phase).sin();
        }
        if matches!(
            c.topology,
            Topology::Resonator | Topology::Dual | Topology::Diode
        ) {
            let cutoff = (tone_cutoff(id) * 3.0_f32.powf(tone)).clamp(220.0, 0.4 * self.fs);
            let coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / self.fs).exp();
            self.tone_lp += coefficient * (out - self.tone_lp);
            out = self.tone_lp;
            let [square, cube, tick, threshold] = shaper(id);
            if tick > 0.0 {
                // One-pole high-pass of the switch's sign: a decaying spike at each crossing.
                let above = out > threshold;
                if above != self.switched {
                    // The step scales with the ring's envelope: a louder ring slews through the
                    // switch faster.
                    let step = 2.0 * tick * self.env;
                    self.spike += if above { step } else { -step };
                    self.switched = above;
                }
                self.spike = flush32(self.spike * self.decay_3000);
            }
            // The even term's slow offset (it follows the squared envelope) leaves through the
            // stage's coupling: a one-pole high-pass at three quarters of the rest pitch keeps the
            // second harmonic and removes the thump.
            let even = square * out * out;
            let corner = (0.75 * c.f1 * ratio).clamp(10.0, 0.4 * self.fs);
            self.even_lp +=
                (1.0 - (-std::f32::consts::TAU * corner / self.fs).exp()) * (even - self.even_lp);
            self.even_lp = flush32(self.even_lp);
            out += even - self.even_lp + cube * out * out * out + self.spike;
            // The diode loop's negative clip: below the knee the excess passes at 8 %.
            let knee = diode_clip(id) * self.level;
            if knee > 0.0 && out < -knee {
                out = -knee + 0.08 * (out + knee);
            }
        }
        // Tone primarily moves the circuit filter above; this small loading change preserves an
        // audible response where a high-band SVF reaches its stability ceiling.
        out *= output_sign(id);
        out *= 1.0 + 0.05 * tone;
        out *= 1. + 0.25 * bipolar(p.attack);
        // The saturator is an emergency bound at reference: the signal enters it at a quarter of
        // its level, so it adds no audible third harmonic, and Character returns the drive to the
        // full-level law at +1 while the level comes back by the same factor.
        let headroom = OUTPUT_HEADROOM.powf(1.0 - character.max(0.0));
        out = (out * headroom * (1.0 + 0.45 * character)).tanh() / headroom;

        // Envelope recursions. The gate holds `env` and `noise_env` (at `hold_t60`, or flat)
        // before they release; Noise decay scales the noise gate's hold with its release.
        let hold_coefficient = |scale: f32| {
            if shape.hold_t60 > 0.0 {
                decay_coefficient(shape.hold_t60 * scale, self.fs)
            } else {
                1.0
            }
        };
        if holding {
            self.env *= hold_coefficient(ds);
        } else {
            self.env *= decay_coefficient(reference_time, self.fs);
        }
        let [_, _, wire_t60] = snare_mix(id);
        let noise_t60 = if c.topology == Topology::Snare {
            wire_t60
        } else {
            c.decay
        };
        // A snare's Decay moves its body pole; its wire gate follows Noise decay alone, so an
        // extended body still rings clear of the wires.
        let noise_scale = if c.topology == Topology::Snare {
            noise_time
        } else {
            ds * noise_time
        };
        if (self.age as f32) < shape.hold * noise_scale * self.fs {
            self.noise_env *= hold_coefficient(noise_scale);
        } else {
            self.noise_env *= decay_coefficient(noise_t60.max(0.001) * noise_scale, self.fs);
        }
        self.stage_env *= decay_coefficient(shape.stage_t60.max(0.001) * ds, self.fs);
        if sweep_depth != 0.0 {
            let time = if capabilities.pitch_decay {
                envelope_time_scale(bipolar(p.pitch_decay))
            } else {
                1.0
            };
            self.pitch_env *= decay_coefficient(sweep_t60 * time, self.fs);
        } else if capabilities.pitch_envelope {
            self.pitch_env *= decay_coefficient(
                reference_time * envelope_time_scale(bipolar(p.pitch_decay)),
                self.fs,
            );
        } else {
            self.pitch_env = 0.0;
        }
        let gated = match c.topology {
            Topology::Snare => self.noise_env,
            Topology::Noise if capabilities.noise_decay => self.noise_env,
            Topology::Noise | Topology::Metal | Topology::Clap | Topology::Guiro => self.env,
            _ => 0.0,
        };
        let target = (1.0 - shape.stage_share) * gated + shape.stage_share * self.stage_env;
        let attack = if shape.attack > 0.0 {
            1.0 - (-1.0 / (shape.attack * self.fs)).exp()
        } else {
            1.0
        };
        self.amp += attack * (target - self.amp);
        let [_, _, low_level, low_t60, low_release] = low_band(id);
        if low_level > 0.0 {
            let low_time = if holding || shape.hold <= 0.0 {
                low_t60
            } else {
                low_release
            } * ds;
            self.low_env *= decay_coefficient(low_time, self.fs);
            self.amp_low += attack * (self.low_env - self.amp_low);
        } else {
            self.low_env = 0.0;
        }
        self.age = self.age.saturating_add(1);
        // Sixty seconds is only an emergency lifetime for extended-decay settings. Ordinary voices
        // park as soon as every envelope reaches the exact-idle threshold.
        if self.age > (self.fs * 60.) as u32
            || (self.env < 1e-8
                && self.stage_env < 1e-8
                && self.pitch_env < 1e-8
                && self.noise_env < 1e-8
                && self.amp.abs() < 1e-8
                && self.low_env < 1e-8
                && self.amp_low.abs() < 1e-8
                && self.thump.iter().all(|t| t.abs() < 1e-8))
        {
            self.reset();
            return 0.;
        }
        out
    }
    /// One struck pole: its sine-phase output blended with its quadrature by `phase`
    /// (`[cos φ, sin φ]`). The quadrature is `(s[n] − r cos ω · s[n−1]) / sin ω`, the pole pair's
    /// cosine-phase impulse response, computed from the same pole the resonator holds.
    /// `ring`, also returning the ring's instantaneous amplitude from its quadrature pair.
    fn ring_and_amplitude(
        &mut self,
        index: usize,
        frequency: f32,
        t60: f32,
        excitation: f32,
        phase: [f32; 2],
    ) -> (f32, f32) {
        // `configure` only rebuilds coefficients, so an unchanged tuning does not need it. A
        // swept model still rebuilds every sample; a steady one stops paying for an `exp`, a
        // `cos` and a `sin`. `NaN` in the held tuning always compares unequal, which is how reset
        // and a rate change force the rebuild.
        if self.mode_tuning[index] != (frequency, t60) {
            self.modes[index].configure(self.fs, frequency, t60);
            self.mode_pole[index] = pole_terms(self.fs, frequency, t60);
            self.mode_tuning[index] = (frequency, t60);
        }
        let sine = self.modes[index].process_strike(excitation);
        let previous = self.previous[index];
        self.previous[index] = sine;
        let (radius_cos, angle_sin) = self.mode_pole[index];
        let cosine = ((f64::from(sine) - radius_cos * f64::from(previous)) / angle_sin) as f32;
        (phase[0] * sine + phase[1] * cosine, sine.hypot(cosine))
    }
    fn ring(
        &mut self,
        index: usize,
        frequency: f32,
        t60: f32,
        excitation: f32,
        phase: [f32; 2],
    ) -> f32 {
        // `configure` only rebuilds coefficients, so an unchanged tuning does not need it. A
        // swept model still rebuilds every sample; a steady one stops paying for an `exp`, a
        // `cos` and a `sin`. `NaN` in the held tuning always compares unequal, which is how reset
        // and a rate change force the rebuild.
        if self.mode_tuning[index] != (frequency, t60) {
            self.modes[index].configure(self.fs, frequency, t60);
            self.mode_pole[index] = pole_terms(self.fs, frequency, t60);
            self.mode_tuning[index] = (frequency, t60);
        }
        let sine = self.modes[index].process_strike(excitation);
        let previous = self.previous[index];
        self.previous[index] = sine;
        if phase[1].abs() < 1.0e-6 {
            return phase[0] * sine;
        }
        let (radius_cos, angle_sin) = self.mode_pole[index];
        let cosine = ((f64::from(sine) - radius_cos * f64::from(previous)) / angle_sin) as f32;
        phase[0] * sine + phase[1] * cosine
    }
    /// Moves this voice's resonances as its own envelope falls (`resonance_drift`), on a block
    /// boundary. A hardware resonance is narrow at the strike and its centre climbs through the
    /// tail, because the lower modes die first; one fixed band cannot follow a recording that
    /// climbs 1.1 kHz while it decays. The drift takes tens of milliseconds, so 64 samples
    /// resolves it at every supported rate, and the block keeps the power out of the sample loop.
    fn drift(&mut self, id: u8) {
        const BLOCK: u32 = 64;
        if !self.age.is_multiple_of(BLOCK) {
            return;
        }
        let [start_hz, end_hz, start_q, end_q, curve, seconds] = resonance_drift(id);
        self.drift_peak = self.drift_peak.max(self.amp);
        let fallen = if seconds > 0.0 {
            // A voice whose envelope holds flat gives the fall nothing to ride — the tambourine
            // holds for 200 ms and has lost 5 % by 45 ms — so it drifts on its own clock instead.
            1.0 - (-(self.age as f32) / (seconds * self.fs)).exp()
        } else if self.drift_peak > 0.0 {
            (1.0 - self.amp / self.drift_peak).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // A recording's drift is fastest while the strike's own modes die, so the progress is
        // curved rather than linear in the envelope's fall.
        let fallen = if curve == 1.0 {
            fallen
        } else {
            fallen.powf(curve)
        };
        self.drift_hz = start_hz * (end_hz / start_hz).powf(fallen);
        self.drift_q = start_q + (end_q - start_q) * fallen;
    }
    fn coloured(&mut self, x: f32, colour: [f32; 5], scale: f32) -> f32 {
        let [high_pass, high_q, band_pass, band_q, low_pass] = colour;
        let mut y = x;
        if high_pass > 0.0 {
            self.filters[0].tune(self.fs, high_pass * scale, high_q);
            y = self.filters[0].process(y).0;
        }
        if band_pass > 0.0 {
            self.filters[1].tune(
                self.fs,
                band_pass * scale * self.drift_hz,
                band_q * self.drift_q,
            );
            y = self.filters[1].process(y).1;
        }
        if low_pass > 0.0 {
            self.filters[2].tune(self.fs, low_pass * scale, 0.707);
            y = self.filters[2].process(y).2;
        }
        y
    }
    /// Gain of a VCA with a conduction threshold (`vca_threshold`): one while the envelope stands
    /// above the threshold relative to the trigger's level, falling linearly to zero across the
    /// width below it.
    fn closing(&self, id: u8) -> f32 {
        let [threshold, width] = vca_threshold(id);
        if threshold <= 0.0 || self.level <= 0.0 {
            return 1.0;
        }
        ((self.amp / self.level - (threshold - width)) / width).clamp(0.0, 1.0)
    }
    /// A trigger edge through the output stage's roll-off (`tone_cutoff`), so the pulse rises
    /// rather than stepping.
    fn edge(&mut self, id: u8, x: f32, tone: f32) -> f32 {
        let cutoff = (tone_cutoff(id) * 3.0_f32.powf(tone)).clamp(220.0, 0.4 * self.fs);
        let coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / self.fs).exp();
        self.tone_lp += coefficient * (x - self.tone_lp);
        self.tone_lp
    }
    /// A noise voice's trigger edge as a band: the output stage's roll-off above, a 400 Hz one-pole
    /// high-pass below (the CR-78 recordings' click sits in 0.5–3 kHz).
    fn band_edge(&mut self, id: u8, x: f32, tone: f32) -> f32 {
        let edge = self.edge(id, x, tone);
        let coefficient = self.pole_400;
        self.aux_lp += coefficient * (edge - self.aux_lp);
        edge - self.aux_lp
    }
    /// A noise voice's narrow resonance (`noise_resonance`), under the voice's own envelope.
    fn noise_resonance(&mut self, id: u8, noise: f32) -> f32 {
        let [centre, q, level, rise] = noise_resonance(id);
        if level == 0.0 {
            return 0.0;
        }
        // Filters 3 and 4 belong to `attack_noise`; a noise voice's colour leaves 2 free.
        self.filters[2].tune(self.fs, centre * self.drift_hz, q * self.drift_q);
        let open = if rise > 0.0 {
            1.0 - (-(self.age as f32) / (rise * self.fs)).exp()
        } else {
            1.0
        };
        level * open * self.amp * self.filters[2].process(noise).1
    }
    /// The unfiltered noise burst at a noise voice's attack (`attack_noise`).
    fn attack_noise(&mut self, id: u8, noise: f32) -> f32 {
        let [level, t60, high_pass, low_pass] = attack_noise(id);
        if level == 0.0 {
            return 0.0;
        }
        self.filters[3].tune(self.fs, high_pass, 0.707);
        let lifted = self.filters[3].process(noise).0;
        self.filters[4].tune(self.fs, low_pass, 0.707);
        let wide = self.filters[4].process(lifted).2;
        let envelope = if t60 > 0.0 {
            self.level * (-6.9077554 * self.age as f32 / (t60 * self.fs)).exp()
        } else {
            self.amp
        };
        level * envelope * wide
    }
    /// Envelope feedthrough: the VCA control, low-passed, crosses two coupling high-passes and
    /// arrives inverted — a dip while the gate opens and a hump when it closes.
    fn feedthrough(&mut self, id: u8) -> f32 {
        let [level, smoothing, coupling] = feedthrough(id);
        if level == 0.0 {
            return 0.0;
        }
        let key = (id, self.fs);
        if self.thump_poles_for != key {
            self.thump_poles = (
                1.0 - (-std::f32::consts::TAU * smoothing / self.fs).exp(),
                1.0 - (-std::f32::consts::TAU * coupling / self.fs).exp(),
            );
            self.thump_poles_for = key;
        }
        let (a, b) = self.thump_poles;
        self.thump[0] += a * (self.amp - self.thump[0]);
        self.thump[1] += b * (self.thump[0] - self.thump[1]);
        let once = self.thump[0] - self.thump[1];
        self.thump[2] += b * (once - self.thump[2]);
        -level * (once - self.thump[2])
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        for m in &mut self.modes {
            m.reset()
        }
        for f in &mut self.filters {
            f.reset()
        }
        self.previous = [0.0; 3];
        self.strike = 0.;
        self.level = 0.;
        self.next_strike = 0;
        self.pulse = 0.;
        self.env = 0.;
        self.stage_env = 0.;
        self.pitch_env = 0.;
        self.noise_env = 0.;
        self.amp = 0.;
        self.low_env = 0.;
        self.amp_low = 0.;
        self.aux_lp = 0.;
        self.top_hp = 0.;
        self.drift_peak = 0.;
        self.drift_hz = 1.;
        self.drift_q = 1.;
        self.mod_phase = 0.;
        self.thump = [0.0; 3];
        self.switched = false;
        self.spike = 0.;
        self.even_lp = 0.;
        self.tone_lp = 0.;
        self.phase = 0.;
        self.metal_squares = start_bank();
        // The output network's own resonance is a high-Q filter and was leaking across a reset,
        // a choke and a model replacement into the next hit.
        self.metal_peak_filter.reset();
        // `Resonator::reset` clears the recurrence, so a held tuning must be rebuilt.
        self.mode_tuning = [(f32::NAN, f32::NAN); 3];
        self.thump_poles_for = (0, 0.0);
        self.metal_blend = 0.0;
        self.metal_ratio = 1.0;
        self.active = false;
        self.age = 0
    }
}
/// One-pole coefficient for a fixed corner: the share of the way to the input each sample moves.
fn one_pole(sample_rate: f32, corner_hz: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * corner_hz / sample_rate).exp()
}

/// Per-sample decay multiplier of a one-pole at a fixed corner.
fn decay_pole(sample_rate: f32, corner_hz: f32) -> f32 {
    (-std::f32::consts::TAU * corner_hz / sample_rate).exp()
}

/// `r cos ω` and `sin ω` of the pole `Resonator::configure` builds, with its clamps.
fn pole_terms(sample_rate: f32, frequency: f32, t60: f32) -> (f64, f64) {
    let fs = f64::from(finite(sample_rate, 48_000.0).max(MIN_SAMPLE_RATE));
    let frequency =
        f64::from(finite(frequency, 100.0).clamp(0.0, MAX_FREQUENCY_FRACTION * fs as f32));
    let t60 = f64::from(finite(t60, 0.1).clamp(1.0 / fs as f32, 60.0));
    let radius = (0.001_f64.ln() / (t60 * fs)).exp().min(1.0 - 1.0e-9);
    let angle = std::f64::consts::TAU * frequency / fs;
    (radius * angle.cos(), angle.sin().abs().max(1.0e-9))
}
/// Per-sample multiplier of an exponential envelope with the given T60.
fn decay_coefficient(t60: f32, fs: f32) -> f32 {
    (-6.9077554 / (t60 * fs)).exp()
}
fn flush(x: f64) -> f64 {
    if x.abs() < 1.0e-30 { 0.0 } else { x }
}
fn flush32(x: f32) -> f32 {
    if x.abs() < 1.0e-20 { 0.0 } else { x }
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
fn extended_decay_scale(value: f32) -> f32 {
    if value < 0.0 {
        0.35_f32.powf(-value)
    } else {
        8.0_f32.powf(value)
    }
}
fn bipolar(v: f32) -> f32 {
    finite(v, 0.).clamp(-1., 1.)
}
fn finite(v: f32, f: f32) -> f32 {
    if v.is_finite() { v } else { f }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Renders one supporting voice from a freshly seeded machine source stream.
    fn render_from_fresh_sources(
        voice: &mut Voice,
        id: u8,
        p: SlotPatch,
        samples: usize,
    ) -> Vec<f32> {
        let mut sources = Sources::new();
        (0..samples)
            .map(|_| {
                let frame = sources.tick(48000.)[machine(id)];
                voice.process(id, p, frame, 120.0)
            })
            .collect()
    }

    #[test]
    fn reset_clears_the_output_networks_own_resonance() {
        // 41 and 51 are the two voices with a `metal_peak` filter, at Q 8 and Q 30. It sat outside
        // `reset`, so a reset, a choke or a model replacement left its ring in the next hit.
        for id in [41_u8, 51] {
            let p = SlotPatch::default();
            let mut fresh_voice = Voice::new();
            fresh_voice.trigger(1., p);
            let fresh = render_from_fresh_sources(&mut fresh_voice, id, p, 8000);

            let mut reused = Voice::new();
            reused.trigger(1., p);
            let _ = render_from_fresh_sources(&mut reused, id, p, 4000);
            reused.reset();
            reused.trigger(1., p);
            let again = render_from_fresh_sources(&mut reused, id, p, 8000);

            assert_eq!(
                fresh, again,
                "model {id} carried filter state across a reset into the next hit"
            );
        }
    }

    #[test]
    fn returning_a_supporting_metal_voice_to_reference_crosses_rather_than_steps() {
        // 51's bank is six squares on free phases; switching to the shared frame on one sample
        // steps every partial at once. Holding the deviation is the control, so the voice's own
        // travel appears in both and only the bank move is under test.
        let id = 51_u8;
        let render_pair = |return_to_reference: bool| {
            let mut sources = Sources::new();
            let mut voice = Voice::new();
            let mut p = SlotPatch {
                pitch_semitones: 7.0,
                ..SlotPatch::default()
            };
            voice.trigger(1., p);
            for _ in 0..4000 {
                let frame = sources.tick(48000.)[machine(id)];
                let _ = voice.process(id, p, frame, 120.0);
            }
            if return_to_reference {
                p.pitch_semitones = 0.0;
            }
            (0..4000)
                .map(|_| {
                    let frame = sources.tick(48000.)[machine(id)];
                    voice.process(id, p, frame, 120.0)
                })
                .collect::<Vec<f32>>()
        };
        assert_crosses(
            render_pair(false),
            render_pair(true),
            "returning to the shared bank",
        );
    }

    #[test]
    fn taking_a_supporting_metal_voice_off_reference_crosses_rather_than_steps() {
        // The other direction, and the sharper one: the private bank is parked with an empty BLEP
        // pipeline, so moving onto it cold is where a switch is most audible.
        let id = 51_u8;
        let render_pair = |leave_reference: bool| {
            let mut sources = Sources::new();
            let mut voice = Voice::new();
            let mut p = SlotPatch::default();
            voice.trigger(1., p);
            for _ in 0..4000 {
                let frame = sources.tick(48000.)[machine(id)];
                let _ = voice.process(id, p, frame, 120.0);
            }
            if leave_reference {
                p.pitch_semitones = 7.0;
            }
            (0..4000)
                .map(|_| {
                    let frame = sources.tick(48000.)[machine(id)];
                    voice.process(id, p, frame, 120.0)
                })
                .collect::<Vec<f32>>()
        };
        assert_crosses(
            render_pair(false),
            render_pair(true),
            "leaving the shared bank",
        );
    }

    /// Requires the move between banks to arrive gradually. Holding the tuning is the control, so
    /// the voice's own travel is in both and only the bank move is measured; what separates a
    /// crossfade from a switch is that the divergence grows rather than arriving whole.
    fn assert_crosses(held: Vec<f32>, moved: Vec<f32>, what: &str) {
        let returned = moved;
        let divergence = |range: std::ops::Range<usize>| {
            let count = range.len();
            let power: f64 = returned[range.clone()]
                .iter()
                .zip(&held[range])
                .map(|(a, b)| {
                    let difference = f64::from(*a) - f64::from(*b);
                    difference * difference
                })
                .sum();
            (power / count as f64).sqrt()
        };
        let early = divergence(0..32);
        let settled = divergence(1024..4000);
        assert!(
            settled > 0.0,
            "{what}: Tune never moved this voice between banks"
        );
        assert!(
            early < settled * 0.25,
            "{what}: the bank arrived {early:.6} of {settled:.6} within 32 samples; it was switched"
        );
    }

    #[test]
    fn every_admitted_row_sounds_and_resets() {
        let mut sources = Sources::new();
        for id in 32..=94 {
            let p = SlotPatch::default();
            let mut v = Voice::new();
            v.trigger(1., p);
            let e: f32 = (0..24000)
                .map(|_| {
                    let f = sources.tick(48000.)[machine(id)];
                    v.process(id, p, f, 120.0).abs()
                })
                .sum();
            assert!(e > 0.01, "silent {id}: {e}");
            v.reset();
            assert_eq!(
                v.process(
                    id,
                    p,
                    Frame {
                        noise: 0.,
                        metal: 0.,
                        oscillators: [0.0; OSCILLATORS],
                    },
                    120.0,
                ),
                0.
            )
        }
    }
    #[test]
    fn compact_open_hat_decay_follows_host_tempo() {
        let duration = |tempo| {
            let mut voice = Voice::new();
            let mut sources = Sources::new();
            let patch = SlotPatch::default();
            voice.trigger(1.0, patch);
            let mut samples = 0;
            while voice.is_active() && samples < 700_000 {
                let frame = sources.tick(48_000.0)[machine(53)];
                let _ = voice.process(53, patch, frame, tempo);
                samples += 1;
            }
            samples
        };
        assert!(duration(60.0) > duration(180.0));
    }

    #[test]
    fn a_drifting_resonance_climbs_through_its_own_tail() {
        // The CR-78 cymbal's recording carries its band from 8.23 kHz at the strike to 8.73 kHz at
        // 250–500 ms (measured 2026-09-20). A fixed band cannot, so the model's must climb too.
        let fs = 48_000.0;
        let x = render(64, fs, 0.5);
        let early = centroid(&x[..(0.010 * fs) as usize], fs, 7_500.0, 9_200.0);
        let late = centroid(
            &x[(0.250 * fs) as usize..(0.500 * fs) as usize],
            fs,
            7_500.0,
            9_200.0,
        );
        assert!(
            late > early + 200.0,
            "64 should climb through its tail, {early:.0} Hz then {late:.0} Hz"
        );
        // Every voice without a fitted drift keeps one fixed band: the multipliers are exactly one,
        // so the tuning it passes its filters is the table's own number.
        for id in [1, 28, 41, 51, 63, 92] {
            assert_eq!(resonance_drift(id), [1.0, 1.0, 1.0, 1.0, 1.0, 0.0]);
        }
        // Only the one voice whose recording carries a measured ripple has one.
        for id in [1, 28, 41, 52, 53, 63, 92] {
            assert_eq!(body_modulation(id), [0.0, 0.0]);
        }
    }

    #[test]
    fn comparison_calibrations_remain_voice_specific() {
        // The 2026-09-19 A/B pass fits, pinned so a later change is deliberate.
        assert_eq!(colour(48), [800.0, 0.707, 3_500.0, 1.0, 0.0]);
        assert_eq!(colour(42), [6_000.0, 0.707, 12_000.0, 0.7, 0.0]);
        assert_eq!(
            metal_frequencies(0)[..BANK],
            [311.55, 574.35, 780.93, 902.3, 917.55, 1149.3]
        );
        assert_eq!(config(37).f1, 299.4);
        assert_eq!(restrikes(40)[0], [2.29, -0.8]);
        assert_eq!(low_band(51), [3_440.0, 10.0, 0.06, 3.6, 1.5]);
        assert_eq!(resonance_drift(64), [0.95, 1.09, 1.0, 2.0, 0.3, 0.0]);
        assert_eq!(resonance_drift(71), [0.97, 1.05, 1.5, 1.0, 1.3, 0.0]);
        assert_eq!(colour(51), [4_500.0, 0.707, 7_300.0, 7.0, 0.0]);
        assert_eq!(colour(52), colour(53));
        assert_eq!(colour(53), [4_500.0, 0.707, 7_250.0, 4.0, 0.0]);
        assert_eq!(resonance_drift(52), resonance_drift(53));
        assert_eq!(colour(64), [4_000.0, 0.707, 0.0, 3.5, 0.0]);
        assert_eq!(resonance_drift(82), [0.99, 1.02, 1.0, 1.25, 0.5, 0.0]);
        assert_eq!(body_modulation(51), [24.5, 0.08]);
        assert_eq!(
            metal_frequencies(1)[..BANK],
            [401.44, 439.6, 480.5, 553.32, 679.74, 971.58]
        );
        assert_eq!(metal_frequencies(2)[..4], [304.26, 448.97, 789.28, 1114.3]);
        assert_eq!(shaper(49), [0.1, 0.7, 0.03, 0.07]);
        assert_eq!(config(85).f1, 51.0);
        assert_eq!(config(90).f1, 353.0);
        assert_eq!(config(92).f1, 11_100.0);
        assert_eq!(config(33).f1, 222.4);
        assert_eq!(config(32).f1, 61.0);
        assert_eq!(native_sweep(60), [1.68, 0.031]);
        assert_eq!(cowbell_pair_indices(46), [6, 7]);
        assert_eq!(metal_frequencies(0)[6..], [571.1, 838.15]);
        assert_eq!(metal_frequencies(3)[..2], [545.0, 775.0]);
    }

    /// Energy-weighted centre of `x` between `lo` and `hi`, on a 50-point grid.
    fn centroid(x: &[f32], fs: f32, lo: f32, hi: f32) -> f32 {
        let mut weighted = 0.0;
        let mut total = 0.0;
        for step in 0..50 {
            let hz = lo + (hi - lo) * step as f32 / 49.0;
            let (mut re, mut im) = (0.0, 0.0);
            for (n, sample) in x.iter().enumerate() {
                let t = n as f32;
                // Hann, so the grid reads one band rather than the whole spectrum's leakage.
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * t / x.len() as f32).cos();
                let angle = std::f32::consts::TAU * hz * t / fs;
                re += w * sample * angle.cos();
                im -= w * sample * angle.sin();
            }
            let power = re * re + im * im;
            weighted += hz * power;
            total += power;
        }
        weighted / total.max(1.0e-30)
    }

    /// One reference hit (velocity 0.82) of a supporting voice, `seconds` long.
    fn render(id: u8, fs: f32, seconds: f32) -> Vec<f32> {
        let mut sources = Sources::new();
        let mut voice = Voice::new();
        voice.set_sample_rate(fs);
        let patch = SlotPatch::default();
        voice.trigger(0.82, patch);
        (0..(fs * seconds) as usize)
            .map(|_| {
                let frame = sources.tick(fs)[machine(id)];
                voice.process(id, patch, frame, 120.0)
            })
            .collect()
    }

    /// The loudest isolated step in a ring, in dB over the median of the 25 ms around it. A real
    /// tick stands far above its neighbourhood; the absolute gate is what makes the measure mean
    /// anything, because an otherwise smooth ring's third difference sits at the f32 rounding
    /// floor and a purely relative measure reads 17–20 dB on a clean 51 Hz sine. Blocks whose
    /// third difference stays under 1e-5 of the peak — about 60 dB under an audible 5 kHz tick —
    /// are that floor and do not count.
    fn worst_tick_db(x: &[f32], fs: f32) -> (f32, f32) {
        let peak = x.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1.0e-12);
        let block = (fs / 1000.0) as usize;
        let blocks: Vec<f32> = x
            .windows(4)
            .map(|w| ((w[3] - 3.0 * w[2] + 3.0 * w[1] - w[0]) / peak).abs())
            .collect::<Vec<_>>()
            .chunks(block)
            .map(|c| c.iter().fold(0.0_f32, |m, s| m.max(*s)))
            .collect();
        let mut worst = (0.0_f32, 0.0_f32);
        for (index, level) in blocks.iter().enumerate().skip(5) {
            if *level <= 1.0e-5 {
                continue;
            }
            let low = index.saturating_sub(12);
            let high = (index + 13).min(blocks.len());
            let mut around: Vec<f32> = blocks[low..index]
                .iter()
                .chain(&blocks[index + 1..high])
                .copied()
                .collect();
            around.sort_by(f32::total_cmp);
            let median = around[around.len() / 2].max(1.0e-30);
            let prominence = 20.0 * (level / median).log10();
            if prominence > worst.0 {
                worst = (prominence, index as f32);
            }
        }
        worst
    }

    /// A ring that the catalogue does not deliberately chop must not tick after its onset. The
    /// owner heard a click on the FR-2L kick (2026-09-19, again on 2026-09-20) that no waveform
    /// showed; the voices here have no switching tick (`shaper`), strike train (`strike_train`) or
    /// restrike after their first milliseconds, so any isolated step in them is a defect. The 606
    /// kick and toms (47–50), the CR-8000 rimshot (40) and the diode toms (34, 35) are excluded:
    /// their ticks are fitted. The whole FR-2L family is in the list because that is where the
    /// owner heard it.
    #[test]
    fn smooth_rings_do_not_tick_after_their_onset() {
        for id in [
            32, 33, 36, 54, 60, 61, 68, 74, 85, 86, 87, 88, 89, 91, 93, 94,
        ] {
            let x = render(id, 48_000.0, 1.0);
            let (prominence, at_ms) = worst_tick_db(&x, 48_000.0);
            assert!(
                prominence <= 12.0,
                "model {id} ticks {prominence:.1} dB over its neighbourhood at {at_ms:.0} ms"
            );
        }
    }

    fn rms_db(x: &[f32], from_ms: f32, to_ms: f32, fs: f32) -> f32 {
        let window = &x[(from_ms * fs / 1000.0) as usize..(to_ms * fs / 1000.0) as usize];
        let power = window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32;
        10.0 * power.max(1.0e-30).log10()
    }

    #[test]
    fn metal_and_noise_voices_rise_instead_of_starting_at_their_peak() {
        // The acquired comparison recordings rise over 1–15 ms; a first sample at the peak was the
        // former instant envelope stepping a filter.
        for id in [41, 42, 43, 51, 52, 53, 56, 57, 58, 63, 64, 71, 82, 92] {
            let x = render(id, 48_000.0, 0.3);
            let peak = x.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            let first = x[0].abs().max(x[1].abs());
            assert!(
                first < 0.25 * peak,
                "model {id}: first {first}, peak {peak}"
            );
        }
    }

    #[test]
    fn noise_filters_reach_centres_above_the_old_chamberlin_ceiling() {
        use mxm_measure::spectrum::{Probe, transfer_gain};
        // A 10 kHz, Q 4 band-pass at 48 kHz: unity at its centre, and the old ~7.3 kHz ceiling now
        // sits on its skirt.
        for (hz, low, high) in [(10_000.0, 0.95, 1.05), (7_300.0, 0.0, 0.4)] {
            let mut filter = Svf::new();
            filter.tune(48_000.0, 10_000.0, 4.0);
            let gain = transfer_gain(Probe::new(hz, 0.25, 0.05, 200.0, 48_000.0), 48_000.0, |x| {
                filter.process(x).1
            })
            .expect("a measured gain");
            assert!((low..=high).contains(&gain), "{hz} Hz: {gain}");
        }
    }

    #[test]
    fn eight_thousand_hats_are_bright_and_the_snare_is_body() {
        use mxm_measure::spectrum::fft;
        let spectrum = |x: &[f32]| {
            let n = 8_192;
            let mut re: Vec<f64> = x[..n].iter().map(|s| f64::from(*s)).collect();
            let mut im = vec![0.0; n];
            fft(&mut re, &mut im).expect("a power-of-two transform");
            (0..n / 2)
                .map(|k| {
                    (
                        k as f64 * 48_000.0 / n as f64,
                        re[k] * re[k] + im[k] * im[k],
                    )
                })
                .collect::<Vec<_>>()
        };
        // The CR-8000 closed hat's recording has its centroid near 10.6 kHz.
        let hat = spectrum(&render(42, 48_000.0, 0.2));
        let centroid =
            hat.iter().map(|(f, p)| f * p).sum::<f64>() / hat.iter().map(|b| b.1).sum::<f64>();
        assert!(centroid > 9_000.0, "closed hat centroid {centroid} Hz");
        // The CR-8000 snare's recording is almost pure 223 Hz body.
        let snare = spectrum(&render(33, 48_000.0, 0.2));
        let band = |lo: f64, hi: f64| {
            snare
                .iter()
                .filter(|(f, _)| (lo..hi).contains(f))
                .map(|b| b.1)
                .sum::<f64>()
        };
        let ratio_db = 10.0 * (band(150.0, 300.0) / band(2_000.0, 20_000.0)).log10();
        assert!(ratio_db > 20.0, "snare body over wire: {ratio_db} dB");
    }

    #[test]
    fn gated_and_two_stage_envelopes_keep_their_recorded_shapes() {
        let fs = 48_000.0;
        // Cowbell: a fast first fall, then a slow tail.
        let bell = render(46, fs, 0.4);
        assert!(rms_db(&bell, 0.0, 5.0, fs) - rms_db(&bell, 40.0, 50.0, fs) > 15.0);
        assert!(rms_db(&bell, 40.0, 50.0, fs) - rms_db(&bell, 140.0, 150.0, fs) < 12.0);
        // Tambourine: the gate holds about 200 ms, then releases.
        let tambourine = render(71, fs, 0.4);
        let held = rms_db(&tambourine, 5.0, 15.0, fs) - rms_db(&tambourine, 150.0, 160.0, fs);
        let released = rms_db(&tambourine, 5.0, 15.0, fs) - rms_db(&tambourine, 300.0, 310.0, fs);
        assert!(
            held < 8.0 && released > 35.0,
            "held {held} dB, released {released} dB"
        );
        // DR-110 clap: separate bursts, quiet between them.
        let clap = render(59, fs, 0.1);
        assert!(rms_db(&clap, 0.0, 2.0, fs) - rms_db(&clap, 8.0, 10.0, fs) > 15.0);
    }

    #[test]
    fn long_guiro_scrapes_slowly_then_fast() {
        let fs = 48_000.0;
        let x = render(72, fs, 0.9);
        let peak = x.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let mut onsets = Vec::new();
        let mut last = -1.0_f32;
        for (n, s) in x.iter().enumerate() {
            let t = n as f32 / fs;
            if s.abs() > 0.2 * peak && t - last > 0.004 {
                onsets.push(t);
                last = t;
            }
        }
        let period = |from: f32, to: f32| {
            let within: Vec<f32> = onsets
                .iter()
                .copied()
                .filter(|t| (from..to).contains(t))
                .collect();
            (within[within.len() - 1] - within[0]) / (within.len() - 1) as f32
        };
        let slow = period(0.05, 0.5);
        let fast = period(0.6, 0.8);
        assert!(
            (1.0 / slow - 77.0).abs() < 5.0,
            "slow rate {} Hz",
            1.0 / slow
        );
        assert!(
            (1.0 / fast - 125.0).abs() < 8.0,
            "fast rate {} Hz",
            1.0 / fast
        );
    }

    #[test]
    fn classic_kick_sweeps_down_into_its_rest_pitch() {
        // Half-cycles between successive zero crossings: the first is short, the ring settles.
        let fs = 48_000.0;
        let x = render(60, fs, 0.3);
        let crossings: Vec<f32> = (1..x.len())
            .filter(|&n| (x[n - 1] < 0.0) != (x[n] < 0.0))
            .map(|n| n as f32 / fs)
            .collect();
        let early = 0.5 / (crossings[1] - crossings[0]);
        let late = 0.5 / (crossings[11] - crossings[10]);
        assert!(early > 1.15 * late, "early {early} Hz, late {late} Hz");
        assert!((late - 59.4).abs() < 1.5, "late {late} Hz");
    }

    #[test]
    fn every_supporting_voice_is_finite_bounded_and_parks_at_every_rate() {
        for fs in [44_100.0_f32, 96_000.0, 192_000.0] {
            let mut sources = Sources::new();
            for id in 32..=94 {
                let mut voice = Voice::new();
                voice.set_sample_rate(fs);
                let patch = SlotPatch::default();
                voice.trigger(0.82, patch);
                let limit = (fs * 12.0) as usize;
                let mut n = 0;
                while voice.is_active() && n < limit {
                    let frame = sources.tick(fs)[machine(id)];
                    let y = voice.process(id, patch, frame, 120.0);
                    assert!(
                        y.is_finite() && y.abs() <= 1.0 / OUTPUT_HEADROOM,
                        "model {id} at {fs}: {y}"
                    );
                    n += 1;
                }
                assert!(
                    !voice.is_active(),
                    "model {id} still sounding at {fs} Hz after 12 s"
                );
            }
        }
    }

    #[test]
    fn machine_sources_are_deterministic_and_separate() {
        let mut a = Sources::new();
        let mut b = Sources::new();
        for _ in 0..100 {
            let x = a.tick(48000.);
            assert_eq!(
                x.iter().map(|f| f.noise).collect::<Vec<_>>(),
                b.tick(48000.).iter().map(|f| f.noise).collect::<Vec<_>>()
            );
            assert_ne!(x[0].noise, x[1].noise)
        }
    }
}

#[cfg(test)]
mod gating_tests {
    use super::*;

    fn ticked(samples: u64, fs: f32) -> Sources {
        let mut sources = Sources::new();
        for _ in 0..samples {
            let _ = sources.tick(fs);
        }
        sources
    }

    #[test]
    fn advancing_lands_exactly_where_ticking_does() {
        // Six machines, forty-eight squares and six noise sources. A resume on the wrong sample
        // would move every supporting model's metal and break the correlation two voices reading
        // one source are supposed to have.
        let fs = 48_000.0;
        for samples in [1_u64, 4, 5, 6, 97, 1_000, 48_000] {
            let mut jumped = Sources::new();
            jumped.advance(samples, fs);
            let stepped = ticked(samples, fs);
            assert_eq!(jumped.noise, stepped.noise, "noise differs after {samples}");
            for machine in 0..6 {
                for oscillator in 0..OSCILLATORS {
                    assert_eq!(
                        jumped.squares[machine][oscillator].phase,
                        stepped.squares[machine][oscillator].phase,
                        "machine {machine} oscillator {oscillator} phase after {samples}"
                    );
                    assert_eq!(
                        jumped.squares[machine][oscillator].pending,
                        stepped.squares[machine][oscillator].pending,
                        "machine {machine} oscillator {oscillator} window after {samples}"
                    );
                }
            }
        }
    }

    #[test]
    fn what_it_renders_next_is_identical_too() {
        let fs = 48_000.0;
        let gap = 2_000;
        let mut jumped = Sources::new();
        jumped.advance(gap, fs);
        let mut stepped = ticked(gap, fs);
        for frame in 0..512 {
            let a = jumped.tick(fs);
            let b = stepped.tick(fs);
            for machine in 0..6 {
                assert_eq!(a[machine].noise, b[machine].noise, "frame {frame} noise");
                assert_eq!(a[machine].metal, b[machine].metal, "frame {frame} metal");
                assert_eq!(
                    a[machine].oscillators, b[machine].oscillators,
                    "frame {frame} oscillators"
                );
            }
        }
    }

    #[test]
    fn a_gap_in_two_halves_equals_one_gap() {
        let fs = 48_000.0;
        let mut split = Sources::new();
        split.advance(700, fs);
        split.advance(1_300, fs);
        let mut whole = Sources::new();
        whole.advance(2_000, fs);
        assert_eq!(split.noise, whole.noise);
        for machine in 0..6 {
            for oscillator in 0..OSCILLATORS {
                assert_eq!(
                    split.squares[machine][oscillator].phase,
                    whole.squares[machine][oscillator].phase
                );
            }
        }
    }
}
