//! EYEw.Io for profile member exclusion. Unlike Tin+.Ei, this uses xZo4.KV
//! (production), not Cu (development). QA/staging must not borrow Tin+'s ID.
//! Native context is the actual configured Slack service and generated REST
//! route. Neither a local WebView URL nor enterprise membership substitutes.
use crate::config::WorkspaceSession;
use crate::slack::{Error, SlackClient};

const ROOTS: [&str; 6] = [
    "slack.com",
    "slack-dev.com",
    "slack-gov.com",
    "slack-gov-dev.com",
    "slack.mil",
    "slack-mil-dev.com",
];

fn invalid() -> Error {
    Error::Api("profile_completion_environment_unavailable".into())
}

/// Inspect the same request used to observe assignments, without executing it
/// or retaining its token/body. Both contexts must select the identical app.
pub fn salesforce_app(
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<&'static str, Error> {
    let configured = url::Url::parse(&workspace.url).map_err(|_| invalid())?;
    if configured.path() != "/" || configured.query().is_some() || configured.fragment().is_some() {
        return Err(invalid());
    }
    let request = crate::slack::api::list_experiments(client, workspace);
    let routed = url::Url::parse(&request.url).map_err(|_| invalid())?;
    if request.method != "POST"
        || routed.path() != "/api/experiments.getByUser"
        || routed.fragment().is_some()
    {
        return Err(invalid());
    }
    let selected = app(&configured)?;
    if selected != app(&routed)? {
        return Err(invalid());
    }
    Ok(selected)
}

fn app(url: &url::Url) -> Result<&'static str, Error> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?;
    let (production, government) = classify(host)?;
    Ok(match (production, government) {
        (true, false) => "A04T99UKKQE",
        (false, false) => "A014JPFMT8T",
        (true, true) => "A1W53KSJQ",
        (false, true) => "A07846N4L",
    })
}

fn suffix(value: &str, prefix: &str, empty: bool) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|v| (empty || !v.is_empty()) && v.bytes().all(|c| c.is_ascii_digit()))
}
fn classify(host: &str) -> Result<(bool, bool), Error> {
    let mut parts: Vec<_> = host.split('.').collect();
    if parts.len() < 2 {
        return Err(invalid());
    }
    let root = parts[parts.len() - 2..].join(".");
    if !ROOTS.contains(&root.as_str()) {
        return Err(invalid());
    }
    parts.truncate(parts.len() - 2);
    enum Environment {
        Production,
        Development,
        Qa,
        Staging,
    }
    let mut env = Environment::Production;
    if let Some(last) = parts.last().copied() {
        if last == "beach" {
            parts.pop();
            parts.pop();
            env = Environment::Development;
        } else if suffix(last, "dev", true)
            || last
                .strip_prefix("dev-gov-")
                .is_some_and(|v| v.bytes().all(|c| c.is_ascii_lowercase()))
        {
            parts.pop();
            env = Environment::Development;
        } else if suffix(last, "qa", true) {
            parts.pop();
            env = Environment::Qa;
        } else if matches!(last, "staging" | "staging-master" | "staging-sandbox") {
            parts.pop();
            env = Environment::Staging;
        } else if suffix(last, "staging", false) {
            // Actual xZo4 treats numbered staging hosts as production.
            parts.pop();
        }
    }
    if parts.last() == Some(&"enterprise") {
        parts.pop();
    }
    if matches!(
        root.as_str(),
        "slack-dev.com" | "slack-gov-dev.com" | "slack-mil-dev.com"
    ) && !matches!(env, Environment::Development | Environment::Qa)
    {
        env = Environment::Development;
    }
    if parts.len() > 1 {
        return Err(invalid());
    }
    Ok((
        matches!(env, Environment::Production),
        matches!(
            root.as_str(),
            "slack-gov.com" | "slack-gov-dev.com" | "slack.mil" | "slack-mil-dev.com"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_eye_w_environment_vectors_include_qa_staging_and_numbered_staging() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("environment-oracle.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let url = url::Url::parse(case["url"].as_str().unwrap()).unwrap();
            assert_eq!(app(&url).unwrap(), case["expected"].as_str().unwrap());
        }
    }
    #[test]
    fn native_environment_does_not_trust_non_slack_or_ambiguous_hosts() {
        for raw in [
            "https://slack.com.example.test/",
            "https://one.two.slack.com/",
            "http://team.slack.com/",
            "https://user@team.slack.com/",
            "https://team.slack.com:9443/",
        ] {
            assert!(app(&url::Url::parse(raw).unwrap()).is_err());
        }
    }
    #[test]
    fn configured_service_and_actual_enterprise_route_must_agree() {
        let client = SlackClient::default();
        let mut owner = WorkspaceSession {
            team_id: "TTEST1234".into(),
            user_id: "USELF1234".into(),
            enterprise_id: Some("ETEST1234".into()),
            name: "Synthetic".into(),
            url: "https://team.slack.com/".into(),
            token: "synthetic".into(),
        };
        assert_eq!(salesforce_app(&client, &owner).unwrap(), "A04T99UKKQE");
        // rest_base's installed enterprise rewrite loses the QA environment.
        // Refuse that disagreement instead of treating it as production.
        owner.url = "https://team.qa.slack.com/".into();
        assert!(salesforce_app(&client, &owner).is_err());
        owner.enterprise_id = None;
        assert_eq!(salesforce_app(&client, &owner).unwrap(), "A014JPFMT8T");
        for url in [
            "https://team.slack.com/other",
            "https://team.slack.com/?override=dev",
            "https://team.slack.com/#dev",
        ] {
            owner.url = url.into();
            assert!(salesforce_app(&client, &owner).is_err());
        }
    }
}
