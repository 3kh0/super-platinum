//! Actual singleton envelope and channel-map order decoded from the same bytes.
use super::*;

#[derive(Clone)]
pub struct OrderedFileInfo {
    pub(super) order: ChannelMapOrder,
}
impl OrderedFileInfo {
    /// Only original response bytes establish insertion order. This carrier is
    /// not owner, requested-file, or publication authority.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let order = ChannelMapOrder::decode(bytes)?;
        let raw = order.carrier();
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || !raw["file"].is_object()
            || !raw["file"]["id"].as_str().is_some_and(valid_id)
        {
            return Err(malformed());
        }
        Ok(Self { order })
    }
    pub fn raw(&self) -> &Value {
        self.order.carrier()
    }
    pub fn file(&self) -> &Value {
        &self.raw()["file"]
    }
}

#[cfg(test)]
mod tests;
