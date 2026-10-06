//! Listening render for catalogue ID 2, Twin-mode snare.
//!
//! `cargo run -p mxm-drum-machine-dsp --release --example drum_machine_twin_mode_snare`

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::ModelId;

const SAMPLE_RATE: f32 = 48_000.0;
const HIT_SECONDS: f32 = 0.75;

fn hit(patch: SlotPatch, velocity: f32, output: &mut Vec<f32>) {
    let mut engine = Engine::new();
    engine.set_sample_rate(SAMPLE_RATE);
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    engine.prepare(&patches);
    let mut trigger = TriggerGroup::new();
    trigger.push(0, velocity);
    engine.trigger_group(&patches, trigger);
    for _ in 0..(HIT_SECONDS * SAMPLE_RATE) as usize {
        let frame = engine.process(&patches);
        output.push(0.7 * (frame[0] + frame[1]));
    }
}

fn patch() -> SlotPatch {
    SlotPatch {
        model: ModelId::TWIN_MODE_SNARE,
        ..SlotPatch::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut samples = Vec::new();
    hit(patch(), 0.8, &mut samples);
    hit(
        SlotPatch {
            noise: -0.75,
            body: 0.35,
            ..patch()
        },
        0.8,
        &mut samples,
    );
    hit(
        SlotPatch {
            noise: 0.75,
            body: -0.25,
            ..patch()
        },
        0.8,
        &mut samples,
    );
    hit(
        SlotPatch {
            tone: -0.8,
            ..patch()
        },
        0.8,
        &mut samples,
    );
    hit(
        SlotPatch {
            tone: 0.8,
            ..patch()
        },
        0.8,
        &mut samples,
    );
    hit(
        SlotPatch {
            pitch_semitones: 7.0,
            decay: 0.5,
            ..patch()
        },
        1.0,
        &mut samples,
    );

    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/twin-mode-snare.wav");
    std::fs::create_dir_all(dir)?;
    mxm_audio_file::write(
        &path,
        &samples,
        1,
        SAMPLE_RATE as u32,
        mxm_audio_file::Target::WavFloat32,
    )?;
    println!("wrote {path}");
    Ok(())
}
