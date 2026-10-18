//! Closed literal fixture forms, independent from the production builders.
use super::*;
// XParams::rest_pairs emits three fresh UUIDs; retain them, validate their
// exact lexical shape and every other ordered pair, never regenerate IDs.
pub(super) fn url(value: &str, method: &str) -> bool {
    let Some((base, query)) = value.split_once('?') else {
        return false;
    };
    if base != format!("https://example.test/api/{method}") || query.contains('#') {
        return false;
    }
    let pairs = query
        .split('&')
        .map(|part| part.split_once('='))
        .collect::<Option<Vec<_>>>();
    let Some(pairs) = pairs else {
        return false;
    };
    if pairs.len() != 10 {
        return false;
    }
    let expected = [
        ("_x_id", None),
        ("_x_csid", None),
        ("_x_desktop_ia", Some("true")),
        ("_x_foreground", Some("true")),
        ("_x_frontend_build_type", Some("current")),
        ("_x_gantry", Some("true")),
        ("_x_num_retries", Some("0")),
        ("_x_version_ts", Some("capture-required")),
        ("fp", None),
        ("slack_route", Some("default")),
    ];
    pairs
        .iter()
        .zip(expected)
        .all(|((key, value), (name, literal))| {
            *key == name
                && match literal {
                    Some(v) => *value == v,
                    None => {
                        value.len() == 32
                            && value
                                .bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                            && value.as_bytes()[12] == b'4'
                            && matches!(value.as_bytes()[16], b'8' | b'9' | b'a' | b'b')
                    }
                }
        })
}
fn exact(fields: &[(String, String)], expected: &[(&str, &str)]) -> bool {
    fields.len() == expected.len()
        && fields
            .iter()
            .zip(expected)
            .all(|((k, v), (ek, ev))| k == ek && v == ev)
}
fn person(value: Option<&str>) -> bool {
    matches!(value, Some("U00000001" | "U00000002" | "W00000003"))
}
fn channel(value: Option<&str>) -> bool {
    matches!(value, Some("D00000002" | "D00000003"))
}
pub(super) fn valid(method: &str, fields: &[(String, String)], list: &str, team: &str) -> bool {
    let f = |key| field(fields, key).unwrap_or("");
    match method {
        "files.info" => exact(fields, &[("file", list)]),
        "lists.records.list" => exact(
            fields,
            &[
                ("list_id", list),
                ("include_subtasks", "true"),
                ("archived", "false"),
                ("include_suggested", "false"),
                ("_x_reason", "get-list-records"),
            ],
        ),
        "team.prefs.get" => exact(
            fields,
            &[
                ("target_team", team),
                ("_x_reason", "native-list-permission"),
            ],
        ),
        "users.prefs.get" => exact(
            fields,
            &[
                ("prefs", "task_lists,assigned_to_list"),
                ("_x_reason", "native-list-completion-permission"),
            ],
        ),
        "experiments.getByUser" => exact(
            fields,
            &[
                (
                    "ssb_platform",
                    &std::env::consts::OS.replace("macos", "darwin"),
                ),
                ("ssb_release_channel", "prod"),
            ],
        ),
        "files.canSee" => {
            person(field(fields, "user_id"))
                && exact(
                    fields,
                    &[
                        ("file_id", list),
                        ("user_id", f("user_id")),
                        ("_x_reason", "canShare"),
                    ],
                )
        }
        "conversations.open" => {
            matches!(field(fields, "users"), Some("U00000002" | "W00000003"))
                && exact(
                    fields,
                    &[
                        ("users", f("users")),
                        ("quiet", "0"),
                        ("return_im", "1"),
                        ("_x_reason", "get_channel_id_for_sharing"),
                    ],
                )
        }
        "conversations.info" => {
            channel(field(fields, "channel"))
                && exact(
                    fields,
                    &[
                        ("channel", f("channel")),
                        ("_x_reason", "channels-api/fetchConversationInfo"),
                    ],
                )
        }
        "files.share" => {
            channel(field(fields, "channel"))
                && ["read", "write"].iter().any(|grant| {
                    exact(
                        fields,
                        &[
                            ("files", list),
                            ("channel", f("channel")),
                            ("resharing_aware", "true"),
                            (
                                "permissions",
                                &json!([{"grant":grant,"file_id":list}]).to_string(),
                            ),
                            ("skip_dlp_user_warning", "false"),
                            ("from_share_modal", "true"),
                            ("_x_reason", "share_dialog_permission_share_to_user"),
                        ],
                    )
                })
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{config::WorkspaceSession, slack::client::SlackClient};
    fn session() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        }
    }
    #[test]
    fn actual_builders_match_only_closed_literal_assignment_forms() {
        let c = SlackClient::default();
        let w = session();
        let list = "F_LIST_FIXTURE";
        let requests = vec![
            api::file_info(&c, &w, list),
            api::lists_records_list(&c, &w, list),
            api::list_allow_lists(&c, &w),
            api::list_personal_task_preferences(&c, &w),
            api::list_experiments(&c, &w),
            api::list_assignee_access(&c, &w, list, "U00000001"),
            api::list_assignee_open_dm(&c, &w, "U00000002"),
            api::list_assignee_channel(&c, &w, "D00000002"),
            api::list_assignee_share(&c, &w, list, "D00000002", AssigneeGrant::Read, None),
            api::list_assignee_share(&c, &w, list, "D00000003", AssigneeGrant::Write, None),
        ];
        for request in requests {
            let (method, fields) = decoded(request).unwrap();
            assert!(valid(&method, &fields, list, &w.team_id), "{method}");
            let mut extra = fields.clone();
            extra.push(("silent_share".into(), "true".into()));
            assert!(!valid(&method, &extra, list, &w.team_id));
            let mut changed = fields.clone();
            changed[0].1.push('x');
            assert!(!valid(&method, &changed, list, &w.team_id));
        }
        for request in [
            api::list_assignee_share(
                &c,
                &w,
                list,
                "D00000002",
                AssigneeGrant::Read,
                Some("T_OTHER"),
            ),
            api::list_assignee_access(&c, &w, list, "U_FOREIGN"),
            api::list_assignee_share(&c, &w, "F_OTHER", "D00000002", AssigneeGrant::Read, None),
        ] {
            let (method, fields) = decoded(request).unwrap();
            assert!(!valid(&method, &fields, list, &w.team_id));
        }
    }
    #[test]
    fn decoder_refuses_foreign_transport_token_duplicate_and_non_form() {
        let c = SlackClient::default();
        let w = session();
        for mutate in 0..9 {
            let mut request = api::file_info(&c, &w, "F_LIST_FIXTURE");
            match mutate {
                0 => request.url = "https://slack.com/api/files.info".into(),
                1 => request.method = "GET",
                2 => {
                    if let RequestBody::Form(f) = &mut request.body {
                        f[0].1 = "foreign".into();
                    }
                }
                3 => {
                    if let RequestBody::Form(f) = &mut request.body {
                        f.push(("file".into(), "F_LIST_FIXTURE".into()));
                    }
                }
                5 => request.url.push_str("&unexpected=value"),
                6 => request.url.push_str("#fragment"),
                7 => request.body = RequestBody::Json(json!({"file":"F_LIST_FIXTURE"})),
                8 => request.url = request.url.replace("_x_num_retries=0", "_x_num_retries=1"),
                _ => {
                    if let RequestBody::Form(f) = &mut request.body {
                        f.push(("token".into(), "fixture-not-a-token".into()));
                    }
                }
            }
            assert!(decoded(request).is_err());
        }
    }
}
