//! Listening render for the first drum-machine circuit.
//!
//! Run with:
//! `cargo run -p mxm-drum-machine-dsp --release --example drum_machine_deep_bridge_kick`
//!
//! Six isolated hits expose the reference, the shortest stock and the extended decay ends, and the
//! creative pitch/body/character travel. This is audition material, not a reference recording or fidelity
//! proof.

use mxm_drum_machine_dsp::deep_bridge_kick::{DeepBridgeKick, Patch};

const SAMPLE_RATE: f32 = 48_000.0;
const HIT_SECONDS: f32 = 1.25;

fn hit(patch: Patch, velocity: f32, output: &mut Vec<f32>) {
    let mut voice = DeepBridgeKick::new();
    voice.set_sample_rate(SAMPLE_RATE);
    voice.trigger(velocity, patch.dynamics, patch.attack);
    for _ in 0..(HIT_SECONDS * SAMPLE_RATE) as usize {
        output.push(0.6 * voice.process(patch));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut samples = Vec::new();
    hit(Patch::default(), 0.8, &mut samples);
    hit(
        Patch {
            decay: -1.0,
            tone: -0.25,
            ..Patch::default()
        },
        0.8,
        &mut samples,
    );
    hit(
        Patch {
            decay: 1.0,
            tone: 0.25,
            ..Patch::default()
        },
        0.8,
        &mut samples,
    );
    hit(
        Patch {
            pitch_semitones: -12.0,
            body: 0.5,
            ..Patch::default()
        },
        0.8,
        &mut samples,
    );
    hit(
        Patch {
            pitch_semitones: 12.0,
            attack: 0.6,
            ..Patch::default()
        },
        0.8,
        &mut samples,
    );
    hit(
        Patch {
            character: 0.75,
            dynamics: 0.5,
            ..Patch::default()
        },
        1.0,
        &mut samples,
    );

    let path = "target/rendered/mxm-drum-machine/deep-bridge-kick.wav";
    std::fs::create_dir_all("target/rendered/mxm-drum-machine")?;
    mxm_audio_file::write(
        path,
        &samples,
        1,
        SAMPLE_RATE as u32,
        mxm_audio_file::Target::WavFloat32,
    )?;
    println!("wrote {path}");
    Ok(())
}
