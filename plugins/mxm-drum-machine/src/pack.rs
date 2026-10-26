//! The drum machine's sample pack: its one-shots, rendered through the reader its frozen kit plays,
//! and what a recording of it loses. The writer, the manifest, the naming and the never-overwriting
//! are mxm-kit's `mxm_drum_shell::pack` (plan §4.7, D9; shared since 2026-10-26).
//!
//! A frozen slot is heard as its capture, then its Pitch and Decay, then Level, Mute, Solo, Pan,
//! Master and the Level/Pan routes; the pack takes the chain up to Pitch and Decay and stops, so a
//! file is at unity gain and centred and moving a fader cannot change an exported byte.

use std::path::Path;

use mxm_drum_machine_dsp::capture::{CaptureVoice, KitCapture, Retrigger};
use mxm_drum_machine_dsp::engine::SlotPatch;
use mxm_drum_machine_dsp::{SLOT_COUNT, model, note_for_slot};
use mxm_drum_shell::pack::{OneShots, render_one_shot};

pub use mxm_drum_shell::pack::{PackError, Written, root, slug};

/// What a recording of the drum machine loses: the manifest's last paragraph.
const LOSSES: &str = "Each one is a single hit frozen at one moment:\n\
     the free-running metal and noise sources a real circuit reads are fixed at the phase they \
     had when it was captured,\n\
     and velocity scales the file rather than re-entering the circuit. Hats and cymbals will \
     repeat exactly rather than varying.\n\
     \nThe models are schematic-derived and their fidelity is unverified against hardware.\n";

/// Renders one slot's exported one-shot: the capture, with its Pitch and Decay applied.
///
/// Reuses the reader the frozen kit plays through, so the file is the sound rather than a second
/// implementation of it. Struck at full velocity because the pack is the instrument, not a
/// performance of it.
#[must_use]
pub fn one_shot(
    capture: &KitCapture,
    slot: usize,
    patch: &SlotPatch,
    sample_rate: f32,
) -> Vec<f32> {
    one_shot_until(capture, slot, patch, sample_rate, &|| false).unwrap_or_default()
}

/// [`one_shot`], stopping if `cancelled` says to (`mxm_drum_shell::pack::render_one_shot`).
///
/// # Errors
///
/// [`PackError::Cancelled`] when it stopped, [`PackError::Allocation`] when its buffer cannot be
/// had.
pub fn one_shot_until(
    capture: &KitCapture,
    slot: usize,
    patch: &SlotPatch,
    sample_rate: f32,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<f32>, PackError> {
    let slot_capture = capture.slot(slot);
    if slot_capture.is_empty() {
        return Ok(Vec::new());
    }
    let mut voice = CaptureVoice::new();
    voice.trigger(
        slot_capture,
        1.0,
        patch.decay,
        Retrigger::Restart,
        sample_rate,
    );
    // Pitching down lengthens the read, so the buffer cannot be sized from the capture alone. The
    // rate is the reader's own: a model without Tune plays at its captured rate.
    let rate = 2.0_f32.powf(slot_capture.playback_semitones(patch.pitch_semitones) / 12.0);
    let bound = ((slot_capture.len() as f32 / rate.max(0.01)).ceil() as usize) + 1;
    render_one_shot(slot, bound, cancelled, || {
        voice
            .is_active()
            .then(|| voice.process(slot_capture, patch.pitch_semitones))
    })
}

/// A captured kit as a pack reads it, each slot played by its patch's Pitch and Decay.
struct Shots<'a> {
    capture: &'a KitCapture,
    patches: &'a [SlotPatch; SLOT_COUNT],
}

impl OneShots for Shots<'_> {
    fn sample_rate(&self) -> f32 {
        self.capture.sample_rate()
    }
    fn is_empty(&self, slot: usize) -> bool {
        self.capture.slot(slot).is_empty()
    }
    fn one_shot_until(
        &self,
        slot: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, PackError> {
        one_shot_until(
            self.capture,
            slot,
            &self.patches[slot],
            self.capture.sample_rate(),
            cancelled,
        )
    }
    fn label(&self, slot: usize) -> &str {
        model::label(self.patches[slot].model)
    }
    fn note(&self, slot: usize) -> Option<u8> {
        note_for_slot(slot)
    }
    fn product(&self) -> &str {
        "mxm-drum-machine"
    }
    fn losses(&self) -> &str {
        LOSSES
    }
}

/// The manifest that ships beside the audio (`mxm_drum_shell::pack::manifest`).
#[must_use]
pub fn manifest(
    kit: &str,
    capture: &KitCapture,
    patches: &[SlotPatch; SLOT_COUNT],
    names: &[Option<String>; SLOT_COUNT],
) -> String {
    mxm_drum_shell::pack::manifest(kit, &Shots { capture, patches }, names)
}

/// Writes `capture` as a pack, each slot played by `patches` (`mxm_drum_shell::pack::write`).
///
/// # Errors
///
/// As the shell's.
pub fn write(
    root: &Path,
    kit: &str,
    capture: &KitCapture,
    patches: &[SlotPatch; SLOT_COUNT],
) -> Result<Written, PackError> {
    mxm_drum_shell::pack::write(root, kit, &Shots { capture, patches })
}

/// [`write`], stopping if `cancelled` says to (`mxm_drum_shell::pack::write_until`).
///
/// # Errors
///
/// As the shell's.
pub fn write_until(
    root: &Path,
    kit: &str,
    capture: &KitCapture,
    patches: &[SlotPatch; SLOT_COUNT],
    cancelled: &dyn Fn() -> bool,
) -> Result<Written, PackError> {
    mxm_drum_shell::pack::write_until(root, kit, &Shots { capture, patches }, cancelled)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use mxm_drum_machine_dsp::capture::capture_kit;
    use mxm_drum_machine_dsp::model::ModelId;
    use std::path::PathBuf;

    fn patches() -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        patches[1].model = ModelId::TWIN_MODE_SNARE;
        patches
    }

    /// A folder for one test's packs, under the build's own folder (never the real pack folder, and
    /// never on C:), empty.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-packs")
            .join(format!(
                "{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch root");
        dir
    }

    fn temp() -> PathBuf {
        scratch("pack")
    }

    #[test]
    fn a_pack_holds_one_file_per_sounding_slot_and_a_manifest() {
        let root = temp();
        let patches = patches();
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        let written = write(&root, "Test Kit", &kit, &patches).expect("a pack");
        assert_eq!(written.files, 2, "two loaded slots, two files");

        let mut names: Vec<String> = std::fs::read_dir(&written.directory)
            .expect("the pack")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "01-deep-bridge-kick.wav".to_owned(),
                "02-twin-mode-snare.wav".to_owned(),
                "kit.txt".to_owned(),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_mix_cannot_change_an_exported_byte() {
        // The pack stops before Level, Pan, Mute and Solo, so moving them must leave the files
        // identical — that is what makes a pack the one-shot rather than a monitoring balance.
        let root = temp();
        let plain = patches();
        let mut mixed = plain;
        mixed[0].level = 0.1;
        mixed[0].pan = -1.0;

        let kit = capture_kit(&plain, 48_000.0).expect("a kit");
        let a = write(&root, "A", &kit, &plain).expect("a pack");
        let b = write(&root, "B", &kit, &mixed).expect("a pack");
        let first = std::fs::read(a.directory.join("01-deep-bridge-kick.wav")).expect("a file");
        let second = std::fs::read(b.directory.join("01-deep-bridge-kick.wav")).expect("a file");
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pitch_and_decay_do_reach_the_file() {
        // The mirror of the test above: the two axes the pack *does* take must change it, or a
        // person could not export the one-shot they shaped.
        let root = temp();
        let plain = patches();
        let mut shaped = plain;
        shaped[0].pitch_semitones = -12.0;

        let kit = capture_kit(&plain, 48_000.0).expect("a kit");
        let a = write(&root, "A", &kit, &plain).expect("a pack");
        let b = write(&root, "B", &kit, &shaped).expect("a pack");
        let first = std::fs::read(a.directory.join("01-deep-bridge-kick.wav")).expect("a file");
        let second = std::fs::read(b.directory.join("01-deep-bridge-kick.wav")).expect("a file");
        assert!(
            second.len() > first.len(),
            "an octave down is a longer file"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cancelled_one_shot_stops() {
        let patches = patches();
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        assert!(matches!(
            one_shot_until(&kit, 0, &patches[0], 48_000.0, &|| true),
            Err(PackError::Cancelled)
        ));
        assert!(!one_shot(&kit, 0, &patches[0], 48_000.0).is_empty());
    }

    #[test]
    fn the_manifest_names_every_file_and_says_what_a_pack_is() {
        let patches = patches();
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        let mut names: [Option<String>; SLOT_COUNT] = std::array::from_fn(|_| None);
        names[0] = Some("01-deep-bridge-kick.wav".to_owned());
        let text = manifest("Test Kit", &kit, &patches, &names);
        assert!(text.contains("01-deep-bridge-kick.wav"));
        assert!(text.contains("note 36"), "the kit note map is in it");
        assert!(text.contains("48000 Hz"));
        assert!(
            text.contains("recordings of the instrument, not the instrument"),
            "the pack says what it is"
        );
        assert!(text.contains("fidelity is unverified"));
    }
}
