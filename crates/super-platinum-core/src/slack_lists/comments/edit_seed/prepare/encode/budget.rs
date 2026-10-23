//! Local serialized-copy cost limits, not a byte-exact allocator model.
use super::*;
use serde::Serialize;
use std::io::{self, Write};
pub(super) const LIMIT: usize = 8 * 1024 * 1024;
pub(super) struct Budget {
    used: usize,
    limit: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            used: 0,
            limit: LIMIT,
        }
    }
}
impl Budget {
    pub fn reserve<T: Serialize + ?Sized>(&mut self, value: &T, copies: usize) -> Result<()> {
        self.reserve_cost(cost(value)?, copies)
    }
    fn reserve_cost(&mut self, cost: usize, copies: usize) -> Result<()> {
        let next = cost
            .checked_mul(copies)
            .and_then(|n| self.used.checked_add(n))
            .filter(|n| *n <= self.limit)
            .ok_or(SeedUnavailable::Limit)?;
        self.used = next;
        Ok(())
    }
}
pub(super) fn check<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    if cost(value)? > LIMIT {
        Err(SeedUnavailable::Limit)
    } else {
        Ok(())
    }
}
fn cost<T: Serialize + ?Sized>(value: &T) -> Result<usize> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(|_| SeedUnavailable::Limit)?;
    Ok(counter.0)
}
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= LIMIT)
            .ok_or_else(|| io::Error::other("encoding cost exceeds local bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_cost_boundary_and_overflow_do_not_allocate_expansions() {
        let mut budget = Budget {
            used: 0,
            limit: 128,
        };
        budget.reserve_cost(63, 2).unwrap();
        assert_eq!(budget.used, 126);
        assert!(budget.reserve_cost(3, 1).is_err());
        assert_eq!(budget.used, 126);
        budget.reserve_cost(1, 2).unwrap();
        assert_eq!(budget.used, 128);
        assert!(budget.reserve_cost(1, 1).is_err());
        assert!(Budget::default().reserve_cost(usize::MAX, 2).is_err());
        assert!(
            Budget {
                used: usize::MAX,
                limit: usize::MAX
            }
            .reserve_cost(1, 1)
            .is_err()
        );
        assert_eq!(cost(&"\n\u{1f600}").unwrap(), 8);
    }
    #[test]
    fn repeated_observation_clone_is_refused_before_budget_advances() {
        let mut observed = EncodingObservations::default();
        observed.emoji.insert(
            ":x:".into(),
            Some(EmojiEncoding {
                name: Some("x".repeat(48)),
                unicode: Some("1f604".into()),
            }),
        );
        let mut context = Context {
            observed: &observed,
            need: None,
            budget: Budget { used: 0, limit: 80 },
        };
        context.emoji(":x:").unwrap();
        let before = context.budget.used;
        assert_eq!(context.emoji(":x:").unwrap_err(), SeedUnavailable::Limit);
        assert_eq!(context.budget.used, before);
        assert!(context.need.is_none());
    }
    #[test]
    fn repeated_mention_expansion_and_header_names_share_the_checked_budget() {
        let mut observed = EncodingObservations::default();
        observed
            .unicode_names
            .insert("1f604".into(), Some("x".repeat(48)));
        let mut context = Context {
            observed: &observed,
            need: None,
            budget: Budget { used: 0, limit: 80 },
        };
        context.unicode_name("1f604").unwrap();
        let before = context.budget.used;
        assert_eq!(
            context.unicode_name("1f604").unwrap_err(),
            SeedUnavailable::Limit
        );
        assert_eq!(context.budget.used, before);
        let op = json!({"insert":"xxxxxx","attributes":{"slackmention":{"id":"U1","label":"x"}}});
        let mut context = Context {
            observed: &observed,
            need: None,
            budget: Budget { used: 0, limit: 80 },
        };
        assert_eq!(
            chunks::semantic::mention(&op, false, &mut context).unwrap_err(),
            SeedUnavailable::Limit
        );
        assert_eq!(context.budget.used, 0);
    }
}
