//! Framework-free sound engine for `mxm-drum-machine`.
//!
//! The implementation contract is `docs/briefs/mxm-drum-machine.md`. The initial work establishes
//! stable model identity and circuit primitives before the complete priority-machine voices land.

#![forbid(unsafe_code)]

mod analogue_909;
pub mod capture;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod deep_bridge_kick;
mod economy_55;
pub mod engine;
pub mod falling_drum;
pub mod jump;
mod legacy;
pub mod metal_808;
pub mod model;
pub mod noise_percussion;
mod pcm_909;
pub mod reset_vco_909;
pub mod resonator;
pub mod rim_clave;
pub mod routing;
pub mod twin_mode_snare;
pub mod velocity;

/// Number of simultaneously addressable drum circuits.
pub const SLOT_COUNT: usize = 16;
/// MIDI note assigned to slot 0.
pub const FIRST_NOTE: u8 = 36;
/// MIDI note assigned to slot 15.
pub const LAST_NOTE: u8 = FIRST_NOTE + SLOT_COUNT as u8 - 1;

/// Converts one of the instrument's fixed MIDI notes to its slot index.
#[inline]
#[must_use]
pub const fn slot_for_note(note: u8) -> Option<usize> {
    if note >= FIRST_NOTE && note <= LAST_NOTE {
        Some((note - FIRST_NOTE) as usize)
    } else {
        None
    }
}

/// Converts a valid slot index to its fixed MIDI note.
#[inline]
#[must_use]
pub const fn note_for_slot(slot: usize) -> Option<u8> {
    if slot < SLOT_COUNT {
        Some(FIRST_NOTE + slot as u8)
    } else {
        None
    }
}

/// A coefficient held against the exact inputs it was derived from.
///
/// A caller hands over every input the expression reads, so a hit returns the held value and a miss
/// rebuilds it from that same expression. The result is bit-identical to recomputing it every
/// sample, which is what lets a site adopt this without moving an approved render. Because the key
/// carries every input — the sample rate included — nothing has to remember to invalidate it: a
/// rate change, a new tuning or a moved control all move the key on their own, and a `NaN` input
/// never compares equal, so it simply rebuilds.
///
/// **Where to use it.** On an always-on path — anything `Engine::process_routed` ticks whatever is
/// loaded, such as the machine-shared banks — hold every coefficient that is not moving. Inside a
/// voice, hold one where it is free to do so, but do not add this state to a voice on the strength
/// of the rule alone: holding costs a tuple comparison, a branch and a read from a larger struct,
/// which is the same order as the `exp` it replaces. On sixteen continuously running voices the
/// whole of this crate's per-voice holding measured inside the run-to-run spread, so several sites
/// are deliberately left rebuilding. The crate's AGENTS.md carries the policy and its NOTES.md
/// (*Realtime and numeric rules*) the figures; add a site when a measurement on a scene where the
/// voices are running says it is worth it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Coefficient<K, V> {
    held: Option<(K, V)>,
}

impl<K: PartialEq + Copy, V: Copy> Coefficient<K, V> {
    pub(crate) const fn new() -> Self {
        Self { held: None }
    }

    /// Returns the coefficient for `key`, building it only when `key` has moved.
    #[inline]
    pub(crate) fn get(&mut self, key: K, build: impl FnOnce(K) -> V) -> V {
        if let Some((held, value)) = self.held {
            if held == key {
                return value;
            }
        }
        let value = build(key);
        self.held = Some((key, value));
        value
    }
}

impl<K: PartialEq + Copy, V: Copy> Default for Coefficient<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Flushes subnormal recursive state to exact zero.
#[inline]
#[must_use]
pub(crate) fn flush(value: f32) -> f32 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_36_through_51_map_bijectively_to_sixteen_slots() {
        assert_eq!(slot_for_note(35), None);
        assert_eq!(slot_for_note(52), None);
        for slot in 0..SLOT_COUNT {
            let note = note_for_slot(slot).expect("valid slot");
            assert_eq!(slot_for_note(note), Some(slot));
        }
        assert_eq!(note_for_slot(SLOT_COUNT), None);
    }
}
