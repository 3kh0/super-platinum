//! Actual Tin+.Ei from xZo4 + /h4w. Inputs are observations, never preferences.
use super::*;

#[derive(Clone, PartialEq)]
pub struct Context {
    location: String,
    workflow_assignment: Value,
    jwt_assignment: Value,
    app: &'static str,
}

impl Context {
    /// Caller must acquire the actual client location and the two exact `_Z`
    /// assignment results for the current owner. Missing observations are errors;
    /// no team URL, organization ID, or newly invented session flag substitutes.
    pub fn observe(location: &str, workflow: &Value, jwt: &Value) -> Result<Self, Error> {
        for value in [workflow, jwt] {
            if !value
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128)
            {
                return Err(invalid());
            }
        }
        let url = url::Url::parse(location).map_err(|_| invalid())?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(invalid());
        }
        let host = url.host_str().ok_or_else(invalid)?;
        let (dev, gov_domain) = environment(host)?;
        // uQr4.nT does not decode keys or '+'; last duplicate key wins. These
        // are exactly the raw spellings that JS unescape maps to the string "1".
        let gov_query = url
            .query()
            .unwrap_or("")
            .split('&')
            .map(|part| part.split_once('=').unwrap_or((part, "")))
            .filter(|(key, _)| *key == "gov_slack")
            .last()
            .is_some_and(|(_, value)| matches!(value, "1" | "%31" | "%u0031"));
        let gov = gov_domain || dev && gov_query;
        let app = match (dev, gov) {
            (true, true) => "A07846N4L",
            (false, true) => "A1W53KSJQ",
            (true, false) => "A014JPFMT8T",
            (false, false) => "A04T99UKKQE",
        };
        Ok(Self {
            location: location.into(),
            workflow_assignment: workflow.clone(),
            jwt_assignment: jwt.clone(),
            app,
        })
    }
    pub fn selected(&self, auth: &Auth) -> bool {
        self.workflow_assignment == "on" && auth.app == self.app
    }
    pub fn jwt(&self) -> bool {
        self.jwt_assignment == "on"
    }
    pub fn app(&self) -> &str {
        self.app
    }
}

fn numeric_suffix(value: &str, prefix: &str, empty: bool) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|s| (empty || !s.is_empty()) && s.bytes().all(|b| b.is_ascii_digit()))
}

fn environment(host: &str) -> Result<(bool, bool), Error> {
    let mut parts: Vec<_> = host.split('.').collect();
    if parts.len() < 2 {
        return Ok((false, false));
    }
    let root = parts[parts.len() - 2..].join(".");
    let known = [
        "slack.com",
        "slack-dev.com",
        "slack-gov.com",
        "slack-gov-dev.com",
        "slack.mil",
        "slack-mil-dev.com",
    ];
    if !known.contains(&root.as_str()) {
        return Ok((false, false));
    }
    parts.truncate(parts.len() - 2);
    let mut dev = false;
    let mut qa = false;
    if let Some(last) = parts.last().copied() {
        if last == "beach" {
            parts.pop();
            parts.pop();
            dev = true;
        } else if numeric_suffix(last, "dev", true)
            || last
                .strip_prefix("dev-gov-")
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_lowercase()))
        {
            parts.pop();
            dev = true;
        } else if numeric_suffix(last, "qa", true) {
            parts.pop();
            qa = true;
        } else if matches!(last, "staging" | "staging-master" | "staging-sandbox")
            || numeric_suffix(last, "staging", false)
        {
            parts.pop();
        }
    }
    if parts.last() == Some(&"enterprise") {
        parts.pop();
    }
    if parts.len() > 1 {
        return Err(invalid());
    }
    if matches!(
        root.as_str(),
        "slack-dev.com" | "slack-gov-dev.com" | "slack-mil-dev.com"
    ) && !dev
        && !qa
    {
        dev = true;
    }
    Ok((
        dev,
        matches!(
            root.as_str(),
            "slack-gov.com" | "slack-gov-dev.com" | "slack.mil" | "slack-mil-dev.com"
        ),
    ))
}
