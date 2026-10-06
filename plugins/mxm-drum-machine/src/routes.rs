//! Permanent route parameters: one presence and signed amount for every target/source pair.

use mxm_drum_machine_dsp::{
    SLOT_COUNT,
    routing::{
        FULL_SCALE, Routing, SOURCE_NAMES, SOURCES, TARGET_NAMES, TARGETS, offer, source, target,
    },
};
use mxm_modulation_params::Route;
use mxm_modulation_params::reading::{self, Fader, Reach};
use nice_plug::prelude::*;

macro_rules! route_ids {
    ($target:literal, $slot:literal) => {
        [
            (
                concat!("route_", $target, "_lfo1_amount_", $slot),
                concat!("route_", $target, "_lfo1_on_", $slot),
            ),
            (
                concat!("route_", $target, "_lfo2_amount_", $slot),
                concat!("route_", $target, "_lfo2_on_", $slot),
            ),
            (
                concat!("route_", $target, "_lfo3_amount_", $slot),
                concat!("route_", $target, "_lfo3_on_", $slot),
            ),
            (
                concat!("route_", $target, "_wheel_amount_", $slot),
                concat!("route_", $target, "_wheel_on_", $slot),
            ),
            (
                concat!("route_", $target, "_pressure_amount_", $slot),
                concat!("route_", $target, "_pressure_on_", $slot),
            ),
            (
                concat!("route_", $target, "_velocity_amount_", $slot),
                concat!("route_", $target, "_velocity_on_", $slot),
            ),
            (
                concat!("route_", $target, "_random_amount_", $slot),
                concat!("route_", $target, "_random_on_", $slot),
            ),
        ]
    };
}

macro_rules! slot_route_ids {
    ($slot:literal) => {
        [
            route_ids!("pitch", $slot),
            route_ids!("decay", $slot),
            route_ids!("attack", $slot),
            route_ids!("tone", $slot),
            route_ids!("body", $slot),
            route_ids!("noise", $slot),
            route_ids!("character", $slot),
            route_ids!("level", $slot),
            route_ids!("pan", $slot),
            route_ids!("pitch_env", $slot),
            route_ids!("pitch_decay", $slot),
            route_ids!("noise_decay", $slot),
            route_ids!("dynamics", $slot),
        ]
    };
}

/// `(amount, presence)` IDs in slot/target/source order.
pub static ROUTE_IDS: [[[(&str, &str); SOURCES]; TARGETS]; SLOT_COUNT] = [
    slot_route_ids!("1"),
    slot_route_ids!("2"),
    slot_route_ids!("3"),
    slot_route_ids!("4"),
    slot_route_ids!("5"),
    slot_route_ids!("6"),
    slot_route_ids!("7"),
    slot_route_ids!("8"),
    slot_route_ids!("9"),
    slot_route_ids!("10"),
    slot_route_ids!("11"),
    slot_route_ids!("12"),
    slot_route_ids!("13"),
    slot_route_ids!("14"),
    slot_route_ids!("15"),
    slot_route_ids!("16"),
];

#[derive(Params)]
pub struct TargetRoutes {
    #[id = "lfo1_on"]
    pub lfo1_on: BoolParam,
    #[id = "lfo1_amount"]
    pub lfo1: FloatParam,
    #[id = "lfo2_on"]
    pub lfo2_on: BoolParam,
    #[id = "lfo2_amount"]
    pub lfo2: FloatParam,
    #[id = "lfo3_on"]
    pub lfo3_on: BoolParam,
    #[id = "lfo3_amount"]
    pub lfo3: FloatParam,
    #[id = "wheel_on"]
    pub wheel_on: BoolParam,
    #[id = "wheel_amount"]
    pub wheel: FloatParam,
    #[id = "pressure_on"]
    pub pressure_on: BoolParam,
    #[id = "pressure_amount"]
    pub pressure: FloatParam,
    #[id = "velocity_on"]
    pub velocity_on: BoolParam,
    #[id = "velocity_amount"]
    pub velocity: FloatParam,
    #[id = "random_on"]
    pub random_on: BoolParam,
    #[id = "random_amount"]
    pub random: FloatParam,
}

impl TargetRoutes {
    fn new(target: usize) -> Self {
        Self {
            lfo1_on: presence(target, source::LFO1),
            lfo1: amount(target, source::LFO1),
            lfo2_on: presence(target, source::LFO2),
            lfo2: amount(target, source::LFO2),
            lfo3_on: presence(target, source::LFO3),
            lfo3: amount(target, source::LFO3),
            wheel_on: presence(target, source::WHEEL),
            wheel: amount(target, source::WHEEL),
            pressure_on: presence(target, source::PRESSURE),
            pressure: amount(target, source::PRESSURE),
            velocity_on: presence(target, source::VELOCITY),
            velocity: amount(target, source::VELOCITY),
            random_on: presence(target, source::RANDOM),
            random: amount(target, source::RANDOM),
        }
    }

    fn routes(&self, slot: usize, target: usize) -> [Route<'_>; SOURCES] {
        [
            route(slot, target, source::LFO1, &self.lfo1_on, &self.lfo1),
            route(slot, target, source::LFO2, &self.lfo2_on, &self.lfo2),
            route(slot, target, source::LFO3, &self.lfo3_on, &self.lfo3),
            route(slot, target, source::WHEEL, &self.wheel_on, &self.wheel),
            route(
                slot,
                target,
                source::PRESSURE,
                &self.pressure_on,
                &self.pressure,
            ),
            route(
                slot,
                target,
                source::VELOCITY,
                &self.velocity_on,
                &self.velocity,
            ),
            route(slot, target, source::RANDOM, &self.random_on, &self.random),
        ]
    }

    fn amount(&self, source: usize) -> &FloatParam {
        match source {
            source::LFO1 => &self.lfo1,
            source::LFO2 => &self.lfo2,
            source::LFO3 => &self.lfo3,
            source::WHEEL => &self.wheel,
            source::PRESSURE => &self.pressure,
            source::VELOCITY => &self.velocity,
            source::RANDOM => &self.random,
            _ => unreachable!("source is not declared"),
        }
    }
}

#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "route_pitch")]
    pub pitch: Box<TargetRoutes>,
    #[nested(id_prefix = "route_decay")]
    pub decay: Box<TargetRoutes>,
    #[nested(id_prefix = "route_attack")]
    pub attack: Box<TargetRoutes>,
    #[nested(id_prefix = "route_tone")]
    pub tone: Box<TargetRoutes>,
    #[nested(id_prefix = "route_body")]
    pub body: Box<TargetRoutes>,
    #[nested(id_prefix = "route_noise")]
    pub noise: Box<TargetRoutes>,
    #[nested(id_prefix = "route_character")]
    pub character: Box<TargetRoutes>,
    #[nested(id_prefix = "route_level")]
    pub level: Box<TargetRoutes>,
    #[nested(id_prefix = "route_pan")]
    pub pan: Box<TargetRoutes>,
    #[nested(id_prefix = "route_pitch_env")]
    pub pitch_env: Box<TargetRoutes>,
    #[nested(id_prefix = "route_pitch_decay")]
    pub pitch_decay: Box<TargetRoutes>,
    #[nested(id_prefix = "route_noise_decay")]
    pub noise_decay: Box<TargetRoutes>,
    #[nested(id_prefix = "route_dynamics")]
    pub dynamics: Box<TargetRoutes>,
}

impl Default for Routes {
    fn default() -> Self {
        Self {
            pitch: Box::new(TargetRoutes::new(target::PITCH)),
            decay: Box::new(TargetRoutes::new(target::DECAY)),
            attack: Box::new(TargetRoutes::new(target::ATTACK)),
            tone: Box::new(TargetRoutes::new(target::TONE)),
            body: Box::new(TargetRoutes::new(target::BODY)),
            noise: Box::new(TargetRoutes::new(target::NOISE)),
            character: Box::new(TargetRoutes::new(target::CHARACTER)),
            level: Box::new(TargetRoutes::new(target::LEVEL)),
            pan: Box::new(TargetRoutes::new(target::PAN)),
            pitch_env: Box::new(TargetRoutes::new(target::PITCH_ENV)),
            pitch_decay: Box::new(TargetRoutes::new(target::PITCH_DECAY)),
            noise_decay: Box::new(TargetRoutes::new(target::NOISE_DECAY)),
            dynamics: Box::new(TargetRoutes::new(target::DYNAMICS)),
        }
    }
}

impl Routes {
    #[cfg(test)]
    pub(crate) fn set_all_present_for_test(&self) {
        use nice_plug::params::InternalParamMut;

        for target in 0..TARGETS {
            let group = self.group(target);
            for present in [
                &group.lfo1_on,
                &group.lfo2_on,
                &group.lfo3_on,
                &group.wheel_on,
                &group.pressure_on,
                &group.velocity_on,
                &group.random_on,
            ] {
                // SAFETY: callers hold exclusive test ownership; no process or GUI thread exists.
                unsafe { present._internal_set_plain_value(true) };
            }
        }
    }

    #[must_use]
    pub fn target(&self, slot: usize, target: usize) -> [Route<'_>; SOURCES] {
        match target {
            target::PITCH => self.pitch.routes(slot, target),
            target::DECAY => self.decay.routes(slot, target),
            target::ATTACK => self.attack.routes(slot, target),
            target::TONE => self.tone.routes(slot, target),
            target::BODY => self.body.routes(slot, target),
            target::NOISE => self.noise.routes(slot, target),
            target::CHARACTER => self.character.routes(slot, target),
            target::LEVEL => self.level.routes(slot, target),
            target::PAN => self.pan.routes(slot, target),
            target::PITCH_ENV => self.pitch_env.routes(slot, target),
            target::PITCH_DECAY => self.pitch_decay.routes(slot, target),
            target::NOISE_DECAY => self.noise_decay.routes(slot, target),
            target::DYNAMICS => self.dynamics.routes(slot, target),
            _ => unreachable!("target is not declared"),
        }
    }

    fn group(&self, target: usize) -> &TargetRoutes {
        match target {
            target::PITCH => &self.pitch,
            target::DECAY => &self.decay,
            target::ATTACK => &self.attack,
            target::TONE => &self.tone,
            target::BODY => &self.body,
            target::NOISE => &self.noise,
            target::CHARACTER => &self.character,
            target::LEVEL => &self.level,
            target::PAN => &self.pan,
            target::PITCH_ENV => &self.pitch_env,
            target::PITCH_DECAY => &self.pitch_decay,
            target::NOISE_DECAY => &self.noise_decay,
            target::DYNAMICS => &self.dynamics,
            _ => unreachable!("target is not declared"),
        }
    }

    #[must_use]
    pub fn routing_from(&self, _previous: &Routing) -> Routing {
        let mut routing = Routing::new();
        for target in 0..TARGETS {
            let rows = self.target(0, target);
            for (source, row) in rows.iter().enumerate() {
                let param = self.group(target).amount(source);
                let assigned = row.is_present();
                let amount = param.smoothed.previous_value();
                // Assignment controls the interface and preset topology. DSP activity is narrower:
                // an assigned zero-depth route stays assigned but leaves the compact sample loop.
                routing.present[target][source] = assigned
                    && (param.value() != 0.0 || amount != 0.0 || param.smoothed.is_smoothing());
                routing.amounts[target][source] = amount;
            }
        }
        routing.compact();
        routing
    }

    #[must_use]
    pub fn omit_captured_parameter(&self, slot: usize, id: &str) -> bool {
        for (target, ids) in ROUTE_IDS[slot].iter().enumerate() {
            let routes = self.target(slot, target);
            for (source, (amount_id, presence_id)) in ids.iter().enumerate() {
                if id == *amount_id || id == *presence_id {
                    return !routes[source].is_present();
                }
            }
        }
        false
    }

    pub fn advance(&self, routing: &mut Routing) {
        for index in 0..routing.live().len() {
            let (target, source) = routing.live()[index];
            routing.amounts[target as usize][source as usize] = self
                .group(target as usize)
                .amount(source as usize)
                .smoothed
                .next();
        }
    }

    #[must_use]
    pub fn parameters(&self, slot: usize) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out = Vec::with_capacity(TARGETS * SOURCES * 2);
        for (target, ids) in ROUTE_IDS[slot].iter().enumerate() {
            let routes = self.target(slot, target);
            for (source, route) in routes.iter().enumerate() {
                out.push((ids[source].1, route.present as &dyn mxm_preset::ErasedParam));
                out.push((ids[source].0, route.amount as &dyn mxm_preset::ErasedParam));
            }
        }
        out
    }
}

#[must_use]
pub fn is_route_parameter(id: &str) -> bool {
    ROUTE_IDS
        .iter()
        .flatten()
        .flatten()
        .any(|(amount, presence)| id == *amount || id == *presence)
}

fn route<'a>(
    slot: usize,
    target: usize,
    source: usize,
    present: &'a BoolParam,
    amount: &'a FloatParam,
) -> Route<'a> {
    Route {
        source: SOURCE_NAMES[source],
        present,
        amount,
        amount_id: ROUTE_IDS[slot][target][source].0,
        present_id: ROUTE_IDS[slot][target][source].1,
    }
}

fn presence(target: usize, source: usize) -> BoolParam {
    BoolParam::new(
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source]),
        false,
    )
}

/// A route amount: signed, starting at zero — the collection's one route parameter
/// (`mxm_modulation_params::reading`), on the travel the pair's offer allows, which here is always
/// both halves.
fn amount(target: usize, source: usize) -> FloatParam {
    reading::amount_param(
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source]),
        reach(target, source),
        Fader::for_offer(offer(target, source), false),
        15.0,
    )
}

/// What a route reads: what it delivers, in its target's unit — semitones on the pitch, and a
/// percentage of the target's own range everywhere else, Amplitude's included (the standard
/// factor's `+100 %` is double, `-100 %` silence; it read `±12.0 dB` before the modulation
/// standard, `plans/plan-modulation-standard.md`).
fn reach(target: usize, source: usize) -> Reach {
    let unit = if target == target::PITCH {
        reading::SEMITONES
    } else {
        reading::PERCENT
    };
    Reach::new(FULL_SCALE[target][source], unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_table_matches_the_derived_parameter_surface() {
        let params = crate::params::MxmDrumMachineParams::default();
        let derived: Vec<_> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let mut route_count = 0;
        for slot in ROUTE_IDS {
            for row in slot {
                for (amount, presence) in row {
                    assert!(derived.contains(&amount.to_owned()), "missing {amount}");
                    assert!(derived.contains(&presence.to_owned()), "missing {presence}");
                    route_count += 2;
                }
            }
        }
        assert_eq!(route_count, SLOT_COUNT * TARGETS * SOURCES * 2);
        assert!(!derived.contains(&"route_pitch_lfo1_amount".to_owned()));
    }

    #[test]
    fn init_has_no_routes_but_dormant_amounts_remain_readable() {
        let routes = Routes::default();
        let routing = routes.routing_from(&Routing::new());
        assert!(!routing.any());
        assert!(routing.present.iter().flatten().all(|present| !present));
        assert!(
            routing
                .amounts
                .iter()
                .flatten()
                .all(|amount| *amount == 0.0)
        );
    }

    #[test]
    fn an_assigned_zero_route_stays_visible_but_leaves_the_dsp_list() {
        use nice_plug::params::InternalParamMut;

        let routes = Routes::default();
        let presence = &routes.pitch.lfo1_on;
        let amount = &routes.pitch.lfo1;
        unsafe {
            presence._internal_set_plain_value(true);
            amount._internal_update_smoother(48_000.0, true);
        }

        let mut routing = routes.routing_from(&Routing::new());
        assert!(routes.target(0, target::PITCH)[source::LFO1].is_present());
        assert!(!routing.any(), "assigned zero depth must cost no route DSP");

        unsafe {
            amount._internal_set_plain_value(0.5);
            amount._internal_update_smoother(48_000.0, false);
        }
        routing = routes.routing_from(&routing);
        assert!(routing.any());
        for _ in 0..1_000 {
            routes.advance(&mut routing);
        }

        unsafe {
            amount._internal_set_plain_value(0.0);
            amount._internal_update_smoother(48_000.0, false);
        }
        routing = routes.routing_from(&routing);
        assert!(routing.any(), "the return to zero remains smoothed");
        for _ in 0..1_000 {
            routes.advance(&mut routing);
        }
        routing = routes.routing_from(&routing);
        assert!(!routing.any(), "settled zero depth leaves the DSP list");
        assert!(
            routes.target(0, target::PITCH)[source::LFO1].is_present(),
            "zero depth must not remove the assignment"
        );
    }

    use mxm_plugin_test::routing_checks;

    /// Each target's group, by index.
    fn group(routes: &Routes, target: usize) -> &TargetRoutes {
        [
            &routes.pitch,
            &routes.decay,
            &routes.attack,
            &routes.tone,
            &routes.body,
            &routes.noise,
            &routes.character,
            &routes.level,
            &routes.pan,
            &routes.pitch_env,
            &routes.pitch_decay,
            &routes.noise_decay,
            &routes.dynamics,
        ][target]
    }

    /// **Every route parameter says what the DSP does** — the modulation standard's plugin half:
    /// each pair's travel is its offer's, its reading carries its target's unit and states what
    /// `mxm_drum_machine_dsp::conformance` measures a slot's own graph delivering, and every reading
    /// survives the host's round trip.
    ///
    /// Falsified before trusted: with Amplitude read in decibels again, it names every Amplitude
    /// pair from a performance source.
    #[test]
    fn every_route_parameter_says_what_the_dsp_does() {
        let routes = Routes::default();
        if let Err(failures) = routing_checks::amounts(
            &mxm_drum_machine_dsp::conformance::Declared,
            |target, source| Some(group(&routes, target).amount(source)),
        ) {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// **Amplitude reads a percentage of the level**, `+100 %` doubling it — the standard factor.
    #[test]
    fn amplitude_reads_the_standard_factor() {
        let route = amount(target::LEVEL, source::VELOCITY);
        assert_eq!(route.normalized_value_to_string(1.0, true), "+100 %");
        assert_eq!(route.normalized_value_to_string(0.25, true), "-50 %");
        assert_eq!(route.name(), "Amplitude from Velocity");
    }

    #[test]
    fn readings_round_trip_without_negative_zero() {
        let route = amount(target::PITCH, source::LFO1);
        for value in [-0.0001, 0.0, 0.0001, -1.0, 1.0] {
            let normalized = route.preview_normalized(value);
            let text = route.normalized_value_to_string(normalized, true);
            assert!(!text.contains("-0.00"), "{text}");
            let parsed = route
                .string_to_normalized_value(&text)
                .expect("route reading");
            assert!((route.preview_plain(parsed) - value).abs() < 0.001);
        }
    }
}
