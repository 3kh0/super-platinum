//! Actual failed-row custody around a separate background metadata response.
use super::*;
use backend::posting as endpoint_posting;

pub(crate) fn requested() -> bool {
    endpoint_posting::requested()
}
fn refresh(p: &Value) -> &Value {
    last(p)["confirmationShare"]["operation"]["postingRefresh"]
        .as_array()
        .and_then(|v| v.last())
        .unwrap_or(&Value::Null)
}
fn failed_row(v: &Value) -> Result<(), String> {
    let row = &v["native"]["shareResendRow"];
    if row["status"] != "Not sent"
        || !row["resend"].is_null()
        || row["cancel"]["text"] != "Cancel"
        || row["cancel"]["hitPoint"].is_null()
    {
        return Err("Original restricted failed row must retain exposed Cancel only".into());
    }
    Ok(())
}
pub(crate) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
) -> Result<Value, String> {
    if !requested() {
        return Err("Named posting fixture required".into());
    }
    let case = endpoint_posting::case();
    let model = backend::capture(&state.peek())?;
    let before = observe(state, desktop, visit).await?;
    let mut captures = json!({});
    let mut evidence = json!({});
    let mut retry = Value::Null;
    if case == "retry-restricted" {
        let row = &before["native"]["shareResendRow"];
        if row["status"] != "Delivery unconfirmed" {
            return Err("Retry requires actual original unknown row".into());
        }
        let activation = evaluate(desktop, &resend::script("click", row)).await?;
        until(
            state,
            visit,
            "posting retry original response held",
            |p, s| {
                last(p)["confirmationShare"]["operation"]["resends"]
                    .as_array()
                    .and_then(|v| v.last())
                    .is_some_and(|v| v["finished"] == false)
                    && s["confirmationAdd"]["share"]["resend"]["held"] == json!([1])
            },
        )
        .await?;
        let held = observe(state, desktop, visit).await?;
        let pending = &held["native"]["shareResendRow"];
        if pending["messageId"] != row["messageId"]
            || !pending["resend"].is_null()
            || !pending["cancel"].is_null()
        {
            return Err("Pending retry changed identity or exposes recovery".into());
        }
        let quiet = quiet(state, desktop, visit).await?;
        if quiet["before"] != held || quiet["after"] != held {
            return Err("Held posting retry changed".into());
        }
        let cap = resend::capture(state, desktop, name, visit, "posting-retry-pending").await?;
        if cap["before"] != held {
            return Err("Retry pending capture changed".into());
        }
        captures["posting-retry-pending"] = cap["path"].clone();
        evidence["posting-retry-pending"] = cap;
        let release = backend::resend::release(&model)?;
        until(
            state,
            visit,
            "restricted retry starts separate refresh",
            |p, s| {
                last(p)["confirmationShare"]["operation"]["resends"]
                    .as_array()
                    .and_then(|v| v.last())
                    .is_some_and(|v| v["finished"] == true)
                    && s["confirmationAdd"]["share"]["posting"]["held"] == json!([1])
                    && refresh(p)["finished"] == false
            },
        )
        .await?;
        retry = json!({"before":before,"activation":activation,"held":held,"quiet":quiet,"release":release,"after":observe(state,desktop,visit).await?});
    }
    until(
        state,
        visit,
        "original independent posting response held",
        |p, s| {
            refresh(p)["finished"] == false
                && s["confirmationAdd"]["share"]["posting"]["held"] == json!([1])
        },
    )
    .await?;
    let pending = observe(state, desktop, visit).await?;
    failed_row(&pending)?;
    let pending_quiet = quiet(state, desktop, visit).await?;
    if pending_quiet["before"] != pending || pending_quiet["after"] != pending {
        return Err("Held posting response changed failed row".into());
    }
    let cap = resend::capture(state, desktop, name, visit, "posting-pending").await?;
    if cap["before"] != pending {
        return Err("Posting pending capture changed".into());
    }
    captures["posting-pending"] = cap["path"].clone();
    evidence["posting-pending"] = cap;
    let mut newer = Value::Null;
    if case == "held-newer" {
        let ingress = endpoint_posting::newer(state).await?;
        let after = observe(state, desktop, visit).await?;
        failed_row(&after)?;
        let quiet = quiet(state, desktop, visit).await?;
        if quiet["before"] != after || quiet["after"] != after {
            return Err("Newer source not settled".into());
        }
        let cap = resend::capture(state, desktop, name, visit, "posting-newer").await?;
        if cap["before"] != after {
            return Err("Newer posting capture changed".into());
        }
        captures["posting-newer"] = cap["path"].clone();
        evidence["posting-newer"] = cap;
        newer = json!({"before":pending,"ingress":ingress,"after":after,"quiet":quiet});
    }
    let release = endpoint_posting::release(&model)?;
    until(state, visit, "original posting response settled", |p, s| {
        refresh(p)["finished"] == true
            && s["confirmationAdd"]["share"]["posting"]["settlements"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    failed_row(&after)?;
    let expected = match case.as_str() {
        "read-failure" => "failed",
        "held-newer" => "retired",
        _ => "applied",
    };
    if refresh(&after["firstPublish"])["outcome"] != expected {
        return Err(format!(
            "Posting refresh expected {expected}: {}",
            refresh(&after["firstPublish"])
        ));
    }
    let quiet = quiet(state, desktop, visit).await?;
    if quiet["before"] != after || quiet["after"] != after {
        return Err("Settled posting response changed".into());
    }
    let cap = resend::capture(state, desktop, name, visit, "posting-settled").await?;
    if cap["before"] != after {
        return Err("Posting settled capture changed".into());
    }
    captures["posting-settled"] = cap["path"].clone();
    evidence["posting-settled"] = cap;
    Ok(
        json!({"before":before,"retry":retry,"pending":pending,"pendingQuiet":pending_quiet,"newer":newer,"release":release,"after":after,"quiet":quiet,"captures":captures,"captureEvidence":evidence}),
    )
}
