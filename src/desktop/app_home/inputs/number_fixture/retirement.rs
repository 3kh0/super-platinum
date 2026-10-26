use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    base::configure(&mut s.write(), base::Reply::Held);
    let first = input(s, d, "live", "6", json!("6"), wire(json!("6"))).await?;
    counts(s, 1, 0, 0, 0).await?;
    let second = input(s, d, "live", "7", json!("7"), wire(json!("7"))).await?;
    counts(s, 2, 0, 0, 0).await?;
    let ids = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    shot(
        s,
        d,
        n,
        "overlapping-enabled",
        json!({"first":first,"second":second,"ids":ids}),
        st,
        cap,
    )
    .await?;
    if !base::release(&mut s.write(), ids[0], base::Reply::Accepted) {
        return Err("First numeric reply missing".into());
    }
    counts(s, 2, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "first-completion-clears-loading",
        json!({"released":ids[0],"stillPending":ids[1]}),
        st,
        cap,
    )
    .await?;
    if !base::release(&mut s.write(), ids[1], base::Reply::Uncertain) {
        return Err("Second numeric reply missing".into());
    }
    counts(s, 2, 2, 0, 0).await?;
    let quiet = silence(s).await?;
    let notice = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "uncertain-no-replay",
        json!({"silence":quiet,"toastDismissal":notice}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Offline);
    let p = input(s, d, "live", "8", json!("8"), wire(json!("8"))).await?;
    counts(s, 3, 3, 0, 0).await?;
    let quiet = silence(s).await?;
    let notice = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "offline-no-replay",
        json!({"input":p,"silence":quiet,"toastDismissal":notice}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Accepted);
    let p = input(s, d, "live", "6", json!("6"), wire(json!("6"))).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(s, d, n, "fresh-explicit", p, st, cap).await?;
    base::configure(&mut s.write(), base::Reply::Rejected);
    let p = input(s, d, "live", "7", json!("7"), wire(json!("7"))).await?;
    counts(s, 5, 5, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "app-rejected",
        json!({"input":p,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Accepted);
    let p = input(s, d, "live", "8", json!("8"), wire(json!("8"))).await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(s, d, n, "rejected-recovery", p, st, cap).await?;
    let index = base::admission::arm(&mut s.write());
    let p = input(s, d, "live", "9", json!("9"), wire(json!("9"))).await?;
    let held = gated(s).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][5]["label"]["text"] = json!("Current release notes")
    });
    let dom_retired = retired(&held, index).await?;
    base::admission::release(&mut s.write())?;
    let quiet = silence(s).await?;
    counts(s, 6, 6, 0, 0).await?;
    let after = snapshot(&s.peek());
    if after["admission"]["attempts"][index]["sourceCurrent"] != false
        || after["admission"]["attempts"][index]["accepted"] != false
    {
        return Err("Numeric source gate did not refuse stale source".into());
    }
    shot(
        s,
        d,
        n,
        "source-retired",
        json!({"attempt":index,"input":p,"held":held,"domRetired":dom_retired,"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let index = base::admission::arm(&mut s.write());
    let p = input(s, d, "live", "8", json!("8"), wire(json!("8"))).await?;
    let held = gated(s).await?;
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    let dom_retired = retired(&held, index).await?;
    base::admission::release(&mut s.write())?;
    let quiet = silence(s).await?;
    counts(s, 6, 6, 0, 0).await?;
    let after = snapshot(&s.peek());
    if after["admission"]["attempts"][index]["sourceCurrent"] != false
        || after["admission"]["attempts"][index]["accepted"] != false
    {
        return Err("Numeric owner gate did not refuse stale owner".into());
    }
    shot(
        s,
        d,
        n,
        "owner-retired",
        json!({"attempt":index,"input":p,"held":held,"domRetired":dom_retired,"silence":quiet}),
        st,
        cap,
    )
    .await
}
