//! Four route slots per drum, each a source, a target and an amount — mxm-model-drums' scheme (the
//! owner, 2026-09-30: the host holds what the panel can show at once) in place of a presence and an
//! amount for every one of the DSP's 13 targets × 7 sources.
//!
//! **The DSP is unchanged.** Its per-slot grid (`mxm_drum_machine_dsp::routing::Routing`) is filled
//! from the four slots: a slot whose source and target both name something the grid has sets that
//! pair present and adds its amount there, so one route reaches exactly what the same pair reached
//! before, at the same per-route reach (`routing::FULL_SCALE`), and a kit with no routes — every
//! factory kit — fills nothing and renders as it did.
//!
//! **What changed in behaviour** (recorded in the plugin's `NOTES.md`): a route is off when its source
//! or its target is Off, and switching its source off keeps its target and amount; four routes a drum
//! is the limit; two routes on one source and target add; a route aimed at a control this machine
//! does not use (12–20) does nothing; and an amount reads as a percentage of its target's full reach
//! for the host, since one parameter serves every target.

use mxm_drum_machine_dsp::routing::{Routing, source, target};
use nice_plug::prelude::*;
use std::sync::Arc;

use crate::params::{CONTROLS, control};

/// The route slots a drum holds.
pub const ROUTES: usize = 4;

/// A route's source, in model-drums' order.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChoice {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "lfo1"]
    #[name = "LFO 1"]
    Lfo1,
    #[id = "lfo2"]
    #[name = "LFO 2"]
    Lfo2,
    #[id = "lfo3"]
    #[name = "LFO 3"]
    Lfo3,
    #[id = "wheel"]
    #[name = "Wheel"]
    Wheel,
    #[id = "pressure"]
    #[name = "Pressure"]
    Pressure,
    #[id = "velocity"]
    #[name = "Velocity"]
    Velocity,
    #[id = "random"]
    #[name = "Random"]
    Random,
}

impl SourceChoice {
    /// The sources a route can take, without Off, in the parameter's order.
    pub const ALL: [Self; 7] = [
        Self::Lfo1,
        Self::Lfo2,
        Self::Lfo3,
        Self::Wheel,
        Self::Pressure,
        Self::Velocity,
        Self::Random,
    ];

    /// The DSP's source, or `None` for Off. The seven are the grid's seven, one for one.
    #[must_use]
    pub const fn dsp(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Lfo1 => Some(source::LFO1),
            Self::Lfo2 => Some(source::LFO2),
            Self::Lfo3 => Some(source::LFO3),
            Self::Wheel => Some(source::WHEEL),
            Self::Pressure => Some(source::PRESSURE),
            Self::Velocity => Some(source::VELOCITY),
            Self::Random => Some(source::RANDOM),
        }
    }

    /// What the panel calls it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Lfo1 => "LFO 1",
            Self::Lfo2 => "LFO 2",
            Self::Lfo3 => "LFO 3",
            Self::Wheel => "Wheel",
            Self::Pressure => "Pressure",
            Self::Velocity => "Velocity",
            Self::Random => "Random",
        }
    }
}

/// What a route moves, in model-drums' order: Off, Control 1…20, Level, Pan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum RouteTarget {
    #[default]
    Off,
    /// One of the slot's general controls, 1…20.
    Control(u8),
    Level,
    Pan,
}

impl RouteTarget {
    /// Targets a route parameter holds: Off, twenty controls, Level and Pan.
    pub const COUNT: usize = CONTROLS + 3;

    /// The target at a parameter's index; out of range is Off.
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        match index {
            1..=CONTROLS => Self::Control(index as u8),
            i if i == CONTROLS + 1 => Self::Level,
            i if i == CONTROLS + 2 => Self::Pan,
            _ => Self::Off,
        }
    }

    /// The target's parameter index, [`Self::from_index`]'s inverse.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Control(k) => usize::from(k),
            Self::Level => CONTROLS + 1,
            Self::Pan => CONTROLS + 2,
        }
    }

    /// **The DSP's target this moves**: each used control its own axis (`params::control`), Level
    /// the Amplitude target, Pan the pan; `None` for Off and for controls 12–20, which mean nothing
    /// here. Every one of the grid's thirteen targets is some route target's.
    #[must_use]
    pub const fn dsp(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Level => Some(target::LEVEL),
            Self::Pan => Some(target::PAN),
            Self::Control(k) => match k as usize {
                control::TUNE => Some(target::PITCH),
                control::DECAY => Some(target::DECAY),
                control::TONE => Some(target::TONE),
                control::ATTACK => Some(target::ATTACK),
                control::DYNAMICS => Some(target::DYNAMICS),
                control::PITCH_ENV => Some(target::PITCH_ENV),
                control::PITCH_DECAY => Some(target::PITCH_DECAY),
                control::BODY => Some(target::BODY),
                control::NOISE => Some(target::NOISE),
                control::NOISE_DECAY => Some(target::NOISE_DECAY),
                control::CHARACTER => Some(target::CHARACTER),
                _ => None,
            },
        }
    }
}

/// One route slot: a source, a target and an amount (`route<r>_{source,target,amount}`).
#[derive(Params)]
pub struct RouteParams {
    #[id = "source"]
    pub source: EnumParam<SourceChoice>,
    #[id = "target"]
    pub target: IntParam,
    #[id = "amount"]
    pub amount: FloatParam,
}

impl RouteParams {
    fn new(r: usize) -> Self {
        Self {
            source: EnumParam::new(format!("Route {r} source"), SourceChoice::Off),
            target: target_param(r),
            // The old pairs' travel and smoothing (`reading::amount_param` on a two-sided offer:
            // −1…+1, linear, 15 ms); its reading is the target's share, since it serves every target.
            amount: FloatParam::new(
                format!("Route {r} amount"),
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(15.0))
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }

    /// What the route moves, as its parameter stands.
    #[must_use]
    pub fn target(&self) -> RouteTarget {
        RouteTarget::from_index(self.target.value().max(0) as usize)
    }

    /// Whether the route is in use: a source and a target, neither Off. One that is not contributes
    /// nothing and keeps its amount.
    #[must_use]
    pub fn in_use(&self) -> bool {
        self.source.value() != SourceChoice::Off && self.target() != RouteTarget::Off
    }

    /// The grid pair `(target, source)` the route fills, if it fills one.
    #[must_use]
    pub fn pair(&self) -> Option<(usize, usize)> {
        Some((self.target().dsp()?, self.source.value().dsp()?))
    }
}

/// The four route slots, nested so their IDs are `route<r>_<field>` inside a slot.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "route1")]
    pub route1: RouteParams,
    #[nested(id_prefix = "route2")]
    pub route2: RouteParams,
    #[nested(id_prefix = "route3")]
    pub route3: RouteParams,
    #[nested(id_prefix = "route4")]
    pub route4: RouteParams,
}

impl Default for Routes {
    fn default() -> Self {
        Self {
            route1: RouteParams::new(1),
            route2: RouteParams::new(2),
            route3: RouteParams::new(3),
            route4: RouteParams::new(4),
        }
    }
}

impl Routes {
    /// The four in order.
    #[must_use]
    pub fn all(&self) -> [&RouteParams; ROUTES] {
        [&self.route1, &self.route2, &self.route3, &self.route4]
    }

    /// **The DSP's grid, filled from the four slots.** A pair is live when a route on it has an
    /// amount, or one still smoothing: as before, an assigned route at settled zero stays assigned
    /// but leaves the compact per-sample list. Two routes on one pair add.
    #[must_use]
    pub fn routing_from(&self, _previous: &Routing) -> Routing {
        let mut routing = Routing::new();
        let all = self.all();
        let pairs = all.map(RouteParams::pair);
        for (r, route) in all.into_iter().enumerate() {
            let Some((target, source)) = pairs[r] else {
                continue;
            };
            let param = &route.amount;
            let amount = param.smoothed.previous_value();
            routing.present[target][source] |=
                param.value() != 0.0 || amount != 0.0 || param.smoothed.is_smoothing();
            // The first route on a pair sets it, so one route alone is exactly the old pair's value.
            if pairs[..r].contains(&pairs[r]) {
                routing.amounts[target][source] += amount;
            } else {
                routing.amounts[target][source] = amount;
            }
        }
        routing.compact();
        routing
    }

    /// Advances the smoothed amount of every route on a live pair, once a sample, into the grid.
    pub fn advance(&self, routing: &mut Routing) {
        let all = self.all();
        let pairs = all.map(RouteParams::pair);
        for (r, route) in all.into_iter().enumerate() {
            let Some((target, source)) = pairs[r] else {
                continue;
            };
            if !routing.present[target][source] {
                continue;
            }
            let next = route.amount.smoothed.next();
            if pairs[..r].contains(&pairs[r]) {
                routing.amounts[target][source] += next;
            } else {
                routing.amounts[target][source] = next;
            }
        }
    }
}

/// A route's target: Off, Control 1…20, Level, Pan.
fn target_param(r: usize) -> IntParam {
    IntParam::new(
        format!("Route {r} target"),
        0,
        IntRange::Linear {
            min: 0,
            max: RouteTarget::COUNT as i32 - 1,
        },
    )
    .with_value_to_string(Arc::new(|value| {
        match RouteTarget::from_index(value.max(0) as usize) {
            RouteTarget::Off => "Off".to_owned(),
            RouteTarget::Control(k) => format!("Control {k}"),
            RouteTarget::Level => "Level".to_owned(),
            RouteTarget::Pan => "Pan".to_owned(),
        }
    }))
    .with_string_to_value(Arc::new(|text| {
        let text = text.trim();
        if text.eq_ignore_ascii_case("off") {
            Some(0)
        } else if text.eq_ignore_ascii_case("level") {
            Some(CONTROLS as i32 + 1)
        } else if text.eq_ignore_ascii_case("pan") {
            Some(CONTROLS as i32 + 2)
        } else {
            text.strip_prefix("Control ")
                .unwrap_or(text)
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|k| (1..=CONTROLS as i32).contains(k))
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_drum_machine_dsp::routing::{SOURCES, TARGETS};
    use nice_plug::params::InternalParamMut;

    /// Sets route `r` to `source` on `target` at `amount`, its smoother at rest there.
    fn set(routes: &Routes, r: usize, source: SourceChoice, target: RouteTarget, amount: f32) {
        let route = routes.all()[r];
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            route.source._internal_set_plain_value(source);
            route
                .target
                ._internal_set_plain_value(target.index() as i32);
            route.amount._internal_set_plain_value(amount);
            route.amount._internal_update_smoother(48_000.0, true);
        }
    }

    #[test]
    fn init_has_no_routes() {
        let routing = Routes::default().routing_from(&Routing::new());
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

    /// **Every grid pair is reachable from a route slot, and each from exactly one source and one
    /// target**: the seven sources one for one, the thirteen targets from Controls 1–11, Level and
    /// Pan, and nothing from Off or Controls 12–20.
    #[test]
    fn every_grid_pair_has_exactly_one_route_spelling() {
        let mut targets = [0; TARGETS];
        for index in 0..RouteTarget::COUNT {
            if let Some(t) = RouteTarget::from_index(index).dsp() {
                targets[t] += 1;
            }
        }
        assert_eq!(targets, [1; TARGETS]);
        for k in control::USED + 1..=CONTROLS {
            assert_eq!(RouteTarget::Control(k as u8).dsp(), None, "control {k}");
        }
        let mut sources = [0; SOURCES];
        for choice in SourceChoice::ALL {
            sources[choice.dsp().expect("a source")] += 1;
        }
        assert_eq!(sources, [1; SOURCES]);
        assert_eq!(SourceChoice::Off.dsp(), None);
        for index in 0..RouteTarget::COUNT {
            assert_eq!(RouteTarget::from_index(index).index(), index);
        }
    }

    /// One route fills its pair alone, at its amount; a route whose source or target is Off, or
    /// whose target this machine does not use, fills nothing and keeps its amount.
    #[test]
    fn a_route_fills_its_pair_and_an_unused_one_fills_nothing() {
        let routes = Routes::default();
        set(&routes, 0, SourceChoice::Lfo2, RouteTarget::Control(1), 0.5);
        let routing = routes.routing_from(&Routing::new());
        assert_eq!(routing.live(), &[(target::PITCH as u8, source::LFO2 as u8)]);
        assert_eq!(routing.amounts[target::PITCH][source::LFO2], 0.5);

        for (source, target) in [
            (SourceChoice::Off, RouteTarget::Control(1)),
            (SourceChoice::Lfo2, RouteTarget::Off),
            (SourceChoice::Lfo2, RouteTarget::Control(12)),
        ] {
            set(&routes, 0, source, target, 0.5);
            assert!(
                !routes.routing_from(&Routing::new()).any(),
                "{source:?} {target:?}"
            );
            assert_eq!(routes.route1.amount.value(), 0.5, "the amount stays");
        }
    }

    /// Two routes on one pair add; each advances its own smoother once a sample.
    #[test]
    fn two_routes_on_one_pair_add() {
        let routes = Routes::default();
        set(&routes, 1, SourceChoice::Random, RouteTarget::Pan, 0.25);
        set(&routes, 3, SourceChoice::Random, RouteTarget::Pan, 0.5);
        let mut routing = routes.routing_from(&Routing::new());
        assert_eq!(routing.live().len(), 1);
        assert_eq!(routing.amounts[target::PAN][source::RANDOM], 0.75);
        routes.advance(&mut routing);
        assert_eq!(routing.amounts[target::PAN][source::RANDOM], 0.75);
    }

    #[test]
    fn an_assigned_zero_route_stays_assigned_but_leaves_the_dsp_list() {
        let routes = Routes::default();
        set(&routes, 0, SourceChoice::Lfo1, RouteTarget::Control(1), 0.0);
        let mut routing = routes.routing_from(&Routing::new());
        assert!(routes.route1.in_use());
        assert!(!routing.any(), "assigned zero depth must cost no route DSP");

        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            routes.route1.amount._internal_set_plain_value(0.5);
            routes
                .route1
                .amount
                ._internal_update_smoother(48_000.0, false);
        }
        routing = routes.routing_from(&routing);
        assert!(routing.any());
        for _ in 0..1_000 {
            routes.advance(&mut routing);
        }

        // SAFETY: as above.
        unsafe {
            routes.route1.amount._internal_set_plain_value(0.0);
            routes
                .route1
                .amount
                ._internal_update_smoother(48_000.0, false);
        }
        routing = routes.routing_from(&routing);
        assert!(routing.any(), "the return to zero remains smoothed");
        for _ in 0..1_000 {
            routes.advance(&mut routing);
        }
        routing = routes.routing_from(&routing);
        assert!(!routing.any(), "settled zero depth leaves the DSP list");
        assert!(
            routes.route1.in_use(),
            "zero depth must not remove the route"
        );
    }

    /// **One amount serves every pair**: the DSP offers each of its pairs on both halves
    /// (`mxm_drum_machine_dsp::conformance::Declared`), so a route slot's −1…+1 travel is every
    /// pair's — the plugin half of the modulation standard that four general route slots can keep.
    /// The pair-by-pair reading `mxm_plugin_test::routing_checks` holds needs a parameter per pair,
    /// which this surface no longer has: a recorded deviation, as model-drums'.
    #[test]
    fn a_route_amount_travels_what_every_pair_is_offered() {
        use mxm_modulation::conformance::Declaration;
        use mxm_modulation_params::reading::Fader;

        let declared = mxm_drum_machine_dsp::conformance::Declared;
        let routes = Routes::default();
        let amount = &routes.route1.amount;
        let travel = (amount.preview_plain(0.0), amount.preview_plain(1.0));
        for t in 0..declared.targets() {
            for s in 0..declared.sources() {
                let offer = declared.offered(t, s);
                assert_eq!(
                    Fader::for_offer(offer, false).bounds(),
                    travel,
                    "{}",
                    declared.name(t, s)
                );
            }
        }
    }

    #[test]
    fn target_text_round_trips() {
        let target = target_param(1);
        for index in 0..RouteTarget::COUNT as i32 {
            let text = target.normalized_value_to_string(target.preview_normalized(index), true);
            let parsed = target
                .string_to_normalized_value(&text)
                .unwrap_or_else(|| panic!("could not parse {text:?}"));
            assert_eq!(target.preview_plain(parsed), index, "{text}");
        }
    }
}
