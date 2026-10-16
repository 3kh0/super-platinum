//! Drive the real ordered-row controls; observe structural focus before revealing captures.
use super::*;
fn values(model: &Value) -> Vec<Value> {
    let value = &model["desired"];
    if let Some(rows) = value["all"].as_array() {
        rows.clone()
    } else if value["column"].is_null() {
        vec![]
    } else {
        vec![value.clone()]
    }
}
fn set(rows: Vec<Value>) -> Value {
    match rows.len() {
        0 => json!({"column":null,"checked":null}),
        1 => rows.into_iter().next().unwrap(),
        _ => json!({"all":rows}),
    }
}
async fn settle(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    desired: Value,
) -> Result<Value, String> {
    for _ in 0..150 {
        let model = draft(state)?;
        if model["desired"] == desired {
            wait_dom(d, &format!("window.__listViewFilter.sameDraft({model})")).await?;
            wait_dom(d,"({ok:!document.getElementById('slack-list-rename')?.dataset.filterFocus&&!document.getElementById('slack-list-rename')?.dataset.filterCapture})").await?;
            return Ok(model);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual complete Filter draft did not settle".into())
}
async fn focus(
    d: &dioxus::desktop::DesktopContext,
    column: Option<&str>,
    part: &str,
) -> Result<Value, String> {
    wait_dom(
        d,
        &format!(
            "window.__listViewFilter.rowFocus({},{})",
            json!(column),
            json!(part)
        ),
    )
    .await
}
async fn begin_structure(
    d: &dioxus::desktop::DesktopContext,
    model: &Value,
    column: Option<&str>,
    part: &str,
) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listViewFilter.beginStructure({model},{},{})",
            json!(column),
            json!(part)
        ),
    )
    .await
}
async fn add(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    column: &str,
) -> Result<Value, String> {
    let before = draft(state)?;
    let mut rows = values(&before);
    let selected = checked(
        d,
        &format!(
            "window.__listViewFilter.select('#slack-list-filter-add-field',{})",
            json!(column)
        ),
    )
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-filter-add')?.disabled===false})",
    )
    .await?;
    let armed = begin_structure(d, &before, Some(column), "operator").await?;
    click(d, "slack-list-filter-add").await?;
    rows.push(if column == "C_CHECK" {
        json!({"column":column,"checked":false})
    } else {
        json!({"column":column,"operator":"equal","value":null})
    });
    let after = settle(state, d, set(rows)).await?;
    if after["choice"] == before["choice"]
        || after["rows"].as_array().map(Vec::len) != before["rows"].as_array().map(|r| r.len() + 1)
    {
        return Err("Add did not append one owned Filter row".into());
    }
    let focused = focus(d, Some(column), "operator").await?;
    Ok(
        json!({"before":before,"selected":selected,"armed":armed,"after":after,"focusBeforeReveal":focused}),
    )
}
async fn operator(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    column: &str,
    op: &str,
) -> Result<Value, String> {
    let before = draft(state)?;
    let mut rows = values(&before);
    let index = rows
        .iter()
        .position(|v| v["column"] == column)
        .ok_or("Missing operator row")?;
    let armed = begin_structure(d, &before, Some(column), "operator").await?;
    let selected = checked(
        d,
        &format!(
            "window.__listViewFilter.rowSelect({},'operator',{})",
            json!(column),
            json!(op)
        ),
    )
    .await?;
    if column == "C_CHECK" {
        rows[index]["checked"] = json!(op == "is_not_empty");
    } else {
        rows[index]["operator"] = json!(op);
        if matches!(op, "is_empty" | "is_not_empty") {
            rows[index]["value"] = Value::Null;
        }
    }
    let after = settle(state, d, set(rows)).await?;
    if before["choice"] == after["choice"]
        || before["rows"][index]["epoch"] == after["rows"][index]["epoch"]
    {
        return Err("Operator change did not rotate exact row/structure epoch".into());
    }
    let focused = focus(d, Some(column), "operator").await?;
    Ok(
        json!({"before":before,"selected":selected,"armed":armed,"after":after,"focusBeforeReveal":focused}),
    )
}
async fn compare(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let added = add(state, d, "C_NUMBER").await?;
    let empty = draft(state)?;
    let disabled=wait_dom(d,"({ok:document.getElementById('slack-list-rename-confirm')?.disabled===true&&window.__listViewFilter.rowControl('C_NUMBER','value')?.value===''})").await?;
    let typing = checked(d, "window.__listViewFilter.typeNumber()").await?;
    let mut rows = values(&empty);
    rows.iter_mut().find(|v| v["column"] == "C_NUMBER").unwrap()["value"] = json!("12.50");
    let typed = settle(state, d, set(rows)).await?;
    if typed["choice"] != empty["choice"] || typed["rows"] != empty["rows"] {
        return Err("Same-editor Number input rotated Filter ownership".into());
    }
    let same = checked(d, "window.__listViewFilter.sameNumber()").await?;
    let changed = operator(state, d, "C_NUMBER", "less_than").await?;
    let remount = wait_dom(d, "window.__listViewFilter.rotatedNumber()").await?;
    Ok(
        json!({"added":added,"incomplete":empty,"disabled":disabled,"typing":typing,"typed":typed,"sameNode":same,"operator":changed,"operatorRemount":remount}),
    )
}
pub(super) async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: usize,
) -> Result<Value, String> {
    checked(d, "window.__listViewFilter.rememberFilter()").await?;
    click(d, "slack-list-filter").await?;
    phase(state, Stage::Editing).await?;
    let opened = draft(state)?;
    settle(state, d, opened["desired"].clone()).await?;
    let initial = wait_dom(d, "window.__listViewFilter.initialFocus()").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if opened["desired"] != opened["original"] {
        return Err("Reopened Filter differs before explicit edit".into());
    }
    let unchanged = wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===true})",
    )
    .await?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Unchanged reopening issued a request".into());
    }
    let compound = rename_fixture::compound_filter_check_enabled();
    let number = rename_fixture::number_filter_check_enabled();
    let mut actions = vec![];
    if mode == 0 {
        let armed = begin_structure(d, &opened, None, "add").await?;
        click(d, "slack-list-filter-clear").await?;
        let after = settle(state, d, set(vec![])).await?;
        if after["choice"] == opened["choice"] {
            return Err("Clear did not rotate structure".into());
        }
        actions.push(
            json!({"clear":after,"armed":armed,"focusBeforeReveal":focus(d,None,"add").await?}),
        );
    } else if compound && mode == 2 {
        let remembered = checked(d, "window.__listViewFilter.rememberSurvivor()").await?;
        let before = draft(state)?;
        let original = values(&before);
        if original
            != vec![
                json!({"column":"C_CHECK","checked":true}),
                json!({"column":"C_NUMBER","operator":"less_than","value":"12.50"}),
            ]
        {
            return Err("Reopened mixed Filter differs".into());
        }
        let id = before["rows"][0]["id"]
            .as_u64()
            .ok_or("Missing removed row")?;
        let armed = begin_structure(d, &before, Some("C_NUMBER"), "field").await?;
        click(d, &format!("filter-remove-{id}")).await?;
        let after = settle(state, d, original[1].clone()).await?;
        if after["rows"] != json!([before["rows"][1]]) || after["choice"] == before["choice"] {
            return Err("Removal replaced surviving row custody".into());
        }
        let survivor = wait_dom(d, "window.__listViewFilter.survivor()").await?;
        actions.push(json!({"remembered":remembered,"armed":armed,"before":before,"after":after,"survivorBeforeReveal":survivor}));
    } else if number && mode == 2 {
        actions.push(operator(state, d, "C_NUMBER", "is_empty").await?);
        wait_dom(
            d,
            "({ok:!window.__listViewFilter.rowControl('C_NUMBER','value')})",
        )
        .await?;
    } else {
        if !number {
            if values(&opened).is_empty() {
                actions.push(add(state, d, "C_CHECK").await?)
            }
            let checked_value = mode == 1;
            if values(&draft(state)?)[0]["checked"] != checked_value {
                actions.push(
                    operator(
                        state,
                        d,
                        "C_CHECK",
                        if checked_value {
                            "is_not_empty"
                        } else {
                            "is_empty"
                        },
                    )
                    .await?,
                );
            }
        }
        if number || compound {
            actions.push(compare(state, d).await?);
        }
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===false})",
    )
    .await?;
    let model = draft(state)?;
    let dom = checked(d, &format!("window.__listViewFilter.sameDraft({model})")).await?;
    if rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Editing Filter issued requests".into());
    }
    Ok(
        json!({"model":model,"dom":dom["dom"],"opened":opened,"initialFocus":initial,"unchanged":{"disabled":unchanged,"ledger":ledger},"actions":actions}),
    )
}
pub(super) async fn capture_view(
    _state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    _stage: &str,
) -> Result<(), String> {
    checked(d, "window.__listViewFilter.captureView()")
        .await
        .map(|_| ())
}
