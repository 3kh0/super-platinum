//! Dynamic configured People/Channel creation through the existing worker.
use super::*;
use rename_fixture::EntityCase;
mod cells;
mod configuration;
pub(super) const EXTRA_STAGES: [&str; 4] = [
    "entity-format",
    "entity-defaults",
    "entity-cell",
    "entity-canceled",
];
async fn click(
    _state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listEntityCreate.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
}
fn case(mode: FieldCreateMode) -> Result<EntityCase, String> {
    if let FieldCreateMode::Entity(case) = mode {
        Ok(case)
    } else {
        Err("Entity case required".into())
    }
}
async fn capture_configuration(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<(), String> {
    for (name, part) in [("entity-format", "format"), ("entity-defaults", "default")] {
        let before = configuration::model(state)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        let geometry = wait_dom(
            d,
            &format!(
                "window.__listEntityCreate.inspectPart({},{})",
                json!(part),
                before
            ),
        )
        .await?;
        proof
            .captures
            .insert(name.into(), harness::capture(d, name).await?);
        if configuration::model(state)? != before
            || rename_fixture::snapshot(&state.peek())? != ledger
        {
            return Err("Entity configuration changed during capture".into());
        }
        proof.stages.push(json!({"stage":name,"model":before,"geometry":geometry,"ledger":ledger,"noSession":true,"noTransport":true}));
    }
    Ok(())
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    index: usize,
) -> Result<(), String> {
    let name = *oracle::STAGES
        .get(index)
        .ok_or("Unknown entity creation stage")?;
    proof.stable(state, usize::from(index >= 5))?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::stage(&ledger, index, &proof.docs, proof.id, proof.mode)?;
    let model = if index == 6 {
        Value::Null
    } else {
        configuration::model(state)?
    };
    let outer = if index == 6 {
        Value::Null
    } else {
        draft(state)?
    };
    let config = if index < 6 {
        Some(configuration::verify(state, d, &case(proof.mode)?.options()).await?)
    } else {
        None
    };
    let custody = if (1..6).contains(&index) {
        let c = rename_fixture::text_create_custody(&state.peek())?;
        if c["serial"] != proof.id
            || c["document"] != proof.docs[0]
            || c["intentDocument"] != proof.docs[0]
            || c["population"] != proof.population
            || c["intentPopulation"] != proof.population
            || c["options"] != case(proof.mode)?.options()
            || c["name"] != proof.mode.name()
            || c["kind"] != proof.mode.kind().as_str()
            || c["choice"] != outer["choice"]
            || c["view"] != "V_FILTER"
            || c["user"] != "U0"
        {
            return Err("Configured entity immutable custody drift".into());
        }
        Some(c)
    } else {
        None
    };
    let reveal = if index == 5 {
        Some(
            checked(
                d,
                &format!("window.__listEntityCreate.inspectPart('summary',{})", model),
            )
            .await?,
        )
    } else if index == 6 {
        Some(checked(d, "window.__listTextCreate.revealTable()").await?)
    } else {
        None
    };
    let geometry = wait_dom(
        d,
        &if index == 6 {
            "window.__listTextCreate.geometry('table')".into()
        } else {
            format!(
                "window.__listEntityCreate.geometry({},{})",
                json!(name),
                model
            )
        },
    )
    .await?;
    proof
        .captures
        .insert(name.into(), harness::capture(d, name).await?);
    proof.stable(state, usize::from(index >= 5))?;
    if rename_fixture::snapshot(&state.peek())? != ledger
        || index < 6 && (configuration::model(state)? != model || draft(state)? != outer)
    {
        return Err("Entity stage changed during capture".into());
    }
    proof.stages.push(json!({"stage":name,"ordinal":index,"geometry":geometry,"reveal":reveal,"configuration":config,"draft":outer,"custody":custody,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let case = case(mode)?;
    let owner = harness::wait_runner(state).await?;
    let account = state
        .peek()
        .core
        .active_account
        .clone()
        .ok_or("Missing synthetic account")?;
    if !account
        .strip_prefix("number-precision-fixture-")
        .is_some_and(|s| {
            s.parse::<u64>()
                .is_ok_and(|id| id > 0 && id.to_string() == s)
        })
    {
        return Err("Unexpected entity synthetic account".into());
    }
    checked(d, include_str!("../text_field_create.js")).await?;
    checked(
        d,
        &format!(
            "window.__listTextCreate.configure({})",
            json!(mode.kind().as_str())
        ),
    )
    .await?;
    checked(d, include_str!("entity/creation.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-new-text-field')&&!document.getElementById('slack-list-rename')})").await?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected Find seed".into());
    }
    checked(d, "window.__listTextCreate.query()").await?;
    let mut ready = false;
    for _ in 0..150 {
        let f = find(state)?;
        if f["input"] == "Launch item 0"
            && f["applied"] == f["input"]
            && f["revision"].as_u64().is_some_and(|r| r > 0)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if !ready {
        return Err("Entity Find did not settle".into());
    }
    let first_find = checked(d, "window.__listTextCreate.bindFind()").await?;
    let before = document(state)?;
    let population = population(state)?;
    if population["records"].as_array().map(Vec::len) != Some(73)
        || population["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Expected raw73 partial fixture".into());
    }
    let mut proof = Proof {
        mode,
        docs: oracle::documents(&before, mode),
        population,
        find: find(state)?,
        id: 0,
        stages: vec![],
        captures: Default::default(),
        select_options: None,
        people_cache: if !case.channel() {
            Some(
                crate::slack_lists::fixture::text_field_create::people::cache::snapshot(
                    &state.peek(),
                )?,
            )
        } else {
            None
        },
        channel_cache: if case.channel() {
            Some(
                crate::slack_lists::fixture::text_field_create::channel::cache::snapshot(
                    &state.peek(),
                )?,
            )
        } else {
            None
        },
    };
    proof.stable(state, 0)?;
    let canceled = open_name(state, d, mode).await?;
    let canceled_config = configuration::configure(state, d, case).await?;
    click(state, d, "slack-list-rename-close").await?;
    let cancel_focus = wait_dom(d, "window.__listTextCreate.returnedAdd()").await?;
    let canceled_ledger = rename_fixture::snapshot(&state.peek())?;
    if canceled_ledger["requests"] != json!([])
        || canceled_ledger["responses"] != json!([])
        || canceled_ledger["finished"] != json!([])
        || rename_fixture::text_create_registry(&state.peek())?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Entity Cancel wrote or retained draft".into());
    }
    let opened = open_name(state, d, mode).await?;
    if opened["model"]["serial"] == canceled["model"]["serial"] {
        return Err("Entity Cancel owner reused".into());
    }
    let configured = configuration::configure(state, d, case).await?;
    proof.id = opened["model"]["serial"]
        .as_u64()
        .ok_or("Missing creation serial")?;
    capture_configuration(state, d, &mut proof).await?;
    checked(d, "window.__listEntityCreate.top()").await?;
    let admitted = draft(state)?;
    stage(state, d, &mut proof, 0).await?;
    click(state, d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    stage(state, d, &mut proof, 1).await?;
    let frozen = rename_fixture::snapshot(&state.peek())?;
    let disabled = checked(
        d,
        &format!(
            "window.__listEntityCreate.disabledAttempts({})",
            configuration::model(state)?
        ),
    )
    .await?;
    if rename_fixture::snapshot(&state.peek())? != frozen {
        return Err("Locked entity settings dispatched another request".into());
    }
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    stage(state, d, &mut proof, 2).await?;
    rename_fixture::stale_text_create_file(&state.peek())?;
    click(state, d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    stage(state, d, &mut proof, 3).await?;
    rename_fixture::hold_read(&state.peek())?;
    click(state, d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact_with_options(
        &held,
        8,
        2,
        false,
        true,
        1,
        &proof.docs,
        proof.id,
        mode,
        None,
    )?;
    let loaded = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.clone())
        .ok_or("Missing captured List")?;
    let saved_find = state
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
    click(state, d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Epoch exhausted")?;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    rename_fixture::seed_text_create_reopen(&mut state.write(), new_visit, loaded, saved_find)?;
    let new_selection = rename::selection(&state.peek()).unwrap();
    if new_visit == old_visit || new_selection == old_selection {
        return Err("Replacement entity owner did not rotate".into());
    }
    proof.stable(state, 0)?;
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    checked(d, "window.__listTextCreate.bindFind()").await?;
    if rename_fixture::snapshot(&state.peek())? != held {
        return Err("Entity held read changed before release".into());
    }
    rename_fixture::release(&state.peek(), true)?;
    let obsolete = completed(state, 3).await?;
    oracle::exact_with_options(
        &obsolete,
        8,
        3,
        false,
        false,
        1,
        &proof.docs,
        proof.id,
        mode,
        None,
    )?;
    phase(state, Stage::Uncertain).await?;
    if !select::configuration::retained(&admitted, &draft(state)?, new_visit, new_selection) {
        return Err("Entity retired read changed captured owner".into());
    }
    stage(state, d, &mut proof, 4).await?;
    click(state, d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    let observed = draft(state)?;
    if observed["observed"]
        != json!({"id":mode.observed().0,"key":mode.observed().1,"name":mode.name()})
    {
        return Err("Entity observed identity mismatch".into());
    }
    stage(state, d, &mut proof, 5).await?;
    checked(d, "window.__listTextCreate.rememberAdd()").await?;
    click(state, d, "slack-list-rename-close").await?;
    let done_focus = wait_dom(d, "window.__listTextCreate.returnedAdd()").await?;
    stage(state, d, &mut proof, 6).await?;
    if rename_fixture::text_create_registry(&state.peek())?
        != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Entity Done retained creation authority".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let compatibility = cells::run(state, d, &mut proof, case).await?;
    proof.stable(state, 1)?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Cell compatibility changed creation ledger".into());
    }
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"mode":mode.kind().as_str(),"entityCase":case.prefix(),"expectedOptions":case.options(),"owner":owner,"account":account,"viewport":compatibility["final"]["viewport"],"noSession":true,"noTransport":true,"httpExecuted":0,"actualCreateForms":1,"creationReads":10,"before":proof.docs[0],"after":proof.docs[1],"population":proof.population,"find":proof.find,"firstFind":first_find,"cancel":{"draft":canceled,"configuration":canceled_config,"focus":cancel_focus,"ledger":canceled_ledger},"configuration":configured,"admitted":admitted,"disabledNoReplay":disabled,"retired":{"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"completed":obsolete},"observed":observed,"doneFocus":done_focus,"ledger":ledger,"cellCompatibility":compatibility,"stages":proof.stages,"captures":proof.captures}),
    )
}
