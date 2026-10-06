//! Audition render for the owner-selected NMF-resynthesized PCM IDs 24–27.
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::ModelId,
};
const FS: f32 = 48000.0;
fn hit(m: ModelId, p: SlotPatch, out: &mut Vec<f32>) {
    let mut ps = [SlotPatch::default(); SLOT_COUNT];
    ps[0] = SlotPatch { model: m, ..p };
    let mut e = Engine::new();
    e.prepare(&ps);
    let mut t = TriggerGroup::new();
    t.push(0, 0.85);
    e.trigger_group(&ps, t);
    for _ in 0..(2.8 * FS) as usize {
        let x = e.process(&ps);
        out.push(0.7 * (x[0] + x[1]));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut o = Vec::new();
    for m in [
        ModelId::SIX_BIT_CLOSED_HAT,
        ModelId::SIX_BIT_OPEN_HAT,
        ModelId::SIX_BIT_CRASH,
        ModelId::SIX_BIT_RIDE,
    ] {
        hit(m, SlotPatch::default(), &mut o);
        hit(
            m,
            SlotPatch {
                pitch_semitones: 5.0,
                decay: 0.35,
                tone: 0.25,
                character: 0.3,
                ..SlotPatch::default()
            },
            &mut o,
        )
    }
    let d = "target/rendered/mxm-drum-machine";
    let p = format!("{d}/tr-909-nmf-pcm-family.wav");
    std::fs::create_dir_all(d)?;
    mxm_audio_file::write(&p, &o, 1, FS as u32, mxm_audio_file::Target::WavFloat32)?;
    println!("wrote {p}");
    Ok(())
}
