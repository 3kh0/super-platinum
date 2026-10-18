//! Ordinary Date authoring: captured uPpd.UE, ListsDateFilter and LsSA.
//! Relative tokens stay relative on the wire; calendar interpretation belongs to
//! saved_views::dates. Timestamp/personal/CreatedTime filter authoring is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFilterOperator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    IsEmpty,
    IsNotEmpty,
}
impl DateFilterOperator {
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
    pub fn valid_value(self, value: Option<&str>) -> bool {
        if !self.requires_value() {
            return value.is_none();
        }
        value.is_some_and(|v| Self::is_preset(v) || Self::is_calendar(v))
    }
    pub fn is_preset(value: &str) -> bool {
        matches!(
            value,
            "today" | "yesterday" | "tomorrow" | "this_week" | "next_week" | "last_week"
        )
    }
    /// Local closed HTML/calendar subset: years 0001..9999, exact Gregorian ISO.
    /// No trimming, locale parsing, UTC conversion, or guessed epoch inputs.
    pub fn is_calendar(value: &str) -> bool {
        value.len() == 10
            && value.as_bytes()[4] == b'-'
            && value.as_bytes()[7] == b'-'
            && value
                .bytes()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
            && !value.starts_with("0000")
            && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .is_ok_and(|v| v.format("%Y-%m-%d").to_string() == value)
    }
}
#[cfg(test)]
mod tests;
