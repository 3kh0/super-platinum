use super::helpers::refresh;
use super::*;
pub(super) async fn selection(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for (index, (stage, block, action, value)) in [
        ("action-selected", "controls", "first", Some(1)),
        ("second-selected", "controls", "second", Some(2)),
        ("accessory-selected", "summary", "accessory", Some(2)),
        ("empty-option", "controls", "second", Some(3)),
        ("input-clear", "pick", "value", None),
    ]
    .into_iter()
    .enumerate()
    {
        let change = choose(s, d, block, action, value).await?;
        counts(s, index + 1, index + 1, 0, 0).await?;
        shot(s, d, n, stage, json!({"change":change}), st, cap).await?;
    }
    let press = click(s, d).await?;
    counts(s, 5, 5, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 5, 5, 1, 1).await?;
    shot(s, d, n, "button-state", json!({"click":press}), st, cap).await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][3]["elements"][0]["initial_option"] = base::option("gamma", "Gamma");
        raw["blocks"][4]["accessory"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "props-changed",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    Ok(())
}
pub(super) async fn pending(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let press = click(s, d).await?;
    let suppressed = blocked_click(s, d).await?;
    counts(s, 0, 0, 1, 0).await?;
    shot(
        s,
        d,
        n,
        "button-held",
        json!({"click":press,"beforeDeadlineSuppression":suppressed}),
        st,
        cap,
    )
    .await?;
    let press = click(s, d).await?;
    counts(s, 0, 0, 2, 0).await?;
    shot(s, d, n, "button-repeat", json!({"click":press}), st, cap).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 0, 0, 2, 1).await?;
    shot(
        s,
        d,
        n,
        "first-completion",
        json!({"otherButtonStillPending":true}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Held);
    let change = choose(s, d, "controls", "first", Some(1)).await?;
    counts(s, 1, 0, 2, 1).await?;
    shot(s, d, n, "static-held", json!({"change":change}), st, cap).await?;
    let press = click(s, d).await?;
    counts(s, 1, 0, 3, 1).await?;
    shot(
        s,
        d,
        n,
        "button-while-static",
        json!({"click":press}),
        st,
        cap,
    )
    .await?;
    buttons::release(&mut s.write(), true);
    counts(s, 1, 0, 3, 2).await?;
    shot(
        s,
        d,
        n,
        "button-completion",
        json!({"staticAndLaterButtonStillPending":true}),
        st,
        cap,
    )
    .await?;
    let change = choose(s, d, "controls", "second", Some(2)).await?;
    counts(s, 2, 0, 3, 2).await?;
    shot(s, d, n, "other-static", json!({"change":change}), st, cap).await?;
    let first = snapshot(&s.peek())["requests"][0]["id"].as_u64().unwrap();
    base::release(&mut s.write(), first, base::Reply::Accepted);
    counts(s, 2, 1, 3, 2).await?;
    shot(
        s,
        d,
        n,
        "static-completion",
        json!({"otherStaticAndButtonStillPending":true}),
        st,
        cap,
    )
    .await?;
    let second = snapshot(&s.peek())["requests"][1]["id"].as_u64().unwrap();
    base::release(&mut s.write(), second, base::Reply::Accepted);
    buttons::release(&mut s.write(), false);
    counts(s, 2, 2, 3, 3).await?;
    let quiet = silence(s).await?;
    let toast = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "uncertain-no-replay",
        json!({"silence":quiet,"toastDismissal":toast}),
        st,
        cap,
    )
    .await?;
    let press = click(s, d).await?;
    counts(s, 2, 2, 4, 3).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 2, 2, 4, 4).await?;
    shot(s, d, n, "fresh-click", json!({"click":press}), st, cap).await?;
    Ok(())
}
pub(super) async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    base::arm_gate(&mut s.write());
    let change = choose(s, d, "controls", "first", Some(1)).await?;
    gate(s).await?;
    let held = snapshot(&s.peek());
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][4]["text"]["text"] = json!("Updated release summary.");
    });
    base::release_gate(&mut s.write());
    refused(s, 1).await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"change":change,"held":held,"refresh":source}),
        st,
        cap,
    )
    .await?;
    base::arm_gate(&mut s.write());
    let change = choose(s, d, "summary", "accessory", Some(2)).await?;
    gate(s).await?;
    let held = snapshot(&s.peek());
    let flags = std::mem::take(
        &mut s
            .write()
            .core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled,
    );
    base::release_gate(&mut s.write());
    refused(s, 2).await?;
    shot(
        s,
        d,
        n,
        "context-retired",
        json!({"change":change,"held":held}),
        st,
        cap,
    )
    .await?;
    s.write()
        .core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled = flags;
    base::configure(&mut s.write(), base::Reply::Offline);
    let change = choose(s, d, "controls", "second", Some(1)).await?;
    counts(s, 1, 1, 0, 0).await?;
    let quiet = silence(s).await?;
    let toast = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "offline-no-replay",
        json!({"change":change,"silence":quiet,"toastDismissal":toast}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Accepted);
    let change = choose(s, d, "controls", "second", Some(2)).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "fresh-selection",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    Ok(())
}
