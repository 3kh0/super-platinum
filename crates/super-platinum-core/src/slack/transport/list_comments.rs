//! Closed List comment reads. Sending lives in a separate single-attempt module.
mod reply;
use super::*;
use crate::slack_lists::comments;
pub use reply::ListCommentReplyFailure;
use std::collections::BTreeMap;

impl Transport {
    pub async fn execute_list_comments_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, Error> {
        self.list_comments_read_once(request, current, read_allowed)
            .await
    }
    pub async fn execute_list_comments_acquire(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, Error> {
        self.list_comments_read_once(request, current, acquire_allowed)
            .await
    }
    async fn list_comments_read_once(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
        allowed: fn(&PreparedRequest) -> bool,
    ) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api("unsupported_list_comments_read".into()));
        }
        // One shared callback serves both the executor's immediate pre-dispatch
        // fences and cancellation while awaiting its bounded response. No lock
        // is held across an await, and poisoning fails closed.
        let current = std::sync::Mutex::new(current);
        let check = || current.lock().is_ok_and(|mut current| current());
        let work = async {
            self.execute_list_rename_once(request, check, allowed)
                .await
                .map_err(|e| match e {
                    ListColumnRenameFailure::NotRequested(e)
                    | ListColumnRenameFailure::Unconfirmed(e) => e,
                })
        };
        guarded(check, work).await
    }
}
async fn guarded<T>(
    mut current: impl FnMut() -> bool,
    work: impl std::future::Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    tokio::pin!(work);
    loop {
        if !current() {
            return Err(Error::Api("list_comment_owner_changed".into()));
        }
        tokio::select! {
            result = &mut work => {
                return if current() { result } else { Err(Error::Api("list_comment_owner_changed".into())) };
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    }
}
fn fields<'a>(request: &'a PreparedRequest, path: &str) -> Option<BTreeMap<&'a str, &'a str>> {
    if request.method != "POST" {
        return None;
    }
    let url = url::Url::parse(&request.url).ok()?;
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != path
        || url.query_pairs().any(|(k, _)| {
            !matches!(
                k.as_ref(),
                "_x_id"
                    | "_x_csid"
                    | "_x_desktop_ia"
                    | "_x_foreground"
                    | "_x_frontend_build_type"
                    | "_x_gantry"
                    | "_x_num_retries"
                    | "_x_version_ts"
                    | "fp"
                    | "slack_route"
            )
        })
    {
        return None;
    }
    let RequestBody::Form(raw) = &request.body else {
        return None;
    };
    if raw
        .iter()
        .try_fold(0usize, |n, (k, v)| {
            n.checked_add(k.len())?.checked_add(v.len())
        })
        .is_none_or(|n| n > 128 * 1024)
    {
        return None;
    }
    let map: BTreeMap<_, _> = raw.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    if map.len() != raw.len() || map.get("token").is_none_or(|v| v.is_empty()) {
        return None;
    }
    Some(map)
}
fn is_channel(value: &str) -> bool {
    value
        .strip_prefix('C')
        .is_some_and(|s| comments::channel(&format!("F{s}")).is_ok())
}
fn read_allowed(request: &PreparedRequest) -> bool {
    let Some(f) = fields(request, "/api/conversations.replies") else {
        return false;
    };
    let Some(channel) = f.get("channel").filter(|v| is_channel(v)) else {
        return false;
    };
    let Some(ts) = f.get("ts") else {
        return false;
    };
    let alias = format!("F{}-", &channel[1..]);
    let resolving = ts.strip_prefix(alias.as_str()).is_some_and(|r| {
        !r.is_empty() && r.len() <= 256 && !r.chars().any(|c| c.is_control() || c.is_whitespace())
    });
    if (!comments::timestamp(ts) && !resolving)
        || f.get("inclusive") != Some(&"true")
        || f.get("limit") != Some(&"28")
        || f.get("cached_latest_updates") != Some(&"{}")
        || f.get("_x_reason") != Some(&"history-api/fetchReplies")
        || f.keys().any(|k| {
            !matches!(
                *k,
                "token"
                    | "channel"
                    | "ts"
                    | "inclusive"
                    | "limit"
                    | "cached_latest_updates"
                    | "_x_reason"
                    | "include_tombstones"
                    | "cursor"
                    | "oldest"
                    | "latest"
            )
        })
        || f.get("include_tombstones").is_some_and(|v| *v != "true")
        || ["oldest", "latest"]
            .iter()
            .any(|k| f.get(k).is_some_and(|v| !comments::timestamp(v)))
        || f.get("cursor")
            .is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control))
        || (f.contains_key("cursor") && (f.contains_key("oldest") || f.contains_key("latest")))
        || (f.contains_key("oldest") && f.contains_key("latest"))
        || (resolving
            && ["cursor", "oldest", "latest"]
                .iter()
                .any(|k| f.contains_key(k)))
    {
        return false;
    }
    true
}

fn acquire_allowed(request: &PreparedRequest) -> bool {
    if let Some(f) = fields(request, "/api/experiments.getByUser") {
        let platform = std::env::consts::OS.replace("macos", "darwin");
        return f.len() == 3
            && f.get("ssb_platform") == Some(&platform.as_str())
            && f.get("ssb_release_channel") == Some(&"prod");
    }
    if let Some(f) = fields(request, "/api/files.info") {
        return f.len() == 2 && f.get("file").is_some_and(|v| comments::channel(v).is_ok());
    }
    if let Some(f) = fields(request, "/api/lists.records.info") {
        return f.len() == 5
            && f.get("list_id")
                .is_some_and(|v| comments::channel(v).is_ok())
            && f.get("id").is_some_and(|v| {
                !v.is_empty()
                    && v.len() <= 256
                    && !v.chars().any(|c| c.is_control() || c.is_whitespace())
            })
            && f.get("include_is_subscribed") == Some(&"true")
            && f.get("_x_reason") == Some(&"get-record-subtasks");
    }
    let Some(f) = fields(request, "/api/conversations.genericInfo") else {
        return false;
    };
    if f.len() != 2 {
        return false;
    }
    let Some(encoded) = f.get("updated_channels") else {
        return false;
    };
    let Ok(Value::Object(channels)) = serde_json::from_str::<Value>(encoded) else {
        return false;
    };
    // This host requires one unconditionally refreshed virtual List conversation.
    // Canonical encoding also refuses duplicate nested keys hidden by JSON parsing.
    channels.len() == 1
        && channels
            .iter()
            .all(|(id, updated)| is_channel(id) && updated.as_u64() == Some(0))
        && serde_json::to_string(&channels).is_ok_and(|v| v == *encoded)
}

#[cfg(test)]
mod tests;
