//! Complete ordinary Number creation configuration from the captured Decimals menu.
use super::{ListError, Value, json};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NumberCreatePrecision(u8);
impl NumberCreatePrecision {
    pub fn capture(value: u8) -> Result<Self, ListError> {
        if value <= 5 {
            Ok(Self(value))
        } else {
            Err(ListError("Choose decimals from 0 to 5."))
        }
    }
    pub fn value(self) -> u8 {
        self.0
    }
    pub fn wire(self) -> Value {
        json!({"precision":self.0})
    }
    pub fn from_wire(value: &Value) -> Result<Self, ListError> {
        let precision = value
            .get("precision")
            .and_then(Value::as_u64)
            .filter(|p| *p <= 5)
            .ok_or(ListError("Unsupported Number creation options."))?;
        let precision = Self(precision as u8);
        if value != &precision.wire() {
            return Err(ListError("Unsupported Number creation options."));
        }
        Ok(precision)
    }
}
