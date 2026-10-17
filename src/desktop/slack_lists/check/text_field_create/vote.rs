//! Actual own-user Vote callbacks through existing Table/detail workers; Board readonly.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::vote as cells};
pub(super) mod creation;
mod oracle;
struct Continuation {
    pages: [Value; 5],
    sync: Value,
    creation: Value,
    find: Value,
    detail_generation: Option<u64>,
    detail_open: Option<u64>,
}
pub(super) fn valid_stage(s: &str) -> bool {
    oracle::STAGES.contains(&s) || ["vote-default", "vote-configured"].contains(&s)
}
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Vote failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn stable(
    state: Signal<ShellState>,
    proof: &Proof,
    c: &Continuation,
    index: usize,
) -> Result<Value, String> {
    let shell = state.peek();
    let ledger = cells::snapshot(&shell)?;
    oracle::exact(&ledger, index, &proof.docs[1], &c.pages, &c.sync)?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Vote projection")?;
    let board = (7..=12).contains(&index);
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.selected_view.as_deref() != Some(if board { "V_VOTE_BOARD" } else { "V_FILTER" })
        || p.layout_kind
            != if board {
                crate::slack_lists::ProjectionLayout::Board
            } else {
                crate::slack_lists::ProjectionLayout::Table
            }
        || p.view_notice.is_some()
        || !p.incomplete
        || p.loading
        || rename::presentation(&shell).is_some()
        || crate::slack_lists::editor::presentation(&shell).is_some()
        || rename_fixture::snapshot(&shell)? != c.creation
        || rename_fixture::text_create_registry(&shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Vote projection/creation isolation drift".into());
    }
    let expected_error = match index {
        1 | 4 | 9 => Some("A cell change is still being sent. Wait before checking its result."),
        2 | 5 | 10 => Some("Refresh to check this cell change before editing again."),
        3 => Some("Slack currently shows a different value. Review it before changing it again."),
        _ => None,
    };
    if p.error.as_deref() != expected_error || p.stale != matches!(index, 2 | 5 | 10) {
        return Err("Vote visible mutation notice/stale phase differs".into());
    }
    let detail =
        crate::slack_lists::record_detail::presentation(shell.slack_lists.view.as_ref().unwrap());
    if detail.is_some() != (8..=11).contains(&index)
        || detail
            .as_ref()
            .is_some_and(|d| d.record_id != cells::ROW || d.unavailable)
    {
        return Err("Vote detail ownership differs".into());
    }
    if let Some(d) = &detail {
        if c.detail_open.is_some_and(|g| d.open_generation != g)
            || c.detail_generation.is_some_and(|g| {
                if index == 11 {
                    d.generation <= g
                } else {
                    d.generation != g
                }
            })
        {
            return Err("Vote detail generation retired or did not refresh".into());
        }
        if index >= 9
            && ledger["preflights"][2]["attempt"]["authority"]
                != json!({"kind":"detail","generation":c.detail_generation})
        {
            return Err("Vote Remove did not capture actual detail generation".into());
        }
    }
    let detail = detail.map(
        |d| json!({"row":d.record_id,"generation":d.generation,"openGeneration":d.open_generation}),
    );
    let media = cells::media::snapshot(&shell)?;
    if media["ready"] != false {
        return Err("Vote fallback cohort unexpectedly obtained media".into());
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != c.pages[oracle::LOADED[index]]
        || find(state)? != c.find
    {
        return Err("Vote raw population/Find drift".into());
    }
    Ok(json!({"ledger":ledger,"detail":detail,"media":media,"error":p.error,"stale":p.stale}))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    c: &Continuation,
    index: usize,
) -> Result<(), String> {
    if proof.stages.len() != 9 + index {
        return Err("Vote stage order changed".into());
    }
    let name = oracle::STAGES[index];
    let mut settled = None;
    for _ in 0..150 {
        if let Ok(v) = stable(state, proof, c, index) {
            settled = Some(v);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let before = settled.ok_or("Vote worker did not settle at exact checkpoint")?;
    let geometry = wait_dom(
        d,
        &format!("window.__listVoteCells.geometry({})", json!(name)),
    )
    .await?;
    let owner = |v: &Value, n: Option<u64>| v.as_str() == n.map(|n| n.to_string()).as_deref();
    if !owner(&geometry["owner"]["visit"], c.sync["visit"].as_u64())
        || !owner(
            &geometry["owner"]["selection"],
            before["ledger"]["selection"].as_u64(),
        )
        || !owner(
            &geometry["owner"]["detailGeneration"],
            before["detail"]["generation"].as_u64(),
        )
        || !owner(
            &geometry["owner"]["detailOpenGeneration"],
            before["detail"]["openGeneration"].as_u64(),
        )
        || stable(state, proof, c, index)? != before
    {
        return Err("Vote changed during exact DOM ownership check".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, c, index)? != before {
        return Err("Vote changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":proof.stages.len(),"stage":name,"geometry":geometry,"cellLedger":before["ledger"],"detail":before["detail"],"media":before["media"],"error":before["error"],"stale":before["stale"],"ledger":c.creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn disabled(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    let dom = checked(d, "window.__listVoteCells.disabledNoReplay()").await?;
    if cells::snapshot(&state.peek())? != before {
        return Err("Disabled Vote replayed".into());
    }
    Ok(json!({"dom":dom,"before":before,"after":cells::snapshot(&state.peek())?}))
}
async fn navigate(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    c: &mut Continuation,
    id: &str,
) -> Result<Value, String> {
    let event = checked(
        d,
        &format!("window.__listVoteCells.navigate({})", json!(id)),
    )
    .await?;
    let reset = wait_dom(
        d,
        &format!("window.__listVoteCells.navigationReady({})", json!(id)),
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
            "window.__listVoteCells.revealSurface({})",
            json!(if id == "V_FILTER" { "table" } else { "board" })
        ),
    )
    .await?;
    Ok(
        json!({"event":event,"reset":reset,"resetModel":before,"query":query,"find":c.find,"bound":bound,"adopted":adopted,"reveal":reveal}),
    )
}
async fn detail(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let event = checked(d, "window.__listVoteCells.openDetail()").await?;
    let ready = wait_dom(d, "window.__listVoteCells.detailReady()").await?;
    let reveal = checked(d, "window.__listVoteCells.revealSurface('detail')").await?;
    Ok(json!({"event":event,"readyBeforeReveal":ready,"reveal":reveal}))
}
async fn close_detail(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let event = checked(d, "window.__listVoteCells.closeDetail()").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-record-detail')})",
    )
    .await?;
    let returned = wait_dom(d, "window.__listVoteCells.returnedDetail()").await?;
    // The Find DOM node was genuinely unmounted. Bind the observed new value/range;
    // no query event, range restoration or keyboard-focus repair occurs here.
    let rebound = checked(d, "window.__listTextCreate.bindFind()").await?;
    if rebound["value"] != returned["after"]["value"]
        || rebound["range"] != returned["after"]["range"]
    {
        return Err("Remounted Find binding differs".into());
    }
    Ok(json!({"event":event,"returnedBeforeReveal":returned,"findRebound":rebound}))
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
        pages: oracle::pages(&proof.population)?,
        sync,
        creation,
        find: find(state)?,
        detail_generation: None,
        detail_open: None,
    };
    checked(d, include_str!("vote/cell.js")).await?;
    let case = if proof.mode == FieldCreateMode::VoteCustom {
        "custom"
    } else {
        "default"
    };
    checked(d, &format!("window.__listVoteCells.init({})", json!(case))).await?;
    let initial = checked(d, "window.__listVoteCells.initial()").await?;
    stage(state, d, proof, &c, 0).await?;
    let first = checked(d, "window.__listVoteCells.toggle()").await?;
    stage(state, d, proof, &c, 1).await?;
    let first_disabled = disabled(state, d).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &c, 2).await?;
    let different = cells::different(&mut state.write())?;
    let first_refresh = checked(d, "window.__listVoteCells.refresh()").await?;
    stage(state, d, proof, &c, 3).await?;
    let retry = checked(d, "window.__listVoteCells.toggle()").await?;
    stage(state, d, proof, &c, 4).await?;
    let retry_disabled = disabled(state, d).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &c, 5).await?;
    let retry_refresh = checked(d, "window.__listVoteCells.refresh()").await?;
    stage(state, d, proof, &c, 6).await?;
    let board = navigate(state, d, &mut c, "V_VOTE_BOARD").await?;
    stage(state, d, proof, &c, 7).await?;
    let opened = detail(d).await?;
    {
        let shell = state.peek();
        let v = crate::slack_lists::record_detail::presentation(
            shell
                .slack_lists
                .view
                .as_ref()
                .ok_or("Missing detail visit")?,
        )
        .ok_or("Missing opened detail")?;
        c.detail_generation = Some(v.generation);
        c.detail_open = Some(v.open_generation);
    }
    stage(state, d, proof, &c, 8).await?;
    let remove = checked(d, "window.__listVoteCells.toggle()").await?;
    stage(state, d, proof, &c, 9).await?;
    let remove_disabled = disabled(state, d).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &c, 10).await?;
    let remove_refresh = checked(d, "window.__listVoteCells.refresh()").await?;
    stage(state, d, proof, &c, 11).await?;
    let closed = close_detail(d).await?;
    checked(d, "window.__listVoteCells.revealSurface('board')").await?;
    stage(state, d, proof, &c, 12).await?;
    let table = navigate(state, d, &mut c, "V_FILTER").await?;
    stage(state, d, proof, &c, 13).await?;
    let geometry = wait_dom(d, "window.__listVoteCells.geometry('restored-table')").await?;
    let final_state = stable(state, proof, &c, 13)?;
    Ok(
        json!({"case":case,"sync":c.sync,"pages":c.pages,"initial":initial,"first":first,"firstDisabled":first_disabled,"different":different,"firstRefresh":first_refresh,"retry":retry,"retryDisabled":retry_disabled,"retryRefresh":retry_refresh,"board":board,"detail":opened,"remove":remove,"removeDisabled":remove_disabled,"removeRefresh":remove_refresh,"closed":closed,"table":table,"geometry":geometry,"final":final_state,"creationLedger":c.creation,"find":c.find}),
    )
}
