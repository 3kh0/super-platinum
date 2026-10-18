use super::{Error, Result};
const MAX_SAFE_TIME: u64 = 9_007_199_254_740_991 - 60_000;
#[derive(Clone, Copy, Debug)]
pub struct Clock(u64);
impl Clock {
    pub fn new(epoch_millis: u64) -> Result<Self> {
        if epoch_millis > MAX_SAFE_TIME {
            return Err(Error::Shape);
        }
        Ok(Self(epoch_millis))
    }
    pub fn millis(self) -> u64 {
        self.0
    }
    pub fn seconds(self) -> u64 {
        self.0 / 1000
    }
}
#[derive(Clone, Copy, Debug)]
pub enum RowKind {
    Existing,
    NonExistent,
    Absent,
}
#[derive(Clone, Debug)]
pub struct Fresh {
    pub id: String,
    pub kind: RowKind,
    pub entropy: f64,
}
impl Fresh {
    /// Actual HyoU floating random, p3xJ ceil, then Tst9 parseInt+finite guard.
    /// Existing canonical rows (including isUnknown) use Infinity without RNG.
    pub fn age_seconds(&self) -> Result<Option<u64>> {
        if matches!(self.kind, RowKind::Existing) {
            return Ok(None);
        }
        let random = self.entropy;
        if !random.is_finite() || !(0.0..1.0).contains(&random) {
            return Err(Error::Shape);
        }
        // JS Number::toString uses fixed decimals down to 1e-6, scientific below.
        // Both formats retain the shortest round-trippable significant digits.
        let text = if random == 0.0 {
            "0".into()
        } else if random < 0.000001 {
            format!("{random:e}")
        } else {
            random.to_string()
        };
        let epsilon = format!("1e-{}", text.len() - 1)
            .parse::<f64>()
            .map_err(|_| Error::Shape)?;
        let jitter = (-0.2 + random * (0.4 + epsilon)).min(0.2);
        let base = if matches!(self.kind, RowKind::NonExistent) {
            1800.0
        } else {
            1.0
        };
        Ok(Some((base + base * jitter).ceil() as u64))
    }
    pub(super) fn deadline(&self, now: Clock) -> Result<(String, Option<u64>)> {
        valid_id(&self.id)?;
        Ok((
            self.id.clone(),
            self.age_seconds()?.map(|age| now.seconds() + age),
        ))
    }
}
pub(super) fn valid_id(id: &str) -> Result<()> {
    if id.len() < 9
        || !matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
        || !id
            .bytes()
            .skip(1)
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(Error::Shape);
    }
    Ok(())
}
/// A nonzero identity supplied by the actual timer owner, not invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerId(u64);
impl TimerId {
    pub fn new(value: u64) -> Result<Self> {
        if value == 0 {
            return Err(Error::Shape);
        }
        Ok(Self(value))
    }
    pub fn value(self) -> u64 {
        self.0
    }
}
/// Exact static source pause handle/effects. Share across CFM registries.
/// Native owns actual two-second timer allocation, cancellation and callback
/// delivery. A returned cancellation is performed BEFORE installing the new
/// timer; cancellation must suppress that timer's later callback. The source's
/// indefinite sentinel1 can name a real host timer, so it is not hidden here.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pause {
    handle: Option<TimerId>,
}
impl Pause {
    pub const DELAY_MS: u64 = 2000;
    /// Read the cancellation before allocating the replacement timer. Native
    /// cancels this handle, allocates/installs the two-second callback, then calls
    /// keypress with its actual ID, all in one synchronous event-loop turn.
    pub fn cancellation_handle(&self) -> Option<TimerId> {
        self.handle
    }
    /// Finish that replacement. Returns the same old handle for verification;
    /// it must not be cancelled a second time after allocating a reused ID.
    pub fn keypress(&mut self, timer: TimerId) -> Option<TimerId> {
        self.handle.replace(timer)
    }
    pub fn indefinitely(&mut self) -> Option<TimerId> {
        self.handle.replace(TimerId(1))
    }
    /// Does not cancel the timer. Its eventual callback can clear a newer pause.
    pub fn unpause(&mut self) {
        self.handle = None;
    }
    /// Deliver only an actual, non-cancelled timer callback. Source callbacks
    /// clear the shared handle unconditionally, including a newer pause.
    pub fn timer_fired(&mut self) {
        self.handle = None;
    }
    pub fn paused(&self) -> bool {
        self.handle.is_some()
    }
}
