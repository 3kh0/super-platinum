//! Closed Vote diagnostic modes; no static alias can enable this workflow.
use super::*;
impl FieldCreateMode {
    pub(crate) fn is_vote(self) -> bool {
        matches!(self, Self::VoteDefault | Self::VoteCustom)
    }
    pub(crate) fn vote_options(self) -> Value {
        match self {
            Self::VoteDefault => json!({"emoji":":sparkling_heart:"}),
            Self::VoteCustom => json!({"emoji":":fixture_vote:","emoji_team_id":"T1"}),
            _ => panic!("Not Vote"),
        }
    }
}
fn gate(flag: Option<&str>, exclusive: bool, alias: Option<&str>) -> Option<FieldCreateMode> {
    if flag != Some("1") || !exclusive {
        return None;
    }
    [FieldCreateMode::VoteDefault, FieldCreateMode::VoteCustom]
        .into_iter()
        .find(|m| {
            alias == Some(m.prefix()) || alias == Some(format!("{}-narrow", m.prefix()).as_str())
        })
}
pub(super) fn environment_gate(flag: Option<&str>) -> Option<FieldCreateMode> {
    let exclusive = [
        "TEXT",
        "NUMBER",
        "NUMBER_PRECISION",
        "RATING",
        "CHECKBOX",
        "DATE",
        "SELECT",
        "MULTI_SELECT",
        "PEOPLE",
        "LINK",
        "CHANNEL",
    ]
    .iter()
    .all(|part| {
        std::env::var_os(format!("SUPER_PLATINUM_LIST_{part}_FIELD_CREATE_CHECK")).is_none()
    });
    let exclusive = exclusive
        && std::env::vars_os().all(|(key, _)| {
            let key = key.to_string_lossy();
            !(key.starts_with("SUPER_PLATINUM_") && key.ends_with("_CHECK"))
                || key == "SUPER_PLATINUM_LIST_VOTE_FIELD_CREATE_CHECK"
        });
    gate(
        flag,
        exclusive,
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_alias_and_one_exclusive_flag() {
        for m in [FieldCreateMode::VoteDefault, FieldCreateMode::VoteCustom] {
            for a in [m.prefix().to_owned(), format!("{}-narrow", m.prefix())] {
                assert_eq!(gate(Some("1"), true, Some(&a)), Some(m));
                assert_eq!(gate(Some("1"), false, Some(&a)), None);
                for f in [None, Some(""), Some("0"), Some("true"), Some("2")] {
                    assert_eq!(gate(f, true, Some(&a)), None);
                }
            }
        }
        for a in [
            None,
            Some("slack-list-vote-field-create"),
            Some("slack-list-vote-create-default-narrow-extra"),
            Some("slack-list-rating-default-field-create"),
        ] {
            assert_eq!(gate(Some("1"), true, a), None);
        }
    }
    #[test]
    fn exact_vote_options_have_no_quantity() {
        assert_eq!(
            FieldCreateMode::VoteDefault.vote_options(),
            json!({"emoji":":sparkling_heart:"})
        );
        assert_eq!(
            FieldCreateMode::VoteCustom.vote_options(),
            json!({"emoji":":fixture_vote:","emoji_team_id":"T1"})
        );
    }
}

#[cfg(test)]
mod named_board_tests {
    use super::*;
    #[test]
    fn actual_fixture_addition_preserves_unrelated_board_and_refuses_ambiguous_identity() {
        for (mode, board) in [
            (FieldCreateMode::RatingDefault, "V_RATING_BOARD"),
            (FieldCreateMode::VoteDefault, "V_VOTE_BOARD"),
        ] {
            let before = json!({"list_metadata":{"schema":[{"id":"C_NAME","key":"NAME"}],"views":[{"id":"V_FILTER","columns":[{"id":"C_NAME"}]},{"id":"V_PRECISION_BOARD","columns":[{"id":"C_NAME"}],"opaque":{"untouched":true}},{"id":board,"columns":[{"id":"C_NAME"}]}]}});
            let mut after = before.clone();
            super::super::add_field(&mut after, mode).unwrap();
            assert_eq!(
                after["list_metadata"]["views"][1],
                before["list_metadata"]["views"][1]
            );
            let (id, key) = mode.observed();
            for i in [0, 2] {
                assert_eq!(
                    after["list_metadata"]["views"][i]["columns"]
                        .as_array_mut()
                        .unwrap()
                        .remove(1),
                    json!({"id":id,"key":key,"visible":true})
                );
            }
            after["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            assert_eq!(after, before);
            for duplicate in [false, true] {
                let mut bad = before.clone();
                let views = bad["list_metadata"]["views"].as_array_mut().unwrap();
                if duplicate {
                    views.push(views[2].clone());
                } else {
                    views.remove(2);
                }
                let unchanged = bad.clone();
                assert!(super::super::add_field(&mut bad, mode).is_err());
                assert_eq!(bad, unchanged);
            }
        }
    }
}
