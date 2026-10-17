//! Seven captures, one explicit create, and actual retained registry/readback paths.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
use rename_fixture::FieldCreateMode;
mod channel;
mod checkbox;
mod date;
mod entity;
mod harness;
mod link;
mod number_precision;
mod oracle;
mod people;
mod rating;
mod select;
mod vote;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    let Some(mode) = rename_fixture::field_create_mode() else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = if mode.is_entity() {
            entity::run(state, &desktop, mode).await
        } else {
            run(state, &desktop, mode).await
        };
        let receipt = match result {
            Ok(v) => v,
            Err(error) => {
                let mut receipt = json!({"ok":false,"error":error,"failedLedger":rename_fixture::snapshot(&state.peek()).ok(),"failedDocument":document(state).ok(),"failedPopulation":population(state).ok()});
                if mode.is_vote() {
                    receipt["failedCellLedger"] = vote::failed_snapshot(&state.peek());
                }
                if mode.is_rating() {
                    receipt["failedCellLedger"] = rating::failed_snapshot(&state.peek());
                }
                if mode == FieldCreateMode::NumberPrecision {
                    receipt["failedCellLedger"] = number_precision::failed_snapshot(&state.peek());
                }
                if mode == FieldCreateMode::People {
                    receipt["failedCellLedger"] = people::failed_snapshot(&state.peek());
                }
                if mode == FieldCreateMode::Link {
                    receipt["failedCellLedger"] = link::failed_snapshot(&state.peek());
                }
                if mode == FieldCreateMode::Channel {
                    receipt["failedCellLedger"] = channel::failed_snapshot(&state.peek());
                } else if mode == FieldCreateMode::Checkbox {
                    receipt["failedCellLedger"] = checkbox::failed_snapshot(&state.peek());
                } else if mode == FieldCreateMode::Date {
                    receipt["failedCellLedger"] = date::failed_snapshot(&state.peek());
                } else if mode.is_select() {
                    receipt["failedCellLedger"] = select::failed_snapshot(&state.peek());
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
    let mut result = json!({"serial":p.serial,"visit":crate::slack_lists::generation(&shell),"selection":rename::selection(&shell),"name":p.name,"view":"V_FILTER","observed":observed,"kind":choice.kind.as_str(),"choice":choice.revision});
    if rename_fixture::field_create_mode() == Some(FieldCreateMode::NumberPrecision) {
        result["numberPrecision"] = json!(choice.number_precision.value());
    }
    if rename_fixture::field_create_mode().is_some_and(FieldCreateMode::is_rating) {
        result["ratingOptions"] = p
            .rating_draft
            .as_ref()
            .map(|o| o.wire())
            .unwrap_or(Value::Null);
        result["ratingPicker"] = json!(p.rating_picker);
    }
    if rename_fixture::field_create_mode().is_some_and(FieldCreateMode::is_vote) {
        result["voteOptions"] = match p.rating_draft.as_ref() {
            Some(o) if o.max() == 1 => {
                super_platinum_core::slack_lists::VoteOptions::capture(o.emoji(), o.team_id())
                    .map_err(|_| "Invalid Vote draft")?
                    .wire()
            }
            Some(_) => return Err("Vote draft unexpectedly contains Quantity".into()),
            None => Value::Null,
        };
        result["votePicker"] = json!(p.rating_picker);
    }
    Ok(result)
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
    let precision = if mode == FieldCreateMode::NumberPrecision {
        Some(number_precision::creation::configure(state, d).await?)
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
    let mut receipt = json!({"model":model,"dom":dom,"input":input,"activation":activation,"initial":initial,"initialDom":initial_dom,"selected":selected});
    if let Some(precision) = precision {
        receipt["numberPrecision"] = precision;
    }
    Ok(receipt)
}
struct Proof {
    mode: FieldCreateMode,
    docs: [Value; 2],
    population: Value,
    find: Value,
    id: u64,
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
    select_options: Option<Value>,
    people_cache: Option<Value>,
    channel_cache: Option<Value>,
}
impl Proof {
    fn stable(&self, state: Signal<ShellState>, mode: usize) -> Result<(), String> {
        let shell = state.peek();
        if let Some(expected) = self.people_cache.as_ref() {
            if crate::slack_lists::fixture::text_field_create::people::cache::snapshot(&shell)?
                != *expected
            {
                return Err("People cache drift during creation".into());
            }
        }
        if let Some(expected) = self.channel_cache.as_ref() {
            if crate::slack_lists::fixture::text_field_create::channel::cache::snapshot(&shell)?
                != *expected
            {
                return Err("Channel cache drift during creation".into());
            }
        }
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
        let actual_ordinal = self.stages.len();
        let ordinal = actual_ordinal
            .checked_sub(if self.mode.is_select() {
                4
            } else if self.mode.is_rating() || self.mode.is_vote() {
                2
            } else {
                0
            })
            .ok_or("Missing Select prelude")?;
        if oracle::STAGES.get(ordinal) != Some(&name) {
            return Err("Wrong Text creation stage order".into());
        }
        let mode = usize::from(ordinal >= 5);
        self.stable(state, mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage_with_options(
            &ledger,
            ordinal,
            &self.docs,
            self.id,
            self.mode,
            self.select_options.as_ref(),
        )?;
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
                || c.get("options")
                    != self.select_options.as_ref().or(if self.mode
                        == FieldCreateMode::NumberPrecision
                    {
                        Some(json!({"precision":3}))
                    } else if self.mode.is_vote() {
                        Some(self.mode.vote_options())
                    } else if self.mode.is_rating() {
                        Some(self.mode.rating_options())
                    } else {
                        self.mode.kind().options()
                    }
                    .as_ref())
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
        let option_dom = if ordinal < 6 {
            if let Some(expected) = self.select_options.as_ref() {
                Some(select::options::verify(state, d, expected).await?)
            } else {
                None
            }
        } else {
            None
        };
        let vote_dom = if self.mode.is_vote() && ordinal < 6 {
            Some(vote::creation::verify(state, d, self.mode).await?)
        } else {
            None
        };
        let rating_dom = if self.mode.is_rating() && ordinal < 6 {
            Some(rating::creation::verify(state, d, self.mode).await?)
        } else {
            None
        };
        let number_dom = if self.mode == FieldCreateMode::NumberPrecision && ordinal < 6 {
            Some(number_precision::creation::verify(state, d).await?)
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
        if let Some(before) = option_dom.as_ref() {
            if select::options::verify(state, d, self.select_options.as_ref().unwrap()).await?
                != *before
            {
                return Err("Select option DOM changed during capture".into());
            }
        }
        if let Some(expected) = vote_dom.as_ref() {
            if vote::creation::verify(state, d, self.mode).await? != *expected {
                return Err("Vote options changed during capture".into());
            }
            self.stable(state, mode)?;
            if rename_fixture::snapshot(&state.peek())? != ledger || draft(state)? != model {
                return Err("Vote custody changed during configuration capture".into());
            }
        }
        if let Some(expected) = rating_dom.as_ref() {
            if rating::creation::verify(state, d, self.mode).await? != *expected {
                return Err("Rating configuration changed during capture".into());
            }
            self.stable(state, mode)?;
            if rename_fixture::snapshot(&state.peek())? != ledger || draft(state)? != model {
                return Err("Rating custody changed after configuration capture".into());
            }
        }
        if let Some(expected) = number_dom.as_ref() {
            if number_precision::creation::verify(state, d).await? != *expected {
                return Err("Number precision changed during capture".into());
            }
            self.stable(state, mode)?;
            if rename_fixture::snapshot(&state.peek())? != ledger || draft(state)? != model {
                return Err("Number custody changed after precision capture".into());
            }
        }
        self.stages.push(harness::creation_stage(self.mode,json!({"ordinal":actual_ordinal,"stage":name,"geometry":geometry,"scrolling":scrolling,"custody":custody,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"draft":model,"noSession":true,"noTransport":true}),option_dom)?);
        if self.mode.is_vote() {
            self.stages.last_mut().unwrap()["vote"] = vote_dom.unwrap_or(Value::Null);
        }
        if self.mode.is_rating() {
            self.stages.last_mut().unwrap()["rating"] = rating_dom.unwrap_or(Value::Null);
        }
        if self.mode == FieldCreateMode::NumberPrecision {
            self.stages.last_mut().unwrap()["numberPrecision"] = number_dom.unwrap_or(Value::Null);
        }
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
            "window.__listTextCreate.configure({},{})",
            json!(mode.kind().as_str()),
            if mode == FieldCreateMode::NumberPrecision {
                3
            } else {
                0
            }
        ),
    )
    .await?;
    if mode == FieldCreateMode::NumberPrecision {
        checked(
            d,
            include_str!("text_field_create/number_precision/creation.js"),
        )
        .await?;
    }
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
        docs: if mode.is_select() {
            [before.clone(), before.clone()]
        } else {
            oracle::documents(&before, mode)
        },
        population,
        find: find(state)?,
        id: 0,
        stages: vec![],
        captures: Default::default(),
        select_options: None,
        people_cache: if mode == FieldCreateMode::People {
            Some(
                crate::slack_lists::fixture::text_field_create::people::cache::snapshot(
                    &state.peek(),
                )?,
            )
        } else {
            None
        },
        channel_cache: if mode == FieldCreateMode::Channel {
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
    if mode.is_vote() {
        checked(d, include_str!("text_field_create/vote/creation.js")).await?;
    }
    if mode.is_rating() {
        checked(d, include_str!("text_field_create/rating/creation.js")).await?;
    }
    let canceled = open_name(state, d, mode).await?;
    let vote_prelude = if mode.is_vote() {
        Some(vote::creation::prelude(state, d, &mut proof).await?)
    } else {
        None
    };
    let rating_prelude = if mode.is_rating() {
        Some(rating::creation::prelude(state, d, &mut proof).await?)
    } else {
        None
    };
    let select_prelude = if mode.is_select() {
        Some(select::options::prelude(state, d, &mut proof).await?)
    } else {
        None
    };
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
    let vote_configuration = if mode.is_vote() {
        Some(vote::creation::configure(state, d, mode).await?)
    } else {
        None
    };
    let rating_configuration = if mode.is_rating() {
        Some(rating::creation::configure(state, d, mode).await?)
    } else {
        None
    };
    let mut select_configuration = Value::Null;
    let mut admitted_choice = if mode.is_rating() || mode.is_vote() {
        draft(state)?
    } else {
        created_choice["model"].clone()
    };
    if mode.is_select() {
        let configured = select::options::fill(state, d).await?;
        let expected = configured["options"].clone();
        rename_fixture::select_create::validate_mode(mode, &expected)?;
        rename_fixture::select_create::bind(
            &state.peek(),
            created_choice["model"]["serial"]
                .as_u64()
                .ok_or("Missing Select serial")?,
            expected.clone(),
        )?;
        proof.docs = oracle::documents_with_options(&before, mode, Some(&expected));
        proof.select_options = Some(expected);
        select::options::top(state, d).await?;
        admitted_choice = select::configuration::admission(
            &created_choice["model"],
            &configured,
            draft(state)?,
            mode,
        )?;
        select_configuration = json!({"fill":configured,"admitted":admitted_choice});
    }
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
        proof.select_options.as_ref(),
    )?;
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
        proof.select_options.as_ref(),
    )?;
    phase(state, Stage::Uncertain).await?;
    proof.stable(state, 0)?;
    let retained = draft(state)?;
    if !select::configuration::retained(&admitted_choice, &retained, new_visit, new_selection) {
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
    oracle::exact_with_options(
        &ledger,
        11,
        4,
        false,
        false,
        1,
        &proof.docs,
        proof.id,
        mode,
        proof.select_options.as_ref(),
    )?;
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
    let cells = if mode.is_vote() {
        Some(vote::run(state, d, &mut proof).await?)
    } else if mode.is_rating() {
        Some(rating::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::NumberPrecision {
        Some(number_precision::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::Channel {
        Some(channel::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::Link {
        Some(link::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::People {
        Some(people::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::Checkbox {
        Some(checkbox::run(state, d, &mut proof).await?)
    } else if mode == FieldCreateMode::Date {
        Some(date::run(state, d, &mut proof).await?)
    } else if mode.is_select() {
        Some(select::run(state, d, &mut proof).await?)
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
    if mode.is_vote() {
        terminal["votePrelude"] = vote_prelude.unwrap();
        terminal["voteConfiguration"] = vote_configuration.unwrap();
        terminal["voteCase"] = json!(mode.prefix());
    }
    if mode.is_rating() {
        terminal["ratingPrelude"] = rating_prelude.unwrap();
        terminal["ratingConfiguration"] = rating_configuration.unwrap();
        terminal["ratingCase"] = json!(mode.prefix());
    }
    if let Some(prelude) = select_prelude {
        terminal["selectFormat"] = json!(mode.select_format());
        terminal["selectConfiguration"] = select_configuration;
        terminal["selectPrelude"] = prelude;
        terminal["expectedOptions"] = proof.select_options.clone().unwrap();
    }
    if let Some(cache) = proof.people_cache {
        terminal["peopleCache"] = cache;
    }
    if let Some(cache) = proof.channel_cache {
        terminal["channelCache"] = cache;
    }
    if mode == FieldCreateMode::NumberPrecision {
        terminal["numberPrecision"] = json!({"schema":1,"value":3});
    }
    if let Some(cells) = cells {
        terminal["cellWorkflow"] = cells;
    }
    Ok(terminal)
}
