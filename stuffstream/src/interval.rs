use time::Duration;

const INTERVALS: &[(u64, &str, &str)] = &[
    (1, "1 seconds", "second"),
    (2, "2 seconds", "second"),
    (5, "5 seconds", "second"),
    (10, "10 seconds", "second"),
    (30, "30 seconds", "second"),
    (60, "1 minute", "minute"),
    (2 * 60, "2 minutes", "minute"),
    (5 * 60, "5 minutes", "minute"),
    (10 * 60, "10 minutes", "minute"),
    (30 * 60, "30 minutes", "minute"),
    (3600, "1 hour", "hour"),
    (2 * 3600, "2 hours", "hour"),
    (5 * 3600, "5 hours", "hour"),
    (10 * 3600, "10 hours", "hour"),
    (24 * 3600, "1 day", "day"),
    (2 * 24 * 3600, "2 days", "day"),
    (7 * 24 * 3600, "1 week", "week"),
    (2 * 7 * 24 * 3600, "2 week", "week"),
    (30 * 24 * 3600, "1 month", "month"),
    (2 * 30 * 24 * 3600, "2 months", "month"),
    (3 * 30 * 24 * 3600, "3 months", "month"),
    (4 * 30 * 24 * 3600, "4 months", "month"),
    (6 * 30 * 24 * 3600, "6 months", "month"),
    (365 * 24 * 3600, "1 year", "year"),
    (2 * 365 * 24 * 3600, "2 years", "year"),
    (5 * 365 * 24 * 3600, "5 years", "year"),
    (10 * 365 * 24 * 3600, "10 years", "year"),
    (20 * 365 * 24 * 3600, "20 years", "year"),
    (50 * 365 * 24 * 3600, "50 years", "year"),
];

#[derive(Debug)]
pub struct CountsInterval {
    pub seconds: u64,
    pub truncate: String,
    pub interval: String,
}

impl From<Duration> for CountsInterval {
    fn from(duration: Duration) -> Self {
        let duration: u64 = duration.whole_seconds().unsigned_abs();
        for (seconds, interval, trunc) in INTERVALS {
            if duration / seconds < 100 {
                return Self {
                    seconds: *seconds,
                    truncate: trunc.to_string(),
                    interval: interval.to_string(),
                };
            }
        }

        Self {
            seconds: 100 * 365 * 24 * 3600,
            truncate: "year".to_string(),
            interval: "100 years".to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn intervals() {
        let i = CountsInterval::from(Duration::seconds(50));
        assert_eq!(i.interval, "1 seconds");

        let i = CountsInterval::from(Duration::hours(4));
        assert_eq!(i.interval, "5 minutes");
    }
}
