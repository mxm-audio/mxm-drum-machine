//! The guards every circuit here puts at its edges: a non-finite control falls back, a bounded one is
//! clamped, and recursive state is flushed to exact zero before it goes subnormal.
//!
//! **One copy each.** They were written into each circuit's module as it was built — the same
//! `finite_or` thirteen times under two names — which `AGENTS.md`'s *extract within this crate once
//! two completed circuits share an API* already asked to be gathered. Bit-identical to every copy
//! they replace, so no render moves. `legacy`'s f64 flush at `1e-30` is a different threshold and
//! stays in `legacy`.

/// `value`, or `fallback` if it is not finite.
#[inline]
#[must_use]
pub(crate) fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// A unit control: non-finite reads as 0, and the rest is held to `0..=1`.
#[inline]
#[must_use]
pub(crate) fn finite_unit(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
}

/// A bipolar control: non-finite reads as 0, and the rest is held to `-1..=1`.
#[inline]
#[must_use]
pub(crate) fn finite_bipolar(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(-1.0, 1.0)
}

/// Flushes subnormal recursive state to exact zero.
#[inline]
#[must_use]
pub(crate) fn flush(value: f32) -> f32 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

/// [`flush`] for the circuits that keep their state in `f64`, at the same threshold.
#[inline]
#[must_use]
pub(crate) fn flush_f64(value: f64) -> f64 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}
