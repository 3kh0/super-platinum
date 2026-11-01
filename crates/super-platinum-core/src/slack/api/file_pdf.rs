//! Isolated PDF-purpose read descriptions. No image or native-ready authority.
use super::*;
use crate::slack::client::RequestBody;
use serde_json::Value;

pub struct Request {
    pub(in crate::slack) inner: PreparedRequest,
    pub(in crate::slack) multipart: bool,
}
pub fn valid_id(id: &str) -> bool {
    (9..=15).contains(&id.len())
        && id.starts_with('F')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
impl Request {
    fn new(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        id: &str,
        ip: bool,
    ) -> Result<Self, Error> {
        if !valid_id(id)
            || workspace.token.is_empty()
            || workspace.token.len() > 16 * 1024
            || workspace
                .token
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(Error::Api("pdf_preview_unavailable".into()));
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
            .map_err(|_| Error::Api("pdf_preview_unavailable".into()))?;
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || url.as_str() != inner.url
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
            || url.path()
                != if ip {
                    "/api/teams.authorized.ip.range.checkIp"
                } else {
                    "/api/files.info"
                }
        {
            return Err(Error::Api("pdf_preview_unavailable".into()));
        }
        if ip {
            inner
                .headers
                .retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
        }
        Ok(Self {
            inner,
            multipart: ip,
        })
    }
    pub fn metadata(c: &SlackClient, w: &WorkspaceSession, id: &str) -> Result<Self, Error> {
        Self::new(c, w, id, false)
    }
    /// Default computed token stays in multipart. Always requires fresh IPtrue;
    /// an absent team preference is never interpreted as permission.
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
pub fn ip_allowed(raw: &Value) -> bool {
    raw.get("ok") == Some(&Value::Bool(true))
        && raw.get("ip_allowed") == Some(&Value::Bool(true))
        && raw.get("error").is_none()
}

#[cfg(test)]
mod tests;
