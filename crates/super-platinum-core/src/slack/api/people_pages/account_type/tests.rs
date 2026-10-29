use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        SlackClient,
        api::people_pages::{self, Input, Sort},
        client::RequestBody,
    },
};
use uuid::Uuid;
#[test]
fn account_type_common_choices_keep_default_shape_omit_any_and_preserve_real_hide_and_team() {
    let client = SlackClient::default();
    let session = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    for hide in [false, true] {
        let input = || Input {
            query: "Alex",
            page: 2,
            query_session: Uuid::from_u128(1),
            search_session: Uuid::from_u128(2),
            sort: Sort::NameDescending,
            hide_deactivated: hide,
            primary_channel: "CONE",
        };
        let base = people_pages::request(&client, &session, input()).unwrap();
        let RequestBody::Form(base_fields) = base.inner.body else {
            panic!("text form")
        };
        assert!(!base_fields.iter().any(|(key, _)| key == "account_type"));
        for (choice, wire, label) in [
            (AccountType::Any, None, "Any account type"),
            (
                AccountType::RegularMembers,
                Some("0,1,2,3,7"),
                "Regular members",
            ),
            (AccountType::Deactivated, Some("6"), "Deactivated"),
        ] {
            assert!(AccountType::parse(choice.value()) == Some(choice));
            assert_eq!(choice.label(), label);
            let r =
                people_pages::request_for_account_type(&client, &session, input(), choice).unwrap();
            let RequestBody::Form(fields) = r.inner.body else {
                panic!("text form")
            };
            let mut expected = base_fields.clone();
            if let Some(value) = wire {
                expected.push(("account_type".into(), value.into()));
            }
            assert_eq!(
                fields, expected,
                "all other original fields and order unchanged, including hide and selected team"
            );
        }
    }
    for value in [
        "",
        "0,1",
        "0,1,2",
        "4,5,8",
        "10",
        "org_admins",
        "workspace_owners",
        "6,6",
        "6,0",
    ] {
        assert!(AccountType::parse(value).is_none());
    }
}
