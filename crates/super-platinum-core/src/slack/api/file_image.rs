//! Closed read-only requests for the isolated private Search image preview.
//! Native metadata form/desktop routing policy; no Source wire equivalence.
use super::*;
use crate::slack::client::RequestBody;

pub struct Request {
    pub(in crate::slack) inner: PreparedRequest,
    pub(in crate::slack) multipart: bool,
}
pub fn valid_id(id: &str) -> bool {
    (9..=15).contains(&id.len())
        && id.starts_with('F')
        && id[1..]
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
impl Request {
    fn new(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        id: &str,
        ip: bool,
    ) -> Result<Self, Error> {
        if !valid_id(id) || workspace.token.is_empty() {
            return Err(Error::Api("image_preview_unavailable".into()));
        }
        let fields = if ip {
            vec![("file", id.into())]
        } else {
            vec![
                ("file", id.into()),
                ("count", "1".into()),
                ("page", "1".into()),
                ("truncate", "false".into()),
            ]
        };
        let mut inner = client.rest_form(
            workspace,
            if ip {
                "teams.authorized.ip.range.checkIp"
            } else {
                "files.info"
            },
            fields,
        );
        let url = url::Url::parse(&inner.url)
            .map_err(|_| Error::Api("image_preview_unavailable".into()))?;
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        {
            return Err(Error::Api("image_preview_unavailable".into()));
        }
        if ip {
            inner
                .headers
                .retain(|(key, _)| !key.eq_ignore_ascii_case("content-type"));
        }
        Ok(Self {
            inner,
            multipart: ip,
        })
    }
    pub fn metadata(c: &SlackClient, w: &WorkspaceSession, id: &str) -> Result<Self, Error> {
        Self::new(c, w, id, false)
    }
    /// Stronger native policy: every preview requires a fresh literal true IP
    /// verdict. No missing team preference is interpreted as permission.
    pub fn ip(c: &SlackClient, w: &WorkspaceSession, id: &str) -> Result<Self, Error> {
        Self::new(c, w, id, true)
    }
    pub fn file_id(&self) -> &str {
        let RequestBody::Form(fields) = &self.inner.body else {
            unreachable!()
        };
        fields.iter().find(|(k, _)| k == "file").unwrap().1.as_str()
    }
    pub fn is_ip(&self) -> bool {
        self.multipart
    }
}
pub fn ip_allowed(raw: &serde_json::Value) -> bool {
    raw.get("ok") == Some(&serde_json::Value::Bool(true))
        && raw.get("ip_allowed") == Some(&serde_json::Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn isolated_image_requests_are_exact_read_only_and_ip_never_defaults() {
        let client = SlackClient::new(Default::default());
        let w = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-token".into(),
        };
        let metadata = Request::metadata(&client, &w, "F12345678").unwrap();
        assert_eq!(metadata.inner.method, "POST");
        assert!(!metadata.is_ip());
        let RequestBody::Form(fields) = metadata.inner.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("file".into(), "F12345678".into()),
                ("count".into(), "1".into()),
                ("page".into(), "1".into()),
                ("truncate".into(), "false".into())
            ]
        );
        let ip = Request::ip(&client, &w, "F12345678").unwrap();
        assert!(
            ip.inner
                .url
                .contains("/api/teams.authorized.ip.range.checkIp?")
        );
        assert!(ip.is_ip());
        assert_eq!(ip.file_id(), "F12345678");
        assert!(
            !ip.inner
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        );
        for raw in [
            json!({}),
            json!({"ok":true}),
            json!({"ok":true,"ip_allowed":1}),
            json!({"ok":true,"ip_allowed":"true"}),
            json!({"ok":false,"ip_allowed":true}),
        ] {
            assert!(!ip_allowed(&raw));
        }
        assert!(ip_allowed(&json!({"ok":true,"ip_allowed":true})));
        assert!(Request::metadata(&client, &w, "Fbad").is_err());
    }
}
