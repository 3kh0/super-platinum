//! Local resource boundary for per-UTF-16-unit attribute copies. This is a
//! serialized-size cost model, not a Slack protocol or exact heap-size limit.
use super::*;
use std::io::{self, Write};
const ATTRIBUTE_COST_LIMIT: usize = 8 * 1024 * 1024;

pub(super) fn reserve(total: &mut usize, value: Option<&Value>, units: usize) -> Result<()> {
    let attributes = match value {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Object(attributes)) if attributes.is_empty() => return Ok(()),
        Some(Value::Object(attributes)) => attributes,
        _ => return Err(SeedUnavailable::MalformedMessage),
    };
    // Count serialization without allocating another buffer; the writer stops
    // when even one copy would exceed the local aggregate allowance.
    let mut count = Count(0);
    serde_json::to_writer(&mut count, attributes).map_err(|_| SeedUnavailable::Limit)?;
    *total = charge(*total, count.0, units)?;
    Ok(())
}
fn charge(total: usize, bytes: usize, units: usize) -> Result<usize> {
    bytes
        .checked_mul(units)
        .and_then(|cost| total.checked_add(cost))
        .filter(|cost| *cost <= ATTRIBUTE_COST_LIMIT)
        .ok_or(SeedUnavailable::Limit)
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|cost| *cost <= ATTRIBUTE_COST_LIMIT)
            .ok_or_else(|| io::Error::other("attribute copy cost limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests;
