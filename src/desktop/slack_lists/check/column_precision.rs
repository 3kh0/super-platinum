//! Explicit disconnected Number-format workflow through the mounted controls.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
mod oracle;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !rename_fixture::precision_check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e,
                "failedLedger":rename_fixture::snapshot(&state.peek()).ok(),
                "failedDocument":document(state).ok(),"failedPopulation":population(state).ok()}),
        };
        // Keep large terminal receipt formatting and stderr I/O off the renderer.
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let line = format!("slack-list-number-precision-check: {receipt}\n");
            std::io::stderr()
                .lock()
                .write_all(line.as_bytes())
                .expect("failed to write Number precision terminal receipt to stderr");
        })
        .await
        .expect("Number precision terminal receipt worker failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|v| v.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Precision did not reach {want:?}"))
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("No loaded List".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("No loaded List".into())
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("No held {key}"))
}
struct Proof {
    before: Value,
    expected: Value,
    population: Value,
    serial: Option<u64>,
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
}
impl Proof {
    async fn stage(
        &mut self,
        state: Signal<ShellState>,
        d: &dioxus::desktop::DesktopContext,
        name: &str,
    ) -> Result<(), String> {
        let ordinal = self.stages.len();
        if oracle::STAGES.get(ordinal) != Some(&name) {
            return Err("Wrong precision stage order".into());
        }
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::ledger(&ledger, ordinal, &self.before, &self.expected, self.serial)?;
        let projection =
            crate::slack_lists::presentation(&state.peek()).ok_or("No current projection")?;
        let layout = match projection.layout_kind {
            crate::slack_lists::ProjectionLayout::Table => "table",
            crate::slack_lists::ProjectionLayout::Board => "board",
        };
        let selected = if ordinal == 9 {
            "V_PRECISION_BOARD"
        } else {
            "V_PRECISION"
        };
        if projection.selected_view.as_deref() != Some(selected)
            || projection.view_notice.is_some()
            || layout != if ordinal == 9 { "board" } else { "table" }
        {
            return Err("Precision saved view fell back or changed layout".into());
        }
        let expected = if ordinal >= 7 {
            &self.expected
        } else {
            &self.before
        };
        if document(state)? != *expected || population(state)? != self.population {
            return Err(format!("Schema or raw population drift at {name}"));
        }
        if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
            return Err("Fixture acquired a live request context".into());
        }
        let geometry = wait_dom(
            d,
            &format!("window.__listPrecision.geometry({})", json!(name)),
        )
        .await?;
        self.captures
            .insert(name.into(), harness::capture(d, name).await?);
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"ledger":ledger,
            "document":document(state)?,"population":population(state)?,"noSession":true,"noTransport":true}));
        Ok(())
    }
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("column_precision.js")).await?;
    wait_dom(d,"({ok:!!document.querySelector('button[data-precision-column=\"C_NUMBER\"]')&&!document.getElementById('slack-list-rename')})").await?;
    let before = document(state)?;
    let mut expected = before.clone();
    let column = expected["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or("No schema")?
        .iter_mut()
        .find(|v| v["id"] == "C_NUMBER" && v["key"] == "estimate")
        .ok_or("No Number field")?;
    if column["type"] != "number" || column["options"] != json!({"precision":0}) {
        return Err("Wrong initial Number options".into());
    }
    column["options"] = json!({"precision":2});
    let initial = population(state)?;
    if initial["records"][5]["fields"][2]["number"] != json!([12.5])
        || initial["records"][11]["parent_record_id"] != "R_LIST_010"
        || initial["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong fixture raw population".into());
    }
    let mut proof = Proof {
        before,
        expected,
        population: initial,
        serial: None,
        stages: vec![],
        captures: Default::default(),
    };
    checked(d, "window.__listPrecision.revealNumber('table')").await?;
    proof.stage(state, d, "initial").await?;
    checked(
        d,
        "window.__listPrecision.click('button[data-precision-column=\"C_NUMBER\"]')",
    )
    .await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-precision')===document.activeElement})",
    )
    .await?;
    checked(
        d,
        "window.__listPrecision.select('#slack-list-precision','2')",
    )
    .await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-precision')?.value==='2'&&!document.getElementById('slack-list-rename-confirm')?.disabled})").await?;
    let p = rename::presentation(&state.peek()).ok_or("No precision draft")?;
    if p.precision
        .as_ref()
        .map(|v| (v.original, v.desired, v.current))
        != Some((0, 2, Some(0)))
    {
        return Err("Native selection did not update typed precision input".into());
    }
    let serial = p.serial;
    proof.serial = Some(serial);
    proof.stage(state, d, "editing").await?;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-confirm')",
    )
    .await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    proof.stage(state, d, "uncertain").await?;
    rename_fixture::stale_precision_file(&state.peek())?;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-check')",
    )
    .await?;
    phase(state, Stage::Conflict).await?;
    proof.stage(state, d, "conflict").await?;
    rename_fixture::hold_read(&state.peek())?;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-check')",
    )
    .await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    proof.stage(state, d, "checking").await?;
    let old_visit = crate::slack_lists::generation(&state.peek()).ok_or("No initial visit")?;
    let old_epoch = state.peek().core.account_epoch;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-leave')",
    )
    .await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch += 1;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    if new_visit == old_visit {
        return Err("Reopen reused visit".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'}})",new_visit)).await?;
    rename_fixture::release(&state.peek(), true)?;
    phase(state, Stage::Uncertain).await?;
    crate::slack_lists::fixture::refresh(&mut state.write());
    wait_dom(
        d,
        "({ok:!!document.getElementById('slack-list-rename-check')})",
    )
    .await?;
    let p = rename::presentation(&state.peek()).ok_or("Lost retained precision request")?;
    if p.serial != serial || p.precision.as_ref().map(|v| v.desired) != Some(2) {
        return Err("Retained precision identity changed".into());
    }
    proof.stage(state, d, "stale").await?;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-check')",
    )
    .await?;
    phase(state, Stage::Observed).await?;
    proof.stage(state, d, "observed").await?;
    checked(d, "window.__listPrecision.rememberHeader()").await?;
    checked(
        d,
        "window.__listPrecision.click('#slack-list-rename-close')",
    )
    .await?;
    wait_dom(d, "window.__listPrecision.returnedHeader()").await?;
    checked(d, "window.__listPrecision.revealNumber('table')").await?;
    proof.stage(state, d, "table").await?;
    checked(
        d,
        "window.__listPrecision.select('#slack-list-view-select','V_PRECISION_BOARD')",
    )
    .await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('.slack-list-card[data-row-id=\"R_LIST_005\"]')})",
    )
    .await?;
    checked(d, "window.__listPrecision.revealNumber('board')").await?;
    proof.stage(state, d, "board").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::ledger(&ledger, 9, &proof.before, &proof.expected, Some(serial))?;
    if rename::presentation(&state.peek()).is_some() {
        return Err("Closed review reappeared".into());
    }
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],
        "preparedRequests":11,"writeRequests":1,"readRequests":10,"noSession":true,"noTransport":true,
        "syntheticInput":true,"actualBuilderDecoderReducers":true,"explicitConfirmation":true,
        "lostAckRetained":true,"conflictingReadback":true,"lateReadRejected":true,"exactReadback":true,
        "partialPopulationPreserved":true,"noMutationReplay":true,"before":proof.before,"after":document(state)?,
        "initialPopulation":proof.population,"finalPopulation":population(state)?,"serial":serial,
        "visitGenerations":[old_visit,new_visit],"accountEpochs":[old_epoch,state.peek().core.account_epoch],
        "stages":proof.stages,"captures":proof.captures,"ledger":ledger,"viewport":eval(d,"[innerWidth,innerHeight]").await?}),
    )
}
