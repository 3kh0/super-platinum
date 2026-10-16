//! Seven captures, two explicit saved-view changes, one real retained registry path.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
mod oracle;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !rename_fixture::visibility_check_enabled() {
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
                .write_all(format!("slack-list-field-visibility-check: {receipt}\n").as_bytes())
                .expect("Visibility terminal output failed");
        })
        .await
        .expect("Visibility terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Visibility did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held visibility {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let ledger = rename_fixture::snapshot(&state.peek())?;
        match ledger["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(ledger),
            Some(n) if n < count => {}
            _ => return Err("Visibility completion count exceeded expected checkpoint".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Visibility owner completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing visibility document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing visibility population".into())
}
fn find(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let v = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    Ok(
        json!({"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}),
    )
}
fn draft(state: Signal<ShellState>, original: bool, desired: bool) -> Result<Value, String> {
    let shell = state.peek();
    let p = rename::presentation(&shell).ok_or("Missing Fields presentation")?;
    let v = p.visibility.ok_or("Missing visibility selection")?;
    if v.column != "C_NUMBER"
        || v.field != "Estimate"
        || v.original != original
        || v.desired != desired
        || v.choice == 0
        || p.view.as_deref() != Some("V_PRECISION")
        || p.precision.is_some()
    {
        return Err("Wrong current visibility draft".into());
    }
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&shell),"selection":rename::selection(&shell),"choice":v.choice,"column":v.column,"original":v.original,"desired":v.desired,"current":v.current}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(
        d,
        &format!(
            "window.__listFieldVisibility.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
    .map(|_| ())
}
async fn open_choice(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    value: bool,
) -> Result<Value, String> {
    checked(d, "window.__listFieldVisibility.rememberFields()").await?;
    click(d, "slack-list-fields").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-field-choice')===document.activeElement})",
    )
    .await?;
    let prior = rename::presentation(&state.peek())
        .and_then(|p| p.visibility)
        .ok_or("Missing first field choice")?
        .choice;
    checked(
        d,
        "window.__listFieldVisibility.select('#slack-list-field-choice','C_NUMBER')",
    )
    .await?;
    wait_dom(d,&format!("({{ok:!!document.getElementById('slack-list-field-choice')&&document.getElementById('slack-list-field-choice').value==='C_NUMBER'&&document.getElementById('slack-list-field-choice').dataset.visibilityChoice!=={} }})",json!(prior.to_string()))).await?;
    checked(
        d,
        &format!(
            "window.__listFieldVisibility.select('#slack-list-field-visible',{})",
            json!(if value { "shown" } else { "hidden" })
        ),
    )
    .await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-rename-confirm')&&!document.getElementById('slack-list-rename-confirm').disabled})").await?;
    draft(state, !value, value)
}
struct Proof {
    before: Value,
    hidden: Value,
    population: Value,
    find: Value,
    hide: u64,
    show: Option<u64>,
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
}
impl Proof {
    fn stable(&self, state: Signal<ShellState>, hidden: bool) -> Result<(), String> {
        let shell = state.peek();
        let p = crate::slack_lists::presentation(&shell).ok_or("No List projection")?;
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || p.selected_view.as_deref() != Some("V_PRECISION")
            || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
            || p.view_notice.is_some()
        {
            return Err("Visibility owner/layout isolation failed".into());
        }
        drop(shell);
        if document(state)? != *if hidden { &self.hidden } else { &self.before }
            || population(state)? != self.population
            || find(state)? != self.find
        {
            return Err("Raw metadata/population/Find drift".into());
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
            return Err("Wrong visibility stage order".into());
        }
        self.stable(state, matches!(ordinal, 4 | 5))?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::ledger(
            &ledger,
            ordinal,
            &self.before,
            &self.hidden,
            self.hide,
            self.show,
        )?;
        let geometry = wait_dom(
            d,
            &format!("window.__listFieldVisibility.geometry({})", json!(name)),
        )
        .await?;
        self.captures
            .insert(name.into(), harness::capture(d, name).await?);
        self.stable(state, matches!(ordinal, 4 | 5))?;
        if rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Visibility changed during capture".into());
        }
        let presentation = if matches!(ordinal, 4 | 6) {
            Value::Null
        } else {
            draft(state, ordinal != 5, ordinal == 5)?
        };
        let same_number = |actual: &Value, expected: Option<u64>| {
            actual.as_str() == expected.map(|n| n.to_string()).as_deref()
        };
        if !same_number(
            &geometry["owner"]["visit"],
            crate::slack_lists::generation(&state.peek()),
        ) || !same_number(
            &geometry["owner"]["selection"],
            rename::selection(&state.peek()),
        ) || !presentation.is_null()
            && (!same_number(
                &geometry["owner"]["serial"],
                presentation["serial"].as_u64(),
            ) || !same_number(
                &geometry["field"]["choice"],
                presentation["choice"].as_u64(),
            ) || geometry["owner"]["view"] != "V_PRECISION")
        {
            return Err(
                "Rendered visibility ownership differs from exact current visit/selection/choice"
                    .into(),
            );
        }
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"draft":presentation,"noSession":true,"noTransport":true}));
        Ok(())
    }
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("field_visibility.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-fields')&&!document.getElementById('slack-list-rename')})").await?;
    let before = document(state)?;
    let mut hidden = before.clone();
    let column = &hidden["list_metadata"]["views"][0]["columns"][1];
    if column["id"] != "C_NUMBER"
        || column["key"] != "estimate"
        || column["visible"] != true
        || hidden["list_metadata"]["schema"][2]["options"] != json!({"precision":2})
    {
        return Err("Wrong visibility fixture metadata".into());
    }
    hidden["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(false);
    let population = population(state)?;
    if population["records"][5]["fields"][2]["number"] != json!([12.5])
        || population["records"][11]["parent_record_id"] != "R_LIST_010"
        || population["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong raw visibility fixture population".into());
    }
    let initial_find = find(state)?;
    if initial_find != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial Find".into());
    }
    let cancel_draft = open_choice(state, d, false).await?;
    click(d, "slack-list-rename-close").await?;
    let cancel_focus = wait_dom(d, "window.__listFieldVisibility.returnedFields()").await?;
    let empty = rename_fixture::snapshot(&state.peek())?;
    if empty["requests"] != json!([])
        || empty["responses"] != json!([])
        || empty["finished"] != json!([])
        || document(state)? != before
        || self::population(state)? != population
        || find(state)? != initial_find
    {
        return Err("Cancel changed canonical state or issued requests".into());
    }
    let hide_draft = open_choice(state, d, false).await?;
    let hide = hide_draft["serial"].as_u64().ok_or("Missing Hide serial")?;
    if hide_draft["serial"] == cancel_draft["serial"] {
        return Err("Canceled intent reused".into());
    }
    let mut proof = Proof {
        before,
        hidden,
        population,
        find: initial_find,
        hide,
        show: None,
        stages: vec![],
        captures: Default::default(),
    };
    proof.stage(state, d, "hide-edit").await?;
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "hide-saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    proof.stage(state, d, "uncertain").await?;
    // Stale metadata is a current-owned Conflict; keep distinct from the next retired completion.
    rename_fixture::stale_visibility_file(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    let conflict = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &conflict,
        7,
        2,
        false,
        false,
        &proof.hidden,
        &proof.before,
        &proof.hidden,
        hide,
        None,
    )?;
    proof.stable(state, false)?;
    let conflict_draft = draft(state, true, false)?;
    rename_fixture::hold_read(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &held,
        8,
        2,
        false,
        true,
        &proof.hidden,
        &proof.before,
        &proof.hidden,
        hide,
        None,
    )?;
    let captured_loaded = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.clone())
        .ok_or("Missing canonical snapshot before retired check setup")?;
    let old_visit = crate::slack_lists::generation(&state.peek()).ok_or("Missing initial visit")?;
    let old_epoch = state.peek().core.account_epoch;
    let old_selection = rename::selection(&state.peek()).ok_or("Missing initial selection")?;
    click(d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Fixture epoch exhausted")?;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    rename_fixture::seed_visibility_reopen(&mut state.write(), new_visit, captured_loaded)?;
    proof.stable(state, false)?;
    let seeded_setup = json!({"document":document(state)?,"population":self::population(state)?,"find":find(state)?,"ledger":rename_fixture::snapshot(&state.peek())?});
    if seeded_setup["ledger"] != held {
        return Err("Seeding replacement visit changed the held check".into());
    }

    let new_selection = rename::selection(&state.peek()).ok_or("Missing reopened selection")?;
    if old_visit == new_visit || old_selection == new_selection {
        return Err("New List visit reused owner/selection".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'}})",new_visit)).await?;
    proof.stable(state, false)?;
    if rename_fixture::snapshot(&state.peek())? != held {
        return Err("Held check changed before obsolete release".into());
    }
    rename_fixture::release(&state.peek(), true)?;
    let retired_completion = completed(state, 3).await?;
    oracle::ledger(
        &retired_completion,
        3,
        &proof.before,
        &proof.hidden,
        hide,
        None,
    )?;
    phase(state, Stage::Uncertain).await?;
    // No fixture refresh occurs between release and these unchanged-state checks.
    proof.stable(state, false)?;
    let retired_document = document(state)?;
    let retired_population = self::population(state)?;
    let retired_find = find(state)?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('slack-list-rename-check')})",
    )
    .await?;
    let retained = draft(state, true, false)?;
    if retained["serial"] != hide || retained["choice"] != hide_draft["choice"] {
        return Err("Retired read replaced retained choice".into());
    }
    proof.stage(state, d, "stale").await?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, true)?;
    let observed_hide = draft(state, true, false)?;
    checked(d, "window.__listFieldVisibility.rememberFields()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listFieldVisibility.returnedFields()").await?;
    proof.stage(state, d, "hidden").await?;
    let show_draft = open_choice(state, d, true).await?;
    let show = show_draft["serial"].as_u64().ok_or("Missing Show serial")?;
    if show == hide || show_draft["serial"] == cancel_draft["serial"] {
        return Err("Show reused prior attempt".into());
    }
    proof.show = Some(show);
    proof.stage(state, d, "show-edit").await?;
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    let show_held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &show_held,
        15,
        4,
        true,
        false,
        &proof.hidden,
        &proof.before,
        &proof.hidden,
        hide,
        Some(show),
    )?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    let show_ack = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &show_ack,
        15,
        5,
        false,
        false,
        &proof.before,
        &proof.before,
        &proof.hidden,
        hide,
        Some(show),
    )?;
    proof.stable(state, true)?;
    let retained_reply = rename_fixture::retained_reply(&state.peek())
        .ok_or("Missing actual retained positive reply")?;
    if retained_reply != show_ack["responses"][14] {
        return Err("Positive reply receipt changed".into());
    }
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, false)?;
    let observed_show = draft(state, false, true)?;
    checked(d, "window.__listFieldVisibility.rememberFields()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listFieldVisibility.returnedFields()").await?;
    checked(d, "window.__listFieldVisibility.revealNumber()").await?;
    proof.stage(state, d, "shown").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::ledger(&ledger, 6, &proof.before, &proof.hidden, hide, Some(show))?;
    let viewport = eval(d, "[innerWidth,innerHeight]").await?;
    proof.stable(state, false)?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Visibility ledger changed before terminal publication".into());
    }
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],"preparedRequests":18,"writeRequests":2,"readRequests":16,
        "noSession":true,"noTransport":true,"syntheticExecutor":true,"actualBuilderDecoderReducers":true,"before":proof.before,"hidden":proof.hidden,"after":document(state)?,
        "initialPopulation":proof.population,"finalPopulation":self::population(state)?,"initialFind":proof.find,"finalFind":find(state)?,
        "cancel":{"draft":cancel_draft,"focus":cancel_focus,"ledger":empty},"hideDraft":hide_draft,"showDraft":show_draft,
        "conflictCheck":{"draft":conflict_draft,"ledger":conflict},"retiredCheck":{"held":held,"setup":seeded_setup,"completion":retired_completion,"documentBeforeRefresh":retired_document,"populationBeforeRefresh":retired_population,"findBeforeRefresh":retired_find,"retained":retained,"visitGenerations":[old_visit,new_visit],"selectionGenerations":[old_selection,new_selection],"accountEpochs":[old_epoch,state.peek().core.account_epoch]},
        "observedHide":observed_hide,"showHeld":show_held,"showAck":{"ledger":show_ack,"retainedReply":retained_reply,"canonicalStayedHidden":true},"observedShow":observed_show,
        "stages":proof.stages,"captures":proof.captures,"ledger":ledger,"viewport":viewport}),
    )
}
