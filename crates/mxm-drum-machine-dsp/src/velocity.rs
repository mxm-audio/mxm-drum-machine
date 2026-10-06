//! **What Dynamics means, on every model family** — the velocity curve's exponent, from one mapping
//! (`plans/plan-modulation-standard.md`, the owner's ruling of 2026-09-26: every modulation means
//! the same). Before it, Dynamics bent the velocity curve in the families with a floor under the hit
//! (three different mappings), and in the families whose level is linear in velocity it was a
//! −3.7…0 dB trim that did not touch the curve at all.
//!
//! **At Dynamics 0 every family keeps its exact law and level**, and every kit ships at 0, so no
//! kit moved. Positive Dynamics flattens the curve (soft hits come up), negative steepens it.
//!
//! - A family with a floor keeps it: `floor + (1 − floor) · v^e`.
//! - A family linear in velocity takes the curve on its velocity term: [`linear`].
//! - A frozen capture, which holds one strike, scales its buffer by [`curve`].

/// The velocity curve's exponent for a Dynamics value: `1 − 0.65·d` above zero, `1 − 2·d` below —
/// the mapping most families already had. A non-finite value is zero.
#[must_use]
pub fn exponent(dynamics: f32) -> f32 {
    let dynamics = if dynamics.is_finite() {
        dynamics.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    if dynamics >= 0.0 {
        1.0 - 0.65 * dynamics
    } else {
        1.0 - 2.0 * dynamics
    }
}

/// `v^e`, with `v` clamped to `0..=1` (a non-finite velocity is silence). **Exactly `v` at an
/// exponent of one**, so a linear family at Dynamics 0 computes what it always computed.
#[must_use]
pub fn curve(velocity: f32, dynamics: f32) -> f32 {
    let velocity = if velocity.is_finite() {
        velocity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let exponent = exponent(dynamics);
    if exponent == 1.0 {
        velocity
    } else {
        velocity.powf(exponent)
    }
}

/// The level a family linear in velocity plays a full-velocity hit at: what its trim was at
/// Dynamics 0, written as that expression so the constant is the same number to the bit.
pub const LINEAR_LEVEL: f32 = 0.65 + 0.35 * (0.5 + 0.5 * 0.0);

/// A linear family's hit level: the curve on its velocity term, at its fixed level.
#[must_use]
pub fn linear(velocity: f32, dynamics: f32) -> f32 {
    curve(velocity, dynamics) * LINEAR_LEVEL
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Dynamics 0 is the identity**, to the bit: the linear families' old `v × trim` and the
    /// frozen reader's `v`.
    #[test]
    fn dynamics_zero_is_the_old_law_to_the_bit() {
        for step in 0..=1000 {
            let v = step as f32 / 1000.0;
            let old_trim = 0.65 + 0.35 * (0.5 + 0.5 * 0.0f32);
            assert_eq!(linear(v, 0.0).to_bits(), (v * old_trim).to_bits());
            assert_eq!(curve(v, 0.0).to_bits(), v.to_bits());
            assert_eq!(curve(v, -0.0).to_bits(), v.to_bits());
        }
    }

    /// Positive Dynamics brings a soft hit up, negative takes it down, and the extremes stay in
    /// range.
    #[test]
    fn dynamics_bends_the_curve_one_way_everywhere() {
        let soft = |d: f32| curve(0.25, d);
        assert!(soft(-1.0) < soft(0.0) && soft(0.0) < soft(1.0));
        for d in [-1.0f32, -0.3, 0.0, 0.4, 1.0, f32::NAN] {
            assert_eq!(curve(1.0, d), 1.0, "a full hit is full at {d}");
            assert_eq!(curve(0.0, d), 0.0);
        }
    }
}
