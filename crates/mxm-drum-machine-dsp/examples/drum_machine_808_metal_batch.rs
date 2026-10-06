//! Audition render for catalogue IDs 13–16, the shared six-square metallic family.
//!
//! Order: cowbell reference/shifted, cymbal reference/short, closed hat reference/bright, open hat
//! reference/long, then an open hat choked by a closed hat.

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::ModelId;
const FS: f32 = 48_000.0;
fn hit(model: ModelId, mut patch: SlotPatch, seconds: f32, out: &mut Vec<f32>) {
    patch.model = model;
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    let mut engine = Engine::new();
    engine.set_sample_rate(FS);
    engine.prepare(&patches);
    let mut trigger = TriggerGroup::new();
    trigger.push(0, 0.82);
    engine.trigger_group(&patches, trigger);
    for _ in 0..(seconds * FS) as usize {
        let frame = engine.process(&patches);
        out.push(0.7 * (frame[0] + frame[1]));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    hit(
        ModelId::TWIN_SQUARE_COWBELL,
        SlotPatch::default(),
        0.7,
        &mut out,
    );
    hit(
        ModelId::TWIN_SQUARE_COWBELL,
        SlotPatch {
            pitch_semitones: 5.0,
            character: 0.4,
            ..SlotPatch::default()
        },
        0.7,
        &mut out,
    );
    hit(
        ModelId::THREE_PATH_CYMBAL,
        SlotPatch::default(),
        1.5,
        &mut out,
    );
    hit(
        ModelId::THREE_PATH_CYMBAL,
        SlotPatch {
            decay: -0.7,
            tone: 0.5,
            ..SlotPatch::default()
        },
        1.0,
        &mut out,
    );
    hit(
        ModelId::SIX_SQUARE_CLOSED_HAT,
        SlotPatch::default(),
        0.5,
        &mut out,
    );
    hit(
        ModelId::SIX_SQUARE_CLOSED_HAT,
        SlotPatch {
            tone: 0.7,
            ..SlotPatch::default()
        },
        0.5,
        &mut out,
    );
    hit(
        ModelId::SIX_SQUARE_OPEN_HAT,
        SlotPatch::default(),
        1.0,
        &mut out,
    );
    hit(
        ModelId::SIX_SQUARE_OPEN_HAT,
        SlotPatch {
            decay: 0.8,
            ..SlotPatch::default()
        },
        1.1,
        &mut out,
    );
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0].model = ModelId::SIX_SQUARE_OPEN_HAT;
    patches[1].model = ModelId::SIX_SQUARE_CLOSED_HAT;
    let mut engine = Engine::new();
    engine.prepare(&patches);
    let mut open = TriggerGroup::new();
    open.push(0, 0.82);
    engine.trigger_group(&patches, open);
    for n in 0..(0.8 * FS) as usize {
        if n == (0.18 * FS) as usize {
            let mut closed = TriggerGroup::new();
            closed.push(1, 0.82);
            engine.trigger_group(&patches, closed);
        }
        let frame = engine.process(&patches);
        out.push(0.7 * (frame[0] + frame[1]));
    }
    let dir = "target/rendered/mxm-drum-machine";
    let path = format!("{dir}/tr-808-metal-batch.wav");
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
