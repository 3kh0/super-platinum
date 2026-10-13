//! Validated recipient sets for Slack's conversations.open endpoint.
use crate::slack::models::UserId;

pub const MAX_RECIPIENTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipients(Vec<UserId>);
impl Recipients {
    pub fn new(users: &[UserId], caller: &str) -> Result<Self, &'static str> {
        let mut unique = Vec::new();
        for user in users {
            if user == caller {
                continue;
            }
            if user.is_empty() || !user.bytes().all(|c| c.is_ascii_alphanumeric()) {
                return Err("Choose a valid person from the results.");
            }
            if !unique.contains(user) {
                unique.push(user.clone());
            }
        }
        if unique.is_empty() {
            return Err("Choose at least one other person.");
        }
        if unique.len() > MAX_RECIPIENTS {
            return Err("Choose up to eight people.");
        }
        Ok(Self(unique))
    }
    pub fn ids(&self) -> &[UserId] {
        &self.0
    }
    pub fn parameter(&self) -> String {
        self.0.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deduplicates_in_selection_order_and_omits_caller() {
        let r = Recipients::new(
            &["U2".into(), "SELF".into(), "U1".into(), "U2".into()],
            "SELF",
        )
        .unwrap();
        assert_eq!(r.parameter(), "U2,U1");
    }
    #[test]
    fn validates_nonempty_eight_person_limit_and_ids() {
        assert!(Recipients::new(&[], "SELF").is_err());
        assert!(Recipients::new(&["SELF".into()], "SELF").is_err());
        for id in ["", "U1,U2", "U1 U2"] {
            assert!(Recipients::new(&[id.into()], "SELF").is_err());
        }
        let users = (1..=8).map(|i| format!("U{i}")).collect::<Vec<_>>();
        assert!(Recipients::new(&users, "SELF").is_ok());
        let mut nine = users;
        nine.push("U9".into());
        assert!(Recipients::new(&nine, "SELF").is_err());
    }
}
