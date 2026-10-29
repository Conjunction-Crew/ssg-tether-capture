//! UTC epoch strings in project files: `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
//!
//! brahe's own `isostring_with_decimals` drops the leading zeros of the fraction
//! (`…47.002464` reads back as `…47.2464`), so no file epoch goes through it.

use brahe::{Epoch, TimeSystem};

/// `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC, microseconds).
pub fn format_utc(epoch: Epoch) -> String {
    let (year, month, day, hour, minute, second, nanoseconds) =
        epoch.to_datetime_as_time_system(TimeSystem::UTC);
    let seconds = second + nanoseconds / 1e9;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{seconds:09.6}Z")
}

/// `YYYY-MM-DDTHH:MM:SS[.fff]` with an optional trailing `Z`, read as UTC. The
/// seconds field is parsed as one number, so `47.002464` is the same `f64` as the
/// literal in the harness. `None` for anything else.
pub fn parse_utc(raw: &str) -> Option<Epoch> {
    let raw = raw.strip_suffix('Z').unwrap_or(raw);
    let (date, time) = raw.split_once('T')?;
    let mut date = date.split('-');
    let mut time = time.splitn(3, ':');
    let field = |part: Option<&str>| part.and_then(|p| p.parse::<u32>().ok());
    let (year, month, day) = (
        field(date.next())?,
        field(date.next())?,
        field(date.next())?,
    );
    let (hour, minute) = (field(time.next())?, field(time.next())?);
    let seconds: f64 = time.next().and_then(|s| s.parse().ok())?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || !(0.0..61.0).contains(&seconds) {
        return None;
    }
    Some(Epoch::from_datetime(
        year,
        month as u8,
        day as u8,
        hour as u8,
        minute as u8,
        seconds,
        0.0,
        TimeSystem::UTC,
    ))
}

/// The wall clock now, rounded to the microsecond: the epoch a new project starts
/// from, so that the manifest's string and the simulation's epoch are the same
/// number (F-06).
pub fn now_utc() -> Epoch {
    let now = Epoch::now();
    parse_utc(&format_utc(now)).unwrap_or(now)
}
