//! Constructed owner/capacity fixture; no server-normalization claim or writes.
use super::*;
pub(crate) mod channel;
pub(crate) mod checkbox;
pub(crate) mod date;
pub(crate) mod link;
pub(crate) mod number_precision;
pub(crate) mod people;
pub(crate) mod rating;
pub(crate) mod select;
pub(crate) mod vote;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-people-defaults-single-create"
            | "slack-list-people-defaults-single-create-narrow"
            | "slack-list-people-defaults-multiple-create"
            | "slack-list-people-defaults-multiple-create-narrow"
            | "slack-list-people-defaults-null-create"
            | "slack-list-people-defaults-null-create-narrow"
            | "slack-list-people-defaults-omitted-create"
            | "slack-list-people-defaults-omitted-create-narrow"
            | "slack-list-channel-defaults-single-create"
            | "slack-list-channel-defaults-single-create-narrow"
            | "slack-list-channel-defaults-multiple-create"
            | "slack-list-channel-defaults-multiple-create-narrow"
            | "slack-list-channel-defaults-null-create"
            | "slack-list-channel-defaults-null-create-narrow"
            | "slack-list-channel-defaults-omitted-create"
            | "slack-list-channel-defaults-omitted-create-narrow"
            | "slack-list-vote-create-default"
            | "slack-list-vote-create-default-narrow"
            | "slack-list-vote-create-custom"
            | "slack-list-vote-create-custom-narrow"
            | "slack-list-text-field-create"
            | "slack-list-text-field-create-narrow"
            | "slack-list-number-field-create"
            | "slack-list-number-field-create-narrow"
            | "slack-list-number-precision-field-create"
            | "slack-list-number-precision-field-create-narrow"
            | "slack-list-checkbox-field-create"
            | "slack-list-checkbox-field-create-narrow"
            | "slack-list-date-field-create"
            | "slack-list-date-field-create-narrow"
            | "slack-list-select-field-create"
            | "slack-list-select-field-create-narrow"
            | "slack-list-multi-select-field-create"
            | "slack-list-multi-select-field-create-narrow"
            | "slack-list-people-field-create"
            | "slack-list-people-field-create-narrow"
            | "slack-list-link-field-create"
            | "slack-list-link-field-create-narrow"
            | "slack-list-channel-field-create"
            | "slack-list-channel-field-create-narrow"
            | "slack-list-rating-field-create"
            | "slack-list-rating-field-create-narrow"
            | "slack-list-rating-default-field-create"
            | "slack-list-rating-default-field-create-narrow"
            | "slack-list-rating-standard-field-create"
            | "slack-list-rating-standard-field-create-narrow"
            | "slack-list-rating-custom-field-create"
            | "slack-list-rating-custom-field-create-narrow"
            | "slack-list-vote-field-create"
            | "slack-list-vote-field-create-narrow"
    ));
    let entity_alias = name.starts_with("slack-list-people-defaults-")
        || name.starts_with("slack-list-channel-defaults-");
    assert!(
        !entity_alias
            || super::super::rename::fixture::field_create_mode()
                .is_some_and(super::super::rename::fixture::FieldCreateMode::is_entity),
        "Configured entity fixture requires exact opt-in"
    );
    let server = seed(shell, name);
    if super::super::rename::fixture::field_create_mode()
        == Some(super::super::rename::fixture::FieldCreateMode::NumberPrecision)
    {
        super::set_locale(shell, Some("en-US"));
    }
    if super::super::rename::fixture::field_create_check_enabled() {
        if let Some(super::super::rename::fixture::FieldCreateMode::Entity(case)) =
            super::super::rename::fixture::field_create_mode()
        {
            if case.channel() {
                channel::cache::seed(shell).expect("Owned entity Channel seed");
            } else {
                people::cache::seed(shell).expect("Owned entity People seed");
            }
        }
        if super::super::rename::fixture::field_create_mode()
            == Some(super::super::rename::fixture::FieldCreateMode::People)
        {
            people::cache::seed(shell).expect("Owned People cache seed");
        }
        if super::super::rename::fixture::field_create_mode()
            == Some(super::super::rename::fixture::FieldCreateMode::Channel)
        {
            channel::cache::seed(shell).expect("Owned Channel cache seed");
        }
        if super::super::rename::fixture::field_create_mode()
            .is_some_and(super::super::rename::fixture::FieldCreateMode::is_rating)
        {
            rating::media::seed(shell).expect("Owned Rating media seed");
        }
        if super::super::rename::fixture::field_create_mode()
            .is_some_and(super::super::rename::fixture::FieldCreateMode::is_vote)
        {
            vote::media::seed(shell).expect("Owned Vote catalog seed");
        }
        super::super::rename::fixture::seed_text_create(shell, server);
        return;
    }
    let visit = generation(shell).unwrap();
    let selection = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::text_create::open_shell(
        shell, visit, selection
    ));
    let serial = super::super::rename::presentation(shell).unwrap().serial;
    if (name.starts_with("slack-list-number-field-create")
        || name.starts_with("slack-list-number-precision-field-create"))
        || name.starts_with("slack-list-checkbox-field-create")
        || name.starts_with("slack-list-date-field-create")
        || name.starts_with("slack-list-people-field-create")
        || name.starts_with("slack-list-link-field-create")
        || name.starts_with("slack-list-channel-field-create")
        || name.starts_with("slack-list-rating-field-create")
        || name.starts_with("slack-list-vote-field-create")
        || (name.starts_with("slack-list-select-field-create")
            || name.starts_with("slack-list-multi-select-field-create"))
    {
        let choice = super::super::rename::text_create::choice(shell).unwrap();
        assert!(super::super::rename::text_create::kind_shell(
            shell,
            serial,
            visit,
            selection,
            choice,
            if name.starts_with("slack-list-select-field-create")
                || name.starts_with("slack-list-multi-select-field-create")
            {
                "select"
            } else if name.starts_with("slack-list-checkbox-field-create") {
                "checkbox"
            } else if name.starts_with("slack-list-vote-field-create") {
                "vote"
            } else if name.starts_with("slack-list-rating-field-create") {
                "rating"
            } else if name.starts_with("slack-list-channel-field-create") {
                "channel"
            } else if name.starts_with("slack-list-link-field-create") {
                "link"
            } else if name.starts_with("slack-list-people-field-create") {
                "user"
            } else if name.starts_with("slack-list-date-field-create") {
                "date"
            } else {
                "number"
            }
        ));
    }
    if name.starts_with("slack-list-number-precision-field-create") {
        let choice = super::super::rename::text_create::choice(shell).unwrap();
        assert!(super::super::rename::text_create::number::choose_shell(
            shell, serial, visit, selection, choice, "3"
        ));
    }
    let choice = super::super::rename::text_create::choice(shell).unwrap();
    assert!(super::super::rename::text_create::input_shell(
        shell,
        serial,
        visit,
        selection,
        choice,
        if name.starts_with("slack-list-select-field-create")
            || name.starts_with("slack-list-multi-select-field-create")
        {
            "Release phase".into()
        } else if (name.starts_with("slack-list-number-field-create")
            || name.starts_with("slack-list-number-precision-field-create"))
        {
            "Release count".into()
        } else if name.starts_with("slack-list-checkbox-field-create") {
            "Verified".into()
        } else if name.starts_with("slack-list-vote-field-create") {
            "Support".into()
        } else if name.starts_with("slack-list-rating-field-create") {
            "Quality".into()
        } else if name.starts_with("slack-list-channel-field-create") {
            "Channel".into()
        } else if name.starts_with("slack-list-link-field-create") {
            "Reference".into()
        } else if name.starts_with("slack-list-people-field-create") {
            "Reviewer".into()
        } else if name.starts_with("slack-list-date-field-create") {
            "Target date".into()
        } else {
            "Release notes".into()
        }
    ));
    if name.starts_with("slack-list-select-field-create")
        || name.starts_with("slack-list-multi-select-field-create")
    {
        use super::super::rename::text_create::select::{Edit, edit_shell};
        for label in [
            "Ready",
            "In progress",
            "Review",
            "Blocked",
            "Waiting",
            "Approved",
            "Scheduled",
            "Équipe 東京",
        ] {
            let epoch = super::super::rename::presentation(shell)
                .unwrap()
                .select_draft
                .unwrap()
                .add_epoch;
            assert!(edit_shell(
                shell,
                serial,
                visit,
                selection,
                choice,
                Edit::Pending {
                    epoch,
                    value: label.into()
                }
            ));
            assert!(edit_shell(
                shell,
                serial,
                visit,
                selection,
                choice,
                Edit::Add { epoch }
            ));
        }
    }
    if name.starts_with("slack-list-multi-select-field-create") {
        assert!(super::super::rename::text_create::select::edit_shell(
            shell,
            serial,
            visit,
            selection,
            choice,
            super::super::rename::text_create::select::Edit::Format(
                super_platinum_core::slack_lists::SelectCreateFormat::Multiple
            ),
        ));
    }
}
pub(super) fn seed(shell: &mut ShellState, name: &str) -> Value {
    super::view_filter::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Shared launch checklist");
    f.document["access"] = json!("owner");
    f.document["list_limits"] = json!({"over_column_maximum":false});
    f.document["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"done","operator":"is_not_empty","values":[]}]);
    if super::super::rename::fixture::field_create_mode()
        .is_some_and(super::super::rename::fixture::FieldCreateMode::is_rating)
    {
        // Constructed saved Board exists before the baseline. No canonical placement claim.
        let mut board = f.document["list_metadata"]["views"][0].clone();
        board["id"] = json!("V_RATING_BOARD");
        board["name"] = json!("Rating board");
        board["type"] = json!("board");
        board["grouping"] = json!({"group_by":"status"});
        f.document["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .push(board);
    }
    if super::super::rename::fixture::field_create_mode()
        .is_some_and(super::super::rename::fixture::FieldCreateMode::is_vote)
    {
        let mut board = f.document["list_metadata"]["views"][0].clone();
        board["id"] = json!("V_VOTE_BOARD");
        board["name"] = json!("Vote board");
        board["type"] = json!("board");
        board["grouping"] = json!({"group_by":"status"});
        f.document["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .push(board);
    }
    let server = f.document.clone();
    assert!(super::super::open_shell(
        shell,
        serde_json::from_value(server.clone()).unwrap()
    ));
    refresh(shell);
    server
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_aliases_preserve_population_and_only_open_review() {
        for name in [
            "slack-list-text-field-create",
            "slack-list-text-field-create-narrow",
            "slack-list-number-field-create",
            "slack-list-number-field-create-narrow",
            "slack-list-checkbox-field-create",
            "slack-list-checkbox-field-create-narrow",
            "slack-list-date-field-create",
            "slack-list-date-field-create-narrow",
        ] {
            let mut shell =
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(shell.core.session.is_none() && shell.core.transport.is_none());
            let f = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.document["list_metadata"]["views"][0]["sorts"],
                json!([{"key":"estimate","ascending":true}])
            );
            assert_eq!(
                f.document["list_metadata"]["views"][0]["filters"],
                json!([{"key":"done","operator":"is_not_empty","values":[]}])
            );
            assert_eq!(f.attempts, 0);
            let p = super::super::super::rename::presentation(&shell).unwrap();
            assert!(p.text_creation && p.can_confirm && p.observed_field.is_none());
            assert_eq!(
                p.name,
                if (name.starts_with("slack-list-number-field-create")
                    || name.starts_with("slack-list-number-precision-field-create"))
                {
                    "Release count"
                } else if name.starts_with("slack-list-checkbox-field-create") {
                    "Verified"
                } else if name.starts_with("slack-list-date-field-create") {
                    "Target date"
                } else {
                    "Release notes"
                }
            );
            let choice = p.field_choice.unwrap();
            assert!(choice.revision > 0);
            assert_eq!(
                choice.kind.as_str(),
                if (name.starts_with("slack-list-number-field-create")
                    || name.starts_with("slack-list-number-precision-field-create"))
                {
                    "number"
                } else if name.starts_with("slack-list-checkbox-field-create") {
                    "checkbox"
                } else if name.starts_with("slack-list-date-field-create") {
                    "date"
                } else {
                    "text"
                }
            );
            super::super::super::rename::precision::discard_fixture(&mut shell);
        }
    }
}
