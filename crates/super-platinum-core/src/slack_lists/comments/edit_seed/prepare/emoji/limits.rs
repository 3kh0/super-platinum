//! Local bounded serialized copies; no source content is shortened.
use super::*;
use serde::Serialize;
use std::io::{self, Write};
const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Default)]
pub(super) struct Budget(usize);
impl Budget {
    pub fn replacement(
        &mut self,
        name: &str,
        tone: &Option<String>,
        attrs: &serde_json::Map<String, Value>,
    ) -> Result<()> {
        let cost = count(&(name, tone, attrs))?
            .checked_add(64)
            .ok_or(SeedUnavailable::Limit)?;
        self.0 = self
            .0
            .checked_add(cost)
            .filter(|n| *n <= LIMIT)
            .ok_or(SeedUnavailable::Limit)?;
        Ok(())
    }
}
pub(super) fn check<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    count(value).map(|_| ())
}
pub(super) fn joined(left: &str, right: &str) -> Result<()> {
    count(left)?
        .checked_add(count(right)?)
        .and_then(|n| n.checked_add(16))
        .filter(|n| *n <= LIMIT)
        .ok_or(SeedUnavailable::Limit)?;
    Ok(())
}
fn count<T: Serialize + ?Sized>(value: &T) -> Result<usize> {
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).map_err(|_| SeedUnavailable::Limit)?;
    Ok(count.0)
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= LIMIT)
            .ok_or_else(|| io::Error::other("emoji copy budget"))?;
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
    fn streaming_copy_limit_accepts_boundary_and_refuses_overflow_without_large_allocations() {
        let mut count = Count(LIMIT - 2);
        assert_eq!(count.write(&[0, 1]).unwrap(), 2);
        assert!(count.write(&[2]).is_err());
        let mut count = Count(usize::MAX);
        assert!(count.write(&[0]).is_err());
    }

    #[test]
    fn repeated_replacement_cost_is_checked_before_materializing_copies() {
        let attrs = serde_json::Map::new();
        let cost = count(&("smile", &None::<String>, &attrs)).unwrap() + 64;
        let mut budget = Budget(LIMIT - cost);
        budget.replacement("smile", &None, &attrs).unwrap();
        assert!(budget.replacement("smile", &None, &attrs).is_err());
        let mut budget = Budget(usize::MAX);
        assert!(budget.replacement("smile", &None, &attrs).is_err());
    }
}
