//! Lock-free DSP-to-editor activity and developer requests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use mxm_drum_machine_dsp::SLOT_COUNT;

/// A capture `process` asked for: the rate to render at, and the engage it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureRequest {
    /// The rate the engine is running at.
    pub sample_rate: f32,
    /// The engage that asked (`MxmDrumMachine::engagement`), which the worker stamps on the kit
    /// so a kit made for an engage since left is refused.
    pub engagement: u32,
}

/// How the last sample-pack export finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Export {
    /// Written, with what it holds and how much clipped.
    Written { files: usize, clipped: usize },
    /// The platform has no local data folder, so there was nowhere to write.
    NoDestination,
    /// It failed, with the reason to show.
    Failed(String),
}

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    slot_peaks: [AtomicU32; SLOT_COUNT],
    /// The last export's outcome, waiting for an editor frame to show it.
    export: std::sync::Mutex<Option<Export>>,
    /// The capture `process` wants, or zero for no request: the rate as `f32` bits in the low
    /// half, the engage that asked in the high half ([`CaptureRequest`]).
    ///
    /// **The whole channel from the audio thread to the renderer is this one word.** `process`
    /// cannot render seconds of audio and cannot take a lock, so it stores a request here; the
    /// plugin's capture thread (`capture_worker`) reads it and does the work. Nothing else is
    /// needed, because a capture is a pure function of the parameters, which both sides can see —
    /// and the engage travels with it so a kit that lands after that engage has ended is known
    /// for what it is: the patch as it was.
    ///
    /// **The rate travels with the request** because only `process` knows it. A kit rendered at
    /// the wrong rate plays back at the wrong speed, and the renderer has no other way to learn
    /// what the host activated at. Zero is the sentinel rather than a second flag: `activate`
    /// refuses any rate below `MIN_SAMPLE_RATE`, so zero can never be a real one.
    ///
    /// Overwriting rather than queueing is the point. A host sweeping a captured axis asks every
    /// block, and coalescing those into "the most recent rate" is what stops the renderer being
    /// handed work faster than it can retire it — the shape that crashed hosts when these
    /// requests were nice-plug background tasks.
    capture_wanted: AtomicU64,
    /// The engage whose kit plays, plus one, or zero for none (`set_frozen`): what Export asks for
    /// the patches that kit was captured from (`capture_worker::Gate::heard_for`).
    frozen: AtomicU64,
    /// The host tempo in force, for the synced rates' readings (`plans/plan-tempo-sync-controls.md`).
    pub tempo: mxm_tempo::TempoCell,
    /// The slots the editor asked to hear, a bit a slot (`request_audition`).
    audition: AtomicU32,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            slot_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            export: std::sync::Mutex::new(None),
            capture_wanted: AtomicU64::new(0),
            frozen: AtomicU64::new(0),
            tempo: mxm_tempo::TempoCell::new(),
            audition: AtomicU32::new(0),
        }
    }
}

impl Telemetry {
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Reports how the last sample-pack export went, for the editor to show.
    ///
    /// Not an audio-thread channel: the export worker writes it and an editor frame reads it, so
    /// unlike the peak meters it can afford a lock and a `String`. It is here rather than in the
    /// editor because the worker has no other way to reach a frame.
    pub fn publish_export(&self, outcome: Export) {
        *self
            .export
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(outcome);
    }

    /// Takes the last export outcome, if one has not been shown yet.
    pub fn take_export(&self) -> Option<Export> {
        self.export
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Asks for a fresh capture at the rate the engine is running. **Audio thread**: one relaxed
    /// store. Asking twice before a frame reads it is the same as asking once, and the later rate
    /// wins — which is what a rate change mid-request should do.
    pub fn request_capture(&self, sample_rate: f32, engagement: u32) {
        self.capture_wanted.store(
            u64::from(engagement) << 32 | u64::from(sample_rate.to_bits()),
            Ordering::Relaxed,
        );
    }

    /// Whether a capture request is waiting, without taking it: the worker's test for a render a
    /// newer request has overtaken.
    pub fn capture_requested(&self) -> bool {
        self.capture_wanted.load(Ordering::Relaxed) != 0
    }

    /// Takes a pending capture request and the rate it was made at, if there is one.
    pub fn take_capture_request(&self) -> Option<CaptureRequest> {
        match self.capture_wanted.swap(0, Ordering::Relaxed) {
            0 => None,
            word => Some(CaptureRequest {
                sample_rate: f32::from_bits(word as u32),
                engagement: (word >> 32) as u32,
            }),
        }
    }

    pub fn publish_slot_peaks(&self, peaks: [f32; SLOT_COUNT]) {
        for (target, peak) in self.slot_peaks.iter().zip(peaks) {
            max_combine(target, peak);
        }
    }

    /// Says which engage's kit plays, or that none does. **Audio thread** (an install or a
    /// disengage) and `activate`: one relaxed store.
    pub fn set_frozen(&self, engagement: Option<u32>) {
        self.frozen.store(
            engagement.map_or(0, |e| u64::from(e) + 1),
            Ordering::Relaxed,
        );
    }

    /// The engage whose kit plays, if one does.
    #[must_use]
    pub fn frozen(&self) -> Option<u32> {
        match self.frozen.load(Ordering::Relaxed) {
            0 => None,
            word => Some((word - 1) as u32),
        }
    }

    /// Asks for `slot` to be struck, as a test of its sound (the editor's model name, clicked or
    /// OPEN on it); `process` strikes it at its next buffer's first sample.
    pub fn request_audition(&self, slot: usize) {
        if slot < SLOT_COUNT {
            self.audition.fetch_or(1 << slot, Ordering::Relaxed);
        }
    }

    /// The slots asked for since the last call, a bit a slot, each taken once.
    #[must_use]
    pub fn take_audition(&self) -> u32 {
        self.audition.swap(0, Ordering::Relaxed)
    }

    #[must_use]
    pub fn take_slot_peaks(&self) -> [f32; SLOT_COUNT] {
        std::array::from_fn(|index| {
            f32::from_bits(self.slot_peaks[index].swap(0, Ordering::Relaxed))
        })
    }
}

fn max_combine(target: &AtomicU32, value: f32) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    let mut current = target.load(Ordering::Relaxed);
    loop {
        if f32::from_bits(current) >= value {
            return;
        }
        match target.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(seen) => current = seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_has_independent_drop_tolerant_activity() {
        let telemetry = Telemetry::default();
        let mut first = [0.0; SLOT_COUNT];
        first[0] = 0.4;
        first[15] = 0.7;
        telemetry.publish_slot_peaks(first);
        let seen = telemetry.take_slot_peaks();
        assert_eq!(seen[0], 0.4);
        assert_eq!(seen[15], 0.7);
        assert_eq!(telemetry.take_slot_peaks(), [0.0; SLOT_COUNT]);
    }

    #[test]
    fn a_capture_request_carries_the_rate_it_was_made_at() {
        // The frame that submits the render cannot know the host's rate, so the request has to
        // carry it. A constant here meant a 96 kHz session played its kit back at half speed.
        let telemetry = Telemetry::default();
        assert_eq!(telemetry.take_capture_request(), None);
        telemetry.request_capture(96_000.0, 7);
        assert_eq!(
            telemetry.take_capture_request(),
            Some(CaptureRequest {
                sample_rate: 96_000.0,
                engagement: 7
            })
        );
        assert_eq!(telemetry.take_capture_request(), None, "taken twice");
    }

    #[test]
    fn asking_twice_asks_once_and_the_later_rate_wins() {
        // `process` asks every block while the kit is stale and a frame arrives every twentieth
        // of a second, so most requests are overwritten rather than seen. The last one is the
        // truthful one — a rate change mid-request must not be answered at the old rate.
        let telemetry = Telemetry::default();
        telemetry.request_capture(44_100.0, 1);
        telemetry.request_capture(192_000.0, 2);
        assert_eq!(
            telemetry.take_capture_request(),
            Some(CaptureRequest {
                sample_rate: 192_000.0,
                engagement: 2
            })
        );
        assert_eq!(telemetry.take_capture_request(), None);
    }
}
