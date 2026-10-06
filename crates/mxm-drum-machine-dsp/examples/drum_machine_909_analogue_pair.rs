//! Audition render for IDs 22–23: reference and shaped hits.
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::ModelId,
};
const FS: f32 = 48_000.0;
fn hit(model: ModelId, p: SlotPatch, out: &mut Vec<f32>) {
    let mut ps = [SlotPatch::default(); SLOT_COUNT];
    ps[0] = SlotPatch { model, ..p };
    let mut e = Engine::new();
    e.prepare(&ps);
    let mut t = TriggerGroup::new();
    t.push(0, 0.85);
    e.trigger_group(&ps, t);
    for _ in 0..(0.9 * FS) as usize {
        let x = e.process(&ps);
        out.push(0.7 * (x[0] + x[1]));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for m in [ModelId::TRIPLE_RESONATOR_RIM, ModelId::FOUR_CELL_CLAP] {
        hit(m, SlotPatch::default(), &mut out);
        hit(
            m,
            SlotPatch {
                decay: 0.4,
                attack: 0.35,
                tone: 0.3,
                character: 0.35,
                ..SlotPatch::default()
            },
            &mut out,
        );
    }
    let d = "target/rendered/mxm-drum-machine";
    let p = format!("{d}/tr-909-analogue-pair.wav");
    std::fs::create_dir_all(d)?;
    mxm_audio_file::write(&p, &out, 1, FS as u32, mxm_audio_file::Target::WavFloat32)?;
    println!("wrote {p}");
    Ok(())
}
