//! Real keyed-row actions; every structural focus witness precedes capture reveal.
use super::*;
pub(super) fn values(model: &Value) -> Vec<Value> {
    let value = &model["desired"];
    if let Some(rows) = value["ordered"].as_array() {
        rows.clone()
    } else if value["column"].is_null() {
        vec![]
    } else {
        vec![value.clone()]
    }
}
fn set(rows: Vec<Value>) -> Value {
    match rows.len() {
        0 => json!({"column":null,"ascending":null}),
        1 => rows.into_iter().next().unwrap(),
        _ => json!({"ordered":rows}),
    }
}
async fn settle(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    desired: Value,
) -> Result<Value, String> {
    for _ in 0..150 {
        let model = draft(state)?;
        if model["desired"] == desired && model["editable"] == true {
            wait_dom(d, &format!("window.__listViewSort.sameDraft({model})")).await?;
            wait_dom(
                d,
                "({ok:!document.getElementById('slack-list-rename')?.dataset.sortFocus})",
            )
            .await?;
            return Ok(model);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Complete owned Sort draft did not settle".into())
}
async fn arm(
    d: &dioxus::desktop::DesktopContext,
    model: &Value,
    target: Option<&str>,
    part: &str,
) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listViewSort.beginStructure({model},{},{})",
            json!(target),
            json!(part)
        ),
    )
    .await
}
async fn finish(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    before: Value,
    desired: Vec<Value>,
    target: Option<&str>,
    part: &str,
    armed: Value,
    activation: Value,
) -> Result<Value, String> {
    let after = settle(state, d, set(desired)).await?;
    if before["choice"] == after["choice"] {
        return Err("Structural Sort action did not rotate choice".into());
    }
    let focus = wait_dom(
        d,
        &format!(
            "window.__listViewSort.structuralFocus({after},{},{})",
            json!(target),
            json!(part)
        ),
    )
    .await?;
    Ok(
        json!({"before":before,"armed":armed,"activation":activation,"after":after,"focusBeforeReveal":focus}),
    )
}
pub(super) async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    checked(d, "window.__listViewSort.rememberSort()").await?;
    click(d, "slack-list-sort").await?;
    phase(state, Stage::Editing).await?;
    let model = draft(state)?;
    settle(state, d, model["desired"].clone()).await?;
    let initial = wait_dom(d, "window.__listViewSort.initialFocus()").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if model["desired"] != model["original"] {
        return Err("Reopened Sort changed before edit".into());
    }
    let disabled = wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===true})",
    )
    .await?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Unchanged reopen issued requests".into());
    }
    Ok(
        json!({"model":model,"initialFocus":initial,"unchanged":{"disabled":disabled,"ledger":ledger}}),
    )
}
pub(super) async fn add(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    column: &str,
) -> Result<Value, String> {
    let before = draft(state)?;
    let mut desired = values(&before);
    let selected = checked(
        d,
        &format!(
            "window.__listViewSort.select('#slack-list-sort-add-field',{})",
            json!(column)
        ),
    )
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-sort-add')?.disabled===false})",
    )
    .await?;
    let armed = arm(d, &before, Some(column), "direction").await?;
    let activation = checked(d, "window.__listViewSort.click('#slack-list-sort-add')").await?;
    desired.push(json!({"column":column,"ascending":true}));
    let mut out = finish(
        state,
        d,
        before,
        desired,
        Some(column),
        "direction",
        armed,
        activation,
    )
    .await?;
    out["selected"] = selected;
    Ok(out)
}
pub(super) async fn action(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    column: Option<&str>,
    kind: &str,
    value: Option<&str>,
) -> Result<Value, String> {
    let before = draft(state)?;
    let mut desired = values(&before);
    let index = column
        .map(|c| {
            desired
                .iter()
                .position(|v| v["column"] == c)
                .ok_or("Missing Sort action row")
        })
        .transpose()?;
    let id = index
        .map(|i| {
            before["rows"][i]
                .as_u64()
                .ok_or("Missing exact Sort row ID")
        })
        .transpose()?;
    let (target, part, selector, selected) = match kind {
        "clear" => {
            desired.clear();
            (None, "add", "#slack-list-sort-clear".to_owned(), None)
        }
        "direction" => {
            let value = value.ok_or("Missing direction")?;
            if !matches!(value, "ascending" | "descending") {
                return Err("Unknown direction".into());
            }
            desired[index.unwrap()]["ascending"] = json!(value == "ascending");
            (
                column.map(str::to_owned),
                "direction",
                format!("#sort-direction-{}", id.unwrap()),
                Some(value),
            )
        }
        "field" => {
            let value = value.ok_or("Missing field")?;
            desired[index.unwrap()]["column"] = json!(value);
            (
                Some(value.to_owned()),
                "field",
                format!("#sort-field-{}", id.unwrap()),
                Some(value),
            )
        }
        "remove" => {
            let i = index.unwrap();
            desired.remove(i);
            let target = desired
                .get(i)
                .or_else(|| desired.last())
                .and_then(|v| v["column"].as_str())
                .map(str::to_owned);
            let part = if target.is_some() { "field" } else { "add" };
            (target, part, format!("#sort-remove-{}", id.unwrap()), None)
        }
        "up" | "down" => {
            let i = index.unwrap();
            let j = if kind == "up" {
                i.checked_sub(1)
            } else {
                i.checked_add(1).filter(|j| *j < desired.len())
            }
            .ok_or("Invalid Sort move")?;
            desired.swap(i, j);
            (
                column.map(str::to_owned),
                "field",
                format!("#sort-{kind}-{}", id.unwrap()),
                None,
            )
        }
        _ => return Err("Unknown Sort action".into()),
    };
    let armed = arm(d, &before, target.as_deref(), part).await?;
    let activation = checked(
        d,
        &if let Some(value) = selected {
            format!(
                "window.__listViewSort.select({},{})",
                json!(selector),
                json!(value)
            )
        } else {
            format!("window.__listViewSort.click({})", json!(selector))
        },
    )
    .await?;
    finish(
        state,
        d,
        before,
        desired,
        target.as_deref(),
        part,
        armed,
        activation,
    )
    .await
}
pub(super) async fn legacy(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: usize,
) -> Result<Value, String> {
    let opened = open(state, d).await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let mut actions = vec![];
    if mode == 0 {
        actions.push(action(state, d, None, "clear", None).await?);
    } else {
        if values(&opened["model"]).is_empty() {
            actions.push(add(state, d, "C_NUMBER").await?);
        }
        let ascending = mode == 1;
        if values(&draft(state)?)[0]["ascending"] != ascending {
            actions.push(
                action(
                    state,
                    d,
                    Some("C_NUMBER"),
                    "direction",
                    Some(if ascending { "ascending" } else { "descending" }),
                )
                .await?,
            );
        }
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===false})",
    )
    .await?;
    let model = draft(state)?;
    let dom = checked(d, &format!("window.__listViewSort.sameDraft({model})")).await?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Sort editing issued requests".into());
    }
    Ok(json!({"model":model,"dom":dom["dom"],"opened":opened,"actions":actions}))
}
