//! Tests for the UTC epoch strings of project files (`project/epoch.rs`, P6 S3).

use brahe::{Epoch, TimeSystem};

use crate::project::{format_utc, parse_utc};
use crate::test_support::{Lcg, fixed_epoch};

/// A random `YYYY-MM-DDTHH:MM:SS.ffffffZ` string (days up to 28, so every month has them).
fn generated_epoch_string(rng: &mut Lcg) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        2000 + rng.below(60),
        1 + rng.below(12),
        1 + rng.below(28),
        rng.below(24),
        rng.below(60),
        rng.below(60),
        rng.below(1_000_000),
    )
}

#[test]
fn epoch_strings_survive_a_parse_and_a_format() {
    let seed = 0x5eed_0e0c;
    let mut rng = Lcg::new(seed);
    for case in 0..2_000 {
        let text = generated_epoch_string(&mut rng);
        let epoch = parse_utc(&text)
            .unwrap_or_else(|| panic!("seed {seed:#x} case {case}: `{text}` did not parse"));
        assert_eq!(
            format_utc(epoch),
            text,
            "seed {seed:#x} case {case}: format(parse(s)) != s"
        );
        let again = parse_utc(&format_utc(epoch)).expect("a formatted epoch parses");
        assert_eq!(
            again, epoch,
            "seed {seed:#x} case {case} (`{text}`): parse(format(e)) != e"
        );
    }
    println!("seed {seed:#x}: 2000 epoch strings round-trip");
}

#[test]
fn a_fraction_with_leading_zeros_is_kept() {
    // The brahe bug: `isostring_with_decimals` writes `…47.2464` for this epoch.
    let epoch = fixed_epoch();
    assert_eq!(format_utc(epoch), "2026-04-03T16:03:47.002464Z");
    assert_eq!(parse_utc("2026-04-03T16:03:47.002464Z"), Some(epoch));
    assert_eq!(
        parse_utc("2026-04-03T16:03:47.002464"),
        Some(epoch),
        "the trailing Z is optional"
    );
}

#[test]
fn whole_seconds_and_the_year_are_zero_padded() {
    let epoch = Epoch::from_datetime(2026, 1, 2, 3, 4, 5.0, 0.0, TimeSystem::UTC);
    assert_eq!(format_utc(epoch), "2026-01-02T03:04:05.000000Z");
    assert_eq!(parse_utc("2026-01-02T03:04:05Z"), Some(epoch));
}

#[test]
fn malformed_epoch_strings_do_not_parse() {
    for bad in [
        "",
        "now",
        "2026-04-03",
        "2026-04-03T16:03",
        "2026-13-03T16:03:47",
        "2026-04-32T16:03:47",
        "2026-04-00T16:03:47",
        "2026-04-03T24:03:47",
        "2026-04-03T16:60:47",
        "2026-04-03T16:03:61",
        "2026-04-03T16:03:-1",
        "2026-04-03-01T16:03:47",
        "2026-04-03T16:03:47ZZ",
        "2026-04-03 16:03:47",
        "x-04-03T16:03:47",
    ] {
        assert!(parse_utc(bad).is_none(), "`{bad}` must not parse");
    }
}
