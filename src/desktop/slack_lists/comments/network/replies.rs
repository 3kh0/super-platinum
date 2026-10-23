//! Retain ordering from the original bounded response, alongside the typed page.
use super::*;

pub(in crate::slack_lists::comments) async fn reply_page(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    prepared: PreparedRequest,
    target: &Target,
) -> Result<Page, Error> {
    if !owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    if crate::fixture::is_fixture() {
        if let Some(result) = fixture::ordered::read(state, owner, serial, &prepared).await {
            let (request_serial, response) = result?;
            let page =
                Page::parse_ordered(target, response).map_err(|e| Error::Api(e.to_string()))?;
            fixture::ordered::observe_page(state, owner, serial, request_serial, &page)?;
            return Ok(page);
        }
        // Existing fixtures provide Values, not original response bytes. They
        // intentionally carry no ordering witness; serialization cannot recover
        // an original style-key order from an already decoded Value.
        let raw = request(state, owner, serial, prepared, true).await?;
        return Page::parse(target, raw).map_err(|e| Error::Api(e.to_string()));
    }
    let context = owner.scope.request(&state.peek()).ok_or_else(unavailable)?;
    let read = context
        .transport
        .execute_list_comments_read_ordered(prepared, || owner.reading(&state.peek(), serial));
    let cancelled = async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !owner.reading(&state.peek(), serial) {
                break;
            }
        }
    };
    let response =
        tokio::select! { biased; _=cancelled=>return Err(unavailable()), result=read=>result? };
    if !owner.reading(&state.peek(), serial) {
        return Err(unavailable());
    }
    Page::parse_ordered(target, response).map_err(|e| Error::Api(e.to_string()))
}
