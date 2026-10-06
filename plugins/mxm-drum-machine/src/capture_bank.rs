//! Handing a rendered kit from the background task to the audio thread (plan §4.7, D9).
//!
//! Engaging Resample while the plugin is running arrives as a parameter change inside `process`,
//! where seconds of audio cannot be rendered. The render therefore happens on nice-plug's
//! background executor and the result has to cross one thread boundary — and
//! [`../../AGENTS.md`](../../AGENTS.md) allows no lock, no allocation and no destructor on the
//! audio side of that crossing.
//!
//! # Why this is not the sampler's `AssetBank`
//!
//! That bank lends an immutable snapshot to every `process` call, so it needs reader counts to
//! know when a retired payload is unreachable. **Nothing is lent here.** The audio thread *takes*
//! the kit, owns it outright inside the engine, and hands the displaced one back. One producer,
//! one consumer, ownership moving in a ring:
//!
//! ```text
//!   worker ──Box::into_raw──► ready ──take──► audio ──engine──► displaced
//!      ▲                                                            │
//!      └──────────────────── retired ◄──────────────────────────────┘
//! ```
//!
//! So the protocol is two pointer swaps and no shared borrow at all, which is the whole reason it
//! fits in a hundred lines instead of two thousand. The ring is bounded by one simple rule and one
//! count. The rule: a ready kit **waits** while any retired one is still unfreed. The count: **two
//! retired slots**, because between two drains the audio thread can displace a kit twice — once
//! by taking a new one, which the rule makes wait for empty slots, and once by disengaging
//! Resample, which must be immediate and so cannot wait. After disengaging it stays live until it
//! takes again (refused while anything is retired) or is activated (which drains first), so a
//! third displacement cannot come before a drain, and the audio thread always has somewhere to put
//! what it displaces and never has to free anything itself.
//!
//! One slot was the first build's count, and it was one short: a stale kit refused while frozen,
//! or a second kit installed in one engage, left the slot full with no capture coming to drain
//! it, and the next disengage found nowhere to go — a debug assertion, or a leaked kit in release
//! (`a_take_and_a_disengage_between_two_drains_both_find_room`).
//!
//! # What each side may do
//!
//! The audio thread only ever **swaps a pointer and moves a value**: no allocation, no lock, and
//! critically no `Drop` — a displaced kit is megabytes of `Vec<f32>` and freeing it in `process`
//! is exactly the stall this indirection exists to prevent. Both allocation and deallocation
//! happen on the worker.

use std::sync::atomic::{AtomicPtr, Ordering};

use mxm_drum_machine_dsp::capture::KitCapture;

/// How many displaced kits can wait for the worker at once: one taken over, one disengaged — the
/// module documentation's count.
const RETIRED_SLOTS: usize = 2;

/// The one-slot handoff between the capture worker and the audio thread.
#[derive(Debug)]
pub struct CaptureBank {
    /// Rendered and waiting for the audio thread to take it.
    ready: AtomicPtr<KitCapture>,
    /// Displaced by a newer kit or by disengaging, waiting for the worker to free them.
    retired: [AtomicPtr<KitCapture>; RETIRED_SLOTS],
}

impl Default for CaptureBank {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBank {
    /// An empty bank.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ready: AtomicPtr::new(std::ptr::null_mut()),
            retired: [
                AtomicPtr::new(std::ptr::null_mut()),
                AtomicPtr::new(std::ptr::null_mut()),
            ],
        }
    }

    /// Offers a freshly rendered kit. **Worker thread.**
    ///
    /// Latest wins: a kit still waiting when a newer one arrives is freed here, on this thread,
    /// because it was never reachable from audio.
    pub fn publish(&self, kit: KitCapture) {
        let raw = Box::into_raw(Box::new(kit));
        let previous = self.ready.swap(raw, Ordering::AcqRel);
        if !previous.is_null() {
            // SAFETY: a non-null `ready` was published by this thread and never taken, so this is
            // the only live pointer to it and it came from `Box::into_raw`.
            drop(unsafe { Box::from_raw(previous) });
        }
    }

    /// Takes a waiting kit, if one is ready **and every retired slot is empty**. **Audio thread.**
    ///
    /// The second condition is half the bound: a ready kit simply waits while the worker still
    /// has an older one to free, so the kit this take displaces has a slot, and so does the one a
    /// disengage may displace after it — [`Self::retire`] never has to choose between leaking and
    /// freeing on the audio thread. A burst of edits collapses to the latest in `ready`, so
    /// waiting costs freshness, not correctness.
    #[must_use]
    pub fn take(&self) -> Option<Box<KitCapture>> {
        if self
            .retired
            .iter()
            .any(|slot| !slot.load(Ordering::Acquire).is_null())
        {
            return None;
        }
        let raw = self.ready.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if raw.is_null() {
            return None;
        }
        // SAFETY: the swap makes this the only pointer to a value `publish` leaked from a `Box`,
        // and `publish` never writes over a pointer it has already handed out.
        Some(unsafe { Box::from_raw(raw) })
    }

    /// Parks a displaced kit for the worker to free. **Audio thread.**
    ///
    /// At most two compare-and-swaps and no `Drop`: freeing megabytes of `Vec<f32>` in `process`
    /// is the stall this type exists to prevent. A slot is free by construction — the module
    /// documentation's count.
    pub fn retire(&self, kit: Box<KitCapture>) {
        let raw = Box::into_raw(kit);
        let parked = self.retired.iter().any(|slot| {
            slot.compare_exchange(
                std::ptr::null_mut(),
                raw,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        });
        // Unreachable under the count. Leaked rather than freed if it ever is, because freeing
        // here is the one thing the audio thread must never do — and a leak that needs a broken
        // invariant is better than a stall that does not.
        debug_assert!(
            parked,
            "a third kit displaced before the worker drained the two already retired"
        );
    }

    /// Frees whatever the audio thread has retired. **Worker or control thread.**
    pub fn drain(&self) {
        for slot in &self.retired {
            let raw = slot.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !raw.is_null() {
                // SAFETY: the swap takes sole ownership of a pointer `retire` leaked from a `Box`.
                drop(unsafe { Box::from_raw(raw) });
            }
        }
    }
}

impl Drop for CaptureBank {
    fn drop(&mut self) {
        for slot in std::iter::once(&self.ready).chain(&self.retired) {
            let raw = slot.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !raw.is_null() {
                // SAFETY: sole ownership at drop; both slots only ever hold leaked `Box`es.
                drop(unsafe { Box::from_raw(raw) });
            }
        }
    }
}

// SAFETY: every access is an atomic swap, and ownership of each `KitCapture` is held by exactly
// one side at a time — the ring in this module's documentation. `KitCapture` is `Send`.
unsafe impl Send for CaptureBank {}
unsafe impl Sync for CaptureBank {}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_drum_machine_dsp::SLOT_COUNT;
    use mxm_drum_machine_dsp::capture::capture_kit;
    use mxm_drum_machine_dsp::engine::SlotPatch;
    use mxm_drum_machine_dsp::model::ModelId;

    fn kit() -> KitCapture {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        capture_kit(&patches, 48_000.0).expect("a kit")
    }

    #[test]
    fn an_empty_bank_hands_out_nothing() {
        let bank = CaptureBank::new();
        assert!(bank.take().is_none());
        bank.drain();
    }

    #[test]
    fn a_published_kit_is_taken_once() {
        let bank = CaptureBank::new();
        bank.publish(kit());
        assert!(bank.take().is_some(), "the audio thread takes it");
        assert!(bank.take().is_none(), "and it is gone, not lent");
    }

    #[test]
    fn the_latest_publication_wins_and_the_older_is_freed() {
        // A burst of edits must not queue a chain of kits: only the newest matters, and the ones
        // it overtook were never reachable from audio, so freeing them here is correct.
        let bank = CaptureBank::new();
        bank.publish(kit());
        bank.publish(kit());
        bank.publish(kit());
        assert!(bank.take().is_some());
        assert!(bank.take().is_none());
    }

    #[test]
    fn a_retired_kit_is_freed_by_the_worker_not_the_audio_thread() {
        let bank = CaptureBank::new();
        bank.publish(kit());
        let taken = bank.take().expect("a kit");
        bank.retire(taken);
        // Nothing has been freed yet: the audio thread only parked it.
        bank.drain();
        // Draining twice is harmless, which is what lets the worker drain unconditionally.
        bank.drain();
    }

    #[test]
    fn a_ready_kit_waits_while_a_retired_one_is_unfreed() {
        // This is what bounds the ring: without it the audio thread could displace a second kit
        // with nowhere to put it, and would have to choose between leaking and freeing in
        // `process`.
        let bank = CaptureBank::new();
        bank.publish(kit());
        let first = bank.take().expect("a kit");
        bank.retire(first);
        bank.publish(kit());
        assert!(
            bank.take().is_none(),
            "nothing is handed out while the worker still owes a free"
        );
        bank.drain();
        assert!(
            bank.take().is_some(),
            "and it is handed out once there is room"
        );
    }

    /// **A take and a disengage between two drains both find room.** The audio thread takes a
    /// kit over an installed one and retires it, then Resample is turned off and it retires the
    /// kit it had just installed — with no drain in between, as when the worker's own drain ran
    /// before the take. Both must park, and nothing more is handed out until the worker drains.
    ///
    /// Falsified before trusted: with one retired slot the second retire trips the assertion.
    #[test]
    fn a_take_and_a_disengage_between_two_drains_both_find_room() {
        let bank = CaptureBank::new();
        bank.publish(kit());
        let installed = bank.take().expect("the first kit");
        bank.publish(kit());
        let replacement = bank.take().expect("the second kit, taken over the first");
        bank.retire(installed);
        // Resample off: the kit that was playing is displaced too.
        bank.retire(replacement);
        bank.publish(kit());
        assert!(
            bank.take().is_none(),
            "nothing is handed out while either slot still owes a free"
        );
        bank.drain();
        assert!(bank.take().is_some(), "and both are freed by one drain");
    }

    #[test]
    fn dropping_the_bank_frees_every_slot() {
        // Proved by Miri or a leak checker in principle; here it is at least exercised, so the
        // `Drop` impl cannot rot unnoticed.
        let bank = CaptureBank::new();
        bank.publish(kit());
        let first = bank.take().expect("a kit");
        bank.publish(kit());
        let second = bank.take().expect("a second kit");
        bank.retire(first);
        bank.retire(second);
        bank.publish(kit());
        drop(bank);
    }

    #[test]
    fn the_bank_crosses_threads() {
        let bank = std::sync::Arc::new(CaptureBank::new());
        let worker = {
            let bank = bank.clone();
            std::thread::spawn(move || {
                for _ in 0..4 {
                    bank.publish(kit());
                    bank.drain();
                }
            })
        };
        let mut taken = 0;
        for _ in 0..64 {
            if let Some(kit) = bank.take() {
                taken += 1;
                bank.retire(kit);
            }
        }
        worker.join().expect("the worker finished");
        while bank.take().is_some() {
            taken += 1;
        }
        assert!(taken > 0, "the audio side saw at least one kit");
        bank.drain();
    }
}
