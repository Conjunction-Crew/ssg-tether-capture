//! The SGP4 state of a GP record from the portable copy of sgp4 (`sgp4p`).
//!
//! [`teme_state`] builds the copy's elements from a [`GpElements`] record the way
//! brahe 1.3.4's `SGPPropagator::from_omm_elements` does, field for field, with
//! the arguments the simulator passes brahe today, and propagates them as
//! brahe's `propagate_internal` does. The result is SGP4's TEME state in metres
//! and metres per second; the copy's math runs on the `libm` crate, so the same
//! record gives the same bits on every platform (F-48).

use brahe::{Epoch, TimeSystem};
use chrono::{Datelike, NaiveDateTime, Timelike};
use nalgebra::Vector6;

use crate::orbit::catalog::GpElements;

/// The TEME state (m, m/s) of `gp` at `epoch` from the SGP4 copy.
pub fn teme_state(gp: &GpElements, epoch: Epoch) -> Result<Vector6<f64>, String> {
    let norad = gp.norad_id;
    let datetime = NaiveDateTime::parse_from_str(&gp.epoch_str, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(&gp.epoch_str, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|e| format!("NORAD {norad}: invalid epoch '{}': {e}", gp.epoch_str))?;
    let classification = match gp.classification.unwrap_or('U') {
        'U' | ' ' => sgp4p::Classification::Unclassified,
        'C' => sgp4p::Classification::Classified,
        'S' => sgp4p::Classification::Secret,
        c => {
            return Err(format!(
                "NORAD {norad}: invalid classification '{c}' (must be 'U', 'C' or 'S')"
            ));
        }
    };
    let elements = sgp4p::Elements {
        object_name: None,
        international_designator: None,
        norad_id: u64::from(norad),
        classification,
        datetime,
        mean_motion_dot: gp.mean_motion_dot.unwrap_or(0.0),
        mean_motion_ddot: gp.mean_motion_ddot.unwrap_or(0.0),
        drag_term: gp.bstar.unwrap_or(0.0),
        element_set_number: 999,
        inclination: gp.inclination_deg,
        right_ascension: gp.raan_deg,
        eccentricity: gp.eccentricity,
        argument_of_perigee: gp.arg_perigee_deg,
        mean_anomaly: gp.mean_anomaly_deg,
        mean_motion: gp.mean_motion_rev_per_day,
        revolution_number: 0,
        ephemeris_type: gp.ephemeris_type.unwrap_or(0),
    };
    let constants = sgp4p::Constants::from_elements(&elements)
        .map_err(|e| format!("NORAD {norad}: SGP4 constants error: {e}"))?;
    let record_epoch = Epoch::from_datetime(
        datetime.year() as u32,
        datetime.month() as u8,
        datetime.day() as u8,
        datetime.hour() as u8,
        datetime.minute() as u8,
        datetime.second() as f64 + datetime.nanosecond() as f64 / 1e9,
        0.0,
        TimeSystem::UTC,
    );
    let minutes = (epoch - record_epoch) / 60.0;
    let prediction = constants
        .propagate(sgp4p::MinutesSinceEpoch(minutes))
        .map_err(|e| format!("NORAD {norad}: SGP4 propagation failed at {epoch}: {e}"))?;
    Ok(Vector6::new(
        prediction.position[0] * 1000.0,
        prediction.position[1] * 1000.0,
        prediction.position[2] * 1000.0,
        prediction.velocity[0] * 1000.0,
        prediction.velocity[1] * 1000.0,
        prediction.velocity[2] * 1000.0,
    ))
}
