//! Audition render for catalogue IDs 11–12, shared-noise maraca and handclap.
//!
//! Order: maraca reference/short/dark/noisy, then clap reference/short/long/bright.

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::ModelId;

const SAMPLE_RATE: f32 = 48_000.0;
const HIT_SECONDS: f32 = 0.65;

fn hit(model: ModelId, mut patch: SlotPatch, output: &mut Vec<f32>) {
    patch.model = model;
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    let mut engine = Engine::new();
    engine.set_sample_rate(SAMPLE_RATE);
    engine.prepare(&patches);
    let mut trigger = TriggerGroup::new();
    trigger.push(0, 0.82);
    engine.trigger_group(&patches, trigger);
    for _ in 0..(HIT_SECONDS * SAMPLE_RATE) as usize {
        let [left, right] = engine.process(&patches);
        output.push(0.7 * (left + right));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut samples = Vec::new();
    hit(
        ModelId::BRIGHT_SHORT_MARACA,
        SlotPatch::default(),
        &mut samples,
    );
    hit(
        ModelId::BRIGHT_SHORT_MARACA,
        SlotPatch {
            decay: -0.7,
            ..SlotPatch::default()
        },
        &mut samples,
    );
    hit(
        ModelId::BRIGHT_SHORT_MARACA,
        SlotPatch {
            tone: -0.7,
            ..SlotPatch::default()
        },
        &mut samples,
    );
    hit(
        ModelId::BRIGHT_SHORT_MARACA,
        SlotPatch {
            noise: 0.7,
            ..SlotPatch::default()
        },
        &mut samples,
    );
    hit(
        ModelId::TRIPLE_PULSE_CLAP,
        SlotPatch::default(),
        &mut samples,
    );
    hit(
        ModelId::TRIPLE_PULSE_CLAP,
        SlotPatch {
            decay: -0.7,
            attack: -0.4,
            ..SlotPatch::default()
        },
        &mut samples,
    );
    hit(
        ModelId::TRIPLE_PULSE_CLAP,
        SlotPatch {
            decay: 0.7,
            ..SlotPatch::default()
        },
        &mut samples,
    );
    hit(
        ModelId::TRIPLE_PULSE_CLAP,
        SlotPatch {
            tone: 0.55,
            character: 0.5,
            ..SlotPatch::default()
        },
        &mut samples,
    );

    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/tr-808-noise-batch.wav");
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
