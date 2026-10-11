//! A dependency-free UTC RFC3339 timestamp. Exists so `LocalWorld::commit_action`
//! can stamp events with the real time they happened, rather than a fixed
//! placeholder string, without pulling in a date/time crate for one field.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_rfc3339() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    rfc3339_from_unix_seconds(duration.as_secs())
}

/// Converts a Unix timestamp (seconds since epoch, UTC) into an RFC3339
/// string with second precision. Implements Howard Hinnant's well-known
/// civil-from-days algorithm.
pub fn rfc3339_from_unix_seconds(total_seconds: u64) -> String {
    let days = (total_seconds / 86_400) as i64;
    let secs_of_day = total_seconds % 86_400;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero_is_the_epoch() {
        assert_eq!(rfc3339_from_unix_seconds(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn leap_day_is_handled() {
        assert_eq!(rfc3339_from_unix_seconds(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn end_of_year_rolls_over_into_the_next() {
        assert_eq!(rfc3339_from_unix_seconds(978_307_199), "2000-12-31T23:59:59Z");
        assert_eq!(rfc3339_from_unix_seconds(978_307_200), "2001-01-01T00:00:00Z");
    }

    #[test]
    fn an_arbitrary_recent_timestamp_matches_the_system_date_command() {
        // Cross-checked against `date -u -d @1789907696` at write time.
        assert_eq!(
            rfc3339_from_unix_seconds(1_789_907_696),
            "2026-09-20T12:34:56Z"
        );
    }

    #[test]
    fn now_produces_a_well_formed_and_plausible_timestamp() {
        let now = now_rfc3339();
        assert_eq!(now.len(), 20);
        assert!(now.starts_with("20")); // still the 21st century
        assert!(now.ends_with('Z'));
    }
}
