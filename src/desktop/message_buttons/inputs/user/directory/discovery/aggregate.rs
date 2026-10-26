//! Existing endpoint fanout, exposed before dispatch for independently owned surfaces.
use super::*;
use super_platinum_core::slack::client::PreparedRequest;
pub(crate) struct Requests {
    pub(crate) users: Result<Option<PreparedRequest>, Error>,
    pub(crate) channels: Result<Option<PreparedRequest>, Error>,
    pub(crate) mpims: Result<Option<PreparedRequest>, Error>,
}
pub(crate) fn prepare(
    ctx: &Context,
    query: &str,
    filter: Option<&super::super::super::conversations::Filter>,
) -> Requests {
    Requests {
        users: if filter.is_none_or(|f| f.allows("im")) {
            users_request(ctx, query, filter).map(Some)
        } else {
            Ok(None)
        },
        channels: if let Some(f) = filter.filter(|f| f.allows("public") || f.allows("private")) {
            channels_request(ctx, query, Some(f), None).map(Some)
        } else {
            Ok(None)
        },
        mpims: if filter.is_some_and(|f| f.allows("mpim")) {
            ctx.client
                .edge_json(
                    &ctx.session,
                    "mpims/search",
                    serde_json::json!({"query":normalize_mpim_query(query),"count":24,"fuzz":1}),
                )
                .map_err(|_| Error::Api("conversation_search_unavailable".into()))
                .map(Some)
        } else {
            Ok(None)
        },
    }
}
pub(crate) struct Responses {
    pub(crate) users: Result<Option<Value>, Error>,
    pub(crate) channels: Result<Option<Value>, Error>,
    pub(crate) mpims: Result<Option<Value>, Error>,
}
async fn read(
    ctx: &Context,
    request: Result<Option<PreparedRequest>, Error>,
) -> Result<Option<Value>, Error> {
    match request? {
        Some(r) => ctx.transport.execute(r).await.map(Some),
        None => Ok(None),
    }
}
pub(crate) async fn execute(
    ctx: &Context,
    requests: Requests,
) -> Result<(Vec<User>, Vec<Channel>), Error> {
    let (users, channels, mpims) = tokio::join!(
        read(ctx, requests.users),
        read(ctx, requests.channels),
        read(ctx, requests.mpims)
    );
    decode(Responses {
        users,
        channels,
        mpims,
    })
}
pub(crate) fn decode(r: Responses) -> Result<(Vec<User>, Vec<Channel>), Error> {
    // Keep the longest independent server cooldown before any ordinary error.
    let delay = [
        r.users.as_ref().err(),
        r.channels.as_ref().err(),
        r.mpims.as_ref().err(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|e| {
        if let Error::RateLimited {
            retry_after_secs: Some(s),
            ..
        } = e
        {
            Some(*s)
        } else {
            None
        }
    })
    .max();
    if let Some(seconds) = delay {
        return Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
        });
    }
    let mut channels = match r.channels? {
        Some(v) => decode_channels(&v)?,
        None => vec![],
    };
    if let Some(value) = r.mpims? {
        let list = value
            .get("mpims")
            .or_else(|| value.get("results"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Api("conversation_search_invalid".into()))?;
        channels.extend(list.iter().take(24).filter_map(mpim));
    }
    let users = match r.users? {
        Some(v) => decode_users(&v)?,
        None => vec![],
    };
    Ok((users, channels))
}
