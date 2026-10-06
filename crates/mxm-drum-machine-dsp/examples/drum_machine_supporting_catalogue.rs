//! One reference hit for every supporting-machine model, IDs 28–94, in permanent ID order.
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::ModelId,
};
const FS: f32 = 48000.0;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for id in 28..=94 {
        let mut ps = [SlotPatch::default(); SLOT_COUNT];
        ps[0].model = ModelId::new(id);
        let mut e = Engine::new();
        e.prepare(&ps);
        let mut t = TriggerGroup::new();
        t.push(0, 0.82);
        e.trigger_group(&ps, t);
        let hit = (FS * 0.65) as usize;
        let fade = (FS * 0.005) as usize;
        for n in 0..hit {
            let x = e.process(&ps);
            // The longest rings are still audible at 650 ms, and a step into the next hit clicks.
            let taper = if n + fade >= hit {
                let phase = std::f32::consts::PI * (hit - n) as f32 / fade as f32;
                0.5 - 0.5 * phase.cos()
            } else {
                1.0
            };
            out.push(0.68 * taper * (x[0] + x[1]));
        }
    }
    let d = "target/rendered/mxm-drum-machine";
    let p = format!("{d}/supporting-catalogue-28-94.wav");
    std::fs::create_dir_all(d)?;
    mxm_audio_file::write(&p, &out, 1, FS as u32, mxm_audio_file::Target::WavFloat32)?;
    println!("wrote {p}");
    Ok(())
}
