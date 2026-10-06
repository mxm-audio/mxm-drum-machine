//! The rest pitch of every pitched model, measured from its own render.
//!
//! A drum's note is where its pitch comes to rest, not an average over the hit: a kick starts with a
//! sweep and settles, and noise and a short decay hide the tone that remains. So each model is
//! measured in an **analysis patch** that isolates the tone — noise and its decay down, the pitch
//! sweep off, the body's decay up — over the late, stable part of the ring, and the two halves of
//! that window must agree before the reading counts.

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, ModelId, ModelSpec};

const RATE: f64 = 48_000.0;
/// Long enough that the late window starts well after any sweep, even at the longest decay.
const FRAMES: usize = 72_000;
const FFT_LEN: usize = 1 << 16;

/// Hann-windowed, zero-padded magnitude spectrum of `x[window]`, and its bin width.
fn spectrum(x: &[f32], window: std::ops::Range<usize>) -> (Vec<f64>, f64) {
    let segment = &x[window];
    let mut re = vec![0.0_f64; FFT_LEN];
    let mut im = vec![0.0_f64; FFT_LEN];
    let n = segment.len();
    for (index, sample) in segment.iter().enumerate() {
        let hann = 0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / (n - 1) as f64).cos();
        re[index] = f64::from(*sample) * hann;
    }
    mxm_measure::spectrum::fft(&mut re, &mut im).expect("a power-of-two transform");
    let magnitude = re[..FFT_LEN / 2]
        .iter()
        .zip(&im[..FFT_LEN / 2])
        .map(|(r, i)| (r * r + i * i).sqrt())
        .collect();
    (magnitude, RATE / FFT_LEN as f64)
}

/// A peak's frequency, refined by a parabola through the log magnitudes around it.
fn interpolated(magnitude: &[f64], bin: usize, bin_hz: f64) -> f64 {
    let (a, b, c) = (
        magnitude[bin - 1].max(1e-30).ln(),
        magnitude[bin].max(1e-30).ln(),
        magnitude[bin + 1].max(1e-30).ln(),
    );
    (bin as f64 + 0.5 * (a - c) / (a - 2.0 * b + c)) * bin_hz
}

/// The lowest spectral peak within 12 dB of the strongest between 25 Hz and 12 kHz: the partial
/// that names a pitched drum's note, and the lower tone of a two-tone cowbell.
fn principal_hz(x: &[f32], window: std::ops::Range<usize>) -> f64 {
    let (magnitude, bin_hz) = spectrum(x, window);
    let lo = (25.0 / bin_hz) as usize;
    let hi = (12_000.0 / bin_hz) as usize;
    let strongest = magnitude[lo..hi].iter().copied().fold(0.0, f64::max);
    let threshold = strongest * 10.0_f64.powf(-12.0 / 20.0);
    let bin = (lo + 1..hi - 1)
        .find(|&k| {
            magnitude[k] >= threshold
                && magnitude[k] >= magnitude[k - 1]
                && magnitude[k] > magnitude[k + 1]
        })
        .expect("a principal partial");
    interpolated(&magnitude, bin, bin_hz)
}

/// The strongest peak within `cents` of `expected_hz`.
fn peak_near_hz(x: &[f32], window: std::ops::Range<usize>, expected_hz: f64, cents: f64) -> f64 {
    let (magnitude, bin_hz) = spectrum(x, window);
    let lo = ((expected_hz * 2f64.powf(-cents / 1200.0)) / bin_hz) as usize;
    let hi = ((expected_hz * 2f64.powf(cents / 1200.0)) / bin_hz) as usize + 1;
    let bin = (lo.max(1)..hi.min(magnitude.len() - 1))
        .max_by(|a, b| magnitude[*a].total_cmp(&magnitude[*b]))
        .expect("a peak near the expected partial");
    interpolated(&magnitude, bin, bin_hz)
}

fn cents(measured: f64, expected: f64) -> f64 {
    1200.0 * (measured / expected).log2()
}

/// A patch that isolates a model's tone for measurement: noise and its decay down, the pitch
/// sweep's time down, the body's decay up. `sweep` is the Pitch envelope deviation: −1 removes a native
/// sweep, while 0 is already sweep-free where the family's sweep is additive.
fn analysis_patch(model: ModelId, sweep: f32, pitch_semitones: f32) -> SlotPatch {
    SlotPatch {
        model,
        pitch_semitones,
        pitch_envelope: sweep,
        pitch_decay: -1.0,
        decay: 1.0,
        noise: -1.0,
        noise_decay: -1.0,
        pan: -1.0,
        ..SlotPatch::default()
    }
}

/// One struck hit of `patch` in slot 1, hard left so the main left channel is its mono signal.
fn render_patch(patch: SlotPatch, frames: usize) -> Vec<f32> {
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0] = patch;
    let mut engine = Engine::new();
    engine.set_sample_rate(RATE as f32);
    engine.prepare(&patches);
    let mut trigger = TriggerGroup::new();
    trigger.push(0, 0.82);
    engine.trigger_group(&patches, trigger);
    (0..frames).map(|_| engine.process(&patches)[0]).collect()
}

/// The late, stable part of a ring: 40–95% of the way from its peak to its −40 dB point.
fn rest_window(x: &[f32]) -> std::ops::Range<usize> {
    const BLOCK: usize = 48;
    let envelope: Vec<f32> = x
        .chunks(BLOCK)
        .map(|c| c.iter().fold(0.0_f32, |m, s| m.max(s.abs())))
        .collect();
    let (peak_block, peak) = envelope
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a rendered hit");
    let end_block = envelope[peak_block..]
        .iter()
        .position(|e| *e < 0.01 * peak)
        .map_or(envelope.len(), |offset| peak_block + offset);
    let span = end_block - peak_block;
    let start = (peak_block + span * 40 / 100) * BLOCK;
    let end = ((peak_block + span * 95 / 100) * BLOCK)
        .min(start + FFT_LEN / 2)
        .max(start + 256);
    start..end.min(x.len())
}

/// The rest pitch and how far it drifts between the window's two halves, in cents.
fn rest_pitch(x: &[f32], window: std::ops::Range<usize>) -> (f64, f64) {
    let hz = principal_hz(x, window.clone());
    let middle = window.start + window.len() / 2;
    let first = peak_near_hz(x, window.start..middle, hz, 50.0);
    let second = peak_near_hz(x, middle..window.end, hz, 50.0);
    (hz, cents(second, first))
}

fn is_pitched(spec: &ModelSpec) -> bool {
    let label = spec.label.to_lowercase();
    spec.id.capabilities().pitch
        && ![
            "hat",
            "cymbal",
            "crash",
            "ride",
            "clap",
            "maraca",
            "tambourine",
            "guiro",
            "brush",
        ]
        .iter()
        .any(|noise| label.contains(noise))
}

fn analyse(model: ModelId) -> Analysis {
    // A native sweep is removed at −1; an additive one is already absent at 0 and inverted at −1.
    // Take whichever leaves the steadier ring.
    [-1.0_f32, 0.0]
        .into_iter()
        .map(|sweep| {
            let x = render_patch(analysis_patch(model, sweep, 0.0), FRAMES);
            let window = rest_window(&x);
            let (hz, drift) = rest_pitch(&x, window.clone());
            Analysis {
                sweep,
                hz,
                drift,
                window,
            }
        })
        .min_by(|a, b| a.drift.abs().total_cmp(&b.drift.abs()))
        .expect("two candidates")
}

/// Runs `each` for every pitched model on its own thread — the renders are independent and a debug
/// build renders slowly — and returns the models in catalogue order with their results.
fn per_pitched_model<T: Send>(
    each: impl Fn(&ModelSpec) -> T + Sync,
) -> Vec<(&'static ModelSpec, T)> {
    std::thread::scope(|scope| {
        let each = &each;
        AVAILABLE_MODELS
            .iter()
            .filter(|spec| is_pitched(spec))
            .map(|spec| (spec, scope.spawn(move || each(spec))))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(spec, handle)| (spec, handle.join().expect("a measurement thread")))
            .collect()
    })
}

/// Every pitched model's analysis, computed once for the whole test binary.
fn analysed(model: ModelId) -> &'static Analysis {
    static ALL: std::sync::OnceLock<Vec<(ModelId, Analysis)>> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| {
        per_pitched_model(|spec| analyse(spec.id))
            .into_iter()
            .map(|(spec, analysis)| (spec.id, analysis))
            .collect()
    });
    &all.iter()
        .find(|(id, _)| *id == model)
        .expect("a pitched model")
        .1
}

/// Asserts that every per-model check passed, naming every failure rather than the first.
fn all_pass(results: Vec<(&'static ModelSpec, Vec<String>)>) {
    let failures: Vec<String> = results
        .into_iter()
        .flat_map(|(spec, problems)| {
            problems
                .into_iter()
                .map(move |problem| format!("model {} ({}): {problem}", spec.id.raw(), spec.label))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

struct Analysis {
    sweep: f32,
    hz: f64,
    drift: f64,
    window: std::ops::Range<usize>,
}

#[test]
fn a_model_has_a_note_exactly_when_it_is_pitched() {
    for spec in &AVAILABLE_MODELS {
        assert_eq!(
            spec.id.reference_pitch_hz().is_some(),
            is_pitched(spec),
            "model {} ({})",
            spec.id.raw(),
            spec.label
        );
        let key = spec.id.chromatic_reference_key();
        match spec.id.reference_pitch_hz() {
            Some(hz) => assert!(
                (440.0 * 2f32.powf((key - 69.0) / 12.0) - hz).abs() < 0.001 * hz,
                "model {} key {key}",
                spec.id.raw()
            ),
            None => assert_eq!(key, 60.0, "model {}", spec.id.raw()),
        }
    }
}

#[test]
fn every_reference_pitch_is_the_models_stable_rest_pitch() {
    all_pass(per_pitched_model(|spec| {
        let table = f64::from(spec.id.reference_pitch_hz().expect("a pitched model"));
        let analysis = analysed(spec.id);
        let mut problems = Vec::new();
        if analysis.drift.abs() > 5.0 {
            problems.push(format!(
                "never settles: {:+.1} cents across its rest window",
                analysis.drift
            ));
        }
        let error = cents(analysis.hz, table);
        if error.abs() > 1.0 {
            problems.push(format!(
                "rests at {:.2} Hz against {table} Hz: {error:+.1} cents",
                analysis.hz
            ));
        }
        problems
    }));
}

#[test]
fn the_analysis_controls_do_not_move_the_rest_pitch() {
    // Noise and decay only reveal the tone. Restoring the noise, or taking a tenth of the added
    // decay back, must leave the rest pitch where it was, or the table would describe the analysis
    // rather than the drum. Each variant must itself ring long enough to settle, so a difference
    // is a pitch that moved, not a window that caught the sweep: at its stock decay a kick can
    // leave under two cycles, which is why the analysis lengthens it.
    all_pass(per_pitched_model(|spec| {
        let analysis = analysed(spec.id);
        let mut problems = Vec::new();
        for (decay, noise) in [(1.0_f32, 0.0_f32), (0.9, -1.0)] {
            let patch = SlotPatch {
                decay,
                noise,
                noise_decay: noise,
                ..analysis_patch(spec.id, analysis.sweep, 0.0)
            };
            let x = render_patch(patch, FRAMES);
            let window = rest_window(&x);
            let (_, drift) = rest_pitch(&x, window.clone());
            if drift.abs() > 5.0 {
                problems.push(format!(
                    "never settles with decay {decay:+} and noise {noise:+}"
                ));
                continue;
            }
            let error = cents(peak_near_hz(&x, window, analysis.hz, 50.0), analysis.hz);
            if error.abs() > 2.0 {
                problems.push(format!(
                    "rests {error:+.1} cents away with decay {decay:+} and noise {noise:+}"
                ));
            }
        }
        problems
    }));
}

#[test]
fn pitched_models_play_in_tune_an_octave_either_way() {
    all_pass(per_pitched_model(|spec| {
        let analysis = analysed(spec.id);
        [-12.0_f64, 12.0]
            .into_iter()
            .filter_map(|offset| {
                let x = render_patch(
                    analysis_patch(spec.id, analysis.sweep, offset as f32),
                    FRAMES,
                );
                let expected = analysis.hz * 2f64.powf(offset / 12.0);
                let error = cents(
                    peak_near_hz(&x, analysis.window.clone(), expected, 100.0),
                    expected,
                );
                (error.abs() > 5.0)
                    .then(|| format!("{offset:+} semitones is {error:+.1} cents out"))
            })
            .collect()
    }));
}

#[test]
#[ignore = "prints the measured table; run with --release -- --ignored --nocapture"]
fn print_the_measured_reference_pitches() {
    for spec in AVAILABLE_MODELS.iter().filter(|spec| is_pitched(spec)) {
        let analysis = analysed(spec.id);
        println!(
            "        {} => Some({:.2}), // {}",
            spec.id.raw(),
            analysis.hz,
            spec.label
        );
    }
}
