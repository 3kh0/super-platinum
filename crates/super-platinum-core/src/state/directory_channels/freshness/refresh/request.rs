//! Closed sQEb resynced-topics requests. No dispatch or publication authority.
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};

pub const EXTRAS: [&str; 10] = [
    "cache_timestamps",
    "features",
    "salesforce_features",
    "misc",
    "subteams",
    "team_prefs",
    "stats",
    "default_workspace",
    "plan_info",
    "feature_usage_data",
];
/// Actual blocking boot Init wrapper (sQEb plus client-boot.c09af).
pub fn initial_init(client: &SlackClient, owner: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        owner,
        "client.init",
        vec![
            ("include_relevant_onboarding", "true".into()),
            ("_x_reason", "fetch-client-init-data".into()),
        ],
    )
}
/// `min_channel_updated` is supplied only from an actual enabled warm-boot
/// source observation. Cold boot never sends it, even if retained by a caller.
pub fn initial_channels(
    client: &SlackClient,
    owner: &WorkspaceSession,
    version_all_channels: bool,
    warm: bool,
    min_channel_updated: Option<u64>,
) -> PreparedRequest {
    let mut fields = vec![
        (
            "_x_reason",
            if warm {
                "deferred-data"
            } else {
                "initial-data"
            }
            .into(),
        ),
        ("version_all_channels", version_all_channels.to_string()),
        ("return_all_relevant_mpdms", "true".into()),
    ];
    if warm && let Some(minimum) = min_channel_updated {
        fields.push(("min_channel_updated", minimum.to_string()));
    }
    client.rest_form(owner, "client.channels", fields)
}
/// cD+s.Id's enabled ytap list, in its original order; no dnd/onboarding.
pub fn initial_extras(client: &SlackClient, owner: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        owner,
        "client.extras",
        vec![
            ("_x_reason", "fetch-client-extras-data".into()),
            ("extras", serde_json::json!(EXTRAS).to_string()),
        ],
    )
}
pub fn init(client: &SlackClient, owner: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        owner,
        "client.init",
        vec![
            ("include_relevant_onboarding", "false".into()),
            ("_x_reason", "resynced-topics".into()),
        ],
    )
}
/// Caller supplies the exact selected request policy; missing observation cannot
/// silently become false. Neither value establishes a complete Channel store.
pub fn channels(
    client: &SlackClient,
    owner: &WorkspaceSession,
    version_all_channels: bool,
) -> PreparedRequest {
    client.rest_form(
        owner,
        "client.channels",
        vec![
            ("_x_reason", "resynced-topics".into()),
            ("version_all_channels", version_all_channels.to_string()),
            ("return_all_relevant_mpdms", "true".into()),
        ],
    )
}
pub fn extras(client: &SlackClient, owner: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        owner,
        "client.extras",
        vec![
            ("_x_reason", "resynced-topics".into()),
            ("extras", serde_json::json!(EXTRAS).to_string()),
        ],
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TONE".into(),
            user_id: "USELF".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn initial_boot_options_are_distinct_from_topic_resync() {
        let c = SlackClient::default();
        let o = owner();
        let init = fields(initial_init(&c, &o), "client.init");
        assert!(init.contains(&("include_relevant_onboarding".into(), "true".into())));
        assert!(init.contains(&("_x_reason".into(), "fetch-client-init-data".into())));
        for (warm, reason) in [(false, "initial-data"), (true, "deferred-data")] {
            let channels = fields(
                initial_channels(&c, &o, false, warm, Some(123)),
                "client.channels",
            );
            assert!(channels.contains(&("_x_reason".into(), reason.into())));
            assert_eq!(
                channels
                    .iter()
                    .find(|(k, _)| k == "min_channel_updated")
                    .map(|(_, v)| v.as_str()),
                warm.then_some("123")
            );
        }
        let extras = fields(initial_extras(&c, &o), "client.extras");
        assert!(extras.contains(&("_x_reason".into(), "fetch-client-extras-data".into())));
        assert!(extras.contains(&("extras".into(), serde_json::json!(EXTRAS).to_string())));
    }
    fn fields(request: PreparedRequest, endpoint: &str) -> Vec<(String, String)> {
        assert_eq!(request.method, "POST");
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.host_str(), Some("fixture.slack.com"));
        assert_eq!(url.path(), format!("/api/{endpoint}"));
        let RequestBody::Form(fields) = request.body else {
            panic!("form required")
        };
        fields
    }
    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }
    #[test]
    fn split_requests_retain_exact_reason_defaults_and_enabled_extras_order() {
        let c = SlackClient::default();
        let o = owner();
        assert_eq!(
            fields(init(&c, &o), "client.init"),
            pairs(&[
                ("token", "synthetic"),
                ("include_relevant_onboarding", "false"),
                ("_x_reason", "resynced-topics")
            ])
        );
        for version in [false, true] {
            assert_eq!(
                fields(channels(&c, &o, version), "client.channels"),
                pairs(&[
                    ("token", "synthetic"),
                    ("_x_reason", "resynced-topics"),
                    (
                        "version_all_channels",
                        if version { "true" } else { "false" }
                    ),
                    ("return_all_relevant_mpdms", "true")
                ])
            );
        }
        let args = fields(extras(&c, &o), "client.extras");
        assert_eq!(
            &args[..2],
            &pairs(&[("token", "synthetic"), ("_x_reason", "resynced-topics")])
        );
        assert_eq!(args.len(), 3);
        assert_eq!(args[2].0, "extras");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&args[2].1).unwrap(),
            EXTRAS
        );
        assert!(!EXTRAS.contains(&"dnd"));
        assert!(!EXTRAS.contains(&"onboarding"));
    }
}
