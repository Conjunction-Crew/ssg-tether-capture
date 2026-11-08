//! Per-phase controller gains (P9 S4, Q10 row 17, E2 (a)): the eight constants of
//! the P7.5 force law (design D16, D18) as optional keys of a phase's
//! `controller`, each defaulting to the constant it replaces. The law's structure
//! is unchanged; with no key (or every key at its default) the forces are the same
//! floats as before.

use serde::{Deserialize, Serialize};

use super::{
    BRANCH_DEAD_BAND, FORCE_RAMP_S, INNER_EDGE_FRACTION, PLANE_HOLD_BAND_M, RADIAL_RATE_BAND_MPS,
    RADIUS_HOLD_BAND_M, SPACING_BAND_M, SPACING_RATE_BAND_MPS,
};

/// A phase's controller gains. Every field is optional; an absent field takes
/// the default the controller had before P9.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct PhaseController {
    /// The circle law: the radius error, in m, at which the radial term that holds
    /// a body on its sphere reaches full weight; proportional below it. Default
    /// 2.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_hold_band_m: Option<f64>,
    /// The circle law: the outward speed against the sphere's own shrink, in m/s,
    /// at which the rate term reaches full weight. Default 0.1; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radial_rate_band_mps: Option<f64>,
    /// The tail's lag law: the error in the root-to-tail chord, in m, at which the
    /// tail's tangent weight saturates. Default 3.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_band_m: Option<f64>,
    /// The tail's lag law: the difference in the ends' speeds along their forward
    /// tangents, in m/s, at which the rate term saturates the tail's tangent
    /// weight. Default 0.3; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_rate_band_mps: Option<f64>,
    /// `hold_plane`: the height above or below the circling plane, in m, at which
    /// the component toward the plane reaches full weight. Default 1.0; greater
    /// than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_hold_band_m: Option<f64>,
    /// The branch law's inner edge as a fraction of the sphere radius: inside it
    /// the body is pushed away from the target. Default 0.8; greater than 0 and
    /// less than 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_edge_fraction: Option<f64>,
    /// The relative width of the hysteresis band on the branch law's inner and
    /// outer edges, and of the velocity kill band (at least one step's Δv); 0
    /// removes the hysteresis. Default 0.02; at least 0 and less than 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_band_fraction: Option<f64>,
    /// Seconds the force bound takes to rise from 0 to `max_force_n` at the
    /// capture start and at every phase entry. Default 1.0; greater than 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_ramp_s: Option<f64>,
}

impl PhaseController {
    /// The gains this phase runs with: each key, or its default.
    pub(crate) fn gains(&self) -> ControllerGains {
        let defaults = ControllerGains::default();
        ControllerGains {
            radius_hold_band_m: self
                .radius_hold_band_m
                .unwrap_or(defaults.radius_hold_band_m),
            radial_rate_band_mps: self
                .radial_rate_band_mps
                .unwrap_or(defaults.radial_rate_band_mps),
            spacing_band_m: self.spacing_band_m.unwrap_or(defaults.spacing_band_m),
            spacing_rate_band_mps: self
                .spacing_rate_band_mps
                .unwrap_or(defaults.spacing_rate_band_mps),
            plane_hold_band_m: self.plane_hold_band_m.unwrap_or(defaults.plane_hold_band_m),
            inner_edge_fraction: self
                .inner_edge_fraction
                .unwrap_or(defaults.inner_edge_fraction),
            dead_band_fraction: self
                .dead_band_fraction
                .unwrap_or(defaults.dead_band_fraction),
            force_ramp_s: self.force_ramp_s.unwrap_or(defaults.force_ramp_s),
        }
    }
}

/// The eight gains a phase runs with, resolved ([`PhaseController::gains`]).
/// The default is the controller's constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllerGains {
    pub radius_hold_band_m: f64,
    pub radial_rate_band_mps: f64,
    pub spacing_band_m: f64,
    pub spacing_rate_band_mps: f64,
    pub plane_hold_band_m: f64,
    pub inner_edge_fraction: f64,
    pub dead_band_fraction: f64,
    pub force_ramp_s: f64,
}

impl Default for ControllerGains {
    fn default() -> Self {
        Self {
            radius_hold_band_m: RADIUS_HOLD_BAND_M,
            radial_rate_band_mps: RADIAL_RATE_BAND_MPS,
            spacing_band_m: SPACING_BAND_M,
            spacing_rate_band_mps: SPACING_RATE_BAND_MPS,
            plane_hold_band_m: PLANE_HOLD_BAND_M,
            inner_edge_fraction: INNER_EDGE_FRACTION,
            dead_band_fraction: BRANCH_DEAD_BAND,
            force_ramp_s: FORCE_RAMP_S,
        }
    }
}

/// What a gain must be.
#[derive(Clone, Copy)]
enum Rule {
    Positive,
    Fraction,
    Band,
}

impl Rule {
    fn holds(self, v: f64) -> bool {
        match self {
            Rule::Positive => v.is_finite() && v > 0.0,
            Rule::Fraction => v > 0.0 && v < 1.0,
            Rule::Band => (0.0..0.5).contains(&v),
        }
    }

    fn message(self) -> &'static str {
        match self {
            Rule::Positive => "must be a finite number greater than 0",
            Rule::Fraction => "must be greater than 0 and less than 1",
            Rule::Band => "must be at least 0 and less than 0.5",
        }
    }
}

/// The broken rules of `controller`, as `(key, message)`, in field order.
pub(crate) fn controller_errors(controller: &PhaseController) -> Vec<(&'static str, &'static str)> {
    [
        (
            "radius_hold_band_m",
            controller.radius_hold_band_m,
            Rule::Positive,
        ),
        (
            "radial_rate_band_mps",
            controller.radial_rate_band_mps,
            Rule::Positive,
        ),
        ("spacing_band_m", controller.spacing_band_m, Rule::Positive),
        (
            "spacing_rate_band_mps",
            controller.spacing_rate_band_mps,
            Rule::Positive,
        ),
        (
            "plane_hold_band_m",
            controller.plane_hold_band_m,
            Rule::Positive,
        ),
        (
            "inner_edge_fraction",
            controller.inner_edge_fraction,
            Rule::Fraction,
        ),
        (
            "dead_band_fraction",
            controller.dead_band_fraction,
            Rule::Band,
        ),
        ("force_ramp_s", controller.force_ramp_s, Rule::Positive),
    ]
    .into_iter()
    .filter(|(_, value, rule)| value.is_some_and(|v| !rule.holds(v)))
    .map(|(key, _, rule)| (key, rule.message()))
    .collect()
}
