//! Audition render for catalogue IDs 3–10, the remaining 808-family struck resonators.
//!
//! Each model renders at reference, then with an audible creative deviation. Hardware fidelity is
//! unverified; this file exists for the required recognisability pass.

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::ModelId;

const SAMPLE_RATE: f32 = 48_000.0;
const HIT_SECONDS: f32 = 0.60;

fn render_hit(model: ModelId, mut patch: SlotPatch, output: &mut Vec<f32>) {
    patch.model = model;
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    let mut engine = Engine::new();
    engine.set_sample_rate(SAMPLE_RATE);
    engine.prepare(&patches);
    let mut triggers = TriggerGroup::new();
    triggers.push(0, 0.82);
    engine.trigger_group(&patches, triggers);
    for _ in 0..(HIT_SECONDS * SAMPLE_RATE) as usize {
        let [left, right] = engine.process(&patches);
        output.push(0.7 * (left + right));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        ModelId::LOW_FALLING_TOM,
        ModelId::LOW_FALLING_CONGA,
        ModelId::MID_FALLING_TOM,
        ModelId::MID_FALLING_CONGA,
        ModelId::HIGH_FALLING_TOM,
        ModelId::HIGH_FALLING_CONGA,
        ModelId::LAYERED_SHORT_RIM,
        ModelId::PURE_HIGH_CLAVE,
    ];
    let mut samples = Vec::new();
    for model in models {
        render_hit(model, SlotPatch::default(), &mut samples);
        render_hit(
            model,
            SlotPatch {
                pitch_semitones: 5.0,
                decay: 0.45,
                tone: 0.35,
                body: 0.4,
                noise: 0.4,
                character: 0.35,
                ..SlotPatch::default()
            },
            &mut samples,
        );
    }

    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/tr-808-resonator-batch.wav");
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
