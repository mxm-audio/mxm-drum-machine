//! The drum machine's routing as the collection's modulation standard checks it
//! (`mxm_modulation::conformance`; `plans/plan-modulation-standard.md`).
//!
//! Behind the `conformance` feature, which only `[dev-dependencies]` enable — this crate's own
//! tests, and the plugin's, whose route readings are held to [`Declared::deliver`] — so no shipped
//! graph carries it. [`Declared`] answers every question through [`crate::routing`]'s own tables
//! and a real [`Graph`], never a copy of them.

use mxm_modulation::conformance::{Declaration, Kind};
use mxm_modulation::standard::{self, Offer, Performance};

use crate::routing::{
    self, AMPLITUDE_BOUND, Graph, Routing, SOURCE_NAMES, SOURCES, Sources, TARGET_NAMES, TARGETS,
    source, target,
};

/// What each target is, for the standard: a pitch, the Amplitude factor, a pan, and every other
/// target a control in its own bipolar range.
const KINDS: [Kind; TARGETS] = {
    let mut kinds = [Kind::Control; TARGETS];
    kinds[target::PITCH] = Kind::Pitch;
    kinds[target::LEVEL] = Kind::Amplitude;
    kinds[target::PAN] = Kind::Pan;
    kinds
};

/// The drum machine's routing declaration — one slot's; every slot's is the same.
#[derive(Debug, Clone, Copy, Default)]
pub struct Declared;

/// Exactly one route, at `amount`.
fn one_route(target: usize, source: usize, amount: f32) -> Routing {
    let mut routing = Routing::new();
    routing.present[target][source] = true;
    routing.amounts[target][source] = amount;
    routing.compact();
    routing
}

/// The seven sources with one of them at `value`.
fn sources_with(source: usize, value: f32) -> Sources {
    let mut sources = Sources::default();
    match source {
        source::LFO1 => sources.lfo1 = value,
        source::LFO2 => sources.lfo2 = value,
        source::LFO3 => sources.lfo3 = value,
        source::WHEEL => sources.wheel = value,
        source::PRESSURE => sources.pressure = value,
        source::VELOCITY => sources.velocity = value,
        _ => sources.random = value,
    }
    sources
}

impl Declaration for Declared {
    fn sources(&self) -> usize {
        SOURCES
    }

    fn targets(&self) -> usize {
        TARGETS
    }

    fn performance(&self, source: usize) -> Option<Performance> {
        routing::PERFORMANCE[source]
    }

    fn kind(&self, target: usize) -> Kind {
        KINDS[target]
    }

    fn machine(&self, _target: usize, _source: usize) -> bool {
        false
    }

    fn offered(&self, target: usize, source: usize) -> Offer {
        routing::offer(target, source)
    }

    /// No Key source: the unit is never read.
    fn key_unit(&self) -> f32 {
        60.0
    }

    /// One route alone through a slot's own [`Graph`], its source publishing `raw`; Amplitude
    /// through the factor the engine applies.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32 {
        let routing = one_route(target, source, amount);
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.publish(&routing, sources_with(source, raw));
        if target == target::LEVEL {
            standard::amplitude_factor(graph.sum(target, &routing, AMPLITUDE_BOUND)) - 1.0
        } else {
            graph.sum(target, &routing, 64.0)
        }
    }

    fn name(&self, target: usize, source: usize) -> String {
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source])
    }
}

#[cfg(test)]
mod tests {
    use mxm_modulation::conformance::{self, Case, Input};

    use super::*;
    use crate::SLOT_COUNT;
    use crate::engine::{Engine, ModulationPatch, SlotPatch, TriggerGroup};
    use crate::model::ModelId;
    use mxm_part_routing::Destination;

    fn report(result: Result<(), Vec<String>>) {
        if let Err(failures) = result {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// **Every pair means what the standard says**: offered on both halves, nothing at its
    /// source's rest, a meaningful move at full, and the standard reach on every pair — this is an
    /// original instrument with no hard-wired depth to keep.
    ///
    /// Falsified before trusted: with Amplitude back at twelve decibels, every Amplitude pair fails.
    #[test]
    fn every_pair_means_what_the_standard_says() {
        report(conformance::check_declaration(&Declared));
    }

    /// **A slot publishes what the standard says**: Velocity as `v − 1` of the hit, the wheel and
    /// pressure as they arrive, each exactly zero at rest.
    ///
    /// Falsified before trusted: publishing the raw velocity fails at every input.
    #[test]
    fn a_slot_publishes_what_the_standard_says() {
        report(conformance::check_publishers(&Declared, |from, input| {
            let mut patches = [SlotPatch::default(); SLOT_COUNT];
            patches[0].model = ModelId::DEEP_BRIDGE_KICK;
            let mut engine = Engine::new();
            engine.prepare(&patches);
            engine.set_slot_routing(0, one_route(target::PITCH, from, 0.0));
            let mut modulation = ModulationPatch::default();
            let velocity = match input {
                Input::Normalised(value) if from == source::VELOCITY => value,
                Input::Normalised(value) if from == source::WHEEL => {
                    modulation.wheel[0] = value;
                    1.0
                }
                Input::Normalised(value) if from == source::PRESSURE => {
                    modulation.pressure[0] = value;
                    1.0
                }
                _ => 1.0,
            };
            let mut triggers = TriggerGroup::new();
            triggers.push(0, velocity);
            engine.trigger_group(&patches, triggers);
            let _ = engine.process_routed(&patches, modulation, &[Destination::Main; SLOT_COUNT]);
            engine.published_for_test(0, from)
        }));
    }

    /// **After a hit, no performance route holds a slot open** — every pair, both halves, the
    /// softest and hardest hits, the wheel and pressure held at full through the hit and let go
    /// after it. A drum has no release: its hit ends by itself, and the Amplitude factor on the
    /// level cannot outlive it. A short model keeps the case count affordable; the key is ignored.
    #[test]
    fn after_a_hit_no_performance_route_holds_a_slot_open() {
        let mut ended = std::collections::HashMap::new();
        report(conformance::check_release_silence(
            &Declared,
            &[],
            |case: Case| {
                let key = (
                    case.target,
                    case.source,
                    case.amount.to_bits(),
                    case.velocity.to_bits(),
                );
                *ended.entry(key).or_insert_with(|| {
                    let mut patches = [SlotPatch::default(); SLOT_COUNT];
                    patches[0].model = ModelId::PURE_HIGH_CLAVE;
                    let mut engine = Engine::new();
                    engine.prepare(&patches);
                    engine.set_slot_routing(0, one_route(case.target, case.source, case.amount));
                    let mut modulation = ModulationPatch {
                        wheel: [1.0; SLOT_COUNT],
                        pressure: [1.0; SLOT_COUNT],
                        ..ModulationPatch::default()
                    };
                    let mut triggers = TriggerGroup::new();
                    triggers.push(0, case.velocity);
                    engine.trigger_group(&patches, triggers);
                    let main = [Destination::Main; SLOT_COUNT];
                    for _ in 0..480 {
                        let _ = engine.process_routed(&patches, modulation, &main);
                    }
                    modulation.wheel = [0.0; SLOT_COUNT];
                    modulation.pressure = [0.0; SLOT_COUNT];
                    (0..96_000).any(|_| {
                        let frame = engine.process_routed(&patches, modulation, &main);
                        frame.main == [0.0; 2] && !engine.is_active()
                    })
                })
            },
        ));
    }
}
