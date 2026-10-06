//! Audition render for IDs 19–21: three reset-VCO tom calibrations, reference then shaped.
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::ModelId,
};
const FS: f32 = 48_000.0;
fn hit(model: ModelId, mut p: SlotPatch, out: &mut Vec<f32>) {
    p.model = model;
    let mut ps = [SlotPatch::default(); SLOT_COUNT];
    ps[0] = p;
    let mut e = Engine::new();
    e.prepare(&ps);
    let mut t = TriggerGroup::new();
    t.push(0, 0.82);
    e.trigger_group(&ps, t);
    for _ in 0..(0.75 * FS) as usize {
        let x = e.process(&ps);
        out.push(0.7 * (x[0] + x[1]));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for model in [
        ModelId::LOW_RESET_TRIAD_TOM,
        ModelId::MID_RESET_TRIAD_TOM,
        ModelId::HIGH_RESET_TRIAD_TOM,
    ] {
        hit(model, SlotPatch::default(), &mut out);
        hit(
            model,
            SlotPatch {
                pitch_semitones: 5.0,
                decay: 0.45,
                attack: 0.35,
                noise: 0.4,
                character: 0.4,
                ..SlotPatch::default()
            },
            &mut out,
        );
    }
    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/tr-909-reset-toms.wav");
    std::fs::create_dir_all(dir)?;
    mxm_audio_file::write(
        &path,
        &out,
        1,
        FS as u32,
        mxm_audio_file::Target::WavFloat32,
    )?;
    println!("wrote {path}");
    Ok(())
}
