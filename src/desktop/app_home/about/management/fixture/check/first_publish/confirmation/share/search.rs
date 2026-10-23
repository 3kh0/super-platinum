//! Actual original query input and controlled ordered directory responses.
use super::*;
use backend::search as data;
use super_platinum_core::workflow_share::search as wire;
mod empty;
mod ranking;
mod selection;
mod startup;
pub(crate) fn startup_retirement_native(base: &str) -> String {
    startup::native(base, "observe")
}
#[cfg(test)]
mod tests;
fn held_query(ledger: &Value, query: &wire::Query) -> Option<usize> {
    let method = query.kinds().first()?.method();
    let held = ledger["held"].as_array()?;
    let mut matching = ledger["requests"].as_array()?.iter().filter(|request| {
        request["method"] == method
            && request["query"] == query.wire()
            && held.contains(&request["serial"])
    });
    let serial = matching.next()?["serial"].as_u64()?;
    if matching.next().is_some() {
        return None;
    }
    usize::try_from(serial).ok()
}
pub(super) fn native_base(base: &str) -> String {
    if !data::requested() {
        return base.into();
    }
    let base = if data::person_enabled() {
        base.replace(
            "document.getElementById('channel-details-trigger')",
            include_str!("dm/header.js").trim(),
        )
        .replace(
            "'#review-destination'",
            &json!(backend::dm::LABEL).to_string(),
        )
    } else {
        base.replace(
            "'#review-destination'",
            &json!(format!(
                "#{}",
                if data::ranking::requested() {
                    data::ranking::DESTINATION_LABEL
                } else {
                    data::LABEL
                }
            ))
            .to_string(),
        )
    };
    base
}
async fn input(desktop: &DesktopContext, query: &str) -> Result<Value, String> {
    evaluate(
        desktop,
        &include_str!("search/input.js").replace("QUERY", &json!(query).to_string()),
    )
    .await
}
async fn ready(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    settled_paint(state, desktop, visit, false).await
}
async fn settled_paint(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    initial: bool,
) -> Result<Value, String> {
    until(state, visit, "original query settled", |p, _| {
        value(p)["bound"] == true
            && value(p)["loading"] == false
            && if initial {
                matches!(
                    value(p)["search"]["status"].as_str(),
                    Some("ready" | "failed" | "partial")
                ) && value(p)["search"]["query"] == ""
            } else {
                matches!(
                    value(p)["search"]["status"].as_str(),
                    Some("ready" | "failed" | "partial")
                )
            }
    })
    .await?;
    if initial {
        until(
            state,
            visit,
            "original initial observation callbacks",
            |p, s| {
                let ledger = &s["confirmationAdd"]["share"]["search"]["ranking"];
                let Some(requests) = ledger["requests"].as_array() else {
                    return false;
                };
                let Some(settled) = ledger["settlements"].as_array() else {
                    return false;
                };
                requests.len() == settled.len()
                    && (value(p)["search"]["status"] == "failed"
                        || [
                            "search.autocomplete.model",
                            "search.autocomplete.offlineFeatures",
                        ]
                        .iter()
                        .all(|method| requests.iter().any(|r| r["method"] == *method)))
            },
        )
        .await?;
    }
    let expected = {
        let s = state.peek();
        value(&product::snapshot(&s, visit)).clone()
    };
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "original query settled paint",
        &include_str!("search/ready.js")
            .replace("EXPECTED_SHARE", &expected.to_string())
            .replace("INITIAL_OPEN", if initial { "true" } else { "false" }),
    )
    .await?;
    observe(state, desktop, visit).await
}
async fn pending(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    label: &str,
    query: &str,
    before: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(Value, usize), String> {
    let plan = wire::Query::new(query).map_err(|_| "Invalid native fixture search query")?;
    let activation = input(desktop, query).await?;
    if activation["ok"] != true {
        return Err("Original query input did not dispatch".into());
    }
    until(state, visit, "actual query first page held", |p, s| {
        let ledger = &s["confirmationAdd"]["share"]["search"];
        let retired_original = if data::ranking::requested()
            && data::ranking::case() == "stale-cancel"
            && query == "review"
        {
            ledger["requests"].as_array().is_some_and(|rows| {
                rows.iter()
                    .filter(|r| r["query"] == "oldreview" && r["method"] == "users/search")
                    .all(|r| {
                        ledger["settlements"].as_array().is_some_and(|values| {
                            values
                                .iter()
                                .any(|v| v["serial"] == r["serial"] && v["returned"] == false)
                        })
                    })
            })
        } else {
            true
        };
        value(p)["search"]["query"] == query
            && value(p)["search"]["status"] == "searching"
            && held_query(&s["confirmationAdd"]["share"]["search"], &plan).is_some()
            && retired_original
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    let serial = held_query(
        &after["server"]["confirmationAdd"]["share"]["search"],
        &plan,
    )
    .ok_or("Original held matching query serial absent")?;
    stage(
        state,
        desktop,
        name,
        label,
        visit,
        json!({"before":before,"activation":activation,"after":after}),
        captures,
        stages,
    )
    .await?;
    Ok((after, serial))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    if data::empty::requested() {
        return empty::run(state, desktop, name, visit, captures, stages).await;
    }
    if backend::startup::requested() {
        return startup::run(state, desktop, name, visit, captures, stages).await;
    }
    if data::ranking::requested() {
        return ranking::run(state, desktop, name, visit, captures, stages).await;
    }
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "click", "workflow-confirmation-share-open", "").await?;
    let opened = settled_paint(state, desktop, visit, true).await?;
    stage(
        state,
        desktop,
        name,
        "share-open",
        visit,
        json!({"before":before,"activation":activation,"after":opened}),
        captures,
        stages,
    )
    .await?;
    let text = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
    if text["ok"] != true {
        return Err("Original optional message input did not dispatch".into());
    }
    until(state, visit, "search original optional message", |p, _| {
        value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
    })
    .await?;
    settled_paint(state, desktop, visit, true).await?;
    let selected = if matches!(data::case().as_str(), "person-success" | "channel-success") {
        Value::Null
    } else {
        let activation = action(
            desktop,
            "select",
            "workflow-share-destination",
            "CADDDEST01",
        )
        .await?;
        if activation["ok"] != true {
            return Err("Original retained destination selection did not dispatch".into());
        }
        until(
            state,
            visit,
            "search original destination selected",
            |p, _| {
                value(p)["selected"] == "CADDDEST01"
                    && value(p)["text"] == backend::MESSAGE
                    && value(p)["bound"] == true
            },
        )
        .await?;
        settled_paint(state, desktop, visit, true).await?;
        activation
    };
    let setup =
        json!({"observation":observe(state,desktop,visit).await?,"text":text,"selection":selected});
    let query = match data::case().as_str() {
        "channel-success" => "research",
        "prefixes" => "@Dana",
        "rapid-query" => "oldDana",
        "rejected" => "rejected",
        "offline" => "offline",
        _ => "Dana",
    };
    let first_label = if data::case() == "prefixes" {
        "member-pending"
    } else {
        "query-pending"
    };
    let (first, serial) = pending(
        state,
        desktop,
        name,
        visit,
        first_label,
        query,
        setup,
        captures,
        stages,
    )
    .await?;
    let model = backend::capture(&state.peek())?;
    if data::case() == "held-cancel" {
        let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
        until(state, visit, "search Cancel closed", |p, _| {
            last(p)["closed"] == true && !value(p)["active"].as_bool().unwrap_or(true)
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "share-cancelled",
            visit,
            json!({"before":first,"activation":activation,"after":after}),
            captures,
            stages,
        )
        .await?;
        let release = data::release(&model, serial)?;
        let z = quiet(state, desktop, visit).await?;
        stage(state,desktop,name,"late-settled",visit,json!({"before":after,"release":release,"after":observe(state,desktop,visit).await?,"quiet":z}),captures,stages).await?;
        return Ok(Value::Null);
    }
    let mut previous = first;
    let mut releases = vec![];
    if data::case() == "rapid-query" {
        let (replacement, second) = pending(
            state,
            desktop,
            name,
            visit,
            "replacement-pending",
            "#research",
            previous,
            captures,
            stages,
        )
        .await?;
        releases.push(data::release(&model, serial)?);
        releases.push(data::release(&model, second)?);
        previous = replacement;
    } else {
        releases.push(data::release(&model, serial)?);
    }
    let resolved = ready(state, desktop, visit).await?;
    let label = match data::case().as_str() {
        "prefixes" => "member-ready",
        "rejected" | "offline" => "query-failed",
        _ => "query-ready",
    };
    stage(
        state,
        desktop,
        name,
        label,
        visit,
        json!({"before":previous,"releases":releases,"after":resolved}),
        captures,
        stages,
    )
    .await?;
    if data::case() == "prefixes" {
        let (pending, serial) = pending(
            state,
            desktop,
            name,
            visit,
            "channel-pending",
            "#research",
            resolved,
            captures,
            stages,
        )
        .await?;
        let release = data::release(&model, serial)?;
        let after = ready(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "channel-ready",
            visit,
            json!({"before":pending,"releases":[release],"after":after}),
            captures,
            stages,
        )
        .await?;
    } else if data::case() == "rapid-query" {
        let z = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "late-settled",
            visit,
            json!({"before":resolved,"after":observe(state,desktop,visit).await?,"quiet":z}),
            captures,
            stages,
        )
        .await?;
    }
    if matches!(data::case().as_str(), "person-success" | "channel-success") {
        return selection::run(state, desktop, name, visit, captures, stages).await;
    }
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "search original Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    stage(state,desktop,name,"share-cancelled",visit,json!({"before":before,"activation":activation,"after":observe(state,desktop,visit).await?}),captures,stages).await?;
    Ok(Value::Null)
}
