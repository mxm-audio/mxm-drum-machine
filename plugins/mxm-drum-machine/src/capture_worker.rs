//! The plugin's own capture thread (plan §4.7).
//!
//! # Why this instrument does not use nice-plug's background executor
//!
//! Every other plugin in the collection that renders off the audio thread schedules onto
//! `AsyncExecutor::execute_background`, and this one started there too. It crashed hosts, and
//! `clap-validator` reproduced it on `param-set-events`. The mechanism is in
//! the nice-plug fork's `src/event_loop/background_thread.rs` (mxm-audio/nice-plug) and it is not
//! a length problem:
//!
//! 1. The worker thread is **shared by every instance of the plugin in the process**, and a task
//!    carries a `Weak` reference to the instance that scheduled it.
//! 2. When the worker picks up a task whose instance has since been destroyed, the upgrade fails
//!    and the worker **returns** — killing the shared thread and dropping the channel's receiver
//!    (`background_thread.rs:148`).
//! 3. The handle outlives it, and its `Drop` then does
//!    `tasks_sender.send(Message::Shutdown).expect(…)` on a disconnected channel, which panics
//!    while unwinding a plugin's teardown (`background_thread.rs:98`).
//!
//! So *any* task still queued when its instance goes away is a live crash, and no amount of
//! chunking or rate-limiting removes it — it only narrows the window. Submitting from an editor
//! frame instead of `process` narrowed it far enough to pass the validator, but it also meant
//! **nothing rendered while the GUI was closed**: a host automating a captured axis, or a
//! controller moving one, left the frozen kit stale for ever.
//!
//! Owning the thread fixes both at once. There is no sharing, so there is no `Weak` to fail; the
//! shutdown is a flag this module owns and joins on; and the worker reads the request straight
//! out of [`Telemetry`], so it does not care whether an editor exists.
//!
//! **The cost, stated plainly:** one parked thread per plugin instance, waking every
//! [`POLL`] to look at one atomic. That is the price of a capture that keeps up with a closed
//! GUI, and it is charged per instance rather than per process.

use std::sync::{Arc, Condvar, Mutex};

use mxm_drum_machine_dsp::SLOT_COUNT;
use mxm_drum_machine_dsp::engine::SlotPatch;

use crate::capture_bank::CaptureBank;
use crate::telemetry::{CaptureRequest, Export, Telemetry};

/// How long the worker sleeps between looks at the capture request.
///
/// `process` cannot wake a condvar — it must not touch a lock — so a pending capture is found by
/// polling. Everything on the control thread notifies instead, so this only bounds the case
/// nothing else covers: a parameter moving with the GUI closed. A capture takes around 200 ms at
/// 48 kHz, so 20 ms of latency in front of it is not a figure anybody can hear, and a parked
/// thread waking fifty times a second costs nothing measurable.
const POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// How much audio one chunk of a capture renders before the worker looks at `alive` again.
///
/// It bounds teardown: `Drop` joins this thread, so the longest a host can wait for the plugin to
/// go away is one chunk. At 48 kHz this is a few milliseconds of work.
const CHUNK_FRAMES: usize = 65_536;

/// What the control thread has asked the worker for, beyond the capture it reads from telemetry.
#[derive(Default)]
struct Pending {
    /// Cleared by [`CaptureWorker::drop`] to end the loop.
    running: bool,
    /// A sample pack the editor asked for: where to write it, and at what rate.
    ///
    /// **The destination travels with the job** rather than being resolved by the worker. The
    /// worker used to call `pack::root()` itself, which meant every test of this path wrote a
    /// real sixteen-file pack into the folder of whoever ran it — fourteen of them, in the
    /// owner's own AppData, before anybody noticed. It is also what lets a chosen folder reach
    /// the writer at all.
    export: Option<(f32, std::path::PathBuf)>,
}

/// What a kit was captured from: its patches, and the kit's name, which its pack takes.
#[derive(Debug, Clone)]
pub struct Heard {
    /// Every slot's patch, as `capture_patches` read it.
    pub patches: [SlotPatch; SLOT_COUNT],
    /// The kit's name then (`kit_name`).
    pub name: String,
}

struct Shared {
    pending: Mutex<Pending>,
    wake: Condvar,
    /// What the last two kits were captured from, with the engage each answered
    /// (`Gate::remember`): Export renders and names the one heard, not the parameters or the kit
    /// loaded since (one capture per engage), and an activation keeps a kit whose patch has not
    /// moved. Two, as a kit for a new engage can be made while the old one still plays.
    heard: Mutex<[Option<(u32, Heard)>; 2]>,
}

impl Shared {
    /// Remembers what the kit answering `engagement` was captured from.
    fn remember(&self, engagement: u32, captured: Heard) {
        let mut heard = self
            .heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        heard[1] = heard[0].take();
        heard[0] = Some((engagement, captured));
    }

    /// What the kit answering `engagement` was captured from, if it is remembered.
    fn heard_for(&self, engagement: u32) -> Option<Heard> {
        self.heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .flatten()
            .find(|(e, _)| *e == engagement)
            .map(|(_, heard)| heard.clone())
    }

    /// Ends the worker loop and wakes it so it notices.
    fn stop(&self) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .running = false;
        self.wake.notify_all();
    }
}

/// Everything the worker needs from the plugin, by shared ownership rather than by borrow.
pub struct Inputs {
    pub params: Arc<crate::params::MxmDrumMachineParams>,
    pub bank: Arc<CaptureBank>,
    pub telemetry: Arc<Telemetry>,
}

/// A way to ask the capture thread for something, which works whether or not one is running.
///
/// It exists from the plugin's construction, because an editor can be opened before `activate`
/// spawns the thread, and it outlives the thread, because an editor can be closed after one has
/// stopped. Neither end is an error: a request made before the thread starts **waits** and is
/// answered when it does, and one made after it has stopped is dropped, because there is nothing
/// left to answer it.
#[derive(Clone)]
pub struct Gate(Arc<Shared>);

impl Default for Gate {
    fn default() -> Self {
        Self(Arc::new(Shared {
            pending: Mutex::new(Pending {
                running: true,
                export: None,
            }),
            wake: Condvar::new(),
            heard: Mutex::new([None, None]),
        }))
    }
}

impl Gate {
    /// Asks for a sample pack in `destination`. **Control thread** — the editor's export button.
    ///
    /// The caller resolves where it goes, so a chosen folder and the default one take the same
    /// path through here, and a test can name a temporary directory instead of a real one.
    pub fn request_export(&self, sample_rate: f32, destination: std::path::PathBuf) {
        self.0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .export = Some((sample_rate, destination));
        self.0.wake.notify_all();
    }

    /// Remembers what the kit answering `engagement` was captured from: the capture thread's, and
    /// `activate`'s. **Never the audio thread** (it locks and allocates).
    pub fn remember(&self, engagement: u32, captured: Heard) {
        self.0.remember(engagement, captured);
    }

    /// What the kit answering `engagement` was captured from, if it is remembered.
    #[must_use]
    pub fn heard_for(&self, engagement: u32) -> Option<Heard> {
        self.0.heard_for(engagement)
    }

    /// Wakes the worker now rather than at the next poll. **Control thread.**
    ///
    /// Only ever an optimisation: the poll finds the same request a beat later.
    pub fn nudge(&self) {
        self.0.wake.notify_all();
    }
}

/// A running capture thread, stopped and joined when it is dropped.
pub struct CaptureWorker {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CaptureWorker {
    /// Spawns the thread behind an existing gate.
    ///
    /// A control-thread operation — `activate`, in practice — so that a plugin merely being
    /// constructed for a host's scan does not pay for a thread it will never use.
    #[must_use]
    pub fn spawn(gate: &Gate, inputs: Inputs) -> Self {
        let shared = Arc::clone(&gate.0);
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name(String::from("mxm-drum-machine-capture"))
            .spawn(move || run(&worker, &inputs))
            .ok();
        Self { shared, thread }
    }
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.shared.stop();
        if let Some(thread) = self.thread.take() {
            // Deliberately not `expect`ed. A panicking join here is precisely the shape that made
            // the shared executor take hosts down with it, and a worker that somehow failed is
            // not a reason to abort a teardown that is otherwise going fine.
            let _ = thread.join();
        }
    }
}

/// The worker loop: wait for something to do, do one bounded piece of it, look again.
fn run(shared: &Shared, inputs: &Inputs) {
    loop {
        let Some(job) = wait_for_work(shared, &inputs.telemetry) else {
            return;
        };
        match job {
            Job::Capture(request) => capture(shared, inputs, request),
            Job::Export(sample_rate, destination) => {
                export(shared, inputs, sample_rate, &destination);
            }
        }
    }
}

enum Job {
    Capture(CaptureRequest),
    Export(f32, std::path::PathBuf),
}

/// Blocks until there is work or the worker is stopped, returning `None` to end the loop.
fn wait_for_work(shared: &Shared, telemetry: &Telemetry) -> Option<Job> {
    let mut pending = shared
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        if !pending.running {
            return None;
        }
        // Export first: it is a person waiting on a button, where a capture is housekeeping.
        if let Some((rate, destination)) = pending.export.take() {
            return Some(Job::Export(rate, destination));
        }
        if let Some(request) = telemetry.take_capture_request() {
            return Some(Job::Capture(request));
        }
        let (next, _) = shared
            .wake
            .wait_timeout(pending, POLL)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending = next;
    }
}

/// Whether the worker should abandon what it is doing and return.
fn stopping(shared: &Shared) -> bool {
    !shared
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .running
}

/// Renders one kit and publishes it for the audio thread to install.
///
/// Chunked so that teardown waits for a chunk rather than a kit, and abandoned rather than
/// published if the plugin goes away mid-render: a kit nobody will ever install is not worth
/// finishing, and the buffers go back with the thread.
///
/// **Abandoned too if a newer request overtakes it.** Within one engage `process` asks once, so a
/// request arriving mid-render means Resample was turned off and on again, or the rate moved: the
/// kit in flight was made for a patch or a rate that is gone. Published, it would install first —
/// the old patch, then every reader reset for the new one — and it was a second displaced kit for
/// the bank to hold. The loop picks the newer request up next.
fn capture(shared: &Shared, inputs: &Inputs, request: CaptureRequest) {
    // Free whatever the audio thread displaced before allocating a replacement, so the handoff
    // stays a ring rather than growing. It is also what lets `CaptureBank::take` accept the next
    // kit at all — it refuses while a retired one is still parked.
    inputs.bank.drain();
    let patches = crate::capture_patches(&inputs.params);
    let name = crate::kit_name(&inputs.params);
    let Ok(mut progress) =
        mxm_drum_machine_dsp::capture::KitCaptureInProgress::begin(&patches, request.sample_rate)
    else {
        // A refusal leaves the instrument live, which is always a correct rendering of the patch.
        return;
    };
    let overtaken = || inputs.telemetry.capture_requested();
    loop {
        if stopping(shared) || overtaken() {
            return;
        }
        match progress.render(CHUNK_FRAMES) {
            Ok(true) => break,
            Ok(false) => {}
            // Abandoned rather than published half-finished.
            Err(_) => return,
        }
    }
    // Once more after the last chunk: a request can land while it renders.
    if overtaken() {
        return;
    }
    // Stamped with the engage that asked, which is how `process` knows it is still the one to
    // install; its patches remembered first, so an Export the moment it installs finds them.
    let kit = progress.finish().answering(request.engagement);
    shared.remember(request.engagement, Heard { patches, name });
    inputs.bank.publish(kit);
    inputs.bank.drain();
}

/// Renders a kit and writes it out as a sample pack, reporting the outcome either way.
///
/// Abandoned if the plugin goes away mid-render, so closing a project does not wait on a pack —
/// and abandoned *before* the first file is written, so it never leaves half a pack behind.
fn export(shared: &Shared, inputs: &Inputs, sample_rate: f32, root: &std::path::Path) {
    let current = crate::capture_patches(&inputs.params);
    // **The kit heard**: frozen, the patches its kit was captured from (an edit since renders
    // nothing, so a pack of the parameters would not be what plays), at the pack's rate; the
    // playback controls (Tune, Decay, Soft hits) as set now, as they are live.
    let heard = inputs
        .telemetry
        .frozen()
        .and_then(|engagement| shared.heard_for(engagement));
    let (captured, patches, name) = match heard {
        Some(heard) => {
            let mut patches = heard.patches;
            for (patch, now) in patches.iter_mut().zip(&current) {
                patch.pitch_semitones = now.pitch_semitones;
                patch.decay = now.decay;
                patch.dynamics = now.dynamics;
            }
            (heard.patches, patches, heard.name)
        }
        None => (current, current, crate::kit_name(&inputs.params)),
    };
    let stop = || stopping(shared);
    let written = mxm_drum_machine_dsp::capture::capture_kit_until(&captured, sample_rate, &stop)
        .map_err(|error| format!("{error:?}"))
        .and_then(|kit| {
            crate::pack::write(root, &name, &kit, &patches).map_err(|error| error.to_string())
        });
    inputs.telemetry.publish_export(match written {
        Ok(written) => Export::Written {
            files: written.files,
            clipped: written.clipped,
        },
        Err(why) => Export::Failed(why),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> Inputs {
        Inputs {
            params: Arc::new(crate::params::MxmDrumMachineParams::default()),
            bank: Arc::new(CaptureBank::new()),
            telemetry: Telemetry::shared(),
        }
    }

    #[test]
    fn a_request_made_with_no_editor_open_is_still_rendered() {
        // The hole that owning the thread exists to close. Nothing here is an editor frame: the
        // request goes in the way `process` makes it, and the kit has to arrive anyway.
        let inputs = inputs();
        let bank = Arc::clone(&inputs.bank);
        let telemetry = Arc::clone(&inputs.telemetry);
        let gate = Gate::default();
        let _worker = CaptureWorker::spawn(&gate, inputs);

        telemetry.request_capture(48_000.0, 1);
        gate.nudge();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let kit = loop {
            if let Some(kit) = bank.take() {
                break kit;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the worker never published a kit"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(kit.sample_rate(), 48_000.0);
        assert_eq!(kit.request(), 1, "stamped with the engage that asked");
    }

    #[test]
    fn the_worker_renders_at_the_rate_it_was_asked_for() {
        let inputs = inputs();
        let bank = Arc::clone(&inputs.bank);
        let telemetry = Arc::clone(&inputs.telemetry);
        let gate = Gate::default();
        let _worker = CaptureWorker::spawn(&gate, inputs);

        telemetry.request_capture(96_000.0, 1);
        gate.nudge();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let kit = loop {
            if let Some(kit) = bank.take() {
                break kit;
            }
            assert!(std::time::Instant::now() < deadline, "no kit");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(kit.sample_rate(), 96_000.0);
    }

    /// **A render a newer request overtakes is abandoned, not published.** Resample turned off and
    /// on again while the first engage's kit is still rendering asks again; the kit in flight was
    /// made for the patch as it was. The render runs on this thread with the newer request already
    /// waiting, so nothing here depends on timing: the look between chunks sees it before the first
    /// chunk and nothing reaches the bank. The same render with nothing waiting publishes, stamped
    /// with its engage — so the refusal is about the newer request and nothing else.
    ///
    /// Falsified before trusted: without the look, the overtaken kit is published.
    #[test]
    fn a_render_a_newer_request_overtakes_is_abandoned() {
        let inputs = inputs();
        let gate = Gate::default();
        let first = CaptureRequest {
            sample_rate: 48_000.0,
            engagement: 1,
        };

        inputs.telemetry.request_capture(48_000.0, 2);
        capture(&gate.0, &inputs, first);
        assert!(
            inputs.bank.take().is_none(),
            "the overtaken render was published"
        );

        let _ = inputs.telemetry.take_capture_request();
        capture(&gate.0, &inputs, first);
        let kit = inputs
            .bank
            .take()
            .expect("with nothing newer waiting, the render is published");
        assert_eq!(kit.request(), 1, "stamped with the engage that asked");
    }

    #[test]
    fn dropping_the_worker_stops_it_promptly_even_mid_capture() {
        // Teardown is the whole reason this type exists, so it is timed rather than assumed.
        let inputs = inputs();
        let telemetry = Arc::clone(&inputs.telemetry);
        let gate = Gate::default();
        let worker = CaptureWorker::spawn(&gate, inputs);
        telemetry.request_capture(192_000.0, 1);
        gate.nudge();
        // Long enough to be inside the render, short enough that it cannot have finished: a kit
        // at 192 kHz takes the better part of a second.
        std::thread::sleep(std::time::Duration::from_millis(60));

        let started = std::time::Instant::now();
        drop(worker);
        let took = started.elapsed();
        assert!(
            took < std::time::Duration::from_secs(5),
            "joining took {took:?}, so a chunk is not bounding teardown"
        );
    }

    /// The one pack under `root`: its name, and every file in it by name.
    fn pack_files(
        root: &std::path::Path,
    ) -> (std::ffi::OsString, Vec<(std::ffi::OsString, Vec<u8>)>) {
        let pack = std::fs::read_dir(root)
            .expect("the root")
            .map(|entry| entry.expect("an entry").path())
            .find(|path| path.is_dir())
            .expect("a pack");
        let mut files: Vec<_> = std::fs::read_dir(&pack)
            .expect("the pack")
            .map(|entry| {
                let path = entry.expect("an entry").path();
                (
                    path.file_name().expect("a name").to_owned(),
                    std::fs::read(&path).expect("a file"),
                )
            })
            .collect();
        files.sort();
        (pack.file_name().expect("a name").to_owned(), files)
    }

    /// Says `name` is the kit loaded, as a kit load does.
    fn load_kit_named(params: &crate::params::MxmDrumMachineParams, name: &str) {
        params
            .preset
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded = Some(mxm_preset::LoadedPreset {
            name: name.to_owned(),
            origin: mxm_preset::Origin::Factory,
        });
    }

    /// **Export writes the kit heard, under its name** (model drums' review, 2026-10-26): frozen,
    /// an edit renders nothing and a kit loaded since is not what plays, so the pack is the patches
    /// the playing kit was captured from, named for it; with nothing frozen, the parameters and the
    /// kit loaded.
    #[test]
    fn an_export_while_frozen_writes_the_kit_heard_not_an_edit_since() {
        use nice_plug::params::InternalParamMut;
        let inputs = inputs();
        for slot in &inputs.params.slots[1..] {
            // SAFETY: exclusive test ownership; no process or GUI thread exists.
            unsafe { slot.model._internal_set_plain_value(0) };
        }
        let gate = Gate::default();
        load_kit_named(&inputs.params, "Kit A");
        let heard = Heard {
            patches: crate::capture_patches(&inputs.params),
            name: crate::kit_name(&inputs.params),
        };
        gate.remember(5, heard.clone());
        inputs.telemetry.set_frozen(Some(5));
        // Kit B loaded while frozen, and an edit: slot 1's Tone, which the capture bakes in.
        load_kit_named(&inputs.params, "Kit B");
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            inputs.params.slots[0]
                .controls
                .get(crate::params::control::TONE)
                ._internal_set_plain_value(0.9)
        };

        let frozen = crate::pack::tests::scratch("heard");
        export(&gate.0, &inputs, 48_000.0, &frozen);
        let reference = crate::pack::tests::scratch("heard-reference");
        let kit =
            mxm_drum_machine_dsp::capture::capture_kit(&heard.patches, 48_000.0).expect("a kit");
        crate::pack::write(&reference, &heard.name, &kit, &heard.patches).expect("a pack");
        assert_eq!(
            pack_files(&frozen),
            pack_files(&reference),
            "the pack is the kit heard, under its name"
        );

        inputs.telemetry.set_frozen(None);
        let live = crate::pack::tests::scratch("heard-live");
        export(&gate.0, &inputs, 48_000.0, &live);
        let ((live_name, live_files), (heard_name, heard_files)) =
            (pack_files(&live), pack_files(&reference));
        assert_ne!(
            live_name, heard_name,
            "with nothing frozen, the kit loaded names the pack"
        );
        assert_ne!(live_files, heard_files, "and its edit is in it");
        for root in [frozen, reference, live] {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn the_gate_works_on_both_sides_of_the_threads_life() {
        // An editor can be opened before `activate` spawns the thread and closed after teardown
        // has joined it, so neither end may be an error. Asked early, the request waits; asked
        // late, it is dropped. Both are checked here because both are documented.
        let gate = Gate::default();
        let inputs = inputs();
        let telemetry = Arc::clone(&inputs.telemetry);

        // Into the build's own folder, never the real sample-pack folder and never on C:.
        let into = crate::pack::tests::scratch("gate");

        // Before the thread exists.
        gate.request_export(48_000.0, into.clone());
        let worker = CaptureWorker::spawn(&gate, inputs);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            // Wrote somewhere or said why; either is the worker having picked the request up.
            if telemetry.take_export().is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "a request made before the thread started was lost"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // After it has stopped: inert, and above all not a panic.
        drop(worker);
        gate.request_export(48_000.0, into);
        gate.nudge();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            telemetry.take_export().is_none(),
            "a stopped worker answered a request"
        );
    }

    #[test]
    fn a_worker_that_is_never_asked_for_anything_still_joins() {
        let worker = CaptureWorker::spawn(&Gate::default(), inputs());
        let started = std::time::Instant::now();
        drop(worker);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "an idle worker should stop on the first wake"
        );
    }
}
