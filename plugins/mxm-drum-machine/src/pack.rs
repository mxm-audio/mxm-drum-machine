//! Writing the kit out as a sample pack (plan §4.7, D9).
//!
//! Sixteen 24-bit WAV one-shots and a manifest, in a flat folder a tracker or a hardware sampler
//! can import without being told anything. This is the half of Resample that leaves the plugin,
//! and it lives here rather than in the DSP crate because
//! [`../../../crates/mxm-drum-machine-dsp/AGENTS.md`](../../../crates/mxm-drum-machine-dsp/AGENTS.md)
//! keeps `mxm-audio-file` out of that crate's shipped graph.
//!
//! # What lands in a file
//!
//! A frozen slot is heard as its capture, then the sample-domain Pitch and Decay, then Level,
//! Mute, Solo, Pan, Master and the Level/Pan routes. **The pack takes that chain up to and
//! including Pitch and Decay, and stops there.** Those two are the one-shot somebody shaped and
//! auditioned; everything after them is monitoring balance belonging to whatever plays the pack
//! next. So every file is at unity gain and centred, a muted slot still exports, and moving a
//! fader cannot change an exported byte.
//!
//! # Never overwriting, on three platforms
//!
//! Sixteen files and a manifest can fail halfway, which per-file atomicity does not prevent. The
//! pack is written into a uniquely named sibling directory and committed by **one directory
//! rename onto a path that does not exist** — a name already taken simply takes the next one. So
//! there is nothing to roll back, and the rename means the same thing on Windows, Linux and
//! macOS, which a rename over an existing directory would not.

use std::path::{Path, PathBuf};

use mxm_audio_file::{Bits, Target};
use mxm_drum_machine_dsp::capture::{CaptureVoice, KitCapture, Retrigger};
use mxm_drum_machine_dsp::engine::SlotPatch;
use mxm_drum_machine_dsp::{SLOT_COUNT, model, note_for_slot};

/// The folder packs are written into, under `data_local`.
///
/// Collection content, so the platform's local data directory rather than the roaming config
/// one — the same rule that puts `mxm/impulses` there, and for the same reason: a pack is audio
/// and audio should not follow a Windows profile around.
#[must_use]
pub fn root_under(data_local: Option<PathBuf>) -> Option<PathBuf> {
    data_local.map(|dir| dir.join("mxm").join("sample-packs"))
}

/// This user's sample-pack folder; `None` where the platform has no local data directory.
///
/// The root is resolved once and injected, exactly as the preset library's and the impulses
/// folder's are, so a test never writes into the folder of whoever ran it.
#[must_use]
pub fn root() -> Option<PathBuf> {
    root_under(dirs::data_local_dir())
}

/// Why a pack could not be written.
#[derive(Debug)]
pub enum PackError {
    /// The platform has no local data directory, so there is nowhere to put a pack.
    NoDestination,
    /// Every slot was `Off`, so there is nothing to export.
    Empty,
    /// Encoding refused the audio, which `mxm-audio-file` does for a non-finite sample.
    Encode(String),
    /// The file system refused.
    Io(std::io::Error),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDestination => write!(f, "this platform has no local data folder to write to"),
            Self::Empty => write!(f, "every slot is Off, so there is nothing to export"),
            Self::Encode(why) => write!(f, "the audio could not be encoded: {why}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl From<std::io::Error> for PackError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a finished pack contains, for the editor to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Where the pack landed.
    pub directory: PathBuf,
    /// How many one-shots it holds.
    pub files: usize,
    /// How many samples clipped, summed over the pack.
    ///
    /// Reported rather than prevented: `mxm-audio-file` counts them and a silently clipped pack
    /// is worse than one that says so. Nothing is normalised — §4.1 forbids a hidden limiter in
    /// the audio path and a file is no different.
    pub clipped: usize,
}

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
    let slot_capture = capture.slot(slot);
    if slot_capture.is_empty() {
        return Vec::new();
    }
    let mut voice = CaptureVoice::new();
    voice.trigger(
        slot_capture,
        1.0,
        patch.decay,
        Retrigger::Restart,
        sample_rate,
    );
    // Pitching down lengthens the read, so the buffer cannot be sized from the capture alone.
    let rate = 2.0_f32.powf(patch.pitch_semitones / 12.0);
    let bound = ((slot_capture.len() as f32 / rate.max(0.01)).ceil() as usize) + 1;
    let mut out = Vec::with_capacity(bound);
    while voice.is_active() && out.len() < bound {
        out.push(voice.process(slot_capture, patch.pitch_semitones));
    }
    out
}

/// A file-system-safe name for a model's public label.
///
/// The label is already the product name, and `mxm-drum-machine-dsp` has a test forbidding maker
/// and model designations in one — so the pack inherits that rule rather than restating it.
#[must_use]
pub fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut last_dash = true;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "slot".to_owned()
    } else {
        out
    }
}

/// The manifest that ships beside the audio.
///
/// Plain text, because the thing reading it next is a person deciding which file is the kick.
#[must_use]
pub fn manifest(
    kit: &str,
    capture: &KitCapture,
    patches: &[SlotPatch; SLOT_COUNT],
    names: &[Option<String>; SLOT_COUNT],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("{kit}\n"));
    out.push_str("Exported from mxm-drum-machine.\n\n");
    out.push_str(&format!(
        "Sample rate: {} Hz, 24-bit WAV\n\n",
        capture.sample_rate() as u32
    ));
    for slot in 0..SLOT_COUNT {
        let Some(name) = names[slot].as_ref() else {
            continue;
        };
        let label = model::label(patches[slot].model);
        // Every slot in the Kit range has a note; the fallback is only so a future slot without
        // one cannot break the manifest.
        let note = note_for_slot(slot).map_or_else(|| "-".to_owned(), |note| note.to_string());
        out.push_str(&format!(
            "{name}  slot {:02}  note {note}  {label}\n",
            slot + 1
        ));
    }
    out.push_str(
        "\nThese are recordings of the instrument, not the instrument. Each one is a single hit \
         frozen at one moment:\n\
         the free-running metal and noise sources a real circuit reads are fixed at the phase \
         they had when it was captured,\n\
         and velocity scales the file rather than re-entering the circuit. Hats and cymbals will \
         repeat exactly rather than varying.\n",
    );
    out.push_str(
        "\nThe models are schematic-derived and their fidelity is unverified against hardware.\n",
    );
    out
}

/// Writes a pack, and returns where it landed.
///
/// **Never call this from an audio thread**: it renders, allocates and touches the file system.
///
/// # Errors
///
/// [`PackError`] when there is no destination, nothing to export, the audio cannot be encoded, or
/// the file system refuses. A failure leaves no partial pack at the destination, because the
/// staging directory is only renamed into place once every file is written.
pub fn write(
    root: &Path,
    kit: &str,
    capture: &KitCapture,
    patches: &[SlotPatch; SLOT_COUNT],
) -> Result<Written, PackError> {
    let rate = capture.sample_rate() as u32;
    let mut rendered: Vec<(usize, String, Vec<f32>)> = Vec::new();
    for (slot, patch) in patches.iter().enumerate() {
        let samples = one_shot(capture, slot, patch, capture.sample_rate());
        if samples.is_empty() {
            continue;
        }
        let name = format!("{:02}-{}.wav", slot + 1, slug(model::label(patch.model)));
        rendered.push((slot, name, samples));
    }
    if rendered.is_empty() {
        return Err(PackError::Empty);
    }

    let (directory, staging) = destination(root, kit)?;
    std::fs::create_dir_all(&staging)?;

    let mut clipped = 0;
    let mut names: [Option<String>; SLOT_COUNT] = std::array::from_fn(|_| None);
    let written = (|| -> Result<usize, PackError> {
        for (slot, name, samples) in &rendered {
            let encoded = mxm_audio_file::write(
                staging.join(name),
                samples,
                1,
                rate,
                Target::Wav(Bits::TwentyFour),
            )
            .map_err(|error| PackError::Encode(format!("{error:?}")))?;
            clipped += encoded.clipped;
            names[*slot] = Some(name.clone());
        }
        let text = manifest(kit, capture, patches, &names);
        std::fs::write(staging.join("kit.txt"), text)?;
        Ok(rendered.len())
    })();

    let files = match written {
        Ok(files) => files,
        Err(error) => {
            // Nothing reached the destination, so there is nothing to roll back — only the
            // staging directory to clean up.
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    std::fs::rename(&staging, &directory)?;
    Ok(Written {
        directory,
        files,
        clipped,
    })
}

/// Picks a free pack name and the staging directory that becomes it.
///
/// An existing pack is never overwritten: a taken name takes the next number. The staging
/// directory is a sibling, so the commit is one rename within a single file system.
fn destination(root: &Path, kit: &str) -> Result<(PathBuf, PathBuf), PackError> {
    let base = slug(kit);
    for attempt in 0..1_000 {
        let name = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let directory = root.join(&name);
        if directory.exists() {
            continue;
        }
        let staging = root.join(format!(".{name}.incomplete"));
        if staging.exists() {
            continue;
        }
        return Ok((directory, staging));
    }
    Err(PackError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "a thousand packs already share this name",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_drum_machine_dsp::capture::capture_kit;
    use mxm_drum_machine_dsp::model::ModelId;

    fn patches() -> [SlotPatch; SLOT_COUNT] {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = ModelId::DEEP_BRIDGE_KICK;
        patches[1].model = ModelId::TWIN_MODE_SNARE;
        patches
    }

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mxm-pack-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary root");
        dir
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
    fn an_existing_pack_is_never_overwritten() {
        let root = temp();
        let patches = patches();
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        let first = write(&root, "Test Kit", &kit, &patches).expect("a pack");
        let second = write(&root, "Test Kit", &kit, &patches).expect("a second pack");
        assert_ne!(first.directory, second.directory, "the name was taken");
        assert!(first.directory.exists(), "and the first is untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_partial_is_left_at_the_destination() {
        // The commit is one rename, so a destination either has a whole pack or nothing. The
        // staging directory is the only thing a failure can leave, and it is not the pack.
        let root = temp();
        let patches = patches();
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        let written = write(&root, "Test Kit", &kit, &patches).expect("a pack");
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .expect("the root")
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_string_lossy().into_owned();
                name.contains("incomplete").then_some(name)
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging was cleaned up: {leftovers:?}"
        );
        assert!(written.directory.join("kit.txt").exists());
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
    fn an_empty_kit_is_refused_rather_than_written() {
        let root = temp();
        let patches = [SlotPatch::default(); SLOT_COUNT];
        let kit = capture_kit(&patches, 48_000.0).expect("a kit");
        assert!(matches!(
            write(&root, "Empty", &kit, &patches),
            Err(PackError::Empty)
        ));
        let _ = std::fs::remove_dir_all(&root);
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

    #[test]
    fn a_slug_is_file_system_safe() {
        assert_eq!(slug("Deep bridge kick"), "deep-bridge-kick");
        assert_eq!(slug("Six-bit crash"), "six-bit-crash");
        assert_eq!(slug("  ...  "), "slot");
        assert_eq!(slug("Early 2L"), "early-2l");
    }

    #[test]
    fn the_folder_is_mxm_sample_packs_under_the_local_data_directory() {
        let base = PathBuf::from("data-local");
        assert_eq!(
            root_under(Some(base.clone())),
            Some(base.join("mxm").join("sample-packs"))
        );
        assert_eq!(root_under(None), None);
    }
}
