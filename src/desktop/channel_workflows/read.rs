use super::*;
use std::time::Duration;
use super_platinum_core::{channel_workflows::Read, slack::Error};

pub(super) fn start(mut state: Signal<ShellState>, mut browse: Signal<Browse>, initial: bool) {
    let Some(mount) = browse.peek().mount.clone() else {
        return;
    };
    if !mount.current(&state.peek()) {
        return;
    }
    // This retained gate precedes all serial/owner changes, including the
    // automatic first mount after a rate-limited panel was closed.
    if cooling(&state.peek()).blocked {
        return;
    }
    if initial && browse.peek().serial != 0 {
        return;
    }
    if browse.peek().status == "loading" && browse.peek().current(&state.peek()) {
        return;
    }
    let owner = Owner::capture(&state.peek(), mount).map(Arc::new);
    let serial = {
        let mut b = browse.write();
        b.lease = owner.as_ref().map(|_| Arc::new(()));
        b.serial = b.serial.wrapping_add(1);
        b.owner = owner.clone();
        b.bookmarks = None;
        b.previews = None;
        b.collections = None;
        b.rows = None;
        b.status = if owner.is_some() { "loading" } else { "failed" };
        b.phase = owner.as_ref().map(|_| "bookmarks");
        b.notice = owner
            .as_ref()
            .is_none()
            .then_some("Workflows could not load. Reconnect and retry.");
        b.serial
    };
    // Details visibility reads the retained root, not the component signal.
    // Publish the revocation to its shell subscribers immediately.
    drop(state.write());
    let Some(owner) = owner else {
        return;
    };
    spawn(async move {
        let result = load(state, browse, &owner, serial).await;
        if !admitted(state, browse, &owner, serial) {
            return;
        }
        let mut b = browse.write();
        b.phase = None;
        match result {
            Ok((collections, previews)) => {
                let rows = previews
                    .as_ref()
                    .map(|p| p.project(&collections))
                    .transpose();
                match rows {
                    Ok(rows) => {
                        let partial = previews.as_ref().is_some_and(|p| !p.all_displayable());
                        b.rows = rows;
                        b.collections = Some(collections);
                        b.status = if partial { "partial" } else { "ready" };
                        b.notice = partial.then_some(
                            "Some workflows could not be loaded. Retry to refresh their details.",
                        );
                    }
                    Err(_) => failed(&mut b),
                }
            }
            Err(_) => failed(&mut b),
        }
    });
}
fn failed(b: &mut Browse) {
    b.status = "failed";
    b.notice = Some("Workflows could not load. Retry to refresh this channel.");
}
fn admitted(
    state: Signal<ShellState>,
    browse: Signal<Browse>,
    owner: &Arc<Owner>,
    serial: u64,
) -> bool {
    owner.current(&state.peek())
        && browse.peek().serial == serial
        && browse
            .peek()
            .owner
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, owner))
}
async fn load(
    state: Signal<ShellState>,
    mut browse: Signal<Browse>,
    owner: &Arc<Owner>,
    serial: u64,
) -> Result<(Collections, Option<Previews>), Error> {
    let raw = request(
        state,
        browse,
        owner,
        serial,
        Read::bookmarks(owner.channel())?,
    )
    .await?;
    if !admitted(state, browse, owner, serial) {
        return Err(unavailable());
    }
    browse.write().bookmarks = Some(raw.clone());
    let collections = Collections::capture(&owner.raw, &raw)?;
    if collections.preview_ids().is_empty() {
        return Ok((collections, None));
    }
    browse.write().phase = Some("previews");
    let raw = request(
        state,
        browse,
        owner,
        serial,
        Read::previews(collections.preview_ids())?,
    )
    .await?;
    if !admitted(state, browse, owner, serial) {
        return Err(unavailable());
    }
    browse.write().previews = Some(raw.clone());
    let previews = Previews::decode(collections.preview_ids(), raw)?;
    Ok((collections, Some(previews)))
}
async fn request(
    mut state: Signal<ShellState>,
    mut browse: Signal<Browse>,
    owner: &Arc<Owner>,
    serial: u64,
    read: Read,
) -> Result<Value, Error> {
    if !admitted(state, browse, owner, serial) || cooling(&state.peek()).blocked {
        return Err(unavailable());
    }
    let request = read.request(&owner.ctx.client, &owner.ctx.session);
    let result = tokio::select! {
        result=async {
            if crate::fixture::is_fixture() {
                fixture::read(state,&request).await.unwrap_or(Err(unavailable()))
            } else {owner.ctx.transport.execute(request).await}
        }=>result,
        _=async {loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !admitted(state,browse,owner,serial) {break;}
        }}=>return Err(unavailable()),
    };
    if !admitted(state, browse, owner, serial) {
        return Err(unavailable());
    }
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        // Only this originally admitted response may mutate retained throttles.
        // Lookup later intentionally does not depend on its old Transport.
        let mut s = state.write();
        if !owner.current(&s) {
            return Err(unavailable());
        }
        if let Some(scope) = cooldown_scope(&s) {
            s.channel_workflows.record(
                scope,
                read.method(),
                *retry_after_secs,
                std::time::Instant::now(),
            );
        }
    }
    let receipt = match &result {
        Ok(raw) => json!({"serial":serial,"owner":owner.id,"method":read.method(),"raw":raw}),
        Err(Error::RateLimited { retry_after_secs }) => {
            json!({"serial":serial,"owner":owner.id,"method":read.method(),"failure":"rate-limited","retryAfterSeconds":retry_after_secs})
        }
        Err(_) => {
            json!({"serial":serial,"owner":owner.id,"method":read.method(),"failure":"read failed"})
        }
    };
    let mut b = browse.write();
    if b.reads.len() == 64 {
        b.reads.remove(0);
    }
    b.reads.push(receipt);
    result
}

fn unavailable() -> Error {
    Error::Api("channel_workflows_owner_unavailable".into())
}
