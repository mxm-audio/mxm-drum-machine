//! Audition render for IDs 17–18: reset-VCO kick and twin-VCO snare.
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
    for _ in 0..(0.8 * FS) as usize {
        let x = e.process(&ps);
        out.push(0.7 * (x[0] + x[1]));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    hit(ModelId::RESET_PUNCH_KICK, SlotPatch::default(), &mut out);
    hit(
        ModelId::RESET_PUNCH_KICK,
        SlotPatch {
            attack: 0.7,
            noise: 0.5,
            ..SlotPatch::default()
        },
        &mut out,
    );
    hit(
        ModelId::RESET_PUNCH_KICK,
        SlotPatch {
            decay: 0.7,
            pitch_semitones: -4.0,
            ..SlotPatch::default()
        },
        &mut out,
    );
    hit(ModelId::RESET_TWIN_SNARE, SlotPatch::default(), &mut out);
    hit(
        ModelId::RESET_TWIN_SNARE,
        SlotPatch {
            noise: 0.7,
            attack: 0.5,
            ..SlotPatch::default()
        },
        &mut out,
    );
    hit(
        ModelId::RESET_TWIN_SNARE,
        SlotPatch {
            tone: 0.7,
            body: 0.5,
            pitch_semitones: 4.0,
            ..SlotPatch::default()
        },
        &mut out,
    );
    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/tr-909-reset-pair.wav");
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
