//! Actual option DOM events and independent captured expected wire configuration.
use super::*;
pub(in super::super) const STAGES: [&str; 4] =
    ["options-1-2", "options-3-4", "options-5-6", "options-7-8"];
fn model(state: Signal<ShellState>) -> Result<Value, String> {
    rename_fixture::select_create::draft(&state.peek())
}
fn proof(m: &Value) -> Value {
    json!({"serial":m["serial"],"visit":m["visit"],"selection":m["selection"],"choice":m["choice"],"epoch":m["epoch"]})
}
fn dom_matches(dom: &Value, m: &Value) -> bool {
    ["serial", "visit", "selection", "choice", "epoch"]
        .iter()
        .all(|k| dom[k].as_str() == m[k].as_u64().map(|n| n.to_string()).as_deref())
        && dom["format"] == m["format"]
        && dom["pending"] == m["pending"]
        && dom["rows"].as_array().is_some_and(|rows| {
            m["rows"].as_array().is_some_and(|model| {
                rows.len() == model.len()
                    && rows.iter().zip(model).all(|(d, m)| {
                        d["generation"].as_str()
                            == m["generation"].as_u64().map(|n| n.to_string()).as_deref()
                            && ["label", "value", "color"].iter().all(|k| d[k] == m[k])
                    })
            })
        })
}
async fn settled(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    count: usize,
    pending: &str,
) -> Result<Value, String> {
    for _ in 0..150 {
        let m = model(state)?;
        let dom = wait_dom(d, "window.__listSelectOptions.snapshot()").await?;
        if m["rows"].as_array().map(Vec::len) == Some(count)
            && m["pending"] == pending
            && dom_matches(&dom, &m)
        {
            return Ok(json!({"model":m,"dom":dom}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Owned Select input/rows did not settle".into())
}
async fn input(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    value: &str,
) -> Result<Value, String> {
    let m = model(state)?;
    let n = m["rows"].as_array().ok_or("Missing rows")?.len();
    checked(
        d,
        &format!(
            "window.__listSelectOptions.input({},{})",
            json!(value),
            proof(&m)
        ),
    )
    .await?;
    let result = settled(state, d, n, value).await?;
    if result["model"]["epoch"] != m["epoch"] || result["model"]["rows"] != m["rows"] {
        return Err("Pending input changed captured rows or epoch".into());
    }
    Ok(result)
}
async fn add(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    label: &str,
    twice: bool,
) -> Result<Value, String> {
    input(state, d, label).await?;
    let before = model(state)?;
    let rows = before["rows"].as_array().ok_or("Missing option rows")?;
    checked(
        d,
        &format!("window.__listSelectOptions.add({},{twice})", proof(&before)),
    )
    .await?;
    let after = settled(state, d, rows.len() + 1, "").await?;
    let next = &after["model"];
    if next["epoch"] == before["epoch"]
        || next["choice"] != before["choice"]
        || next["rows"].as_array().unwrap()[..rows.len()] != rows[..]
        || next["rows"][rows.len()]["label"] != label.trim()
    {
        return Err("Add failed synchronous consumption or retained options".into());
    }
    Ok(json!({"before":before,"after":after,"twice":twice}))
}
async fn remove(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    generation: u64,
) -> Result<Value, String> {
    let before = model(state)?;
    let mut expected = before["rows"].as_array().ok_or("Missing rows")?.clone();
    let n = expected.len();
    expected.retain(|r| r["generation"] != generation);
    if expected.len() + 1 != n {
        return Err("Missing exact row generation".into());
    }
    checked(
        d,
        &format!(
            "window.__listSelectOptions.remove({generation},{})",
            proof(&before)
        ),
    )
    .await?;
    let after = settled(state, d, n - 1, before["pending"].as_str().unwrap()).await?;
    if after["model"]["rows"] != json!(expected) || after["model"]["epoch"] != before["epoch"] {
        return Err("Remove affected another owned row".into());
    }
    Ok(json!({"before":before,"after":after,"removed":generation}))
}
pub(in super::super) async fn top(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listSelectOptions.top({})", proof(&model(state)?)),
    )
    .await
}
async fn format(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    checked_value: bool,
) -> Result<Value, String> {
    let before = model(state)?;
    let wanted = if checked_value {
        "multi_select"
    } else {
        "single_select"
    };
    let changed = before["format"] != wanted;
    let action = checked(
        d,
        &format!(
            "window.__listSelectOptions.format({checked_value},{})",
            proof(&before)
        ),
    )
    .await?;
    for _ in 0..150 {
        let after = model(state)?;
        if after["format"] == wanted
            && (after["choice"] != before["choice"]) == changed
            && (after["epoch"] != before["epoch"]) == changed
        {
            if after["rows"] != before["rows"]
                || after["pending"] != before["pending"]
                || after["serial"] != before["serial"]
                || after["visit"] != before["visit"]
                || after["selection"] != before["selection"]
            {
                return Err("Format changed owned option input".into());
            }
            let dom = wait_dom(
                d,
                &format!(
                    "window.__listSelectOptions.formatSettled({})",
                    proof(&after)
                ),
            )
            .await?;
            if !dom_matches(&dom["after"], &after) || model(state)? != after {
                return Err("Format DOM/model changed".into());
            }
            return Ok(
                json!({"before":before,"after":after,"changed":changed,"action":action,"dom":dom}),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Cardinality event did not settle".into())
}
pub(in super::super) async fn fill(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if model(state)?["rows"] != json!([]) {
        return Err("Expected empty fresh option draft".into());
    }
    let mode = rename_fixture::field_create_mode().ok_or("Missing mode")?;
    let cardinality = if mode == FieldCreateMode::MultiSelect {
        Some(format(state, d, true).await?)
    } else {
        None
    };
    let mut adds = vec![];
    for label in rename_fixture::select_create::LABELS {
        let value = if label == rename_fixture::select_create::LABELS[1] {
            format!("  {label}  ")
        } else {
            label.into()
        };
        adds.push(add(state, d, &value, false).await?);
    }
    let final_state = settled(state, d, 8, "").await?;
    let dom = &final_state["dom"];
    // Build expectation from actual DOM strings, not request/intent/server under test.
    let choices = dom["rows"]
        .as_array()
        .ok_or("Missing rendered rows")?
        .iter()
        .map(|r| json!({"label":r["label"],"value":r["value"],"color":r["color"]}))
        .collect::<Vec<_>>();
    let options = json!({"format":dom["format"],"choices":choices});
    rename_fixture::select_create::validate_mode(mode, &options)?;
    Ok(json!({"options":options,"state":final_state,"adds":adds,"cardinality":cardinality}))
}
async fn within_keyboard_deadline(
    deadline: tokio::time::Instant,
    future: impl std::future::Future<Output = Result<Value, String>>,
) -> Result<Value, String> {
    let result = tokio::time::timeout_at(deadline, future)
        .await
        .map_err(|_| "Trusted Tab sequence timed out".to_owned())?;
    // Timeout polls the inner future first. A ready result after a blocked poll
    // must still fail the same absolute deadline.
    if tokio::time::Instant::now() >= deadline {
        return Err("Trusted Tab sequence timed out".into());
    }
    result
}
fn cleanup_result(
    primary: Result<Value, String>,
    cleanup: Result<Value, String>,
) -> Result<Value, String> {
    match (primary, cleanup) {
        (Err(primary), Err(cleanup)) => Err(format!("{primary}; keyboard cleanup: {cleanup}")),
        (Err(primary), _) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(format!("Keyboard cleanup failed: {cleanup}")),
        (Ok(value), Ok(_)) => Ok(value),
    }
}
async fn keyboard(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = model(state)?;
    let started = tokio::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(15);
    let mode = rename_fixture::field_create_mode().ok_or("Missing mode")?;
    let nonce =
        std::env::var(format!("{}_NONCE", mode.env_prefix())).map_err(|_| "Missing owned nonce")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let identity = json!({"version":1,"fixture":fixture,"nonce":nonce,"pid":std::process::id(),"owner":proof(&before)});
    let run = within_keyboard_deadline(deadline, async {
        let begin = checked(
            d,
            &format!("window.__listSelectOptions.beginTabs({})", proof(&before)),
        )
        .await?;
        for ordinal in 0..4 {
            let arm = checked(d, &format!("window.__listSelectOptions.armTab({ordinal})")).await?;
            let mut request = identity.clone();
            request["ordinal"] = json!(ordinal);
            request["expected"] = arm["expected"].clone();
            tokio::task::spawn_blocking(move || {
                use std::io::Write;
                std::io::stderr()
                    .lock()
                    .write_all(format!("{}-keyboard: {request}\n", mode.prefix()).as_bytes())
            })
            .await
            .map_err(|_| "Keyboard receipt writer panicked")?
            .map_err(|_| "Keyboard receipt write failed")?;
            loop {
                let status = checked(d, "window.__listSelectOptions.tabStatus()").await?;
                let count = status["count"].as_u64().ok_or("Missing native Tab count")?;
                if count > ordinal + 1 {
                    return Err("Extra native Tab event".into());
                }
                if count == ordinal + 1 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if model(state)? != before {
                return Err("Keyboard traversal changed owned Select draft".into());
            }
        }
        let result = checked(d, "window.__listSelectOptions.endTabs()").await?;
        Ok(json!({"begin":begin,"result":result}))
    })
    .await;
    let sequence_ms = started.elapsed().as_secs_f64() * 1000.0;
    // Cleanup is a separate bounded phase and never replaces the primary failure.
    let cleanup_started = tokio::time::Instant::now();
    let cleanup_deadline = cleanup_started + std::time::Duration::from_secs(2);
    let cleanup = tokio::time::timeout_at(
        cleanup_deadline,
        checked(d, "window.__listSelectOptions.cleanupTabs()"),
    )
    .await
    .map_err(|_| "Keyboard cleanup timed out".to_owned())
    .and_then(|r| r);
    let events = cleanup_result(run, cleanup)?;
    if model(state)? != before {
        return Err("Keyboard cleanup changed owned Select draft".into());
    }
    let mut completion = identity;
    completion["events"] = events["result"]["events"].clone();
    completion["cleaned"] = json!(true);
    completion["sequenceElapsedMs"] = json!(sequence_ms);
    completion["cleanupElapsedMs"] = json!(cleanup_started.elapsed().as_secs_f64() * 1000.0);
    let receipt = completion.clone();
    // The immediate completion is observation only, never another input/ACK request.
    tokio::time::timeout_at(
        cleanup_deadline,
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("{}-keyboard-complete: {receipt}\n", mode.prefix()).as_bytes())
        }),
    )
    .await
    .map_err(|_| "Keyboard completion output timed out")?
    .map_err(|_| "Keyboard completion writer panicked")?
    .map_err(|_| "Keyboard completion write failed")?;
    if tokio::time::Instant::now() >= cleanup_deadline {
        return Err("Keyboard cleanup/completion deadline".into());
    }
    Ok(
        json!({"begin":events["begin"],"result":events["result"],"completion":completion,"cleaned":true,"sameDraft":true,"inputTargetingAtomic":false}),
    )
}
#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[tokio::test]
    async fn actual_keyboard_deadline_bounds_slow_await_and_late_ready_result() {
        struct DropProbe(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let polled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reached.clone();
        let polls = polled.clone();
        let drop_probe = DropProbe(dropped.clone());
        // Keep the sender held: scheduling cannot make this input ready when
        // both a short deadline and a longer timer have elapsed before a poll.
        let (held, mut receiver) = tokio::sync::oneshot::channel::<()>();
        let result = within_keyboard_deadline(
            tokio::time::Instant::now() + std::time::Duration::from_millis(2),
            async move {
                let _drop_probe = drop_probe;
                std::future::poll_fn(move |cx| {
                    polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::future::Future::poll(std::pin::Pin::new(&mut receiver), cx)
                })
                .await
                .expect("the held sender must not finish before timeout");
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(json!({"ok":true}))
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(polled.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            held.send(()).is_err(),
            "timeout must drop its pending input"
        );
        let result = within_keyboard_deadline(
            tokio::time::Instant::now() + std::time::Duration::from_millis(2),
            async {
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok(json!({"ok":true}))
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            within_keyboard_deadline(
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                async { Ok(json!({"ok":true})) }
            )
            .await
            .unwrap(),
            json!({"ok":true})
        );
    }
    #[test]
    fn cleanup_failure_never_masks_primary_or_allows_success() {
        assert_eq!(
            cleanup_result(Err("primary".into()), Err("cleanup".into())).unwrap_err(),
            "primary; keyboard cleanup: cleanup"
        );
        assert_eq!(
            cleanup_result(Err("primary".into()), Ok(json!({"ok":true}))).unwrap_err(),
            "primary"
        );
        assert!(cleanup_result(Ok(json!({"ok":true})), Err("cleanup".into())).is_err());
    }
}
pub(in super::super) async fn prelude(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Proof,
) -> Result<Value, String> {
    checked(d, include_str!("options.js")).await?;
    let initial = model(state)?;
    if initial["format"] != "single_select"
        || initial["rows"] != json!([])
        || initial["pending"] != ""
    {
        return Err("Select prelude did not start empty".into());
    }
    // Both input events are dispatched in one evaluation task before a DOM patch.
    let consecutive = checked(d, &format!("(()=>{{const first=window.__listSelectOptions.input('R',{});const second=window.__listSelectOptions.input('Ready',{});return{{ok:first.ok&&second.ok,first,second}};}})()",proof(&initial),proof(&initial))).await?;
    settled(state, d, 0, "Ready").await?;
    let first = add(state, d, "Ready", true).await?;
    let cardinality = if p.mode == FieldCreateMode::MultiSelect {
        input(state, d, &"x".repeat(257)).await?;
        let mut events = vec![];
        for checked in [true, false, true, true] {
            events.push(format(state, d, checked).await?);
        }
        input(state, d, "").await?;
        Some(events)
    } else {
        None
    };
    let temp = add(state, d, "Temporary", false).await?;
    let old = temp["after"]["model"]["rows"][1]["generation"]
        .as_u64()
        .unwrap();
    let removed = remove(state, d, old).await?;
    let again = add(state, d, "Temporary", false).await?;
    let fresh = again["after"]["model"]["rows"][1]["generation"]
        .as_u64()
        .unwrap();
    if fresh == old {
        return Err("Removed row generation reused".into());
    }
    let removed_again = remove(state, d, fresh).await?;
    // Use actual kind events; the shared pure/VDOM tests retain old callback proofs.
    top(state, d).await?;
    checked(d, "window.__listTextCreate.selectKind('text')").await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-new-field-kind')?.value==='text'&&!document.getElementById('slack-list-select-options')})").await?;
    checked(d, "window.__listTextCreate.selectKind('select')").await?;
    let reset = settled(state, d, 0, "").await?;
    if reset["model"]["format"] != "single_select" || reset["model"]["choice"] == initial["choice"]
    {
        return Err("Type ABA reused epoch".into());
    }
    let mut invalid = vec![];
    for text in [" ".to_owned(), "x".repeat(257)] {
        let result = input(state, d, &text).await?;
        let gate = checked(
            d,
            &format!(
                "window.__listSelectOptions.invalid({})",
                proof(&model(state)?)
            ),
        )
        .await?;
        invalid.push(json!({"input":result,"gate":gate}));
    }
    input(state, d, "").await?;
    let config = fill(state, d).await?;
    input(state, d, "Ninth").await?;
    let capacity = checked(
        d,
        &format!(
            "window.__listSelectOptions.capacity({})",
            proof(&model(state)?)
        ),
    )
    .await?;
    input(state, d, "").await?;
    for (index, name) in STAGES.iter().enumerate() {
        if p.stages.len() != index {
            return Err("Wrong option capture order".into());
        }
        p.stable(state, 0)?;
        let before = model(state)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        if ledger["requests"] != json!([])
            || ledger["responses"] != json!([])
            || ledger["finished"] != json!([])
            || ledger["writePending"] != false
            || ledger["readPending"] != false
        {
            return Err("Option draft emitted a request".into());
        }
        let geometry = checked(
            d,
            &format!(
                "window.__listSelectOptions.pair({},{})",
                index * 2,
                proof(&before)
            ),
        )
        .await?;
        if !dom_matches(&geometry["snapshot"], &before) {
            return Err("Option pair DOM differs from owned draft".into());
        }
        p.captures
            .insert((*name).into(), harness::capture(d, name).await?);
        p.stable(state, 0)?;
        if model(state)? != before || rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Option draft changed during capture".into());
        }
        p.stages.push(json!({"ordinal":index,"stage":name,"geometry":geometry,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"draft":before,"noSession":true,"noTransport":true}));
    }
    let tabs = keyboard(state, d).await?;
    top(state, d).await?;
    Ok(
        json!({"initial":initial,"consecutiveInputs":consecutive,"first":first,"temporary":temp,"removed":removed,"again":again,"removedAgain":removed_again,"typeReset":reset,"cardinality":cardinality,"invalid":invalid,"configuration":config,"capacity":capacity,"keyboard":tabs}),
    )
}

pub(in super::super) async fn verify(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    expected: &Value,
) -> Result<Value, String> {
    let m = model(state)?;
    let dom = checked(d, "window.__listSelectOptions.snapshot()").await?;
    if !dom_matches(&dom, &m) || m["pending"] != "" {
        return Err("Captured Select options no longer match owned DOM".into());
    }
    let rows = dom["rows"].as_array().ok_or("Missing captured rows")?;
    let actual = json!({"format":dom["format"],"choices":rows.iter().map(|r|json!({"label":r["label"],"value":r["value"],"color":r["color"]})).collect::<Vec<_>>()});
    if actual != *expected {
        return Err("Captured Select options changed".into());
    }
    Ok(json!({"dom":dom,"model":m,"options":actual}))
}
