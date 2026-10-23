use super::*;
use super_platinum_core::{
    slack::{api, client::PreparedRequest},
    slack_lists::comments::{ReplyQuery, Tombstones},
};
mod replies;
pub(super) use replies::reply_page;

pub(super) struct Loaded {
    pub page: Page,
    pub projection: Projection,
}

pub(super) async fn request(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    request: PreparedRequest,
    replies: bool,
) -> Result<Value, Error> {
    if !owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    let read = async {
        if crate::fixture::is_fixture() {
            if !fixture::enabled(&state.peek()) {
                return Err(Error::Offline("Closed List comments fixture".into()));
            }
            if fixture::compose::enabled(&state.peek()) {
                fixture::compose::preflight_request(state, &request).await
            } else {
                fixture::request(state, &request).await
            }
        } else {
            let context = owner.scope.request(&state.peek()).ok_or_else(unavailable)?;
            if replies {
                context
                    .transport
                    .execute_list_comments_read(request, || owner.reading(&state.peek(), serial))
                    .await
            } else {
                context
                    .transport
                    .execute_list_comments_acquire(request, || owner.reading(&state.peek(), serial))
                    .await
            }
        }
    };
    let cancelled = async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !owner.reading(&state.peek(), serial) {
                break;
            }
        }
    };
    let raw =
        tokio::select! { biased; _=cancelled=>return Err(unavailable()), result=read=>result? };
    if !owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    Ok(raw)
}
pub(super) async fn acquire(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    old: &Target,
    cursor: Option<String>,
) -> Result<Loaded, Error> {
    if !owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    let (client, session) = if crate::fixture::is_fixture() {
        if !fixture::enabled(&state.peek()) {
            return Err(unavailable());
        }
        (state.peek().core.client.clone(), fixture::session())
    } else {
        let context = owner.scope.request(&state.peek()).ok_or_else(unavailable)?;
        (context.client, context.session)
    };
    let file = request(
        state,
        owner,
        serial,
        api::file_info(&client, &session, old.list_id()),
        false,
    )
    .await?;
    let record = request(
        state,
        owner,
        serial,
        api::list_record_subtasks(&client, &session, old.list_id(), old.record_id()),
        false,
    )
    .await?;
    let target = old
        .refresh(&file, &record, cursor.is_some())
        .map_err(|e| Error::Api(e.to_string()))?;
    let assignments = request(
        state,
        owner,
        serial,
        api::list_experiments(&client, &session),
        false,
    )
    .await?;
    let projection = Projection::observed(assignments).map_err(|e| Error::Api(e.to_string()))?;
    // No synchronized file-topic cache is claimed by this panel.
    let prepared = api::list_record_comments_replies(
        &client,
        &session,
        &target,
        &ReplyQuery {
            cursor,
            ..Default::default()
        },
        Tombstones::Include,
    )?;
    let page = reply_page(state, owner, serial, prepared, &target).await?;
    Ok(Loaded { page, projection })
}
