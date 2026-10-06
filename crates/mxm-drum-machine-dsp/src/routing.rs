//! Shared modulation routing as this instrument defines it.
//!
//! The shared crate owns publication timing and compacted route laws. This module owns the drum
//! machine's seven sources, thirteen targets, target units, and one source frame per slot. Every slot
//! owns its topology; held velocity/random and channel controllers differ per slot while the three
//! kit-wide LFO values are generated once per sample and projected into whichever frames route them.
//!
//! # The collection's standard
//!
//! Wheel, Pressure, Velocity and Random mean what they mean on every instrument, and every route
//! reaches what it reaches on every instrument ([`mxm_modulation::standard`];
//! `plans/plan-modulation-standard.md`): this is an original instrument with no hard-wired
//! modulation to keep, so every pair is an added one. Velocity is `v − 1`, so a route from it does
//! nothing at the hardest hit, and **the Amplitude target is the standard factor** — it was a sum of
//! up to ±12 dB, and it keeps its permanent id, `level`.

use mxm_modulation::standard::{self, AMPLITUDE_SUM_BOUND, Law, Offer, Performance, reach};
use mxm_modulation::{Compacted, SourceFrame};

pub mod source {
    pub const LFO1: usize = 0;
    pub const LFO2: usize = 1;
    pub const LFO3: usize = 2;
    pub const WHEEL: usize = 3;
    pub const PRESSURE: usize = 4;
    pub const VELOCITY: usize = 5;
    pub const RANDOM: usize = 6;
}

pub const SOURCES: usize = 7;
pub const SOURCE_NAMES: [&str; SOURCES] = [
    "LFO 1", "LFO 2", "LFO 3", "Wheel", "Pressure", "Velocity", "Random",
];

/// Which of the standard's performance sources each source is — `None` for the three LFOs.
pub const PERFORMANCE: [Option<Performance>; SOURCES] = [
    None,
    None,
    None,
    Some(Performance::Wheel),
    Some(Performance::Pressure),
    Some(Performance::Velocity),
    Some(Performance::Random),
];

pub mod target {
    pub const PITCH: usize = 0;
    pub const DECAY: usize = 1;
    pub const ATTACK: usize = 2;
    pub const TONE: usize = 3;
    pub const BODY: usize = 4;
    pub const NOISE: usize = 5;
    pub const CHARACTER: usize = 6;
    /// **Amplitude**: the standard factor on the slot's level. Its permanent id is `level`.
    pub const LEVEL: usize = 7;
    pub const PAN: usize = 8;
    pub const PITCH_ENV: usize = 9;
    pub const PITCH_DECAY: usize = 10;
    pub const NOISE_DECAY: usize = 11;
    pub const DYNAMICS: usize = 12;
}

pub const TARGETS: usize = 13;
pub const TARGET_NAMES: [&str; TARGETS] = [
    "Pitch",
    "Decay",
    "Attack",
    "Tone",
    "Body",
    "Noise",
    "Character",
    "Amplitude",
    "Pan",
    "Pitch envelope",
    "Pitch decay",
    "Noise decay",
    "Dynamics",
];

/// Each target's law, for the standard's offer: the amplitude factor on the level, and sums.
pub const LAW: [Law; TARGETS] = {
    let mut law = [Law::Sum; TARGETS];
    law[target::LEVEL] = Law::Factor;
    law
};

/// Whether and how a pair is offered — `standard::offer`. No pair is the machine's own, and every
/// target sums or scales, so every pair is offered on both halves.
#[must_use]
pub const fn offer(target: usize, source: usize) -> Offer {
    standard::offer(LAW[target], PERFORMANCE[source], false)
}

/// The Amplitude sum's bound — the standard's, silence to double.
pub const AMPLITUDE_BOUND: f32 = AMPLITUDE_SUM_BOUND;

/// Per-route reach in each target's domain — **the collection's standard reach on every pair**,
/// deliberately uniform by target because this original instrument has no prior hard-wired
/// modulation depth to preserve.
///
/// Pitch is semitones, an octave at full. Amplitude is the standard factor's whole swing, silence to
/// double; it was twelve decibels either way. Pan is its own −1…+1, and every other target is its
/// parameter's own bipolar unit, the whole of it at full.
pub const FULL_SCALE: [[f32; SOURCES]; TARGETS] = {
    let mut table = [[reach::CONTROL; SOURCES]; TARGETS];
    table[target::PITCH] = [reach::PITCH_SEMITONES; SOURCES];
    table[target::LEVEL] = [reach::AMPLITUDE; SOURCES];
    table
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routing {
    pub present: [[bool; SOURCES]; TARGETS],
    pub amounts: [[f32; SOURCES]; TARGETS],
    live: [(u8, u8); TARGETS * SOURCES],
    live_len: usize,
    needed: [bool; SOURCES],
}

impl Default for Routing {
    fn default() -> Self {
        Self::new()
    }
}

impl Routing {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            present: [[false; SOURCES]; TARGETS],
            amounts: [[0.0; SOURCES]; TARGETS],
            live: [(0, 0); TARGETS * SOURCES],
            live_len: 0,
            needed: [false; SOURCES],
        }
    }

    pub fn compact(&mut self) {
        self.live_len = 0;
        self.needed = [false; SOURCES];
        for (target, row) in self.present.iter().enumerate() {
            for (source, present) in row.iter().copied().enumerate() {
                if present {
                    self.live[self.live_len] = (target as u8, source as u8);
                    self.live_len += 1;
                    self.needed[source] = true;
                }
            }
        }
    }

    #[must_use]
    pub fn live(&self) -> &[(u8, u8)] {
        &self.live[..self.live_len]
    }

    #[must_use]
    pub const fn needs(&self, source: usize) -> bool {
        self.needed[source]
    }

    #[must_use]
    pub const fn any(&self) -> bool {
        self.live_len != 0
    }
}

/// The seven source values projected into one slot this sample.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources {
    pub lfo1: f32,
    pub lfo2: f32,
    pub lfo3: f32,
    pub wheel: f32,
    pub pressure: f32,
    pub velocity: f32,
    pub random: f32,
}

#[derive(Debug, Clone)]
pub struct Graph {
    frame: SourceFrame<SOURCES>,
    live: [Compacted<SOURCES>; TARGETS],
    needed: [bool; SOURCES],
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: SourceFrame::default(),
            live: std::array::from_fn(|_| Compacted::default()),
            needed: [false; SOURCES],
        }
    }

    /// Rebuilds compacted routes and clears a source that has just become readable.
    pub fn set_topology(&mut self, routing: &Routing) {
        for (target, live) in self.live.iter_mut().enumerate() {
            live.build(&routing.present[target]);
        }
        for (source, previous) in self.needed.iter_mut().enumerate() {
            let now = routing.needs(source);
            if now && !*previous {
                self.frame.clear(source);
            }
            *previous = now;
        }
    }

    #[inline]
    pub fn publish(&mut self, routing: &Routing, sources: Sources) {
        self.frame.begin_sample();
        let values = [
            sources.lfo1,
            sources.lfo2,
            sources.lfo3,
            sources.wheel,
            sources.pressure,
            sources.velocity,
            sources.random,
        ];
        for (source, value) in values.into_iter().enumerate() {
            if routing.needs(source) {
                self.frame.write(source, value);
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn sum(&self, target: usize, routing: &Routing, bound: f32) -> f32 {
        mxm_modulation::sum_scaled(
            &self.frame,
            &self.live[target],
            &routing.amounts[target],
            &FULL_SCALE[target],
            bound,
        )
    }

    pub fn reset(&mut self) {
        self.frame.reset();
        self.needed = [false; SOURCES];
    }

    /// What the frame holds for one source, for tests.
    #[cfg(test)]
    pub(crate) fn read_for_test(&self, source: usize) -> f32 {
        self.frame.read(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_routes_ignore_dormant_amounts_and_readding_restores_them() {
        let mut routing = Routing::new();
        routing.amounts[target::PITCH][source::LFO1] = 0.5;
        routing.compact();
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.publish(
            &routing,
            Sources {
                lfo1: 1.0,
                ..Sources::default()
            },
        );
        assert_eq!(graph.sum(target::PITCH, &routing, 24.0), 0.0);

        routing.present[target::PITCH][source::LFO1] = true;
        routing.compact();
        graph.set_topology(&routing);
        graph.publish(
            &routing,
            Sources {
                lfo1: 1.0,
                ..Sources::default()
            },
        );
        assert_eq!(graph.sum(target::PITCH, &routing, 24.0), 6.0);
    }

    #[test]
    fn newly_read_source_cannot_return_an_ancient_value() {
        let mut routing = Routing::new();
        routing.present[target::PAN][source::RANDOM] = true;
        routing.amounts[target::PAN][source::RANDOM] = 1.0;
        routing.compact();
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.publish(
            &routing,
            Sources {
                random: 0.75,
                ..Sources::default()
            },
        );
        assert_eq!(graph.sum(target::PAN, &routing, 1.0), 0.75);

        routing.present[target::PAN][source::RANDOM] = false;
        routing.compact();
        graph.set_topology(&routing);
        routing.present[target::PAN][source::RANDOM] = true;
        routing.compact();
        graph.set_topology(&routing);
        // A backward/read-before-publish seam would see zero here, never the prior hit's 0.75.
        assert_eq!(graph.sum(target::PAN, &routing, 1.0), 0.0);
    }
}
