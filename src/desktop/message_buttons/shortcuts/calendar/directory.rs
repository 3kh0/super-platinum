//! Calendar-owned directory routing. An endpoint is not a user/channel grant.
use super::*;
use super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint;

#[derive(Clone, Copy)]
pub(super) enum Query {
    Guests(u64),
    Sharing(u64),
}
fn current(s: &ShellState, id: u64, query: Query, ctx: &Context) -> bool {
    editable(s, id)
        && context(s, id).is_some_and(|owned| std::ptr::eq(owned.as_ref(), ctx))
        && active(s, id).is_some_and(|c| match query {
            Query::Guests(generation) => c.guest_generation == generation,
            Query::Sharing(generation) => c.share_generation == generation,
        })
}
pub(super) async fn read(
    mut state: Signal<ShellState>,
    id: u64,
    query: Query,
    ctx: &Context,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let cached = {
        let s = state.peek();
        if !current(&s, id, query, ctx) {
            return Err(cal::invalid());
        }
        active(&s, id)
            .and_then(|c| c.directory_endpoint.as_ref())
            .filter(|endpoint| endpoint.current(&ctx.session, Instant::now()))
            .cloned()
    };
    let endpoint = match cached {
        Some(endpoint) => endpoint,
        None => {
            let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
            let raw = load::read(state, id, ctx, request).await?;
            let mut s = state.write();
            if !current(&s, id, query, ctx) {
                return Err(cal::invalid());
            }
            let endpoint = Arc::new(DirectoryEndpoint::decode(
                &ctx.session,
                &raw,
                Instant::now(),
            )?);
            active_mut(&mut s, id).unwrap().directory_endpoint = Some(endpoint.clone());
            endpoint
        }
    };
    let request = {
        let s = state.peek();
        if !current(&s, id, query, ctx) {
            return Err(cal::invalid());
        }
        endpoint.request(&ctx.client, &ctx.session, Instant::now(), path, body)?
    };
    let result = load::read(state, id, ctx, request).await;
    if !current(&state.peek(), id, query, ctx) {
        return Err(cal::invalid());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_does_not_extend_query_visit_or_context_authority() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s, "app-calendar");
        let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
        d.fixture = false;
        d.calendar.as_mut().unwrap().fixture = false;
        let id = d.id;
        let ctx = d.owner.ctx.clone();
        let c = d.calendar.as_mut().unwrap();
        c.guest_generation = 7;
        c.share_generation = 9;
        assert!(current(&s, id, Query::Guests(7), &ctx));
        assert!(current(&s, id, Query::Sharing(9), &ctx));
        assert!(!current(&s, id, Query::Guests(6), &ctx));
        assert!(!current(&s, id, Query::Sharing(8), &ctx));
        assert!(!current(&s, id.wrapping_add(1), Query::Guests(7), &ctx));
        active_mut(&mut s, id).unwrap().confirm_close = true;
        assert!(!current(&s, id, Query::Guests(7), &ctx));
        active_mut(&mut s, id).unwrap().confirm_close = false;
        assert!(current(&s, id, Query::Guests(7), &ctx));
        s.channel_generation = s.channel_generation.wrapping_add(1);
        assert!(!current(&s, id, Query::Guests(7), &ctx));
    }
}
