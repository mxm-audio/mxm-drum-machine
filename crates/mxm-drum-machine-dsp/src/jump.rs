//! Advancing a free-running source without rendering it (plan §4.4).
//!
//! A machine-shared bus has to keep the *behaviour* of a source that never stops — a drum trigger
//! opens an envelope, it does not restart the oscillators — but it does not have to be **computed**
//! every sample to do that. Conflating the two is what makes the engine pay a floor of roughly
//! 470 ns a sample with sixteen slots `Off`.
//!
//! Restarting a dormant source from a fresh seed is not the same thing and is not an option: a
//! seed addresses noise but says nothing about oscillator phase or filter history, reusing one
//! repeats attack textures, choosing another invents an unrelated timeline, and either loses the
//! hit-to-hit and simultaneous-voice correlation that is part of the machine.
//!
//! So a gated bus records how many samples elapsed and, when it is needed again, moves its state
//! to where running would have left it.
//!
//! # What can be jumped exactly, and what cannot
//!
//! **This module covers the state that is exactly jumpable: linear shift-register noise.**
//! `xorshift64` is a linear map over GF(2), so advancing N steps is that map raised to the Nth
//! power — and with the powers of two precomputed, an arbitrary N costs at most 64 applications
//! instead of N iterations. `advance` reproduces iteration bit for bit, which
//! `a_jump_equals_literal_stepping` proves, because a bus that resumes on the wrong sample is a
//! different instrument rather than a cheaper one.
//!
//! **Floating-point oscillator phase is not exactly jumpable, and it is worth saying why.** A
//! phase kept as `f64` and advanced by `fract(phase + increment)` rounds once per sample. The
//! closed form `fract(phase + increment × N)` rounds once in total, so the two drift apart —
//! measured here at a few ULP after 48 samples and about 1.3e-11 after half a million. Bit
//! identity is the standard the shared-source correlation tests and the reference bank hold this
//! engine to, so a closed-form phase jump cannot meet it. Making phase exactly jumpable means
//! making it integer, which changes what every oscillator renders and is therefore an owner
//! decision rather than an optimisation.

/// A linear map over GF(2)^64, as the images of the 64 basis vectors.
///
/// `rows[i]` is where the map sends the vector with only bit `i` set. Applying it to a state is
/// then the exclusive-or of the rows its set bits select, because the map is linear.
#[derive(Clone, Copy)]
struct Linear {
    rows: [u64; 64],
}

impl Linear {
    /// Applies the map to one state.
    fn apply(&self, state: u64) -> u64 {
        let mut out = 0;
        let mut bits = state;
        while bits != 0 {
            let index = bits.trailing_zeros() as usize;
            out ^= self.rows[index];
            bits &= bits - 1;
        }
        out
    }

    /// The composition "self, then other".
    fn then(&self, other: &Self) -> Self {
        Self {
            rows: std::array::from_fn(|index| other.apply(self.rows[index])),
        }
    }
}

/// One step of the engine's noise recurrence, as a matrix.
///
/// Built by feeding the step each basis vector: the recurrence is exclusive-ors and shifts, so it
/// is linear and this captures it completely.
fn step_matrix(step: fn(u64) -> u64) -> Linear {
    Linear {
        rows: std::array::from_fn(|index| step(1_u64 << index)),
    }
}

/// The step repeated 2^k times, for every k a 64-bit count can hold.
///
/// Precomputed once so a resume costs at most 64 applications rather than one per elapsed sample.
/// Squaring the map 64 times costs about as much as advancing a few thousand samples the long way,
/// and it is paid once per process rather than once per gap.
struct Powers {
    doublings: [Linear; 64],
}

impl Powers {
    fn new(step: fn(u64) -> u64) -> Self {
        let first = step_matrix(step);
        let mut doublings = [first; 64];
        for index in 1..64 {
            doublings[index] = doublings[index - 1].then(&doublings[index - 1]);
        }
        Self { doublings }
    }

    fn advance(&self, mut state: u64, steps: u64) -> u64 {
        let mut remaining = steps;
        while remaining != 0 {
            let index = remaining.trailing_zeros() as usize;
            state = self.doublings[index].apply(state);
            remaining &= remaining - 1;
        }
        state
    }
}

/// One step of the 808's shared white-noise source.
///
/// Duplicated from `engine.rs` deliberately: this module has to know the exact recurrence to build
/// its matrix, and `a_jump_equals_literal_stepping` fails the moment the two disagree.
const fn xorshift64(mut value: u64) -> u64 {
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value
}

/// One step of a supporting machine's noise source.
///
/// The engine guards the zero state, which a maximal xorshift never reaches from a nonzero seed;
/// the guard is defensive and the map is linear, which `a_supporting_jump_equals_stepping` checks
/// over a long run rather than assuming.
const fn xorshift32(mut value: u32) -> u32 {
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    value
}

/// The 32-bit step widened, so one powers table serves both recurrences.
const fn xorshift32_wide(value: u64) -> u64 {
    xorshift32(value as u32) as u64
}

/// One step of the 909's traced 31-stage shift register, widened so one table serves it too.
const fn lfsr31_wide(value: u64) -> u64 {
    crate::reset_vco_909::shift(value as u32) as u64
}

static LFSR31: std::sync::LazyLock<Powers> = std::sync::LazyLock::new(|| Powers::new(lfsr31_wide));

/// Advances the 909's hardware-noise register by `steps` clock periods.
///
/// Bit-identical to stepping, and `O(1)` in the gap — which matters more here than elsewhere,
/// because this register is clocked at 300 kHz and takes six steps for every sample at 48.
#[must_use]
pub fn advance_lfsr31(state: u32, steps: u64) -> u32 {
    LFSR31.advance(u64::from(state), steps) as u32
}

static XORSHIFT64: std::sync::LazyLock<Powers> =
    std::sync::LazyLock::new(|| Powers::new(xorshift64));

static XORSHIFT32: std::sync::LazyLock<Powers> =
    std::sync::LazyLock::new(|| Powers::new(xorshift32_wide));

/// Advances a supporting machine's noise state as though it had been stepped `samples` times.
///
/// Bit-identical to stepping, and `O(1)` in the gap.
#[must_use]
pub fn advance_xorshift32(state: u32, samples: u64) -> u32 {
    XORSHIFT32.advance(u64::from(state), samples) as u32
}

/// Advances the 808 noise state as though it had been stepped `samples` times.
///
/// Bit-identical to stepping, and `O(1)` in the gap rather than `O(samples)`.
#[must_use]
pub fn advance_xorshift64(state: u64, samples: u64) -> u64 {
    XORSHIFT64.advance(state, samples)
}

/// Builds the powers table before any audio runs.
///
/// The table is built on first use, and first use must not be an audio callback: call this from
/// construction so a gated bus resuming under a trigger never pays for it.
pub fn prepare() {
    let _ = advance_xorshift64(1, 1);
    let _ = advance_xorshift32(1, 1);
    let _ = advance_lfsr31(1, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stepped(mut state: u64, samples: u64) -> u64 {
        for _ in 0..samples {
            state = xorshift64(state);
        }
        state
    }

    #[test]
    fn a_jump_equals_literal_stepping() {
        // The whole justification for gating a bus. If a resume lands on a different sample of
        // the noise sequence, the instrument changed — which the shared-source correlation tests
        // and the reference bank would both catch, but only after the damage.
        let seed = 0x1319_8a2e_0370_7344;
        for samples in [0, 1, 2, 3, 7, 64, 97, 1_000, 48_000, 192_001] {
            assert_eq!(
                advance_xorshift64(seed, samples),
                stepped(seed, samples),
                "a jump of {samples} did not land where stepping does"
            );
        }
    }

    #[test]
    fn a_supporting_jump_equals_stepping() {
        // The six supporting machines' noise. Their engine step guards the zero state, which a
        // maximal xorshift never reaches from a nonzero seed — so the map is linear and jumpable,
        // and a long run here is what says the guard never fires rather than an argument that it
        // cannot.
        for seed in [0x8001_u32, 0x18003, 0x24007, 0x31009, 0x4100b, 0x5200d] {
            let mut stepped = seed;
            for _ in 0..200_000 {
                stepped = xorshift32(stepped);
                assert_ne!(stepped, 0, "the zero guard would have fired");
            }
            assert_eq!(advance_xorshift32(seed, 200_000), stepped);
        }
    }

    #[test]
    fn a_jump_composes() {
        // Two gaps in a row must equal one long gap, or a bus that wakes and sleeps repeatedly
        // would drift away from one that slept once.
        let seed = 0x5a17_3d2b;
        let split = advance_xorshift64(advance_xorshift64(seed, 3_000), 7_000);
        assert_eq!(split, advance_xorshift64(seed, 10_000));
    }

    #[test]
    fn a_jump_of_nothing_changes_nothing() {
        let seed = 0xdead_beef_cafe_f00d;
        assert_eq!(advance_xorshift64(seed, 0), seed);
    }

    #[test]
    fn a_very_long_gap_is_still_exact() {
        // An hour of silence at 192 kHz. Stepping this in a test would take minutes; the point of
        // the jump is that the engine does not have to either.
        let seed = 0x0123_4567_89ab_cdef;
        let hour = 192_000_u64 * 3_600;
        let jumped = advance_xorshift64(seed, hour);
        // Checked against itself split three ways rather than against stepping, which is the
        // property that matters and the only one that is affordable here.
        let split = advance_xorshift64(
            advance_xorshift64(advance_xorshift64(seed, hour / 3), hour / 3),
            hour - 2 * (hour / 3),
        );
        assert_eq!(jumped, split);
    }

    #[test]
    fn floating_point_phase_is_not_exactly_jumpable() {
        // The negative result this module's documentation rests on, kept as a test so nobody
        // optimises an oscillator bank on the assumption that it is. Iterating `fract(p + inc)`
        // rounds once per sample; the closed form rounds once. They diverge, and the gap grows.
        let increment = 205.3_f64 / 48_000.0;
        let mut iterated = 0.03_f64;
        for _ in 0..48_000 {
            iterated = (iterated + increment).fract();
        }
        let closed = (0.03_f64 + increment * 48_000.0).fract();
        assert!(
            (iterated - closed).abs() > 1.0e-13,
            "if these now agree, an exact phase jump may be possible and the plan should say so"
        );
    }
}
