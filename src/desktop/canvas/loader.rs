//! Paired canonical/export reads. Export content never grants edit authority.
use super::*;
use super_platinum_core::slack::api::CanvasCollabSnapshot;

type Canonical = Result<Arc<CanvasCollabSnapshot>, &'static str>;
type Export = Result<(File, CanvasDocument), &'static str>;
pub(super) mod fixture;

pub(super) async fn load(
    mut state: Signal<ShellState>,
    mut reader: Signal<Reader>,
    target: CanvasTarget,
) {
    if comments::annotation::fixture::inline_requested() {
        comments::annotation::fixture::refresh_reader(&mut state.write(), &mut reader.write());
        return;
    }
    if text::split_recovery_fixture::enabled(&state.peek()) {
        text::split_recovery_fixture::reload(state, reader).await;
        return;
    }
    if text::structure_fixture::enabled(&state.peek()) {
        let mut shell = state.write();
        text::structure_fixture::refresh(&mut shell);
        checklist::reader::update_mapping(&shell, &mut reader.write());
        return;
    }
    if text::underline_fixture::enabled(&state.peek()) {
        let mut shell = state.write();
        text::underline_fixture::refresh(&mut shell);
        checklist::reader::update_mapping(&shell, &mut reader.write());
        return;
    }
    if text::heading_fixture::enabled(&state.peek()) {
        let mut shell = state.write();
        text::heading_fixture::refresh(&mut shell);
        checklist::reader::update_mapping(&shell, &mut reader.write());
        return;
    }
    if text::fixture::enabled(&state.peek()) {
        let mut shell = state.write();
        text::fixture::refresh(&mut shell);
        checklist::reader::update_mapping(&shell, &mut reader.write());
        return;
    }
    if checklist::fixture::enabled(&state.peek()) {
        let mut shell = state.write();
        checklist::fixture::refresh(&mut shell);
        checklist::reader::update_mapping(&shell, &mut reader.write());
        return;
    }
    let read = {
        let mut shell = state.write();
        if shell
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.id != target.id)
            || checklist::presentation(&shell).is_none_or(|view| {
                view.operation
                    .is_some_and(|(_, _, stage)| stage.blocks_refresh())
            })
        {
            return;
        }
        checklist::begin_read(&mut shell)
    };
    if let Some(read) = read {
        read_pair(state, reader, read).await;
    }
}

pub(super) async fn read_pair(
    mut state: Signal<ShellState>,
    mut reader: Signal<Reader>,
    read: checklist::Read,
) {
    checklist::recovery::load(state, &read).await;
    let destination =
        comments::fixture::canonical_files::destination_canvas::enabled(&state.peek(), &read.file)
            && read.current(&state.peek());
    let auto_refresh = comments::fixture::canonical_files::auto_refresh::enabled(&state.peek())
        && read.current(&state.peek());
    let return_body =
        comments::fixture::canonical_files::return_body::enabled(&state.peek(), &read.file)
            && read.current(&state.peek());
    let context = read.request(&state.peek());
    if !destination && !return_body && !auto_refresh && context.is_none() {
        if checklist::finish_read(
            &mut state.write(),
            &read,
            Err("Connect to your workspace to read this canvas."),
        ) {
            reader.write().error = Some("Connect to your workspace to read this canvas.".into());
        }
        return;
    }
    let generation = {
        let mut r = reader.write();
        r.generation = r.generation.wrapping_add(1);
        r.loading = true;
        r.error = None;
        r.generation
    };
    // Neither half spawns a detached task or installs state independently.
    // The event effect stays busy until both bounded requests have settled.
    let (canonical, export) = if auto_refresh {
        match comments::fixture::canonical_files::auto_refresh::read_pair(state, &read).await {
            Ok((canonical, export)) => (Ok(canonical), Ok(export)),
            Err(error) => (Err(error.clone()), Err(error)),
        }
    } else if return_body {
        match comments::fixture::canonical_files::return_body::read_pair(state, &read).await {
            Ok((canonical, export)) => (Ok(canonical), Ok(export)),
            Err(error) => (Err(error.clone()), Err(error)),
        }
    } else if destination {
        match comments::fixture::canonical_files::destination_canvas::read_pair(state, &read).await
        {
            Ok((canonical, export)) => (Ok(canonical), Ok(export)),
            Err(error) => (Err(error.clone()), Err(error)),
        }
    } else if crate::slack_lists::comments::fixture::file_navigation::named() {
        match crate::slack_lists::comments::fixture::file_navigation::canvas_read_pair(
            state, &read.file,
        )
        .await
        {
            Ok((canonical, export)) => (Ok(canonical), Ok(export)),
            Err(error) => (Err(error.clone()), Err(error)),
        }
    } else {
        let ctx = context.expect("live Canvas read requires its original context");
        tokio::join!(
            api::fetch_canvas_collab(&ctx.transport, &ctx.client, &ctx.session, &read.file),
            api::fetch_canvas(&ctx.transport, &ctx.client, &ctx.session, &read.file),
        )
    };
    let canonical = canonical.map(Arc::new).map_err(|error| {
        if error.is_offline() {
            "Reconnect to refresh canvas state. The current view is still here."
        } else {
            "Canvas state could not refresh. Try again or open it in Slack."
        }
    });
    let export = export.map_err(|error| {
        if error.is_offline() {
            "Reconnect to load this canvas. Your current view is still here."
        } else {
            "This canvas could not load. Try again or open it in Slack."
        }
    });
    {
        let mut shell = state.write();
        // Do not access a destroyed/replacement component's Reader after awaiting.
        if !read.owns_view(&shell) {
            return;
        }
        finish(
            &mut shell,
            &mut reader.write(),
            &read,
            generation,
            canonical,
            export,
        );
    }
    checklist::recovery::settle(state, &read).await;
}

fn finish(
    shell: &mut ShellState,
    reader: &mut Reader,
    read: &checklist::Read,
    generation: u64,
    canonical: Canonical,
    export: Export,
) -> bool {
    if !read.owns_view(shell) || reader.generation != generation {
        return false;
    }
    let canonical = canonical.and_then(|snapshot| {
        if snapshot.file_id() == read.file {
            Ok(snapshot)
        } else {
            Err("The canvas identity changed. Refresh to check it.")
        }
    });
    let export = export.and_then(|(file, document)| {
        if file.id.as_deref() == Some(read.file.as_str()) {
            Ok((file, document))
        } else {
            Err("The canvas identity changed. Refresh to check it.")
        }
    });
    let error = canonical
        .as_ref()
        .err()
        .copied()
        .or_else(|| export.as_ref().err().copied());
    if error.is_some() {
        // A 429/error in either half stops event-driven recovery, including
        // when the response's revision was invalidated while it was in flight.
        checklist::event_reads::block_after_error(shell, read);
    }
    let authoritative = reader.finish_export(generation, read.current(shell)) && error.is_none();
    let canonical = match error {
        Some(error) => Err(error),
        None => canonical,
    };
    if checklist::finish_read(shell, read, canonical) {
        if let Some(attempt) = checklist::pending_attempt(shell) {
            checklist::confirm_readback(shell, &attempt, read);
        }
    }
    reader.error = error.map(str::to_owned);
    reader.editable = false;
    if let Ok((file, document)) = export {
        // During an event stream, a first-load export is still useful as a
        // read-only preview. Keep existing content on later invalidated reads.
        // No canonical snapshot or editable permission comes from this preview.
        if authoritative || reader.document.is_none() {
            reader.title = Some(super_platinum_core::state::file_title(&file));
            reader.editable = authoritative
                && file
                    .extra
                    .get("editable")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
            reader.document = Some(Arc::new(document));
        }
    }
    checklist::reader::update_mapping(shell, reader);
    true
}

#[cfg(test)]
mod tests;
