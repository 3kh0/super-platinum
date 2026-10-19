//! Same-byte response plus opaque occurrence-indexed channel-map order.
use super::super::file_title_source::ChannelMapOrder;
use super::*;
#[derive(Clone)]
pub struct OrderedResponse {
    order: ChannelMapOrder,
}
impl OrderedResponse {
    /// Decode original response bytes, never a serialization of parsed Value.
    /// This is not account/query/source-publication authority.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let order = ChannelMapOrder::decode(bytes)?;
        response(order.carrier())?;
        Ok(Self { order })
    }
    pub fn raw(&self) -> &Value {
        self.order.carrier()
    }
    pub fn rows(&self) -> &[Value] {
        self.raw()["files"].as_array().expect("decoded files array")
    }
    pub fn channel_map_order(&self) -> &ChannelMapOrder {
        &self.order
    }
}
