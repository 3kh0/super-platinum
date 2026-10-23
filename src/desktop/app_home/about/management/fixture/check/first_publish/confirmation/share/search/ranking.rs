//! Native mixed-ranking exercises, using only original query/selection controls.
use super::*;

/// Diagnostic only for the named synthetic ranking fixture. Preserve the
/// original failed observation without copying unrelated account/UI state.
fn failed_resolution(after: &Value, label: &str, scenario: &str) -> Option<String> {
    let share = value(&after["firstPublish"]);
    let search = &share["search"];
    if scenario == "read-failure" || search["status"] != "failed" {
        return None;
    }
    let ledger = &after["server"]["confirmationAdd"]["share"]["search"];
    let original_reads = search["reads"].as_array().map(|rows| {
        rows.iter()
            .map(|r| {
                json!({
                    "kind":r["kind"],"method":r["method"],"arguments":r["arguments"],
                    "orderedKeys":r["orderedKeys"],"failure":r["failure"],
                    "rawOk":r["raw"]["ok"],"rawError":r["raw"]["error"],
                    "rawReceipt":r["raw"]["opaque_receipt"],"nextMarker":r["raw"]["next_marker"]
                })
            })
            .collect::<Vec<_>>()
    });
    let diagnostic = json!({
        "fixtureOnly":true,"case":scenario,"stage":label,
        "share":{"active":share["active"],"bound":share["bound"],
            "loading":share["loading"],"selected":share["selected"]},
        "search":{"query":search["query"],"revision":search["revision"],
            "status":search["status"],"results":search["results"],
            "failures":search["failures"],"ranking":search["ranking"],
            "originalReads":original_reads},
        "rankingLedger":ledger["ranking"],
        "searchTransport":{"requestCount":ledger["requests"].as_array().map(Vec::len),
            "responseCount":ledger["responses"].as_array().map(Vec::len),
            "held":ledger["held"],"releases":ledger["releases"],
            "settlements":ledger["settlements"],"postingSources":ledger["postingSources"]}
    });
    Some(format!("Ranking resolution failed: {diagnostic}"))
}

async fn resolve(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    label: &str,
    before: Value,
    serial: usize,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let model = backend::capture(&state.peek())?;
    let release = data::release(&model, serial)?;
    let first = ready(state, desktop, visit).await?;
    if let Some(error) = failed_resolution(&first, label, &data::ranking::case()) {
        return Err(error);
    }
    // Cache refresh belongs to the account and can outlive the ranked query.
    // Wait for the actual closed callbacks before freezing full-ledger PNGs.
    until(state, visit, "original ranking cache callbacks", |_, s| {
        let r = &s["confirmationAdd"]["share"]["search"]["ranking"];
        let Some(requests) = r["requests"].as_array() else {
            return false;
        };
        let Some(settlements) = r["settlements"].as_array() else {
            return false;
        };
        [
            "search.autocomplete.model",
            "search.autocomplete.offlineFeatures",
        ]
        .iter()
        .all(|method| {
            requests.iter().any(|q| {
                q["method"] == *method
                    && settlements
                        .iter()
                        .any(|v| v["serial"] == q["serial"] && v["returned"] == true)
            })
        })
    })
    .await?;
    let after = ready(state, desktop, visit).await?;
    if let Some(error) = failed_resolution(&after, label, &data::ranking::case()) {
        return Err(error);
    }
    stage(
        state,
        desktop,
        name,
        label,
        visit,
        json!({"before":before,"releases":[release],"after":after}),
        captures,
        stages,
    )
    .await?;
    Ok(after)
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn retains_failed_original_ranking_observation_without_unrelated_state() {
        let search = json!({"query":"review","revision":7,"status":"failed","results":null,
            "failures":[{"stage":"ranking","site":"names"}],"ranking":null,
            "reads":[{"kind":"members","method":"users/search","raw":{"ok":true,
                "opaque_receipt":"original","results":[{"private":"not copied"}]}}]});
        let share = json!({"active":true,"bound":true,"loading":false,"selected":"",
            "text":"unrelated text","search":search});
        let ledger = json!({"ranking":{
            "navigationOwner":"12345678-1234-4234-8234-123456789abc", "requests":[],
            "responses":[],"settlements":[]},"requests":[],"responses":[],
            "held":[],"releases":[],"settlements":[],"postingSources":[]});
        let observation = json!({"firstPublish":{"attempts":[{"confirmationShare":share}]},
            "server":{"confirmationAdd":{"share":{"search":ledger}}},
            "unrelated":"not copied"});
        let error = failed_resolution(&observation, "query-ready", "cold-mixed").unwrap();
        assert!(error.contains("names"));
        assert!(error.contains("12345678-1234-4234-8234-123456789abc"));
        assert!(error.contains("original"));
        assert!(!error.contains("unrelated"));
        assert!(!error.contains("private"));
        assert!(failed_resolution(&observation, "query-failed", "read-failure").is_none());
    }
    #[test]
    fn successful_resolution_has_no_failure_diagnostic() {
        let observation = json!({"firstPublish":{"attempts":[{"confirmationShare":{
            "search":{"status":"ready"}}}]}});
        assert!(failed_resolution(&observation, "query-ready", "cold-mixed").is_none());
    }
}
async fn cancel(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    before: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "ranking original Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-cancelled",
        visit,
        json!({"before":before,"activation":activation,"after":after}),
        captures,
        stages,
    )
    .await?;
    Ok(after)
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let scenario = data::ranking::case();
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
    until(state, visit, "ranking original optional text", |p, _| {
        value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
    })
    .await?;
    settled_paint(state, desktop, visit, true).await?;
    let selected = if data::ranking::destination().is_none() {
        let activation = action(
            desktop,
            "select",
            "workflow-share-destination",
            "CADDDEST01",
        )
        .await?;
        until(state, visit, "ranking retained selection", |p, _| {
            value(p)["selected"] == "CADDDEST01" && value(p)["bound"] == true
        })
        .await?;
        settled_paint(state, desktop, visit, true).await?;
        activation
    } else {
        Value::Null
    };
    let mut before =
        json!({"observation":observe(state,desktop,visit).await?,"text":text,"selection":selected});
    if scenario == "cached-write" {
        let (held, serial) = pending(
            state,
            desktop,
            name,
            visit,
            "warm-pending",
            "oldreview",
            before,
            captures,
            stages,
        )
        .await?;
        before = resolve(
            state,
            desktop,
            name,
            visit,
            "warm-ready",
            held,
            serial,
            captures,
            stages,
        )
        .await?;
    }
    let query = if scenario == "prefixes" {
        "@review"
    } else if scenario == "stale-cancel" {
        "oldreview"
    } else {
        "review"
    };
    let label = if scenario == "prefixes" {
        "member-pending"
    } else {
        "query-pending"
    };
    let (held, serial) = pending(
        state, desktop, name, visit, label, query, before, captures, stages,
    )
    .await?;
    if scenario == "stale-cancel" {
        let (replacement, new_serial) = pending(
            state,
            desktop,
            name,
            visit,
            "replacement-pending",
            "review",
            held,
            captures,
            stages,
        )
        .await?;
        let cancelled = cancel(state, desktop, name, visit, replacement, captures, stages).await?;
        let model = backend::capture(&state.peek())?;
        let releases = [
            data::release(&model, serial)?,
            data::release(&model, new_serial)?,
        ];
        let z = quiet(state, desktop, visit).await?;
        let after = observe(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "late-settled",
            visit,
            json!({"before":cancelled,"releases":releases,"quiet":z,"after":after}),
            captures,
            stages,
        )
        .await?;
        return Ok(Value::Null);
    }
    let label = if scenario == "prefixes" {
        "member-ready"
    } else if scenario == "read-failure" {
        "query-failed"
    } else {
        "query-ready"
    };
    let mut resolved = resolve(
        state, desktop, name, visit, label, held, serial, captures, stages,
    )
    .await?;
    if scenario == "prefixes" {
        let (held, serial) = pending(
            state,
            desktop,
            name,
            visit,
            "channel-pending",
            "#Review",
            resolved,
            captures,
            stages,
        )
        .await?;
        resolved = resolve(
            state,
            desktop,
            name,
            visit,
            "channel-ready",
            held,
            serial,
            captures,
            stages,
        )
        .await?;
    }
    if data::ranking::destination().is_some() {
        return selection::run(state, desktop, name, visit, captures, stages).await;
    }
    cancel(state, desktop, name, visit, resolved, captures, stages).await?;
    Ok(Value::Null)
}
