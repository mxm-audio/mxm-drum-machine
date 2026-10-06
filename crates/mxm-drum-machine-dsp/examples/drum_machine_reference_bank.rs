//! Writes one isolated zero-deviation render per available model for external-reference calibration.
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::AVAILABLE_MODELS,
};
const FS: f32 = 48_000.0;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = "target/rendered/mxm-drum-machine/reference-bank";
    std::fs::create_dir_all(dir)?;
    for spec in &AVAILABLE_MODELS {
        let mut patches = [SlotPatch::default(); SLOT_COUNT];
        patches[0].model = spec.id;
        let mut engine = Engine::new();
        engine.prepare(&patches);
        let mut group = TriggerGroup::new();
        group.push(0, 0.82);
        engine.trigger_group(&patches, group);
        // Eight seconds retains the calibrated centre-position cymbal and clap tails. A shorter
        // fixed window silently truncated the very evidence this bank exists to compare.
        let mut audio = Vec::with_capacity((FS * 8.0) as usize);
        for _ in 0..audio.capacity() {
            let frame = engine.process(&patches);
            audio.push(0.68 * (frame[0] + frame[1]));
        }
        let slug = spec.label.to_ascii_lowercase().replace(' ', "-");
        let path = format!("{dir}/{:03}-{slug}.wav", spec.id.raw());
        mxm_audio_file::write(
            &path,
            &audio,
            1,
            FS as u32,
            mxm_audio_file::Target::WavFloat32,
        )?;
    }
    println!("wrote 94 isolated renders to {dir}");
    Ok(())
}
