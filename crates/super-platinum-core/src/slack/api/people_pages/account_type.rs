//! Common source options only. Enterprise roles, Guests and Frontline require
//! additional state/assignment observations and are deliberately absent.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccountType {
    Any,
    RegularMembers,
    Deactivated,
}
impl AccountType {
    pub fn value(self) -> &'static str {
        match self {
            Self::Any => "0,1,2,3,4,5,6,7,8,9,10",
            Self::RegularMembers => "0,1,2,3,7",
            Self::Deactivated => "6",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "0,1,2,3,4,5,6,7,8,9,10" => Some(Self::Any),
            "0,1,2,3,7" => Some(Self::RegularMembers),
            "6" => Some(Self::Deactivated),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any account type",
            Self::RegularMembers => "Regular members",
            Self::Deactivated => "Deactivated",
        }
    }
    pub(super) fn wire(self) -> Option<&'static str> {
        match self {
            Self::Any => None,
            _ => Some(self.value()),
        }
    }
}
#[cfg(test)]
mod tests;
