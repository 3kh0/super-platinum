//! Explicit caller-owned application presentation, not an observed Slack account
//! locale. Retained laT0 uses canvas/Untitled; Fbwy returns the source literal
//! for en-US before consulting translation bundles. Other consumers must supply
//! their own observation. No shared/global default or locale catalog is created.
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(crate) struct CanvasTitle;
impl CanvasTitle {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) const fn value(self) -> &'static str {
        "Untitled"
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"canvas","source":"Untitled",
            "value":self.value(),"origin":"application-presentation"})
    }
    pub(crate) fn proof(self, current: bool) -> Value {
        json!({"current":current,"descriptor":self.descriptor()})
    }
}
