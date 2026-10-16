/// Operators actually offered by Slack's ordinary Number filter menu.
/// The broader read evaluator's <= and >= do not authorize their creation here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberFilterOperator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    IsEmpty,
    IsNotEmpty,
}
impl NumberFilterOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::NotEqual => "not_equal",
            Self::LessThan => "less_than",
            Self::GreaterThan => "greater_than",
            Self::IsEmpty => "is_empty",
            Self::IsNotEmpty => "is_not_empty",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "equal" => Self::Equal,
            "not_equal" => Self::NotEqual,
            "less_than" => Self::LessThan,
            "greater_than" => Self::GreaterThan,
            "is_empty" => Self::IsEmpty,
            "is_not_empty" => Self::IsNotEmpty,
            _ => return None,
        })
    }
    pub fn requires_value(self) -> bool {
        !matches!(self, Self::IsEmpty | Self::IsNotEmpty)
    }
    /// Local bounded decimal/exponent subset, not a claimed Slack input limit.
    /// Validation never normalizes the captured string; empty operators take None.
    pub fn valid_value(self, value: Option<&str>) -> bool {
        if !self.requires_value() {
            return value.is_none();
        }
        value.is_some_and(valid_number)
    }
}
fn valid_number(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    let bytes = value.as_bytes();
    let mut i = usize::from(bytes[0] == b'-');
    let integer = i;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let has_integer = i > integer;
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let fraction = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == fraction {
            return false;
        }
    } else if !has_integer {
        return false;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exponent = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == exponent {
            return false;
        }
    }
    i == bytes.len() && value.parse::<f64>().is_ok_and(f64::is_finite)
}
