//! mxm-drum-machine D7 compatibility through MXM Player's real CLAP boundary.
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

fn load_non_default_kit(session: &mut Session) {
    std::fs::copy(
        fixture("non-default-kit.clapstate"),
        session.dir().join("preset.clapstate"),
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

#[test]
fn pre_d7_state_opens_with_routing_defaults_and_bit_exact_main_audio() {
    let Some(mut session) = loaded("drum-d7-old-state") else {
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
    let pitch = params
        .iter()
        .find(|p| p.module == "Slot 1" && p.name == "Pitch")
        .unwrap();
    assert!(
        (pitch.value - 0.71).abs() < 1.0e-6,
        "old sound parameter was not restored"
    );

    // The original pre-D7 render, `non-default-kit-main.wav`, proved this bit-exact against the
    // retained pre-D7 bundle until the slot's model was deliberately refitted. The current render
    // is re-captured only for such a sound change, never to let a routing change pass.
    let rendered = render_note_36(&mut session);
    let recorded = decode(&fixture("non-default-kit-main-current.wav"));
    if cfg!(target_os = "windows") {
        assert_eq!(
            rendered, recorded,
            "default D7 routing changed the main render of a pre-D7 state"
        );
    } else {
        // The recording is Windows' bits: each platform's maths library rounds in its own way (the
        // owner, 2026-10-06: pin on Windows only). Within rounding it must still be the same
        // render; a routing change moves it by far more (silence, a doubled or a missing slot).
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
            "default D7 routing changed the main render of a pre-D7 state (worst sample off by {worst})"
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
