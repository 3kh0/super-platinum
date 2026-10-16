//! Exact named disconnected Find fixture. Synthetic DOM input, no live transport.
use super::*;
use crate::slack_lists::fixture;
mod harness;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::find::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("slack-list-find-check: {receipt}");
    });
}
fn requests(state: Signal<ShellState>, opens: usize) -> Result<Value, String> {
    let shell = state.peek();
    let cells = fixture::attempt_count(&shell);
    let creation = fixture::creation_attempts(&shell);
    let deleted = crate::slack_lists::delete::fixture::attempts(&shell);
    let restored = crate::slack_lists::delete::fixture::restore_attempts(&shell);
    let opened = fixture::record_open_count(&shell);
    if !fixture::enabled(&shell)
        || cells != 0
        || creation != 0
        || deleted != 0
        || restored != 0
        || opened != opens
        || !fixture::record_open_valid(&shell)
    {
        return Err(format!(
            "Find request boundary changed: cells={cells}, create={creation}, delete={deleted}, restore={restored}, opened={opened}, expected={opens}"
        ));
    }
    Ok(
        json!({"find":0,"cell":cells,"create":creation,"delete":deleted,"restore":restored,"recordOpen":opened,"transport":false,"session":false}),
    )
}
fn source(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .ok_or("Missing loaded Find source")?;
    Ok(json!({"document":loaded.document.raw,"records":loaded.records.raw}))
}
async fn settled(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let view =
        crate::slack_lists::presentation(&state.peek()).ok_or("Missing current Find projection")?;
    let rows = if view.layout_kind == crate::slack_lists::ProjectionLayout::Board {
        view.lanes
            .iter()
            .take(4)
            .flat_map(|lane| lane.rows.iter().take(10).map(|r| r.id.clone()))
            .collect::<Vec<_>>()
    } else {
        view.entries
            .iter()
            .take(50)
            .filter_map(|entry| match entry {
                crate::slack_lists::TableEntry::Row(row) => Some(row.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    wait_dom(d,&format!("(()=>{{const input=document.getElementById('slack-list-find-input'),rows=[...document.querySelectorAll('.slack-list-layout [data-row-id]')].map(r=>r.dataset.rowId),status=document.querySelector('.slack-list-pagination')?.textContent;return {{ok:input?.value==={}&&JSON.stringify(rows)===JSON.stringify({})&&status?.trim().startsWith({}),rows,status}};}})()",json!(view.find_applied),json!(rows),json!(format!("{} item",view.matched_rows)))).await?;
    Ok(())
}
async fn query(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    focus: bool,
    opens: usize,
) -> Result<(), String> {
    requests(state, opens)?;
    checked(
        d,
        &format!("window.__listFind.input({},{focus})", json!(text)),
    )
    .await?;
    applied(state, d, text, focus, opens).await
}
async fn applied(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    focus: bool,
    opens: usize,
) -> Result<(), String> {
    for _ in 0..80 {
        if crate::slack_lists::presentation(&state.peek())
            .is_some_and(|v| v.find_applied == text && v.find_input == text)
        {
            requests(state, opens)?;
            settled(state, d).await?;
            if focus {
                wait_dom(d,"({ok:document.activeElement===document.getElementById('slack-list-find-input')})").await?;
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Find query did not apply: {text:?}"))
}
async fn pending(state: Signal<ShellState>, text: &str) -> Result<Value, String> {
    // Fail if the debounce already applied; never claim retirement of unseen work.
    for _ in 0..10 {
        let p = crate::slack_lists::presentation(&state.peek())
            .ok_or("Missing pending Find projection")?;
        if p.find_applied == text {
            return Err("Find debounce applied before pending precondition was observed".into());
        }
        if p.find_input == text {
            return Ok(
                json!({"input":p.find_input,"applied":p.find_applied,"revision":p.find_revision,"visit":p.generation}),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Err("Find input never reached native pending state".into())
}
async fn stage(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    geometry.push(wait_dom(d, &format!("window.__listFind.geometry({})", json!(name))).await?);
    captures.insert(name.into(), harness::capture(d, name).await?);
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    requests(state, 0)?;
    let board = owner["fixture"].as_str().unwrap().contains("board");
    let initial = source(state)?;
    checked(d, include_str!("find.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-find-input')&&document.querySelector('.slack-list-dialog')?.dataset.findMounted==='true'})").await?;
    checked(d, "window.__listFind.shortcut()").await?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    stage(d, "initial", &mut captures, &mut geometry).await?;
    // Latest raw query wins; no trim and no API activity during debounce.
    query(state, d, "71", true, 0).await?;
    checked(
        d,
        "(()=>{window.__listFind.input('not-latest');return window.__listFind.input('72');})()",
    )
    .await?;
    query(state, d, "72", true, 0).await?;
    let p = crate::slack_lists::presentation(&state.peek()).ok_or("Missing Find presentation")?;
    if p.matched_rows != 2
        || p.total_rows != 73
        || !p.incomplete
        || !p.rows.iter().any(|r| r.id == "R_LIST_072")
        || p.columns.iter().any(|c| c.id == "C_NUMBER")
    {
        return Err("Hidden full-schema match or incomplete count changed".into());
    }
    stage(d, "filtered", &mut captures, &mut geometry).await?;
    query(state, d, " LÁUNCH ", true, 0).await?;
    if crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .matched_rows
        != 0
    {
        return Err("Find trimmed a literal query".into());
    }
    stage(d, "empty", &mut captures, &mut geometry).await?;
    checked(d,"(()=>{document.getElementById('slack-list-find-input').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()").await?;
    applied(state, d, "", true, 0).await?;
    query(state, d, "Láunch", true, 0).await?;
    if crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .matched_rows
        < 60
    {
        return Err("Find accent normalization did not match".into());
    }
    checked(d, "window.__listFind.click('#slack-list-find-clear')").await?;
    applied(state, d, "", true, 0).await?;
    if board {
        wait_dom(d,"({ok:document.querySelectorAll('.slack-list-lane').length===4&&!!document.querySelector('.slack-list-lane-next:not(:disabled)')})").await?;
        checked(d, "window.__listFind.click('.slack-list-lane-next')").await?;
        wait_dom(d,"({ok:!document.querySelector('.slack-list-lane-previous')?.disabled&&!document.querySelector('.slack-list-card[data-row-id=\"R_LIST_000\"]')})").await?;
    } else {
        checked(d, "window.__listFind.click('#slack-list-next')").await?;
        wait_dom(d,"({ok:document.querySelector('.slack-list-grid tbody tr')?.getAttribute('aria-rowindex')==='52'})").await?;
    }
    stage(d, "paged", &mut captures, &mut geometry).await?;
    query(state, d, "Launch", true, 0).await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('.slack-list-layout [data-row-id=\"R_LIST_000\"]')})",
    )
    .await?;
    if board {
        checked(d, "window.__listFind.click('#slack-list-lanes-next')").await?;
        wait_dom(
            d,
            "({ok:document.querySelector('.slack-list-lane')?.dataset.groupKey==='OPT_FIND_4'})",
        )
        .await?;
        query(state, d, "Launch ", true, 0).await?;
        wait_dom(d,"({ok:document.querySelector('.slack-list-lane')?.dataset.groupKey==='OPT_FIND_0'&&document.querySelector('.slack-list-lane-previous')?.disabled})").await?;
        checked(d, "window.__listFind.click('#slack-list-lanes-next')").await?;
    } else {
        checked(d, "window.__listFind.click('#slack-list-next')").await?;
    }
    query(state, d, "", true, 0).await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('.slack-list-layout [data-row-id=\"R_LIST_000\"]')})",
    )
    .await?;
    stage(d, "page-reset", &mut captures, &mut geometry).await?;

    checked(d, "window.__listFind.beginDraft()").await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rich-draft')?.contentEditable==='true'})",
    )
    .await?;
    checked(d, "window.__listFind.changeDraft()").await?;
    stage(d, "draft-edited", &mut captures, &mut geometry).await?;
    query(state, d, "72", false, 0).await?;
    wait_dom(d, "window.__listFind.draft(true)").await?;
    stage(d, "draft-hidden", &mut captures, &mut geometry).await?;
    query(state, d, "", false, 0).await?;
    wait_dom(d, "window.__listFind.draft(false)").await?;
    stage(d, "draft-restored", &mut captures, &mut geometry).await?;
    checked(d, "window.__listFind.undoDraft()").await?;
    checked(d, "window.__listFind.redoDraft()").await?;
    checked(d, "window.__listFind.click('#slack-list-editor-cancel')").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-cell-editor')})",
    )
    .await?;
    if source(state)? != initial {
        return Err("Find or discarded editor mutated canonical source".into());
    }

    let original_user = state.peek().core.workspaces["T1"].users["U1"].clone();
    let rt = state.peek().core.workspaces["T1"].rt_generation;
    {
        let mut shell = state.write();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .get_mut("U1")
            .unwrap()
            .extra
            .insert("_real_name_normalized_lc".into(), json!("find-name-before"));
    }
    query(state, d, "find-name-before", true, 0).await?;
    if crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .matched_rows
        < 60
    {
        return Err("Hidden user Find did not match".into());
    }
    let mut changed = original_user.clone();
    changed
        .extra
        .insert("_real_name_normalized_lc".into(), json!("find-name-after"));
    let event = super_platinum_core::slack::events::RtEvent::UserChanged(changed.clone());
    let change = crate::slack_lists::labels::prepare(&state.peek(), "T1", rt, &event)
        .ok_or("Missing Find metadata dependency")?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("U1".into(), changed);
    if !change.apply(&mut state.write())
        || crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .matched_rows
            != 0
    {
        return Err("Normalized-name-only update did not reproject Find".into());
    }
    settled(state, d).await?;
    stage(d, "metadata-change", &mut captures, &mut geometry).await?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("U1".into(), original_user);
    query(state, d, "72", true, 0).await?;
    fixture::find::change_backend(&mut state.write(), 1)?;
    fixture::refresh(&mut state.write());
    if crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .rows
        .iter()
        .any(|row| row.id == "R_LIST_072")
    {
        return Err("Changed hidden number remained a Find match".into());
    }
    fixture::find::change_backend(&mut state.write(), 2)?;
    fixture::refresh(&mut state.write());
    if crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .matched_rows
        != 0
    {
        return Err("Changed schema retained excluded Find kind".into());
    }
    settled(state, d).await?;
    stage(d, "refresh-change", &mut captures, &mut geometry).await?;
    fixture::find::change_backend(&mut state.write(), 0)?;
    fixture::refresh(&mut state.write());
    query(state, d, "", true, 0).await?;

    checked(d, "window.__listFind.click('#slack-list-rename-view')").await?;
    wait_dom(d,"({ok:document.activeElement?.id==='slack-list-rename-name'&&!!document.getElementById('slack-list-find-input')?.getClientRects().length})").await?;
    checked(d,"(()=>{const before=document.activeElement;before.dispatchEvent(new KeyboardEvent('keydown',{key:'f',ctrlKey:true,bubbles:true,cancelable:true}));return {ok:document.activeElement===before&&before.id==='slack-list-rename-name'&&!!document.getElementById('slack-list-find-input')?.getClientRects().length};})()").await?;
    stage(d, "modal-boundary", &mut captures, &mut geometry).await?;
    checked(d, "window.__listFind.click('#slack-list-rename-close')").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-rename')})").await?;
    checked(
        d,
        "window.__listFind.click('[data-row-id=\"R_LIST_000\"] .slack-list-record-open')",
    )
    .await?;
    record_detail::wait_record_opens(state, 1).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-record-detail')&&!document.getElementById('slack-list-find-input')})").await?;
    checked(d, "window.__listFind.click('#slack-list-record-delete')").await?;
    wait_dom(
        d,
        "({ok:document.activeElement?.id==='slack-list-delete-cancel'})",
    )
    .await?;
    checked(d,"(()=>{const before=document.activeElement;before.dispatchEvent(new KeyboardEvent('keydown',{key:'f',ctrlKey:true,bubbles:true,cancelable:true}));return {ok:document.activeElement===before&&before.id==='slack-list-delete-cancel'};})()").await?;
    stage(d, "detail-boundary", &mut captures, &mut geometry).await?;
    checked(d, "window.__listFind.click('#slack-list-delete-cancel')").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-delete-confirm')})",
    )
    .await?;
    checked(d, "window.__listFind.click('#slack-list-record-back')").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-find-input')&&!document.getElementById('slack-list-record-detail')})").await?;
    requests(state, 1)?;
    checked(d, "window.__listFind.input('late-query')").await?;
    let pending_view = pending(state, "late-query").await?;
    checked(d, "window.__listFind.choose('')").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-find-input')&&document.querySelector('table')?.getAttribute('role')==='treegrid'})").await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let p = crate::slack_lists::presentation(&state.peek()).unwrap();
    if p.selected_view.is_some() || !p.find_input.is_empty() || !p.find_applied.is_empty() {
        return Err("Late query crossed view selection".into());
    }
    stage(d, "fallback", &mut captures, &mut geometry).await?;
    checked(d, "window.__listFind.choose('V_LIST_REORDERED')").await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('slack-list-find-input')})",
    )
    .await?;
    checked(d, "window.__listFind.input('late-close')").await?;
    let pending_visit = pending(state, "late-close").await?;
    let visit = crate::slack_lists::generation(&state.peek()).unwrap();
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(initial["document"].clone()).map_err(|e| e.to_string())?;
    crate::slack_lists::close_shell(&mut state.write());
    if !crate::slack_lists::open_shell(&mut state.write(), file) {
        return Err("Fixture reopen failed".into());
    }
    fixture::refresh(&mut state.write());
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let p = crate::slack_lists::presentation(&state.peek()).unwrap();
    if p.generation == visit || !p.find_input.is_empty() || !p.find_applied.is_empty() {
        return Err("Late query crossed visit".into());
    }
    wait_dom(d,"({ok:!!document.getElementById('slack-list-find-input')&&document.querySelector('.slack-list-dialog')?.dataset.findMounted==='true'})").await?;
    checked(d, "window.__listFind.shortcut()").await?;
    let final_requests = requests(state, 1)?;
    if source(state)? != initial {
        return Err("Final source differs from original fixture".into());
    }
    stage(d, "final", &mut captures, &mut geometry).await?;
    Ok(
        json!({"ok":true,"owner":owner,"board":board,"syntheticInput":true,"sourceKept":true,"requests":final_requests,"pending":[pending_view,pending_visit],"checks":{"fullSchema":true,"literalAccent":true,"latestQuery":true,"pageReset":true,"draftRetained":true,"selectionRetained":true,"undoRedo":true,"modalBoundary":true,"pendingObserved":true,"metadata":true,"refresh":true,"detailBoundary":true,"viewBoundary":true,"visitBoundary":true},"captures":captures,"geometry":geometry}),
    )
}
