//! Shortcut bookmark writes retain raw responses and never replay or redirect.
//! This is a deliberate safety adaptation: shipped generic requests allow retries.
use super::*;

impl Transport {
    /// The caller owns target policy, the original native selection and each
    /// independent settings receipt. A later read is never a replacement ACK.
    pub async fn execute_workflow_confirmation_add(
        &self,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api(
                "workflow_management_not_sent_confirmation_add_method".into(),
            ));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Workflow shortcut bookmark was not sent while offline".into(),
            ));
        }
        let http = self
            .single_attempt_client()
            .map_err(|_| Error::Api("workflow_management_not_sent_client".into()))?;
        self.execute_once_using(&request, true, http)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => Error::Transport(
                    "Workflow shortcut bookmark response was lost after dispatch".into(),
                ),
                error => error,
            })
    }
}

fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/api/bookmarks.add"
        || !super::copy_permission::metadata_query(&url)
    {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    fields.iter().all(|(key, value)| {
        seen.insert(key.as_str())
            && match key.as_str() {
                "token" => !value.is_empty(),
                "channel" => {
                    identifier(value) && matches!(value.as_bytes().first(), Some(b'C' | b'G'))
                }
                "_x_reason" => value == "channel-tab-add-bookmark",
                "shortcut_id" => identifier(value),
                "type" => value == "shortcut",
                "title" | "link" => true,
                _ => false,
            }
    }) && [
        "token",
        "channel",
        "shortcut_id",
        "type",
        "title",
        "_x_reason",
    ]
    .iter()
    .all(|key| seen.contains(key))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests;
