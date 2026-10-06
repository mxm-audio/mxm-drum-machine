//! Reports the engine's render cost as a share of one core's realtime deadline.
//!
//! The catalogue's per-slot cost is not the interesting number: every model early-outs when its
//! circuit is silent, so an idle slot is a branch. What this example exists to measure is the cost
//! that does *not* depend on what is loaded — the machine-shared sources in `Engine::process_routed`
//! advance every sample whether or not a consumer reads them. `IDLE` below is that floor, measured
//! with all sixteen slots `Off`; every other scene is read against it.
//!
//! This is a wall-clock harness, not a test. It asserts nothing and gates nothing: figures move with
//! the machine, its thermal state and the optimiser. Compare scenes within one run rather than
//! across runs, and quote the run's own `IDLE` when reporting a change.
//!
//! Run it with optimisation, or the numbers describe a debug build and nothing else:
//!
//! ```text
//! cargo run --release --example drum_machine_cpu_cost -p mxm-drum-machine-dsp
//! ```
use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    engine::{Engine, SlotPatch, TriggerGroup},
    model::{AVAILABLE_MODELS, ModelId},
};
use std::time::{Duration, Instant};

/// Rates the plugin accepts, spanning the range the shared banks are specified over.
const RATES: [f32; 2] = [48_000.0, 192_000.0];
/// Rendered seconds per timed repetition. Long enough that a scene's own tails and envelopes are
/// inside the window rather than only its attack transient.
const SECONDS: f32 = 1.0;
/// Discarded warm-up seconds, so the figure describes steady state rather than first-touch cache
/// misses and the branch predictor learning one scene's dispatch.
const WARMUP_SECONDS: f32 = 0.5;
/// Timed repetitions per scene, of which the fastest is reported.
///
/// A single timing measures this engine plus whatever else the machine was doing, and on a loaded
/// or thermally throttling laptop that interference is larger than the differences being looked
/// for — it produced a run where removing work appeared to make the engine 45% slower. Interference
/// can only ever add time, so the minimum is the estimate least polluted by it. The spread against
/// the median is printed so a run too noisy to trust says so instead of reading as a result.
const REPEATS: usize = 7;

struct Scene {
    name: &'static str,
    /// Models to install, one per slot; `OFF` leaves the slot silent.
    models: [ModelId; SLOT_COUNT],
    /// Creative pitch deviation applied to every sounding slot. A nonzero value moves metal models
    /// off the machine-shared bank and onto their slot-private one, which is the expensive path.
    pitch_semitones: f32,
    /// Whether to strike the sounding slots. An untriggered slot measures dispatch alone.
    strike: bool,
    /// Re-strike at this rate in Hz, so the scene measures voices that are actually running.
    /// Without it every scene is dominated by decayed slots taking their early-out, which hides
    /// anything costing per active voice per sample. Expressed in Hz rather than samples so the
    /// scene is the same musical figure at every sample rate. 0 never re-strikes.
    retrigger_hz: f32,
}

fn main() {
    println!(
        "mxm-drum-machine render cost — fastest of {REPEATS} x {SECONDS:.0} s, {WARMUP_SECONDS:.1} s warm-up"
    );
    println!("Wall-clock, this machine, this build. Read scenes against IDLE within one run.");
    println!("`noise` is median/fastest: at 1.10 or above the machine was too busy to trust.\n");

    for rate in RATES {
        println!("=== {:.1} kHz ===", rate / 1_000.0);
        println!(
            "{:<34} {:>10} {:>12} {:>9} {:>8}",
            "scene", "ns/sample", "% of core", "vs idle", "noise"
        );
        let mut idle_ns = 0.0_f64;
        for scene in scenes() {
            let (ns_per_sample, noise) = measure(&scene, rate);
            // One core's realtime budget is one sample period. The share is how much of that period
            // this scene spends, so 100% means the engine alone saturates a core at this rate.
            let share = ns_per_sample * f64::from(rate) / 1.0e9 * 100.0;
            let idle = scene.name.starts_with("IDLE");
            if idle {
                idle_ns = ns_per_sample;
            }
            let relative = if idle {
                "—".to_string()
            } else {
                format!("{:.1}x", ns_per_sample / idle_ns.max(f64::EPSILON))
            };
            println!(
                "{:<34} {ns_per_sample:>10.1} {share:>11.2}% {relative:>9} {noise:>8.2}",
                scene.name
            );
        }
        println!();
    }
}

/// Renders one scene `REPEATS` times, returning the fastest ns/sample and the median's ratio to it.
fn measure(scene: &Scene, rate: f32) -> (f64, f64) {
    let samples = f64::from(SECONDS * rate);
    let mut timings: Vec<f64> = (0..REPEATS)
        .map(|_| render_once(scene, rate).as_secs_f64() * 1.0e9 / samples)
        .collect();
    timings.sort_by(f64::total_cmp);
    let fastest = timings[0];
    let median = timings[REPEATS / 2];
    (fastest, median / fastest.max(f64::EPSILON))
}

/// Renders one scene once and returns the time spent in `process` alone.
fn render_once(scene: &Scene, rate: f32) -> Duration {
    let mut engine = Engine::new();
    engine.set_sample_rate(rate);

    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    for (patch, &model) in patches.iter_mut().zip(&scene.models) {
        patch.model = model;
        patch.pitch_semitones = scene.pitch_semitones;
    }
    engine.prepare(&patches);

    if scene.strike {
        let mut group = TriggerGroup::new();
        for (index, &model) in scene.models.iter().enumerate() {
            if model != ModelId::OFF {
                group.push(index, 0.82);
            }
        }
        engine.trigger_group(&patches, group);
    }

    for _ in 0..(WARMUP_SECONDS * rate) as usize {
        std::hint::black_box(engine.process(&patches));
    }

    // Re-strike so the measured window opens on a fresh hit rather than on whatever survived the
    // warm-up; a scene whose models decayed during warm-up would otherwise measure its own silence.
    if scene.strike {
        let mut group = TriggerGroup::new();
        for (index, &model) in scene.models.iter().enumerate() {
            if model != ModelId::OFF {
                group.push(index, 0.82);
            }
        }
        engine.trigger_group(&patches, group);
    }

    let retrigger_interval = if scene.retrigger_hz > 0.0 {
        (rate / scene.retrigger_hz).round().max(1.0) as usize
    } else {
        0
    };
    let start = Instant::now();
    for sample in 0..(SECONDS * rate) as usize {
        if retrigger_interval > 0 && sample % retrigger_interval == 0 {
            let mut group = TriggerGroup::new();
            for (index, &model) in scene.models.iter().enumerate() {
                if model != ModelId::OFF {
                    group.push(index, 0.82);
                }
            }
            engine.trigger_group(&patches, group);
        }
        std::hint::black_box(engine.process(&patches));
    }
    start.elapsed()
}

fn scenes() -> Vec<Scene> {
    let filled = |model: ModelId| [model; SLOT_COUNT];
    let mut single = [ModelId::OFF; SLOT_COUNT];
    single[0] = ModelId::THREE_PATH_CYMBAL;

    vec![
        Scene {
            name: "IDLE — sixteen slots Off",
            models: [ModelId::OFF; SLOT_COUNT],
            pitch_semitones: 0.0,
            strike: false,
            retrigger_hz: 0.0,
        },
        Scene {
            name: "one cymbal, struck",
            models: single,
            pitch_semitones: 0.0,
            strike: true,
            retrigger_hz: 0.0,
        },
        Scene {
            name: "16x kick, struck",
            models: filled(ModelId::DEEP_BRIDGE_KICK),
            pitch_semitones: 0.0,
            strike: true,
            retrigger_hz: 0.0,
        },
        Scene {
            name: "16x cymbal, shared bank",
            models: filled(ModelId::THREE_PATH_CYMBAL),
            pitch_semitones: 0.0,
            strike: true,
            retrigger_hz: 0.0,
        },
        // The worst case the architecture allows: every slot pitched off the shared bank, so each
        // runs its own six-square metal oscillator set on top of the machine-shared one.
        Scene {
            name: "16x cymbal, slot-private banks",
            models: filled(ModelId::THREE_PATH_CYMBAL),
            pitch_semitones: 5.0,
            strike: true,
            retrigger_hz: 0.0,
        },
        Scene {
            name: "16x distinct, struck",
            models: sixteen_distinct(),
            pitch_semitones: 0.0,
            strike: true,
            retrigger_hz: 0.0,
        },
        // The scene that actually holds sixteen voices open: a hit on every slot at 10 Hz, so the
        // window is running circuits rather than early-outs.
        Scene {
            name: "16x distinct, re-struck at 10 Hz",
            models: sixteen_distinct(),
            pitch_semitones: 0.0,
            strike: true,
            retrigger_hz: 10.0,
        },
    ]
}

/// Sixteen different models spread across the catalogue, as a kit-shaped load rather than sixteen
/// copies of one circuit sharing one branch history.
fn sixteen_distinct() -> [ModelId; SLOT_COUNT] {
    let mut models = [ModelId::OFF; SLOT_COUNT];
    let stride = AVAILABLE_MODELS.len() / SLOT_COUNT;
    for (index, model) in models.iter_mut().enumerate() {
        *model = AVAILABLE_MODELS[index * stride].id;
    }
    models
}
