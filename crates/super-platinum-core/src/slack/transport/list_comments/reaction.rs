//! One guarded reaction dispatch. Returned JSON is evidence, not settlement.
use super::*;

#[derive(Debug)]
pub enum ListCommentReactionFailure {
    NotRequested(Error),
    Unconfirmed(Error),
}

impl Transport {
    pub async fn execute_list_comment_reaction(
        &self,
        request: PreparedRequest,
        channel: &str,
        ts: &str,
        name: &str,
        adding: bool,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListCommentReactionFailure> {
        if !matches_target(&request, channel, ts, name, adding) {
            return Err(ListCommentReactionFailure::NotRequested(Error::Api(
                "invalid_list_comment_reaction_request".into(),
            )));
        }
        // No read-style cancellation: once issued, even a retired owner must
        // receive the original result for its receipt. Shared mechanics enforce
        // both pre-dispatch owner/health fences, timeout, no retry or redirects.
        self.execute_list_rename_once(request, current, allowed)
            .await
            .map_err(|failure| match failure {
                ListColumnRenameFailure::NotRequested(error) => {
                    ListCommentReactionFailure::NotRequested(error)
                }
                ListColumnRenameFailure::Unconfirmed(error) => {
                    ListCommentReactionFailure::Unconfirmed(error)
                }
            })
    }
}

fn path(adding: bool) -> &'static str {
    if adding {
        "/api/reactions.add"
    } else {
        "/api/reactions.remove"
    }
}
fn matches_target(
    request: &PreparedRequest,
    channel: &str,
    ts: &str,
    name: &str,
    adding: bool,
) -> bool {
    let Some(fields) = fields(request, path(adding)) else {
        return false;
    };
    valid(&fields)
        && fields.get("channel") == Some(&channel)
        && fields.get("timestamp") == Some(&ts)
        && fields.get("name") == Some(&name)
}
fn allowed(request: &PreparedRequest) -> bool {
    [true, false]
        .into_iter()
        .any(|adding| fields(request, path(adding)).is_some_and(|fields| valid(&fields)))
}
fn valid(fields: &BTreeMap<&str, &str>) -> bool {
    fields.len() == 4
        && fields.get("channel").is_some_and(|v| is_channel(v))
        && fields
            .get("timestamp")
            .is_some_and(|v| comments::timestamp(v))
        && fields.get("name").is_some_and(|name| {
            !name.is_empty()
                && name.len() <= 256
                && !name.chars().any(|c| c.is_whitespace() || c.is_control())
        })
    // fields() already requires a nonempty token and refuses duplicate keys.
}

#[cfg(test)]
mod tests;
