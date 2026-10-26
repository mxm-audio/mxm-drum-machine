//! Resample's capture: the one part of it mxm-kit's `mxm-drum-shell` cannot know, what the drum
//! machine's engine renders and how. The capture thread, the bank, the kit each engage heard, the
//! state machine and the pack's writer are the shell's (`mxm_drum_shell::capture`, plan §4.7).

use std::path::Path;
use std::sync::Arc;

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::capture::{KitCapture, KitCaptureInProgress, capture_patch};
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch};
use mxm_drum_shell::capture::Capturer;
use mxm_drum_shell::pack::Written;

use crate::params::MxmDrumMachineParams;

/// The drum machine's capture, reading its parameters.
pub struct Capture(pub Arc<MxmDrumMachineParams>);

impl Capturer for Capture {
    const THREAD_NAME: &'static str = "mxm-drum-machine-capture";
    type Patches = [SlotPatch; SLOT_COUNT];
    type Kit = KitCapture;
    type Progress = KitCaptureInProgress;
    type Engine = Engine;

    fn read(&self) -> (Self::Patches, String) {
        (crate::capture_patches(&self.0), crate::kit_name(&self.0))
    }

    fn same_capture(was: &Self::Patches, now: &Self::Patches) -> bool {
        was.iter()
            .zip(now)
            .all(|(was, now)| capture_patch(*was) == capture_patch(*now))
    }

    fn begin(
        &self,
        patches: &Self::Patches,
        sample_rate: f32,
        _abandon: &dyn Fn() -> bool,
    ) -> Option<Self::Progress> {
        KitCaptureInProgress::begin(patches, sample_rate).ok()
    }

    fn render(progress: &mut Self::Progress, frames: usize) -> Option<bool> {
        progress.render(frames).ok()
    }

    fn finish(progress: Self::Progress, engagement: u32) -> Self::Kit {
        progress.finish().answering(engagement)
    }

    /// Tune and Decay as set now; a pack is struck at full strength, so Soft hits, a velocity
    /// curve, cannot reach it.
    fn live(heard: &Self::Patches, now: &Self::Patches) -> Self::Patches {
        let mut played = *heard;
        for (patch, now) in played.iter_mut().zip(now) {
            patch.pitch_semitones = now.pitch_semitones;
            patch.decay = now.decay;
        }
        played
    }

    fn write_pack(
        &self,
        root: &Path,
        name: &str,
        captured: &Self::Patches,
        played: &Self::Patches,
        sample_rate: f32,
        stop: &dyn Fn() -> bool,
    ) -> Result<Written, String> {
        mxm_drum_machine_dsp::capture::capture_kit_until(captured, sample_rate, stop)
            .map_err(|error| format!("{error:?}"))
            .and_then(|kit| {
                crate::pack::write_until(root, name, &kit, played, stop)
                    .map_err(|error| error.to_string())
            })
    }

    fn install(engine: &mut Self::Engine, kit: Option<Box<Self::Kit>>) -> Option<Box<Self::Kit>> {
        engine.set_captures(kit)
    }

    fn is_frozen(engine: &Self::Engine) -> bool {
        engine.is_frozen()
    }

    fn kit_engagement(kit: &Self::Kit) -> u32 {
        kit.request()
    }

    fn kit_rate(kit: &Self::Kit) -> f32 {
        kit.sample_rate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::control;
    use mxm_drum_shell::bank::CaptureBank;
    use mxm_drum_shell::capture::{CaptureWorker, Gate, Heard, Inputs};
    use mxm_drum_shell::signals::Export;
    use nice_plug::params::InternalParamMut;

    /// The first slot playing, the fifteen others Off: a debug build captures it quickly.
    fn params() -> Arc<MxmDrumMachineParams> {
        let params = MxmDrumMachineParams::default();
        for slot in &params.slots[1..] {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { slot.model._internal_set_plain_value(0) };
        }
        Arc::new(params)
    }

    /// The one pack under `root`: its name, and every file in it by name.
    fn pack_files(root: &Path) -> (std::ffi::OsString, Vec<(std::ffi::OsString, Vec<u8>)>) {
        let pack = std::fs::read_dir(root)
            .expect("the root")
            .map(|entry| entry.expect("an entry").path())
            .find(|path| path.is_dir())
            .expect("a pack");
        let mut files: Vec<_> = std::fs::read_dir(&pack)
            .expect("the pack")
            .map(|entry| {
                let path = entry.expect("an entry").path();
                (
                    path.file_name().expect("a name").to_owned(),
                    std::fs::read(&path).expect("a file"),
                )
            })
            .collect();
        files.sort();
        (pack.file_name().expect("a name").to_owned(), files)
    }

    /// Says `name` is the kit loaded, as a kit load does.
    fn load_kit_named(params: &MxmDrumMachineParams, name: &str) {
        params
            .preset
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded = Some(mxm_preset::LoadedPreset {
            name: name.to_owned(),
            origin: mxm_preset::Origin::Factory,
        });
    }

    /// **Export writes the kit heard, under its name**, through the shell's thread as the editor
    /// asks for it: frozen, a kit loaded since and an edit of a control the capture bakes in are not
    /// what plays, so the pack is the patches the playing kit was captured from, named for it.
    #[test]
    fn an_export_while_frozen_writes_the_kit_heard_not_an_edit_since() {
        let params = params();
        let capture = Capture(Arc::clone(&params));
        load_kit_named(&params, "Kit A");
        let (heard, name) = capture.read();
        let telemetry = crate::telemetry::Telemetry::shared();
        let gate = Gate::default();
        gate.remember(
            5,
            Heard {
                patches: heard,
                name,
            },
        );
        telemetry.signals.set_frozen(Some(5));
        // Kit B loaded while frozen, and an edit: slot 1's Tone.
        load_kit_named(&params, "Kit B");
        // SAFETY: as above.
        unsafe {
            params.slots[0]
                .controls
                .get(control::TONE)
                ._internal_set_plain_value(0.9);
        }

        let frozen = crate::pack::tests::scratch("heard");
        let worker = CaptureWorker::spawn(
            &gate,
            Inputs {
                capturer: Arc::new(capture),
                bank: Arc::new(CaptureBank::new()),
                telemetry: Arc::clone(&telemetry),
            },
        );
        gate.request_export(48_000.0, frozen.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let outcome = loop {
            if let Some(outcome) = telemetry.signals.take_export() {
                break outcome;
            }
            assert!(std::time::Instant::now() < deadline, "no export");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        drop(worker);
        assert!(matches!(outcome, Export::Written { .. }), "{outcome:?}");

        let reference = crate::pack::tests::scratch("heard-reference");
        let kit = mxm_drum_machine_dsp::capture::capture_kit(&heard, 48_000.0).expect("a kit");
        crate::pack::write(&reference, "Kit A", &kit, &heard).expect("a pack");
        assert_eq!(
            pack_files(&frozen),
            pack_files(&reference),
            "the pack is the kit heard, under its name"
        );
        for root in [frozen, reference] {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    /// The controls that stay live while frozen are Tune and Decay; a capture is the same kit when
    /// only the mix moved.
    #[test]
    fn tune_and_decay_stay_live_and_the_mix_is_not_a_new_capture() {
        let (heard, _) = Capture(params()).read();
        let mut now = heard;
        now[0].pitch_semitones = -5.0;
        now[0].decay = 0.25;
        now[0].dynamics = 0.9;
        now[0].level = 0.1;
        let played = Capture::live(&heard, &now);
        assert_eq!(played[0].pitch_semitones, -5.0);
        assert_eq!(played[0].decay, 0.25);
        assert_eq!(
            played[0].dynamics, heard[0].dynamics,
            "Soft hits is not a pack's"
        );
        let mut mixed = heard;
        mixed[0].level = 0.1;
        mixed[0].pan = -1.0;
        assert!(Capture::same_capture(&heard, &mixed));
        let mut toned = heard;
        toned[0].tone = heard[0].tone + 0.5;
        assert!(!Capture::same_capture(&heard, &toned));
    }
}
