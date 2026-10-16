//! Eight captures, three explicit saved-view sort changes, one real retained registry path.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
mod oracle;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !rename_fixture::sort_check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"failedLedger":rename_fixture::snapshot(&state.peek()).ok(),"failedDocument":document(state).ok(),"failedPopulation":population(state).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-view-sort-check: {receipt}\n").as_bytes())
                .expect("Sort terminal output failed");
        })
        .await
        .expect("Sort terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Sort did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held sort {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let ledger = rename_fixture::snapshot(&state.peek())?;
        match ledger["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(ledger),
            Some(n) if n < count => {}
            _ => return Err("Sort completion count exceeded expected checkpoint".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Sort owner completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing sort document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing sort population".into())
}
fn find(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let v = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    Ok(
        json!({"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}),
    )
}

fn choice(value: &super_platinum_core::slack_lists::ViewSortChoice) -> Value {
    use super_platinum_core::slack_lists::ViewSortChoice;
    match value {
        ViewSortChoice::None => json!({"column":null,"ascending":null}),
        ViewSortChoice::Number {
            column_id,
            ascending,
        } => json!({"column":column_id,"ascending":ascending}),
    }
}
fn draft(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = rename::presentation(&shell).ok_or("Missing Sort presentation")?;
    let s = p.sort.ok_or("Missing typed Sort choice")?;
    if p.view.as_deref() != Some("V_SORT")
        || s.choice == 0
        || p.precision.is_some()
        || p.visibility.is_some()
    {
        return Err("Wrong Sort presentation".into());
    }
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&shell),"selection":rename::selection(&shell),"choice":s.choice,"original":choice(&s.original),"desired":choice(&s.desired),"current":s.current.as_ref().map(choice)}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(
        d,
        &format!("window.__listViewSort.click({})", json!(format!("#{id}"))),
    )
    .await
    .map(|_| ())
}
async fn open_choice(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: usize,
) -> Result<Value, String> {
    checked(d, "window.__listViewSort.rememberSort()").await?;
    click(d, "slack-list-sort").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-sort-field')===document.activeElement})",
    )
    .await?;
    let old = draft(state)?;
    let field = if mode == 0 { "" } else { "C_NUMBER" };
    let selected = checked(
        d,
        &format!(
            "window.__listViewSort.select('#slack-list-sort-field',{})",
            json!(field)
        ),
    )
    .await?;
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-sort-field')?.value==={}}})",
            json!(field)
        ),
    )
    .await?;
    // A field change rotates the typed revision. Wait for the actual rerender
    // before dispatching direction, so the callback captures the new choice.
    if old["desired"]["column"] != json!(if mode == 0 { None } else { Some("C_NUMBER") }) {
        wait_dom(d,&format!("({{ok:document.getElementById('slack-list-sort-field')?.dataset.sortChoice!=={}}})",json!(old["choice"].as_u64().unwrap().to_string()))).await?;
    }
    let direction = if mode == 2 { "descending" } else { "ascending" };
    if mode != 0 {
        checked(
            d,
            &format!(
                "window.__listViewSort.select('#slack-list-sort-direction',{})",
                json!(direction)
            ),
        )
        .await?;
    }
    wait_dom(d,"({ok:!!document.getElementById('slack-list-rename-confirm')&&!document.getElementById('slack-list-rename-confirm').disabled})").await?;
    let current = draft(state)?;
    let desired = if mode == 0 {
        json!({"column":null,"ascending":null})
    } else {
        json!({"column":"C_NUMBER","ascending":mode==1})
    };
    if current["desired"] != desired {
        return Err("Wrong desired sort after actual select events".into());
    }
    let dom = checked(d, "window.__listViewSort.draft()").await?;
    if dom["serial"].as_str() != current["serial"].as_u64().map(|n| n.to_string()).as_deref()
        || dom["choice"].as_str() != current["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || dom["field"] != field
        || dom["direction"] != direction
    {
        return Err("Rendered sort callback choice drift".into());
    }
    Ok(json!({"model":current,"dom":dom,"selected":selected}))
}
struct Proof {
    docs: [Value; 3],
    population: Value,
    find: Value,
    ids: [u64; 3],
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
}
impl Proof {
    fn stable(&self, state: Signal<ShellState>, mode: usize) -> Result<(), String> {
        let shell = state.peek();
        let p = crate::slack_lists::presentation(&shell).ok_or("Missing List projection")?;
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || p.selected_view.as_deref() != Some("V_SORT")
            || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
            || p.view_notice.is_some()
            || !p.incomplete
        {
            return Err("Sort owner/layout/partial isolation failed".into());
        }
        let ids = p
            .entries
            .iter()
            .filter_map(|e| match e {
                crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if ids != oracle::order(mode) {
            return Err(format!("Wrong projected Sort order: {ids:?}"));
        }
        drop(shell);
        if document(state)? != self.docs[mode]
            || population(state)? != self.population
            || find(state)? != self.find
        {
            return Err("Raw metadata/population/active Find drift".into());
        }
        Ok(())
    }
    async fn stage(
        &mut self,
        state: Signal<ShellState>,
        d: &dioxus::desktop::DesktopContext,
        name: &str,
    ) -> Result<(), String> {
        let ordinal = self.stages.len();
        if oracle::STAGES.get(ordinal) != Some(&name) {
            return Err("Wrong Sort stage order".into());
        }
        let mode = [0, 0, 0, 0, 1, 2, 2, 0][ordinal];
        self.stable(state, mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage(&ledger, ordinal, &self.docs, self.ids)?;
        if matches!(ordinal, 4 | 5 | 7) {
            checked(d, "window.__listViewSort.revealFirst()").await?;
        }
        let geometry = wait_dom(
            d,
            &format!("window.__listViewSort.geometry({})", json!(name)),
        )
        .await?;
        let model = if matches!(ordinal, 4 | 5 | 7) {
            Value::Null
        } else {
            draft(state)?
        };
        let owner = |v: &Value, n: Option<u64>| v.as_str() == n.map(|n| n.to_string()).as_deref();
        if !owner(
            &geometry["owner"]["visit"],
            crate::slack_lists::generation(&state.peek()),
        ) || !owner(
            &geometry["owner"]["selection"],
            rename::selection(&state.peek()),
        ) || !model.is_null()
            && (!owner(&geometry["owner"]["serial"], model["serial"].as_u64())
                || !owner(&geometry["draft"]["choice"], model["choice"].as_u64())
                || geometry["owner"]["view"] != "V_SORT")
        {
            return Err("Rendered Sort owner differs from actual state".into());
        }
        self.captures
            .insert(name.into(), harness::capture(d, name).await?);
        self.stable(state, mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Sort changed during capture".into());
        }
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"draft":model,"noSession":true,"noTransport":true}));
        Ok(())
    }
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("view_sort.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-sort')&&!document.getElementById('slack-list-rename')})").await?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial Find".into());
    }
    checked(d, "window.__listViewSort.query()").await?;
    let mut applied = false;
    for _ in 0..100 {
        let f = find(state)?;
        if f["input"] == "Launch item 0"
            && f["applied"] == f["input"]
            && f["revision"].as_u64().is_some_and(|r| r > 0)
        {
            applied = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if !applied {
        return Err("Actual Find debounce did not apply".into());
    }
    let first_find = checked(d, "window.__listViewSort.bindFind()").await?;
    let before = document(state)?;
    let population = population(state)?;
    if before["list_metadata"]["views"][0]["id"] != "V_SORT"
        || before["list_metadata"]["views"][0]["sorts"] != json!([])
        || population["records"].as_array().map(Vec::len) != Some(73)
        || population["records"][2]["fields"][2]["number"] != json!([-3.0])
        || population["records"][6]["fields"][2]["number"] != json!([])
        || population["records"][11]["parent_record_id"] != "R_LIST_010"
        || population["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong Sort fixture raw source".into());
    }
    let mut proof = Proof {
        docs: oracle::documents(&before),
        population,
        find: find(state)?,
        ids: [0; 3],
        stages: vec![],
        captures: Default::default(),
    };
    proof.stable(state, 0)?;
    let canceled = open_choice(state, d, 2).await?;
    click(d, "slack-list-rename-close").await?;
    let cancel_focus = wait_dom(d, "window.__listViewSort.returnedSort()").await?;
    let canceled_ledger = rename_fixture::snapshot(&state.peek())?;
    if canceled_ledger["requests"] != json!([])
        || canceled_ledger["responses"] != json!([])
        || canceled_ledger["finished"] != json!([])
    {
        return Err("Cancel issued requests".into());
    }
    proof.stable(state, 0)?;
    let asc = open_choice(state, d, 1).await?;
    proof.ids[0] = asc["model"]["serial"]
        .as_u64()
        .ok_or("Missing Asc serial")?;
    if asc["model"]["serial"] == canceled["model"]["serial"] {
        return Err("Canceled sort identity reused".into());
    }
    proof.stage(state, d, "asc-edit").await?;
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "asc-saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    proof.stage(state, d, "uncertain").await?;
    rename_fixture::stale_sort_file(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    proof.stage(state, d, "stale").await?;
    rename_fixture::hold_read(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&held, 8, 2, false, true, 1, &proof.docs, proof.ids)?;
    let captured_loaded = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.clone())
        .ok_or("Missing captured canonical List")?;
    let captured_find = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .find
        .clone();
    let old_visit = crate::slack_lists::generation(&state.peek()).unwrap();
    let old_selection = rename::selection(&state.peek()).unwrap();
    let old_epoch = state.peek().core.account_epoch;
    click(d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Fixture epoch exhausted")?;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    rename_fixture::seed_sort_reopen(
        &mut state.write(),
        new_visit,
        captured_loaded,
        captured_find,
    )?;
    proof.stable(state, 0)?;
    let setup = json!({"document":document(state)?,"population":self::population(state)?,"find":find(state)?,"ledger":rename_fixture::snapshot(&state.peek())?});
    let new_selection = rename::selection(&state.peek()).unwrap();
    if old_visit == new_visit || old_selection == new_selection || setup["ledger"] != held {
        return Err("Wrong replaced visit or changed held read".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    let rebound_find = checked(d, "window.__listViewSort.bindFind()").await?;
    proof.stable(state, 0)?;
    if rename_fixture::snapshot(&state.peek())? != held {
        return Err("Held read changed before release".into());
    }
    rename_fixture::release(&state.peek(), true)?;
    let obsolete = completed(state, 3).await?;
    oracle::exact(&obsolete, 8, 3, false, false, 1, &proof.docs, proof.ids)?;
    phase(state, Stage::Uncertain).await?;
    proof.stable(state, 0)?;
    let retained = draft(state)?;
    let mut expected_retained = asc["model"].clone();
    expected_retained["visit"] = json!(new_visit);
    expected_retained["selection"] = json!(new_selection);
    if retained != expected_retained {
        return Err("Retired check replaced original intent or choice".into());
    }
    let retired = json!({"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"setup":setup,"completed":obsolete,"document":document(state)?,"population":self::population(state)?,"find":find(state)?,"reboundFind":rebound_find,"retained":retained});
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 1)?;
    let observed_asc = draft(state)?;
    checked(d, "window.__listViewSort.rememberSort()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listViewSort.returnedSort()").await?;
    proof.stage(state, d, "ascending").await?;
    let desc = open_choice(state, d, 2).await?;
    proof.ids[1] = desc["model"]["serial"]
        .as_u64()
        .ok_or("Missing Desc serial")?;
    let desc_ack = save_positive(state, d, &proof, 1, 15, 4, 2).await?;
    proof.stable(state, 1)?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 2)?;
    let observed_desc = draft(state)?;
    checked(d, "window.__listViewSort.rememberSort()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listViewSort.returnedSort()").await?;
    proof.stage(state, d, "descending").await?;
    let clear = open_choice(state, d, 0).await?;
    proof.ids[2] = clear["model"]["serial"]
        .as_u64()
        .ok_or("Missing Clear serial")?;
    proof.stage(state, d, "clear-edit").await?;
    let clear_ack = save_positive(state, d, &proof, 2, 22, 6, 0).await?;
    proof.stable(state, 2)?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 0)?;
    let observed_clear = draft(state)?;
    checked(d, "window.__listViewSort.rememberSort()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listViewSort.returnedSort()").await?;
    proof.stage(state, d, "cleared").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let viewport = checked(d, "({ok:true,viewport:[innerWidth,innerHeight]})").await?;
    proof.stable(state, 0)?;
    oracle::exact(&ledger, 25, 8, false, false, 0, &proof.docs, proof.ids)?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Final Sort ledger drift".into());
    }
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"noSession":true,"noTransport":true,"before":proof.docs[0],"ascending":proof.docs[1],"descending":proof.docs[2],"population":proof.population,"find":proof.find,"firstFind":first_find,"attempts":proof.ids,"cancel":{"draft":canceled,"focus":cancel_focus,"ledger":canceled_ledger},"choices":{"ascending":asc,"descending":desc,"clear":clear},"retiredCheck":retired,"observed":{"ascending":observed_asc,"descending":observed_desc,"clear":observed_clear},"positiveAcks":{"descending":desc_ack,"clear":clear_ack},"stages":proof.stages,"captures":proof.captures,"ledger":ledger,"viewport":viewport["viewport"]}),
    )
}
async fn save_positive(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &Proof,
    before: usize,
    count: usize,
    finished: usize,
    after: usize,
) -> Result<Value, String> {
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &held,
        count,
        finished,
        true,
        false,
        before,
        &proof.docs,
        proof.ids,
    )?;
    proof.stable(state, before)?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    let ack = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &ack,
        count,
        finished + 1,
        false,
        false,
        after,
        &proof.docs,
        proof.ids,
    )?;
    proof.stable(state, before)?;
    let retained =
        rename_fixture::retained_reply(&state.peek()).ok_or("Missing real positive receipt")?;
    if retained != ack["responses"][count - 1] {
        return Err("Positive ACK retention differs".into());
    }
    Ok(
        json!({"held":held,"ledger":ack,"retainedReply":retained,"documentBeforeReadback":document(state)?}),
    )
}
