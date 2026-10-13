use super::ListError;

/// Slack's local List HLC. This is ordering metadata, not an idempotency key.
#[derive(Clone, Debug)]
pub struct ListClock {
    milliseconds: u64,
    counter: u16,
    node: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListTimestamp(String);
impl ListTimestamp {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ListTimestamp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl Default for ListClock {
    fn default() -> Self {
        Self::new()
    }
}
impl ListClock {
    pub fn new() -> Self {
        // PHBi uses the last 16 characters of a UUID without its dashes.
        let node = uuid::Uuid::new_v4().simple().to_string()[16..].to_owned();
        let milliseconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        Self {
            milliseconds,
            counter: 0,
            node,
        }
    }

    /// Captured getTimestamp: max(local, now), increment at the same millisecond,
    /// reset when time advances, reject >60 seconds of drift and counter overflow.
    /// Errors leave the clock unchanged, allowing recovery when real time catches up.
    pub fn tick(&mut self, now_ms: u64) -> Result<ListTimestamp, ListError> {
        let milliseconds = self.milliseconds.max(now_ms);
        if milliseconds > 8_640_000_000_000_000 || milliseconds - now_ms > 60_000 {
            return Err(ListError(
                "The system clock cannot timestamp this List change.",
            ));
        }
        let counter = if milliseconds == self.milliseconds {
            self.counter
                .checked_add(1)
                .ok_or(ListError("Too many List changes occurred at once."))?
        } else {
            0
        };
        self.milliseconds = milliseconds;
        self.counter = counter;
        Ok(ListTimestamp(format!(
            "{milliseconds}-{counter:04X}-{}",
            self.node
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_matches_captured_monotonic_format_and_uuid_node() {
        let mut clock = ListClock {
            milliseconds: 1000,
            counter: 9,
            node: "89abcdef01234567".into(),
        };
        assert_eq!(
            clock.tick(1000).unwrap().as_str(),
            "1000-000A-89abcdef01234567"
        );
        assert_eq!(
            clock.tick(900).unwrap().as_str(),
            "1000-000B-89abcdef01234567"
        );
        assert_eq!(
            clock.tick(1001).unwrap().as_str(),
            "1001-0000-89abcdef01234567"
        );
        let clock = ListClock::new();
        assert_eq!(clock.node.len(), 16);
        assert!(
            clock
                .node
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
    }
    #[test]
    fn drift_overflow_and_invalid_date_do_not_mutate_clock() {
        let mut clock = ListClock {
            milliseconds: 100_000,
            counter: u16::MAX,
            node: "89abcdef01234567".into(),
        };
        assert!(clock.tick(39_999).is_err());
        assert!(clock.tick(40_000).is_err());
        assert_eq!(clock.counter, u16::MAX);
        assert_eq!(clock.milliseconds, 100_000);
        assert_eq!(
            clock.tick(100_001).unwrap().as_str(),
            "100001-0000-89abcdef01234567"
        );
        assert!(clock.tick(8_640_000_000_000_001).is_err());
        assert_eq!(clock.milliseconds, 100_001);
        clock.counter = 0;
        assert!(clock.tick(40_001).is_ok());
    }
}
