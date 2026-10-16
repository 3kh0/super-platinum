//! Seven captures, one explicit create, and actual retained registry/readback paths.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
use rename_fixture::FieldCreateMode;
mod checkbox;
mod harness;
mod oracle;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    let Some(mode) = rename_fixture::field_create_mode() else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop, mode).await {
            Ok(v) => v,
            Err(error) => {
                let mut receipt = json!({"ok":false,"error":error,"failedLedger":rename_fixture::snapshot(&state.peek()).ok(),"failedDocument":document(state).ok(),"failedPopulation":population(state).ok()});
                if mode == FieldCreateMode::Checkbox {
                    receipt["failedCellLedger"] = checkbox::failed_snapshot(&state.peek());
                }
                receipt
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("{}-check: {receipt}\n", mode.prefix()).as_bytes())
                .expect("Text creation terminal output failed");
        })
        .await
        .expect("Text creation terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Text creation did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held Text {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let ledger = rename_fixture::snapshot(&state.peek())?;
        match ledger["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(ledger),
            Some(n) if n < count => {}
            _ => return Err("Text creation completion count exceeded expected checkpoint".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Text creation owner completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing Text document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing Text population".into())
}
fn find(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let v = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    Ok(
        json!({"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}),
    )
}

fn draft(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = rename::presentation(&shell).ok_or("Missing Text presentation")?;
    if !p.text_creation
        || p.view.as_deref() != Some("V_FILTER")
        || p.precision.is_some()
        || p.visibility.is_some()
        || p.sort.is_some()
        || p.filter.is_some()
    {
        return Err("Wrong Text presentation".into());
    }
    let choice = p.field_choice.ok_or("Missing creation choice")?;
    let observed = p
        .observed_field
        .as_ref()
        .map(|v| json!({"id":v.id,"key":v.key,"name":v.name}));
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&shell),"selection":rename::selection(&shell),"name":p.name,"view":"V_FILTER","observed":observed,"kind":choice.kind.as_str(),"choice":choice.revision}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(
        d,
        &format!("window.__listTextCreate.click({})", json!(format!("#{id}"))),
    )
    .await
    .map(|_| ())
}
async fn input_model(
    state: Signal<ShellState>,
    mode: FieldCreateMode,
    name: &str,
    prior: &Value,
    rotate: bool,
) -> Result<Value, String> {
    for _ in 0..150 {
        let model = draft(state)?;
        if model["kind"] == mode.kind().as_str()
            && model["name"] == name
            && model["choice"].as_u64().is_some_and(|n| n > 0)
            && (model["choice"] != *prior) == rotate
        {
            return Ok(model);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual field input model did not settle with exact type-choice ownership".into())
}
async fn open_name(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    checked(d, "window.__listTextCreate.rememberAdd()").await?;
    let activation = checked(
        d,
        "window.__listTextCreate.click('#slack-list-new-text-field')",
    )
    .await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-name')===document.activeElement})",
    )
    .await?;
    let initial = draft(state)?;
    let initial_dom = checked(d, "window.__listTextCreate.draft()").await?;
    if initial["kind"] != "text"
        || initial["name"] != ""
        || !initial["choice"].as_u64().is_some_and(|n| n > 0)
        || initial_dom["kind"] != "text"
        || initial_dom["choice"].as_str()
            != initial["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || initial_dom["nameChoice"] != initial_dom["choice"]
        || initial_dom["kindDisabled"] != false
        || initial_dom["nameDisabled"] != false
    {
        return Err("New shared field dialog did not start with current Text default".into());
    }
    let selected = if mode != FieldCreateMode::Text {
        let event = checked(
            d,
            &format!(
                "window.__listTextCreate.selectKind({})",
                json!(mode.kind().as_str())
            ),
        )
        .await?;
        let model = input_model(state, mode, "", &initial["choice"], true).await?;
        wait_dom(
            d,
            &format!(
                "window.__listTextCreate.choiceReady({},{})",
                json!(mode.kind().as_str()),
                json!(model["choice"].as_u64().unwrap().to_string())
            ),
        )
        .await?;
        if model["kind"] != mode.kind().as_str() || model["choice"] == initial["choice"] {
            return Err("Actual Number choice was not adopted with new type revision".into());
        }
        Some(json!({"event":event,"model":model}))
    } else {
        None
    };
    let before_name = draft(state)?;
    let input = checked(
        d,
        &format!("window.__listTextCreate.input({})", json!(mode.name())),
    )
    .await?;
    let model = input_model(state, mode, mode.name(), &before_name["choice"], false).await?;
    wait_dom(
        d,
        &format!(
            "window.__listTextCreate.nameReady({},{},{})",
            json!(mode.kind().as_str()),
            json!(mode.name()),
            json!(model["choice"].as_u64().unwrap().to_string())
        ),
    )
    .await?;
    let dom = checked(d, "window.__listTextCreate.draft()").await?;
    if model["name"] != mode.name()
        || model["kind"] != mode.kind().as_str()
        || model["choice"] != before_name["choice"]
        || !model["observed"].is_null()
        || dom["name"] != model["name"]
        || dom["kind"] != model["kind"]
        || dom["choice"].as_str() != model["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || dom["nameChoice"] != dom["choice"]
        || dom["serial"].as_str() != model["serial"].as_u64().map(|n| n.to_string()).as_deref()
    {
        return Err("Actual field input callback did not retain name and exact type choice".into());
    }
    Ok(
        json!({"model":model,"dom":dom,"input":input,"activation":activation,"initial":initial,"initialDom":initial_dom,"selected":selected}),
    )
}
struct Proof {
    mode: FieldCreateMode,
    docs: [Value; 2],
    population: Value,
    find: Value,
    id: u64,
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
}
impl Proof {
    fn stable(&self, state: Signal<ShellState>, mode: usize) -> Result<(), String> {
        let shell = state.peek();
        let p = crate::slack_lists::presentation(&shell).ok_or("Missing List projection")?;
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || p.selected_view.as_deref() != Some("V_FILTER")
            || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
            || p.view_notice.is_some()
            || !p.incomplete
        {
            return Err("Text creation owner/layout/partial isolation failed".into());
        }
        if mode == 1
            && !p
                .columns
                .iter()
                .any(|c| c.id == self.mode.observed().0 && c.label == self.mode.name())
        {
            return Err("Observed field missing from selected Table projection".into());
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
            return Err(format!("Wrong projected Text creation order: {ids:?}"));
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
            return Err("Wrong Text creation stage order".into());
        }
        let mode = usize::from(ordinal >= 5);
        self.stable(state, mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage(&ledger, ordinal, &self.docs, self.id, self.mode)?;
        let scrolling = if ordinal == 6 {
            Some(checked(d, "window.__listTextCreate.revealTable()").await?)
        } else {
            None
        };
        let geometry = wait_dom(
            d,
            &format!("window.__listTextCreate.geometry({})", json!(name)),
        )
        .await?;
        let model = if ordinal == 6 {
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
                || geometry["draft"]["name"] != model["name"]
                || geometry["draft"]["kind"] != model["kind"]
                || !owner(&geometry["draft"]["choice"], model["choice"].as_u64())
                || geometry["draft"]["nameChoice"] != geometry["draft"]["choice"]
                || geometry["owner"]["view"] != "V_FILTER")
        {
            return Err("Rendered Text creation owner differs from actual state".into());
        }
        let custody = if (1..6).contains(&ordinal) {
            let c = rename_fixture::text_create_custody(&state.peek())?;
            if c["serial"] != self.id
                || c["document"] != self.docs[0]
                || c["intentDocument"] != self.docs[0]
                || c["population"] != self.population
                || c["intentPopulation"] != self.population
                || c["name"] != self.mode.name()
                || c["kind"] != self.mode.kind().as_str()
                || c.get("options") != self.mode.kind().options().as_ref()
                || c["choice"] != model["choice"]
                || c["view"] != "V_FILTER"
                || c["user"] != "U0"
            {
                return Err("Original create custody changed".into());
            }
            Some(c)
        } else {
            None
        };
        self.captures
            .insert(name.into(), harness::capture(d, name).await?);
        self.stable(state, mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Text creation changed during capture".into());
        }
        if !model.is_null() && draft(state)? != model {
            return Err("Field draft changed during capture".into());
        }
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"scrolling":scrolling,"custody":custody,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"draft":model,"noSession":true,"noTransport":true}));
        Ok(())
    }
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("text_field_create.js")).await?;
    checked(
        d,
        &format!(
            "window.__listTextCreate.configure({})",
            json!(mode.kind().as_str())
        ),
    )
    .await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-new-text-field')&&!document.getElementById('slack-list-rename')})").await?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial Find".into());
    }
    checked(d, "window.__listTextCreate.query()").await?;
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
    let first_find = checked(d, "window.__listTextCreate.bindFind()").await?;
    let before = document(state)?;
    let population = population(state)?;
    if before["list_metadata"]["views"][0]["id"] != "V_FILTER"
        || before["list_metadata"]["views"][0]["filters"]
            != json!([{"key":"done","operator":"is_not_empty","values":[]}])
        || population["records"].as_array().map(Vec::len) != Some(73)
        || population["records"][2]["fields"][2]["number"] != json!([-3.0])
        || population["records"][6]["fields"]
            .as_array()
            .is_none_or(|cells| {
                cells.iter().any(|cell| cell["key"] == "done")
                    || cells
                        .iter()
                        .find(|cell| cell["key"] == "estimate")
                        .map(|cell| &cell["number"])
                        != Some(&json!([]))
            })
        || population["records"][0]["fields"][1]["checkbox"] != false
        || population["records"][1]["fields"][1]["checkbox"] != true
        || before["list_metadata"]["views"][0]["sorts"]
            != json!([{"key":"estimate","ascending":true}])
        || before["list_metadata"]["views"][0]["info_column_filters"] != json!([])
        || population["records"][11]["parent_record_id"] != "R_LIST_010"
        || population["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong Text creation fixture raw source".into());
    }
    let mut proof = Proof {
        mode,
        docs: oracle::documents(&before, mode),
        population,
        find: find(state)?,
        id: 0,
        stages: vec![],
        captures: Default::default(),
    };
    proof.stable(state, 0)?;
    let canceled = open_name(state, d, mode).await?;
    click(d, "slack-list-rename-close").await?;
    let cancel_focus = wait_dom(d, "window.__listTextCreate.returnedAdd()").await?;
    let cancel_registry = rename_fixture::text_create_registry(&state.peek())?;
    if cancel_registry != json!({"present":false,"serial":null,"job":null,"intent":false})
        || rename::presentation(&state.peek()).is_some()
    {
        return Err("Cancel retained a registry entry".into());
    }
    let canceled_ledger = rename_fixture::snapshot(&state.peek())?;
    if canceled_ledger["requests"] != json!([])
        || canceled_ledger["responses"] != json!([])
        || canceled_ledger["finished"] != json!([])
    {
        return Err("Cancel issued requests".into());
    }
    proof.stable(state, 0)?;
    let created_choice = open_name(state, d, mode).await?;
    proof.id = created_choice["model"]["serial"]
        .as_u64()
        .ok_or("Missing Text serial")?;
    if created_choice["model"]["serial"] == canceled["model"]["serial"] {
        return Err("Canceled Text identity reused".into());
    }
    proof.stage(state, d, "field-edit").await?;
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "field-saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    proof.stage(state, d, "uncertain").await?;
    rename_fixture::stale_text_create_file(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    proof.stage(state, d, "conflict").await?;
    rename_fixture::hold_read(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&held, 8, 2, false, true, 1, &proof.docs, proof.id, mode)?;
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
    rename_fixture::seed_text_create_reopen(
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
    let rebound_find = checked(d, "window.__listTextCreate.bindFind()").await?;
    proof.stable(state, 0)?;
    if rename_fixture::snapshot(&state.peek())? != held {
        return Err("Held read changed before release".into());
    }
    rename_fixture::release(&state.peek(), true)?;
    let obsolete = completed(state, 3).await?;
    oracle::exact(
        &obsolete,
        8,
        3,
        false,
        false,
        1,
        &proof.docs,
        proof.id,
        mode,
    )?;
    phase(state, Stage::Uncertain).await?;
    proof.stable(state, 0)?;
    let retained = draft(state)?;
    let mut expected_retained = created_choice["model"].clone();
    expected_retained["visit"] = json!(new_visit);
    expected_retained["selection"] = json!(new_selection);
    if retained != expected_retained {
        return Err("Retired check replaced original intent or choice".into());
    }
    let retired = json!({"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"setup":setup,"completed":obsolete,"document":document(state)?,"population":self::population(state)?,"find":find(state)?,"reboundFind":rebound_find,"retained":retained});
    proof.stage(state, d, "retired").await?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 1)?;
    let observed = draft(state)?;
    if observed["observed"]
        != json!({"id":mode.observed().0,"key":mode.observed().1,"name":mode.name()})
    {
        return Err("Actual observed field identity differs".into());
    }
    proof.stage(state, d, "observed").await?;
    checked(d, "window.__listTextCreate.rememberAdd()").await?;
    click(d, "slack-list-rename-close").await?;
    let done_focus = wait_dom(d, "window.__listTextCreate.returnedAdd()").await?;
    proof.stage(state, d, "table").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let viewport = checked(d, "({ok:true,viewport:[innerWidth,innerHeight]})").await?;
    proof.stable(state, 1)?;
    oracle::exact(&ledger, 11, 4, false, false, 1, &proof.docs, proof.id, mode)?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Final Text ledger drift".into());
    }
    let done_registry = rename_fixture::text_create_registry(&state.peek())?;
    let custody = rename_fixture::text_create_custody(&state.peek()).err();
    // Done removes the registry entry; the last captured custody must still bind
    // the original immutable intent, while no live entry may admit another write.
    if done_registry != json!({"present":false,"serial":null,"job":null,"intent":false})
        || custody.is_none()
        || rename::presentation(&state.peek()).is_some()
    {
        return Err("Done retained an active Text entry".into());
    }
    let cells = if mode == FieldCreateMode::Checkbox {
        Some(checkbox::run(state, d, &mut proof).await?)
    } else {
        None
    };
    if cells
        .as_ref()
        .is_some_and(|c| c["geometry"]["viewport"] != viewport["viewport"])
    {
        return Err("Checkbox viewport changed after creation".into());
    }
    let mut terminal = json!({"ok":true,"mode":mode.kind().as_str(),"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"noSession":true,"noTransport":true,"before":proof.docs[0],"after":proof.docs[1],"population":proof.population,"find":proof.find,"firstFind":first_find,"attempt":proof.id,"cancel":{"draft":canceled,"focus":cancel_focus,"registry":cancel_registry,"ledger":canceled_ledger},"create":created_choice,"retiredCheck":retired,"observed":observed,"doneFocus":done_focus,"doneRegistry":done_registry,"stages":proof.stages,"captures":proof.captures,"ledger":ledger,"viewport":viewport["viewport"]});
    if let Some(cells) = cells {
        terminal["cellWorkflow"] = cells;
    }
    Ok(terminal)
}
