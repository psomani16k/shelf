pub use std::time::Duration;
use std::{
    fmt::{self, Display},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
pub use tokio::time::{Instant, sleep, sleep_until, timeout};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    time: SystemTime,
}

impl Time {
    pub fn now() -> Self {
        Time {
            time: SystemTime::now(),
        }
    }

    pub fn unix_nanos(&self) -> u128 {
        self.time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    /// Drops the fraction of a second, matching the precision `Display` writes.
    pub fn whole_seconds(self) -> Self {
        let fraction = self
            .time
            .duration_since(UNIX_EPOCH)
            .map(|d| Duration::from_nanos(d.subsec_nanos().into()))
            .unwrap_or_default();
        Time {
            time: self.time - fraction,
        }
    }
}

impl From<SystemTime> for Time {
    fn from(time: SystemTime) -> Self {
        Time { time }
    }
}

impl Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only a clock past the year 9999 is out of jiff's range.
        let timestamp = jiff::Timestamp::try_from(self.time).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        write!(f, "{timestamp:.0}")
    }
}

/// Reads what `Display` writes, and any other RFC 3339 timestamp: other offsets and fractional
/// seconds parse too, but `Display` writes them back as UTC whole seconds.
impl FromStr for Time {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let timestamp: jiff::Timestamp = s
            .parse()
            .with_context(|| format!("{s:?} isn't an RFC 3339 timestamp"))?;
        Ok(Time {
            time: timestamp.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_is_utc_whole_seconds() {
        let time = Time {
            time: UNIX_EPOCH + Duration::new(1_761_453_900, 999_999_999),
        };
        assert_eq!(time.to_string(), "2025-10-26T04:45:00Z");
    }

    #[test]
    fn whole_seconds_round_trips_through_text() {
        let time = Time::from(UNIX_EPOCH + Duration::new(1_761_453_900, 999_999_999)).whole_seconds();
        assert_eq!(time.unix_nanos(), 1_761_453_900_000_000_000);
        assert_eq!(time.to_string().parse::<Time>().unwrap(), time);
    }

    #[test]
    fn parse_reverses_display() {
        let time: Time = "2025-10-26T04:45:00Z".parse().unwrap();
        assert_eq!(time.unix_nanos(), 1_761_453_900_000_000_000);
        assert_eq!(time.to_string(), "2025-10-26T04:45:00Z");
    }

    #[test]
    fn parse_accepts_offsets_and_fractions() {
        let time: Time = "2025-10-26T10:15:00.5+05:30".parse().unwrap();
        assert_eq!(time.unix_nanos(), 1_761_453_900_500_000_000);
        assert_eq!(time.to_string(), "2025-10-26T04:45:00Z");
    }

    #[test]
    fn parse_rejects_non_timestamps() {
        assert!("2025-10-26".parse::<Time>().is_err());
        assert!("2025-10-26T04:45:00".parse::<Time>().is_err());
        assert!("yesterday".parse::<Time>().is_err());
    }
}
