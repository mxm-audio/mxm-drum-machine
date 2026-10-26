//! Lock-free DSP-to-editor activity and developer requests; Resample's and the audition's signals
//! are mxm-kit's `mxm_drum_shell::signals`, held here.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_shell::signals::Signals;

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    slot_peaks: [AtomicU32; SLOT_COUNT],
    /// The host tempo in force, for the synced rates' readings (`plans/plan-tempo-sync-controls.md`).
    pub tempo: mxm_tempo::TempoCell,
    /// Resample's and the audition's: the capture `process` asks for, which engage's kit plays, the
    /// export's outcome, the slots the editor asks to hear.
    pub signals: Signals,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            slot_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            tempo: mxm_tempo::TempoCell::new(),
            signals: Signals::new(),
        }
    }
}

impl AsRef<Signals> for Telemetry {
    fn as_ref(&self) -> &Signals {
        &self.signals
    }
}

impl Telemetry {
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn publish_slot_peaks(&self, peaks: [f32; SLOT_COUNT]) {
        for (target, peak) in self.slot_peaks.iter().zip(peaks) {
            max_combine(target, peak);
        }
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
}
