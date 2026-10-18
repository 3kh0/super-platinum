//! Three configured Rating cases share the existing creation and cell workers.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::rating as cells};
pub(super) mod creation;
mod keyboard;
mod oracle;
struct Continuation {
    pages: [Value; 5],
    sync: Value,
    creation: Value,
    find: Value,
    custom_ready: bool,
}
pub(super) fn valid_stage(stage: &str) -> bool {
    oracle::STAGES.contains(&stage)
        || [
            "quantity-one",
            "quantity-five",
            "standard-picker",
            "standard-config",
            "custom-picker",
            "custom-config",
            "custom-ready",
        ]
        .contains(&stage)
}
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Rating failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Rating(original) = e.original.as_ref() else {
        return Err("Nonrating editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::Rating
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong Rating editor identity".into());
    }
    Ok(
        json!({"generation":e.generation,"row":e.row_id,"column":e.column_id,"original":original,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error,"options":e.rating.as_ref().map(|o|o.wire())}),
    )
}
fn stable(
    state: Signal<ShellState>,
    proof: &Proof,
    c: &Continuation,
    index: usize,
) -> Result<Value, String> {
    let shell = state.peek();
    let ledger = cells::snapshot(&shell)?;
    oracle::exact(
        &ledger,
        index,
        &proof.docs[1],
        &c.pages,
        &c.sync,
        proof.mode.rating_value(),
    )?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Rating List projection")?;
    let board = matches!(index, 4 | 5 | 10..=15);
    let expected_view = if board { "V_RATING_BOARD" } else { "V_FILTER" };
    let expected_layout = if board {
        crate::slack_lists::ProjectionLayout::Board
    } else {
        crate::slack_lists::ProjectionLayout::Table
    };
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.selected_view.as_deref() != Some(expected_view)
        || p.layout_kind != expected_layout
        || p.view_notice.is_some()
        || !p.incomplete
        || p.loading
        || rename::presentation(&shell).is_some()
        || rename_fixture::snapshot(&shell)? != c.creation
        || rename_fixture::text_create_registry(&shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Rating projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    let has_editor = matches!(index,0..=2|6..=8|12..=14|16..=18);
    let saving = matches!(index, 1 | 7 | 13 | 17);
    let uncertain = matches!(index, 2 | 8 | 14 | 18);
    if has_editor {
        let original = if matches!(index,6..=8|16..=18) {
            json!(proof.mode.rating_value())
        } else {
            Value::Null
        };
        if model.is_null()
            || model["original"] != original
            || model["disabled"] != json!(saving || uncertain)
            || model["saving"] != saving
            || model["stale"] != uncertain
            || model["options"] != proof.mode.rating_options()
        {
            return Err("Rating editor phase/original custody differs".into());
        }
    } else if !model.is_null() {
        return Err("Resolved Rating editor retained".into());
    }
    let detail =
        crate::slack_lists::record_detail::presentation(shell.slack_lists.view.as_ref().unwrap());
    if detail.is_some() != matches!(index, 5 | 11..=15)
        || detail
            .as_ref()
            .is_some_and(|d| d.record_id != cells::ROW || d.unavailable)
    {
        return Err("Rating detail ownership differs".into());
    }
    let detail = detail.map(
        |d| json!({"row":d.record_id,"generation":d.generation,"openGeneration":d.open_generation}),
    );
    let media = cells::media::snapshot(&shell)?;
    if media["ready"] != c.custom_ready {
        return Err("Rating fixture media changed outside disclosed transition".into());
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != c.pages[oracle::LOADED[index]]
        || find(state)? != c.find
    {
        return Err("Rating raw population/Find changed".into());
    }
    Ok(json!({"ledger":ledger,"editor":model,"detail":detail,"media":media}))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    c: &Continuation,
    index: usize,
    custom: bool,
) -> Result<(), String> {
    let expected = 9 + index + usize::from(c.custom_ready);
    if proof.stages.len() != expected {
        return Err("Rating checkpoint order changed".into());
    }
    let name = if custom {
        "custom-ready"
    } else {
        oracle::STAGES[index]
    };
    let mut before = None;
    for _ in 0..150 {
        if let Ok(value) = stable(state, proof, c, index) {
            before = Some(value);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let before = before.ok_or("Rating controller did not settle")?;
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listRatingCells.geometry({},{},{})",
            json!(name),
            before["editor"]["generation"],
            c.custom_ready
        ),
    )
    .await?;
    if geometry["owner"]["visit"].as_str()
        != c.sync["visit"].as_u64().map(|n| n.to_string()).as_deref()
        || geometry["owner"]["selection"].as_str()
            != before["ledger"]["selection"]
                .as_u64()
                .map(|n| n.to_string())
                .as_deref()
        || geometry["owner"]["generation"].as_str()
            != before["editor"]["generation"]
                .as_u64()
                .map(|n| n.to_string())
                .as_deref()
        || stable(state, proof, c, index)? != before
    {
        return Err("Rating changed during DOM geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, c, index)? != before {
        return Err("Rating changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":proof.stages.len(),"stage":name,"geometry":geometry,"cellLedger":before["ledger"],"editor":before["editor"],"detail":before["detail"],"media":before["media"],"ledger":c.creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    original: Option<u8>,
) -> Result<u64, String> {
    checked(d, "window.__listRatingCells.open()").await?;
    for _ in 0..150 {
        let generation = editor(&state.peek())?["generation"].as_u64();
        if let Some(g) = generation {
            wait_dom(
                d,
                &format!("window.__listRatingCells.ready({},{g})", json!(original)),
            )
            .await?;
            return Ok(g);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Rating editor did not open".into())
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listRatingCells.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    let event = click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(d, &format!("window.__listRatingCells.noop({g})")).await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["original"] != Value::Null
        || model["disabled"] != false
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || cells::snapshot(&state.peek())? != before
    {
        return Err("Rating no-op changed worker/draft".into());
    }
    Ok(
        json!({"before":before,"event":event,"dom":dom,"editor":model,"after":cells::snapshot(&state.peek())?}),
    )
}
async fn choose(
    d: &dioxus::desktop::DesktopContext,
    k: u8,
    g: u64,
    want: Option<u8>,
) -> Result<Value, String> {
    let event = checked(d, &format!("window.__listRatingCells.choose({k},{g})")).await?;
    let result = wait_dom(
        d,
        &format!("window.__listRatingCells.chosen({},{g})", json!(want)),
    )
    .await?;
    Ok(json!({"event":event,"result":result}))
}
async fn write(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    c: &Continuation,
    index: usize,
    g: u64,
) -> Result<Value, String> {
    stage(state, d, proof, c, index, false).await?;
    click(d, "slack-list-editor-save", g).await?;
    stage(state, d, proof, c, index + 1, false).await?;
    let before = cells::snapshot(&state.peek())?;
    let duplicate = checked(
        d,
        &format!("window.__listRatingCells.disabledNoReplay({g})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != before {
        return Err("Disabled Rating controls replayed".into());
    }
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, c, index + 2, false).await?;
    click(d, "slack-list-editor-refresh", g).await?;
    let returned = wait_dom(d, "window.__listRatingCells.returned()").await?;
    stage(state, d, proof, c, index + 3, false).await?;
    Ok(json!({"disabled":duplicate,"returned":returned}))
}
async fn navigate(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    c: &mut Continuation,
    id: &str,
) -> Result<Value, String> {
    let event = checked(
        d,
        &format!("window.__listRatingCells.navigate({})", json!(id)),
    )
    .await?;
    let reset = wait_dom(
        d,
        &format!("window.__listRatingCells.navigationReady({})", json!(id)),
    )
    .await?;
    let before = find(state)?;
    if before != json!({"input":"","applied":"","revision":0}) {
        return Err("Actual view navigation did not reset Find".into());
    }
    let query = checked(d, "window.__listTextCreate.query()").await?;
    for _ in 0..150 {
        let f = find(state)?;
        if f["input"] == "Launch item 0"
            && f["applied"] == "Launch item 0"
            && f["revision"].as_u64().is_some_and(|n| n > 0)
        {
            c.find = f;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if find(state)? != c.find || c.find == before {
        return Err("Find did not reapply".into());
    }
    let bound = checked(d, "window.__listTextCreate.bindFind()").await?;
    let adopted = cells::adopt_view(&mut state.write(), id)?;
    let reveal = checked(
        d,
        &format!(
            "window.__listRatingCells.revealSurface({})",
            json!(if id == "V_FILTER" { "table" } else { "board" })
        ),
    )
    .await?;
    Ok(
        json!({"event":event,"reset":reset,"resetModel":before,"query":query,"find":c.find,"bound":bound,"adopted":adopted,"reveal":reveal}),
    )
}
async fn detail(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let event = checked(d, "window.__listRatingCells.openDetail()").await?;
    let ready = wait_dom(d, "window.__listRatingCells.detailReady()").await?;
    let reveal = checked(d, "window.__listRatingCells.revealSurface('detail')").await?;
    Ok(json!({"event":event,"readyBeforeReveal":ready,"reveal":reveal}))
}
async fn close_detail(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    c: &Continuation,
) -> Result<Value, String> {
    let before = find(state)?;
    if before != c.find {
        return Err("Rating Find model changed before detail close".into());
    }
    let event = checked(d, "window.__listRatingCells.closeDetail()").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-record-detail')})",
    )
    .await?;
    let rebound = wait_dom(d, "window.__listRatingCells.rebindFind()").await?;
    let after = find(state)?;
    if after != before {
        return Err("Rating Find model changed during detail close".into());
    }
    Ok(json!({"event":event,"rebound":rebound,"before":before,"after":after}))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<Value, String> {
    proof.stable(state, 1)?;
    let creation = rename_fixture::snapshot(&state.peek())?;
    let sync = cells::arm(
        &mut state.write(),
        &proof.docs[0],
        &proof.docs[1],
        &proof.population,
        proof.id,
        proof.mode,
    )?;
    let mut c = Continuation {
        pages: oracle::pages(&proof.population, proof.mode.rating_value())?,
        sync,
        creation,
        find: find(state)?,
        custom_ready: false,
    };
    checked(d, include_str!("rating/cell.js")).await?;
    let case = match proof.mode {
        FieldCreateMode::RatingDefault => "default",
        FieldCreateMode::RatingStandard => "standard",
        FieldCreateMode::RatingCustom => "custom",
        _ => return Err("Not a Rating case".into()),
    };
    checked(
        d,
        &format!("window.__listRatingCells.init({})", json!(case)),
    )
    .await?;
    let initial = checked(d, "window.__listRatingCells.initial()").await?;
    let first = open(state, d, None).await?;
    let missing_noop = noop(state, d, first).await?;
    // The no-op Save proof intentionally focuses Save. Explicitly hand focus
    // back to the owned Rating choice before admitting trusted keyboard input.
    checked(
        d,
        &format!("window.__listRatingCells.keyboardFocus({first})"),
    )
    .await?;
    let keys = keyboard::run(state, d, proof.mode, first).await?;
    let before = cells::snapshot(&state.peek())?;
    click(d, "slack-list-editor-cancel", first).await?;
    let cancel = wait_dom(d, "window.__listRatingCells.returned(true)").await?;
    if cells::snapshot(&state.peek())? != before {
        return Err("Keyboard draft Cancel changed ledger".into());
    }
    let g = open(state, d, None).await?;
    let set = choose(
        d,
        proof.mode.rating_value(),
        g,
        Some(proof.mode.rating_value()),
    )
    .await?;
    let first_write = write(state, d, proof, &c, 0, g).await?;
    let media = if proof.mode == FieldCreateMode::RatingCustom {
        let transition = cells::media::ready(&mut state.write())?;
        c.custom_ready = true;
        stage(state, d, proof, &c, 3, true).await?;
        Some(transition)
    } else {
        None
    };
    let board = navigate(state, d, &mut c, "V_RATING_BOARD").await?;
    stage(state, d, proof, &c, 4, false).await?;
    let first_detail = detail(d).await?;
    stage(state, d, proof, &c, 5, false).await?;
    let first_close = close_detail(state, d, &c).await?;
    let table = navigate(state, d, &mut c, "V_FILTER").await?;
    let g = open(state, d, Some(proof.mode.rating_value())).await?;
    let same = choose(d, proof.mode.rating_value(), g, None).await?;
    let same_clear = write(state, d, proof, &c, 6, g).await?;
    let empty_board = navigate(state, d, &mut c, "V_RATING_BOARD").await?;
    stage(state, d, proof, &c, 10, false).await?;
    let second_detail = detail(d).await?;
    stage(state, d, proof, &c, 11, false).await?;
    let g = open(state, d, None).await?;
    choose(
        d,
        proof.mode.rating_value(),
        g,
        Some(proof.mode.rating_value()),
    )
    .await?;
    let reset = write(state, d, proof, &c, 12, g).await?;
    let second_close = close_detail(state, d, &c).await?;
    let last_table = navigate(state, d, &mut c, "V_FILTER").await?;
    let g = open(state, d, Some(proof.mode.rating_value())).await?;
    let explicit = click(d, "slack-list-rating-clear", g).await?;
    wait_dom(d, &format!("window.__listRatingCells.chosen(null,{g})")).await?;
    // Delay the final cleared capture until its independent reopen/no-op/Cancel proof.
    stage(state, d, proof, &c, 16, false).await?;
    click(d, "slack-list-editor-save", g).await?;
    stage(state, d, proof, &c, 17, false).await?;
    let before = cells::snapshot(&state.peek())?;
    let clear_disabled = checked(
        d,
        &format!("window.__listRatingCells.disabledNoReplay({g})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != before {
        return Err("Disabled final Rating controls replayed".into());
    }
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &c, 18, false).await?;
    click(d, "slack-list-editor-refresh", g).await?;
    let returned = wait_dom(d, "window.__listRatingCells.returned()").await?;
    let g = open(state, d, None).await?;
    let final_noop = noop(state, d, g).await?;
    click(d, "slack-list-editor-cancel", g).await?;
    let final_return = wait_dom(d, "window.__listRatingCells.returned(true)").await?;
    stage(state, d, proof, &c, 19, false).await?;
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listRatingCells.geometry('cleared',null,{})",
            c.custom_ready
        ),
    )
    .await?;
    let final_state = stable(state, proof, &c, 19)?;
    Ok(
        json!({"case":case,"sync":c.sync,"pages":c.pages,"initial":initial,"missingNoop":missing_noop,"keyboard":keys,"cancel":cancel,"set":set,"first":first_write,"media":media,"board":board,"detail":first_detail,"detailClose":first_close,"table":table,"same":same,"sameClear":same_clear,"emptyBoard":empty_board,"emptyDetail":second_detail,"emptyDetailClose":second_close,"reset":reset,"lastTable":last_table,"explicitClear":explicit,"clearDisabled":clear_disabled,"returned":returned,"finalNoop":final_noop,"finalReturn":final_return,"geometry":geometry,"final":final_state,"creationLedger":c.creation,"find":c.find}),
    )
}

#[cfg(test)]
#[path = "rating/borrow_tests.rs"]
mod borrow_tests;
