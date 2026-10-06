//! Builds a private, local A/B comparison page: every model at its reference setting against the
//! hardware recording it was fitted to. The method, the mapping file's shape and the other tools are
//! `docs/drum-model-fitting.md`.
//!
//! ```text
//! cargo run -p mxm-drum-machine-dsp --release --example drum_machine_ab_page -- <mapping.json> <site dir>
//! ```
//!
//! Local only: the recordings are third-party, so neither the site nor the mapping (absolute paths
//! into private folders) is ever committed or published.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mxm_audio_file::{Bits, Target};
use mxm_drum_machine_dsp::SLOT_COUNT;
// The page's decode, trim and body loudness live in the listener, so the page and every listening
// report read a sound the same way (`plans/plan-mxm-listening.md` §1).
use mxm_drum_machine_dsp::engine::{Engine, SlotPatch, TriggerGroup};
use mxm_drum_machine_dsp::model::{AVAILABLE_MODELS, ModelId};
use mxm_listening::prep::{body_rms, decode_mono, peak, trim};

const RATE: u32 = 48_000;

struct Entry {
    id: u8,
    label: String,
    group: String,
    machine: String,
    voice: String,
    reference: Option<String>,
    source: String,
    note: String,
}

fn text(value: &serde_json::Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// The reference hit every calibration used: zero deviation, velocity 0.82, alone in a slot.
fn render(model: ModelId) -> Vec<f32> {
    let mut patches = [SlotPatch::default(); SLOT_COUNT];
    patches[0].model = model;
    patches[0].pan = -1.0;
    let mut engine = Engine::new();
    engine.set_sample_rate(RATE as f32);
    engine.prepare(&patches);
    // Five milliseconds of silence before the hit, so the file starts at zero. Trimming to the
    // onset used to leave the model's first sample at -39 dB, and the owner heard that step as a
    // click on a kick with no noise to mask it.
    let mut audio: Vec<f32> = (0..RATE as usize / 200)
        .map(|_| engine.process(&patches)[0])
        .collect();
    let mut trigger = TriggerGroup::new();
    trigger.push(0, 0.82);
    engine.trigger_group(&patches, trigger);
    audio.extend((0..RATE as usize * 8).map(|_| engine.process(&patches)[0]));
    audio
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mapping_path = PathBuf::from(args.next().expect("mapping JSON"));
    let out = PathBuf::from(args.next().expect("output directory"));
    let audio = out.join("audio");
    std::fs::create_dir_all(&audio)?;

    // Every rebuild gets its own URLs: a browser that cached the last render would otherwise play
    // yesterday's drum, and the owner would hear no difference after a fix.
    let version = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mapping: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mapping_path)?)?;
    let entries: Vec<Entry> = mapping
        .as_array()
        .expect("an array")
        .iter()
        .map(|v| Entry {
            id: v["id"].as_u64().expect("id") as u8,
            label: text(v, "label"),
            group: text(v, "group"),
            machine: text(v, "machine"),
            voice: text(v, "voice"),
            reference: v["reference"].as_str().map(str::to_owned),
            source: text(v, "source"),
            note: text(v, "note"),
        })
        .collect();
    assert_eq!(entries.len(), AVAILABLE_MODELS.len());

    let mut sections: Vec<(String, String, String)> = Vec::new();
    let mut matched = 0;
    for (entry, spec) in entries.iter().zip(AVAILABLE_MODELS.iter()) {
        assert_eq!(entry.id, spec.id.raw(), "mapping out of order");
        let ours = trim(&render(spec.id), RATE);
        let reference = match &entry.reference {
            Some(path) => match decode_mono(Path::new(path)) {
                Ok((x, rate)) => Some((trim(&x, rate), rate)),
                Err(error) => {
                    eprintln!("model {}: cannot decode {path}: {error}", entry.id);
                    None
                }
            },
            None => None,
        };
        let (mut ours, reference) = match reference {
            Some((mut theirs, rate)) => {
                // Loudness-match the original to the model over the hit's body, then give both one
                // common gain that keeps the louder peak at -1 dBFS.
                let gain = body_rms(&ours, RATE) / body_rms(&theirs, rate).max(1e-9);
                theirs.iter_mut().for_each(|s| *s *= gain);
                let common = (0.89 / peak(&ours).max(peak(&theirs))).min(1.0);
                theirs.iter_mut().for_each(|s| *s *= common);
                let ours: Vec<f32> = ours.iter().map(|s| s * common).collect();
                matched += 1;
                (ours, Some((theirs, rate)))
            }
            None => {
                let common = (0.89 / peak(&ours)).min(1.0);
                (ours.iter().map(|s| s * common).collect(), None)
            }
        };
        ours.truncate(RATE as usize * 8);
        let a = format!("{:03}-model.wav", entry.id);
        mxm_audio_file::write(
            audio.join(&a),
            &ours,
            1,
            RATE,
            Target::Wav(Bits::TwentyFour),
        )?;
        let b = match &reference {
            Some((theirs, rate)) => {
                let name = format!("{:03}-original.wav", entry.id);
                mxm_audio_file::write(
                    audio.join(&name),
                    theirs,
                    1,
                    *rate,
                    Target::Wav(Bits::TwentyFour),
                )?;
                Some(name)
            }
            None => None,
        };

        let mut row = String::new();
        write!(
            row,
            r#"<article class="row" data-id="{id}"{blindable}>
  <div class="name"><span class="id">{id:02}</span> {label}<div class="voice">{voice}</div><div class="rv-cell" data-id="{id}"></div></div>
  <div class="play">"#,
            id = entry.id,
            label = html_escape(&entry.label),
            voice = html_escape(&entry.voice),
            blindable = if b.is_some() {
                ""
            } else {
                r#" data-single="1""#
            },
        )?;
        write!(
            row,
            r#"<button class="pa" data-src="audio/{a}?v={version}">Model</button>"#
        )?;
        match &b {
            Some(b) => write!(
                row,
                r#"<button class="pb" data-src="audio/{b}?v={version}">Original</button><button class="both" data-first="audio/{a}?v={version}" data-second="audio/{b}?v={version}">Both</button>"#
            )?,
            None => write!(
                row,
                r#"<span class="none">No recording of this voice</span>"#
            )?,
        }
        write!(
            row,
            r#"</div>
  <div class="source">{source}{note}</div>
  <details class="wave"><summary>Waveforms</summary>{figures}</details>
</article>
"#,
            figures = ["onset", "cycles", "whole", "spectrogram"]
                .iter()
                .map(|view| format!(
                    r#"<div class="figure"><div class="sheet" data-id="{id:03}" data-view="{view}"><img src="images/{id:03}-{view}.png?v={version}" loading="lazy" alt="{label} {view} waveforms"><canvas></canvas></div><div class="tools"><button class="undo">Undo</button><button class="clear">Clear</button><input class="note" placeholder="What is wrong here?"><button class="send">Send to Claude</button><span class="status"></span></div></div>"#,
                    id = entry.id,
                    label = html_escape(&entry.label),
                ))
                .collect::<String>(),
            source = html_escape(&entry.source),
            note = if entry.note.is_empty() { String::new() } else { format!(r#"<div class="note">{}</div>"#, html_escape(&entry.note)) },
        )?;
        match sections
            .iter_mut()
            .find(|(group, _, _)| *group == entry.group)
        {
            Some((_, _, rows)) => rows.push_str(&row),
            None => sections.push((entry.group.clone(), entry.machine.clone(), row)),
        }
    }

    let mut body = String::new();
    let mut nav = String::new();
    for (group, machine, rows) in &sections {
        let anchor = group.to_lowercase().replace(' ', "-");
        write!(nav, r##"<a href="#{anchor}">{}</a>"##, html_escape(group))?;
        write!(
            body,
            r#"<section id="{anchor}"><h2>{} <span class="machine">{}</span></h2>
{rows}</section>
"#,
            html_escape(group),
            html_escape(machine)
        )?;
    }
    let page = TEMPLATE
        .replace("{{NAV}}", &nav)
        .replace("{{BODY}}", &body)
        .replace("{{MATCHED}}", &matched.to_string())
        .replace("{{TOTAL}}", &entries.len().to_string())
        .replace("{{VERSION}}", &version.to_string());
    // The page reads this back every twenty seconds. A page left open across a rebuild keeps its
    // own decoded buffers, so the owner can approve a drum while hearing the build before it —
    // which happened once, and cost a verdict on the 606 closed hat.
    std::fs::write(out.join("build.txt"), version.to_string())?;
    std::fs::write(out.join("index.html"), page)?;
    println!(
        "{matched} of {} models have an original; wrote {}",
        entries.len(),
        out.display()
    );
    Ok(())
}

const TEMPLATE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Drum A/B</title>
<style>
:root { --bg:#f6f7f9; --card:#fff; --text:#1b1f24; --muted:#5d6670; --line:#dde1e6; --accent:#247f91; --a:#247f91; --b:#b0602a; }
@media (prefers-color-scheme: dark) { :root { --bg:#121417; --card:#1b1f24; --text:#e8eaed; --muted:#9aa3ad; --line:#2c3238; --accent:#4cc9d8; --a:#4cc9d8; --b:#e89a5e; } }
* { box-sizing: border-box; }
body { margin:0; font:15px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; background:var(--bg); color:var(--text); }
header { position:sticky; top:0; z-index:2; background:var(--bg); border-bottom:1px solid var(--line); padding:12px 16px; }
.wrap { max-width:1100px; margin:0 auto; }
h1 { font-size:18px; margin:0 0 4px; }
.lede { color:var(--muted); margin:0 0 8px; max-width:70ch; }
nav { display:flex; flex-wrap:wrap; gap:6px 14px; }
nav a { color:var(--accent); text-decoration:none; font-weight:600; }
.controls { display:flex; gap:16px; align-items:center; margin-top:8px; flex-wrap:wrap; }
.controls label { display:flex; gap:6px; align-items:center; }
main { padding:8px 16px 48px; max-width:1100px; margin:0 auto; }
section { margin-top:24px; }
h2 { font-size:17px; margin:0 0 8px; }
.machine { color:var(--muted); font-weight:500; }
.row { display:grid; grid-template-columns: minmax(180px, 1.2fr) auto minmax(200px, 1.6fr); gap:8px 16px; align-items:center; background:var(--card); border:1px solid var(--line); border-radius:8px; padding:10px 12px; margin:6px 0; }
.id { color:var(--muted); font-variant-numeric:tabular-nums; }
.voice, .source { color:var(--muted); font-size:13px; }
.note { color:var(--muted); font-size:12px; font-style:italic; margin-top:2px; }
.play { display:flex; gap:8px; align-items:center; flex-wrap:wrap; }
button { font:inherit; font-weight:700; min-width:44px; min-height:36px; padding:0 12px; border-radius:6px; border:1px solid var(--line); background:var(--card); color:var(--text); cursor:pointer; }
button.pa { border-color:var(--a); color:var(--a); }
button.pb { border-color:var(--b); color:var(--b); }
button.playing { background:var(--accent); color:var(--card) !important; border-color:var(--accent) !important; }
button.both { font-weight:500; }
.hint { font-size:13px; color:var(--muted); }
.wave { grid-column: 1 / -1; }
.wave summary { cursor:pointer; color:var(--accent); font-weight:600; font-size:14px; }
.figure { margin:10px 0 18px; }
.sheet { position:relative; width:100%; max-width:1560px; }
.sheet img { display:block; width:100%; height:auto; border:1px solid var(--line); border-radius:4px; }
.sheet canvas { position:absolute; inset:0; width:100%; height:100%; cursor:crosshair; touch-action:none; }
.tools { display:flex; gap:8px; align-items:center; margin-top:6px; flex-wrap:wrap; }
.tools button { font-weight:500; }
.tools .note { flex:1; min-width:200px; min-height:36px; font:inherit; padding:4px 8px; border:1px solid var(--line); border-radius:6px; background:var(--card); color:var(--text); }
.status { font-size:13px; color:var(--muted); }
.none { color:var(--muted); font-size:13px; }
.rv-cell { margin-top:6px; display:flex; flex-direction:column; gap:4px; align-items:flex-start; }
.badge { font-size:12px; font-weight:700; padding:2px 8px; border-radius:10px; }
.badge.st-approved { background:#d9f2e3; color:#1d6b3f; }
.badge.st-changed { background:#fde7c7; color:#8a4b00; }
.badge.st-review { background:#e6e8eb; color:#454c55; }
.badge.st-needs_work { background:#f9d6dc; color:#8f1d33; }
@media (prefers-color-scheme: dark) {
  .badge.st-approved { background:#1f3d2b; color:#8fe0b0; } .badge.st-changed { background:#4a3312; color:#ffc978; }
  .badge.st-review { background:#2c3238; color:#c5ccd3; } .badge.st-needs_work { background:#4a1f28; color:#ff9aad; }
}
.reason { font-size:12px; color:var(--muted); max-width:34ch; }
.owner-note { font-size:12px; color:var(--text); max-width:34ch; border-left:3px solid var(--line); padding-left:6px; }
.rv-cell button { min-height:28px; font-size:12px; padding:0 8px; font-weight:600; }
.row.just-set { opacity:0.55; }
.review-actions { display:flex; gap:6px; }
.row.hidden { display:none; }
#counts { font-size:13px; color:var(--muted); }
#stale { margin-top:10px; padding:8px 12px; border-radius:6px; background:#7a2d2d; color:#fff; font-size:14px; }
@media (max-width: 640px) { .row { grid-template-columns: 1fr; } }
</style>
</head>
<body>
<header><div class="wrap">
  <h1>mxm-drum-machine against the originals</h1>
  <p class="lede"><b>Model</b> is mxm-drum-machine at its reference setting (zero deviation, velocity 0.82). <b>Original</b> is the hardware recording it was calibrated against; <b>Both</b> plays the model, then the original. <b>Waveforms</b> under each drum: <i>Onset</i> is the attack, <i>Cycles</i> sixteen periods on a grid one half wavelength of the measured rest pitch apart, <i>Whole hit</i> both waveforms and their decay in dB, <i>Spectrogram</i> where the energy sits in time and frequency, with a map of where the model is louder (red) or quieter (blue) than the original. Time zero is each file's own onset, drawn from 2 ms before it. Draw on any image, add a note and <b>Send to Claude</b>. {{MATCHED}} of {{TOTAL}} models have one. Both are mono and loudness-matched over the body of the hit, then share one gain with the louder peak at −1 dBFS. Local only: the recordings are third-party.</p>
  <nav>{{NAV}}</nav>
  <div class="controls"><span class="hint">Click a button again, or press Space, to stop.</span>
    <label>Show <select id="filter"><option value="attention">Needs your ear (changed, to review, needs work)</option><option value="all">All drums</option><option value="approved">Approved</option></select></label>
    <span id="counts"></span> <span id="save-status" class="hint"></span></div>
  <div id="stale" hidden>This page is from an earlier build. <b>Reload</b> before you judge anything:
    the drums you play are the ones this page loaded, not the ones on disk.</div>
</div></header>
<main>
{{BODY}}
</main>
<script>
// A rebuild while the page is open: the audio it plays is what it decoded, so say so rather than
// let the owner approve a drum they are not hearing.
const BUILD = "{{VERSION}}";
setInterval(async () => {
  try {
    const seen = (await (await fetch('build.txt', { cache: 'no-store' })).text()).trim();
    if (seen && seen !== BUILD) document.getElementById('stale').hidden = false;
  } catch (e) { /* the server is down; nothing to say */ }
}, 20000);

// Web Audio, not an <audio> element: the element clicked at the end of a decaying drum, in a
// different place each play, and the owner spent a review chasing it in the model. Decoding once
// and scheduling a buffer removes the decoder and the element's own stop from the path, and a 5 ms
// gain ramp keeps a stop from stepping.
const ctx = new (window.AudioContext || window.webkitAudioContext)();
const buffers = new Map();
let current = null;
let playing = null;

async function buffer(src) {
  if (!buffers.has(src)) {
    const response = await fetch(src);
    buffers.set(src, await ctx.decodeAudioData(await response.arrayBuffer()));
  }
  return buffers.get(src);
}

function stop() {
  if (playing) {
    const { source, gain } = playing;
    const now = ctx.currentTime;
    gain.gain.cancelScheduledValues(now);
    gain.gain.setValueAtTime(gain.gain.value, now);
    gain.gain.linearRampToValueAtTime(0, now + 0.005);
    source.stop(now + 0.006);
    playing = null;
  }
  if (current) current.classList.remove('playing');
  current = null;
}

async function play(button, sources) {
  const same = current === button;
  stop();
  if (same) return;
  button.classList.add('playing');
  current = button;
  if (ctx.state === 'suspended') await ctx.resume();
  const decoded = await Promise.all(sources.map(buffer));
  if (current !== button) return;
  const gain = ctx.createGain();
  gain.connect(ctx.destination);
  let at = ctx.currentTime + 0.02;
  let last = null;
  for (const b of decoded) {
    const source = ctx.createBufferSource();
    source.buffer = b;
    source.connect(gain);
    source.start(at);
    // A short gap between the two, so the second is heard as a new hit.
    at += b.duration + 0.25;
    last = source;
  }
  playing = { source: last, gain };
  last.onended = () => { if (playing && playing.source === last) stop(); };
}

document.addEventListener('click', e => {
  const single = e.target.closest('button.pa, button.pb');
  if (single) return play(single, [single.dataset.src]);
  const both = e.target.closest('button.both');
  if (both) return play(both, [both.dataset.first, both.dataset.second]);
});

let review = {};
const LABELS = { approved: '✓ Approved', changed: '↻ Changed, re-listen', review: '• To review', needs_work: '✕ Needs work' };
function renderCell(cell) {
  const r = review[cell.dataset.id] || { status: 'review', reason: '', notes: [] };
  const notes = (r.notes || []).map(n => `<div class="owner-note">You, ${n.date}: ${String(n.note).replace(/</g, '&lt;')}</div>`).join('');
  cell.innerHTML = `<span class="badge st-${r.status}">${LABELS[r.status] || r.status}</span>` +
    (r.reason ? `<div class="reason">${r.reason}</div>` : '') + notes +
    `<div class="review-actions"><button class="approve">Approve</button><button class="needs">Needs work</button><button class="reset">Reset</button></div>`;
  cell.closest('.row').dataset.status = r.status;
}
function renderCounts() {
  const counts = { approved: 0, changed: 0, review: 0, needs_work: 0 };
  document.querySelectorAll('.rv-cell').forEach(cell => {
    const s = (review[cell.dataset.id] || { status: 'review' }).status;
    counts[s] = (counts[s] || 0) + 1;
  });
  document.getElementById('counts').textContent =
    `${counts.approved} approved · ${counts.changed} changed · ${counts.review} to review · ${counts.needs_work} need work`;
}
// Rows are hidden only on load and when the filter changes, never under the cursor: a row that
// vanished after Approve moved the next row's buttons to where the owner was about to click.
function applyFilter() {
  const f = document.getElementById('filter').value;
  document.querySelectorAll('.row').forEach(row => {
    const s = row.dataset.status;
    const show = f === 'all' || (f === 'approved' ? s === 'approved' : s !== 'approved');
    row.classList.toggle('hidden', !show);
    row.classList.remove('just-set');
  });
}
function renderAll() {
  document.querySelectorAll('.rv-cell').forEach(renderCell);
  renderCounts();
  applyFilter();
}
document.getElementById('filter').addEventListener('change', applyFilter);
fetch('review.json?' + Date.now()).then(r => r.ok ? r.json() : {}).then(j => { review = j; renderAll(); }).catch(renderAll);
document.addEventListener('click', e => {
  const cell = e.target.closest('.rv-cell');
  if (!cell || !e.target.matches('button')) return;
  if (e.target.matches('.approve')) return send(cell, 'approve', '');
  if (e.target.matches('.reset')) return send(cell, 'reset', '');
  let input = cell.querySelector('.needs-note');
  if (!input) {
    input = Object.assign(document.createElement('input'), { className: 'needs-note note', placeholder: 'What is wrong? Enter to save' });
    cell.appendChild(input);
    input.addEventListener('keydown', ev => { if (ev.key === 'Enter') send(cell, 'needs_work', input.value); });
    input.focus();
    return;
  }
  send(cell, 'needs_work', input.value);
});
async function send(cell, action, note) {
  const status = document.getElementById('save-status');
  let saved;
  try {
    const response = await fetch('/review', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id: cell.dataset.id, action, note }) });
    if (!response.ok) throw new Error('HTTP ' + response.status);
    saved = await response.json();
  } catch (error) {
    status.textContent = 'Not saved: is the local server running? (' + error.message + ')';
    return;
  }
  review = saved;
  renderCell(cell);
  renderCounts();
  cell.closest('.row').classList.add('just-set');
  status.textContent = `Saved #${cell.dataset.id}: ${LABELS[review[cell.dataset.id].status]}.`;
}

document.addEventListener('keydown', e => {
  if (e.code === 'Space' && !e.target.closest('input')) { e.preventDefault(); stop(); }
});

// Drawing: strokes live in image pixels, so what is sent lines up with the waveform exactly.
const sheets = new WeakMap();
function sheet(element) {
  if (sheets.has(element)) return sheets.get(element);
  const img = element.querySelector('img');
  const canvas = element.querySelector('canvas');
  const ctx = canvas.getContext('2d');
  const state = { strokes: [], active: null, img, canvas };
  function redraw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.lineWidth = 4; ctx.lineCap = 'round'; ctx.lineJoin = 'round'; ctx.strokeStyle = '#e0197d';
    for (const stroke of state.strokes) {
      ctx.beginPath();
      stroke.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
      if (stroke.length === 1) ctx.lineTo(stroke[0][0] + 0.1, stroke[0][1]);
      ctx.stroke();
    }
  }
  function size() { canvas.width = img.naturalWidth || 1560; canvas.height = img.naturalHeight || 736; redraw(); }
  if (img.complete) size(); else img.addEventListener('load', size);
  function point(e) {
    const r = canvas.getBoundingClientRect();
    return [(e.clientX - r.left) * canvas.width / r.width, (e.clientY - r.top) * canvas.height / r.height];
  }
  canvas.addEventListener('pointerdown', e => {
    canvas.setPointerCapture(e.pointerId);
    state.active = [point(e)];
    state.strokes.push(state.active);
    redraw();
  });
  canvas.addEventListener('pointermove', e => { if (state.active) { state.active.push(point(e)); redraw(); } });
  const end = () => { state.active = null; };
  canvas.addEventListener('pointerup', end);
  canvas.addEventListener('pointercancel', end);
  state.redraw = redraw;
  sheets.set(element, state);
  return state;
}
document.addEventListener('toggle', e => {
  if (e.target.matches && e.target.matches('details.wave') && e.target.open) e.target.querySelectorAll('.sheet').forEach(sheet);
}, true);
document.addEventListener('click', async e => {
  const figure = e.target.closest('.figure');
  if (!figure) return;
  const element = figure.querySelector('.sheet');
  const state = sheet(element);
  const status = figure.querySelector('.status');
  if (e.target.matches('button.undo')) { state.strokes.pop(); state.redraw(); }
  if (e.target.matches('button.clear')) { state.strokes.length = 0; state.redraw(); }
  if (!e.target.matches('button.send')) return;
  if (location.protocol === 'file:') {
    status.textContent = 'Open http://127.0.0.1:8777/ to send (a local file cannot save).';
    return;
  }
  const out = document.createElement('canvas');
  out.width = state.canvas.width; out.height = state.canvas.height;
  const c = out.getContext('2d');
  c.drawImage(state.img, 0, 0);
  c.drawImage(state.canvas, 0, 0);
  const row = figure.closest('.row');
  status.textContent = 'Sending...';
  try {
    const response = await fetch('/annotate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        id: element.dataset.id,
        view: element.dataset.view,
        label: row.querySelector('.name').firstChild.nextSibling.textContent.trim(),
        note: figure.querySelector('.note').value,
        png: out.toDataURL('image/png'),
      }),
    });
    const result = await response.json();
    status.textContent = result.saved ? 'Saved ' + result.saved + '. Tell Claude.' : 'Not saved.';
  } catch (error) {
    status.textContent = 'Not saved: is the local server running?';
  }
});
</script>
</body>
</html>
"##;
