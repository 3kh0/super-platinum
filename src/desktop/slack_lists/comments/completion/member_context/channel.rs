use super::*;
use crate::app_home::channel_fetches;
use std::collections::BTreeSet;
use super_platinum_core::{block_actions::directory_endpoint::channel_info, slack::api};

pub(super) async fn acquire(
    mut state: Signal<ShellState>,
    owner: &Arc<Fence>,
    id: &str,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Arc<canonical::Canonical>, Error> {
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let source = canonical::capture(&mut state.write(), &owner.context.session.team_id)?;
    if source.row(id).is_some() {
        return Ok(source);
    }
    // The valid missing-ID selector is an ephemeral unknown object. It is not
    // inserted as a real channel or decorated using the List document's flags.
    let versions = BTreeMap::from([(id.to_owned(), 0)]);
    let mut read = channel_info::Read::capture(vec![id.to_owned()], Some(versions.clone()))?;
    let admitted = |s: &ShellState| current(s) && source.current(s);
    let publication =
        channel_fetches::search::Read::capture(state, &owner.context.session.team_id, &admitted)?;
    let original = |s: &ShellState| admitted(s) && publication.current(s);
    let endpoint = read::endpoint(state, owner, &original).await?;
    let ctx = &owner.context;
    let mut rows = Vec::new();
    let mut markers = BTreeSet::new();
    for _ in 0..50 {
        let request = endpoint.channel_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
        let scoped = |s: &ShellState| original(s) && endpoint.current(&ctx.session, Instant::now());
        let (raw, keys) =
            read::execute(state, owner, request, read::Kind::Channel, &scoped).await?;
        let page = channel_info::Page::decode(&read, raw, keys)?;
        if !page.pending_ids().is_empty() || !page.invalid_ids().is_empty() {
            return Err(unavailable());
        }
        rows.extend_from_slice(page.rows());
        if let Some(marker) = page.next_marker() {
            if !markers.insert(marker.to_owned()) {
                return Err(unavailable());
            }
            read = read.next_page(marker)?;
            continue;
        }
        // Only a genuinely missing conditional result goes through the actual
        // genericInfo fallback, retaining the same original publication.
        if rows.is_empty() || !page.failed_ids().is_empty() {
            let request = api::conversations_generic_info(&ctx.client, &ctx.session, &versions);
            let (raw, _) = read::execute(
                state,
                owner,
                request,
                read::Kind::Generic(versions.clone()),
                &original,
            )
            .await?;
            if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
                return Err(unavailable());
            }
            let fallback = raw
                .get("channels")
                .map(|v| v.as_array().ok_or_else(unavailable))
                .transpose()?
                .cloned()
                .unwrap_or_default();
            if fallback.iter().any(|r| r["id"].as_str() != Some(id)) {
                return Err(unavailable());
            }
            rows.extend(fallback);
        }
        return publication.publish(state, rows, current).await;
    }
    Err(unavailable())
}
