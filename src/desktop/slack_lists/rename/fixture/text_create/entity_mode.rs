//! Exact synthetic configured entity cases; never a server normalization oracle.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntityCase {
    PeopleSingle,
    PeopleMultiple,
    PeopleNull,
    PeopleOmitted,
    ChannelSingle,
    ChannelMultiple,
    ChannelNull,
    ChannelOmitted,
}
impl EntityCase {
    pub(crate) const ALL: [Self; 8] = [
        Self::PeopleSingle,
        Self::PeopleMultiple,
        Self::PeopleNull,
        Self::PeopleOmitted,
        Self::ChannelSingle,
        Self::ChannelMultiple,
        Self::ChannelNull,
        Self::ChannelOmitted,
    ];
    pub(crate) fn channel(self) -> bool {
        matches!(
            self,
            Self::ChannelSingle | Self::ChannelMultiple | Self::ChannelNull | Self::ChannelOmitted
        )
    }
    pub(crate) fn multiple(self) -> bool {
        matches!(
            self,
            Self::PeopleMultiple
                | Self::PeopleOmitted
                | Self::ChannelMultiple
                | Self::ChannelOmitted
        )
    }
    pub(crate) fn empty(self) -> Option<bool> {
        match self {
            Self::PeopleNull | Self::ChannelNull => Some(false),
            Self::PeopleOmitted | Self::ChannelOmitted => Some(true),
            _ => None,
        }
    }
    pub(crate) fn ids(self) -> [&'static str; 2] {
        if self.channel() {
            ["G00000002", "C00000001"]
        } else {
            ["U00000002", "U00000001"]
        }
    }
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::PeopleSingle => "slack-list-people-defaults-single-create",
            Self::PeopleMultiple => "slack-list-people-defaults-multiple-create",
            Self::PeopleNull => "slack-list-people-defaults-null-create",
            Self::PeopleOmitted => "slack-list-people-defaults-omitted-create",
            Self::ChannelSingle => "slack-list-channel-defaults-single-create",
            Self::ChannelMultiple => "slack-list-channel-defaults-multiple-create",
            Self::ChannelNull => "slack-list-channel-defaults-null-create",
            Self::ChannelOmitted => "slack-list-channel-defaults-omitted-create",
        }
    }
    pub(crate) fn kind(self) -> ColumnCreateKind {
        if self.channel() {
            ColumnCreateKind::ChannelSingleDefault
        } else {
            ColumnCreateKind::PeopleSingleDefault
        }
    }
    pub(crate) fn options(self) -> Value {
        let mut v = json!({"format":if self.multiple(){"multi_entity"}else{"single_entity"},"show_member_name":self.channel()});
        match self.empty() {
            Some(true) => {}
            Some(false) => v["default_value"] = Value::Null,
            None => {
                let ids = self.ids();
                v["default_value"] = json!(ids.join("|"));
                v["default_value_typed"] = if self.channel() {
                    json!({"channel":ids})
                } else {
                    json!({"user":ids})
                };
            }
        }
        v
    }
}
pub(crate) fn gate(
    flag: Option<&str>,
    exclusive: bool,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    if flag != Some("1") || !exclusive {
        return None;
    }
    EntityCase::ALL
        .into_iter()
        .find(|case| {
            alias == Some(case.prefix())
                || alias == Some(format!("{}-narrow", case.prefix()).as_str())
        })
        .map(FieldCreateMode::Entity)
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack_lists::{EntityCreateDomain, EntityCreateOptions};
    #[test]
    fn eight_cases_have_exact_isolated_aliases_and_closed_options() {
        for case in EntityCase::ALL {
            for alias in [
                case.prefix().to_owned(),
                format!("{}-narrow", case.prefix()),
            ] {
                assert_eq!(
                    gate(Some("1"), true, Some(&alias)),
                    Some(FieldCreateMode::Entity(case))
                );
                assert!(gate(Some("1"), false, Some(&alias)).is_none());
                for flag in [None, Some(""), Some("true"), Some("0")] {
                    assert!(gate(flag, true, Some(&alias)).is_none());
                }
            }
            assert!(gate(Some("1"), true, Some(&format!("{}-extra", case.prefix()))).is_none());
            let domain = if case.channel() {
                EntityCreateDomain::Channel
            } else {
                EntityCreateDomain::People
            };
            let typed = EntityCreateOptions::from_wire(domain, &case.options()).unwrap();
            assert_eq!(typed.wire(), case.options());
            assert_eq!(
                typed.selected_ids().len(),
                if case.empty().is_some() { 0 } else { 2 }
            );
        }
    }
    #[test]
    fn empty_defaults_are_distinct_and_single_keeps_two_ids() {
        for (single, multi, null, omitted) in [
            (
                EntityCase::PeopleSingle,
                EntityCase::PeopleMultiple,
                EntityCase::PeopleNull,
                EntityCase::PeopleOmitted,
            ),
            (
                EntityCase::ChannelSingle,
                EntityCase::ChannelMultiple,
                EntityCase::ChannelNull,
                EntityCase::ChannelOmitted,
            ),
        ] {
            let mut a = single.options();
            a["format"] = json!("multi_entity");
            assert_eq!(a, multi.options());
            assert_eq!(null.options().get("default_value"), Some(&Value::Null));
            assert!(omitted.options().get("default_value").is_none());
            assert!(null.options().get("default_value_typed").is_none());
            assert!(omitted.options().get("default_value_typed").is_none());
        }
    }
}
