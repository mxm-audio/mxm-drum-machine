//! mxm-drum-machine through MXM Player's real CLAP boundary: the output layouts, the instance
//! settings through CLAP state, and the recorded kit's render on the general controls.
//!
//! Build first with `cargo xtask bundle -p mxm-drum-machine --release`.

use mxm_player::session::Session;
use std::path::{Path, PathBuf};

const PLUGIN: &str = "dk.mxm.mxm-drum-machine";
const SKIP: &str = "skipping: run `cargo xtask bundle -p mxm-drum-machine --release`";

fn bundle() -> Option<PathBuf> {
    let path = mxm_player_harness::workspace_root().join("target/bundled/mxm-drum-machine.clap");
    path.exists().then_some(path)
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mxm-drum-machine-pre-d7")
        .join(name)
}

fn loaded(name: &str) -> Option<Session> {
    let bundle = bundle()?;
    let search = bundle.parent().unwrap().to_path_buf();
    let mut session = Session::scratch(name, vec![search]);
    session.load(bundle, PLUGIN);
    Some(session)
}

fn decode(path: &Path) -> Vec<f32> {
    mxm_audio_file_decode::decode_file(
        path,
        &mxm_audio_file_decode::Limits::new(
            usize::MAX,
            mxm_audio_file_decode::AtLimit::Refuse,
            mxm_audio_file_decode::Keep::AllUpTo(8),
        ),
    )
    .expect("baseline WAV decodes")
    .interleaved
}

#[test]
fn player_skips_the_full_layout_and_selects_stereo_compatibility() {
    let Some(mut session) = loaded("drum-d7-stereo-layout") else {
        eprintln!("{SKIP}");
        return;
    };
    let plugin = session.state().plugin.expect("loaded drum machine");
    assert_eq!(plugin.channels, 2);
    assert!(
        plugin.selection.contains("configuration 1")
            && plugin.selection.contains("Stereo compatibility"),
        "{}",
        plugin.selection
    );
}

/// Reads the plugin itself. `PlayerState` deliberately corrects readback to the player's remembered
/// patch, so it cannot show what a state load restored.
fn slot_1(session: &mut Session, name: &str) -> (u32, String) {
    let param = session
        .app()
        .engine_mut()
        .read_params()
        .params
        .into_iter()
        .find(|p| p.module == "Slot 1" && p.name == name)
        .unwrap();
    (param.id, param.text)
}

/// Sets a CLAP plain value, as a host automation write does. The CLI's `set` clamps to 0…1, which
/// cannot reach a stepped 0…16 selector's upper values.
fn set(session: &mut Session, id: u32, plain: f64) {
    session.app().set_parameter(id, plain);
    session.advance_blocks(2).unwrap();
}

#[test]
fn instance_settings_fold_to_main_in_stereo_and_round_trip_through_clap_state() {
    let Some(mut session) = loaded("drum-d7-instance-state") else {
        eprintln!("{SKIP}");
        return;
    };
    let (output, _) = slot_1(&mut session, "Output");
    let (channel, _) = slot_1(&mut session, "MIDI channel");

    // Stereo compatibility folds individual output 8 to main.
    set(&mut session, output, 8.0);
    assert_eq!(slot_1(&mut session, "Output").1, "8");
    session.clear_capture();
    session.app().note_on(36, 0.82);
    session.advance_blocks(8).unwrap();
    session.app().note_off(36);
    assert!(
        session.captured().iter().any(|sample| *sample != 0.0),
        "a slot stored on an individual output went silent in stereo compatibility"
    );

    set(&mut session, channel, 8.0);
    assert_eq!(slot_1(&mut session, "MIDI channel").1, "Ch 8");
    assert!(
        session
            .app()
            .run_cli_command("dumpstate")
            .contains("\"ok\"")
    );
    set(&mut session, output, 0.0);
    set(&mut session, channel, 0.0);
    assert_eq!(slot_1(&mut session, "Output").1, "L+R");
    assert_eq!(slot_1(&mut session, "MIDI channel").1, "Kit");

    assert!(
        session
            .app()
            .run_cli_command("loadstate")
            .contains("\"ok\"")
    );
    assert_eq!(slot_1(&mut session, "Output").1, "8");
    assert_eq!(slot_1(&mut session, "MIDI channel").1, "Ch 8");
}

/// The named controls of before, by the general control each became (`params::control`; the owner,
/// 2026-10-07: a mechanical mapping, the same sound).
const MAPPING: [(&str, &str); 11] = [
    ("pitch", "c01"),
    ("decay", "c02"),
    ("tone", "c03"),
    ("attack", "c04"),
    ("dynamics", "c05"),
    ("pitch_env", "c06"),
    ("pitch_decay", "c07"),
    ("body", "c08"),
    ("noise", "c09"),
    ("noise_decay", "c10"),
    ("character", "c11"),
];

/// **The fixture's state on today's IDs**: `non-default-kit.clapstate` with each named control's ID
/// rewritten by [`MAPPING`], its plain value untouched. The plugin migrates nothing ("There are no
/// saved projects - we are in pre alpha", the owner, 2026-09-30): this rewrite is the test's, the
/// mapping written down once where a recorded render can hold it. Its retired route IDs stay in
/// the state, unknown, and are skipped as any unknown ID is.
///
/// A CLAP state from nice-plug is the JSON's length as a little-endian `u64`, then the JSON.
fn non_default_kit_on_todays_ids() -> Vec<u8> {
    let bytes = std::fs::read(fixture("non-default-kit.clapstate")).unwrap();
    let length = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let mut json = String::from_utf8(bytes[8..8 + length].to_vec()).unwrap();
    for (before, after) in MAPPING {
        for slot in 1..=16 {
            let (from, to) = (
                format!("\"{before}_{slot}\":"),
                format!("\"{after}_{slot}\":"),
            );
            assert!(json.contains(&from), "the fixture holds {from}");
            json = json.replace(&from, &to);
        }
    }
    let mut state = (json.len() as u64).to_le_bytes().to_vec();
    state.extend_from_slice(json.as_bytes());
    state
}

fn load_non_default_kit(session: &mut Session) {
    std::fs::write(
        session.dir().join("preset.clapstate"),
        non_default_kit_on_todays_ids(),
    )
    .unwrap();
    assert!(
        session
            .app()
            .run_cli_command("loadstate")
            .contains("loaded")
    );
}

/// Note 36 at velocity 0.82, 48 blocks held and 16 released, after a two-block settle.
fn render_note_36(session: &mut Session) -> Vec<f32> {
    session.advance_blocks(2).unwrap();
    session.clear_capture();
    session.app().note_on(36, 0.82);
    session.advance_blocks(48).unwrap();
    session.app().note_off(36);
    session.advance_blocks(16).unwrap();
    session.captured()
}

/// **The recorded kit sounds the same on the general controls** (the owner, 2026-10-07: a mechanical
/// mapping, the same sound). The fixture's state, its IDs rewritten by [`MAPPING`], restores Tune at
/// 0.71 of its travel and plays the recorded render bit for bit.
///
/// It replaces `pre_d7_state_opens_with_routing_defaults_and_bit_exact_main_audio`, retired on
/// 2026-10-07 because the plugin no longer reads the pre-D7 state's IDs and migrates none ("There are
/// no saved projects - we are in pre alpha", the owner, 2026-09-30). Its render and its state stay:
/// this is now what they hold (`NOTES.md` § Verification evidence).
#[test]
fn the_recorded_kit_on_the_general_controls_renders_its_recording_bit_exact() {
    let Some(mut session) = loaded("drum-general-controls") else {
        eprintln!("{SKIP}");
        return;
    };
    load_non_default_kit(&mut session);

    let state = session.state();
    let params = &state.plugin.as_ref().unwrap().params;
    for slot in 1..=16 {
        let module = format!("Slot {slot}");
        for (name, expected) in [("Output", "L+R"), ("MIDI channel", "Kit")] {
            let param = params
                .iter()
                .find(|p| p.module == module && p.name == name)
                .unwrap();
            assert_eq!(param.text, expected, "{module}/{name}");
        }
    }
    let tune = params
        .iter()
        .find(|p| p.module == "Slot 1" && p.name == "Control 1")
        .unwrap();
    assert!(
        (tune.value - 0.71).abs() < 1.0e-6,
        "Tune was not restored onto Control 1"
    );

    // `non-default-kit-main-current.wav` is re-captured only for a deliberate change to the slot's
    // model sound, never to let a parameter or routing change pass.
    let rendered = render_note_36(&mut session);
    let recorded = decode(&fixture("non-default-kit-main-current.wav"));
    if cfg!(target_os = "windows") {
        assert_eq!(
            rendered, recorded,
            "the general controls changed the recorded kit's main render"
        );
    } else {
        // The recording is Windows' bits: each platform's maths library rounds in its own way (the
        // owner, 2026-10-06: pin on Windows only). Within rounding it must still be the same
        // render; a mapping or routing change moves it by far more (silence, a control on the
        // wrong axis, a doubled or a missing slot).
        assert_eq!(
            rendered.len(),
            recorded.len(),
            "the render's length changed"
        );
        let worst = rendered
            .iter()
            .zip(&recorded)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            worst < 1.0e-3,
            "the general controls changed the recorded kit's main render (worst sample off by {worst})"
        );
    }
}

/// Rewrites `non-default-kit-main-current.wav` after a deliberate change to the sound of the kit's
/// slot-1 model; record the reason and the new checksum in the fixture README.
/// `cargo test -p mxm-drum-machine-host-tests --test behaviour -- --ignored recapture`
#[test]
#[ignore = "rewrites a fixture; run only after a deliberate sound change"]
fn recapture_the_current_non_default_kit_render() {
    let Some(mut session) = loaded("drum-d7-recapture") else {
        eprintln!("{SKIP}");
        return;
    };
    load_non_default_kit(&mut session);
    let audio = render_note_36(&mut session);
    mxm_audio_file::write(
        fixture("non-default-kit-main-current.wav"),
        &audio,
        2,
        48_000,
        mxm_audio_file::Target::WavFloat32,
    )
    .unwrap();
}
