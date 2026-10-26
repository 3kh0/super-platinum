use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    base::configure(&mut s.write(), base::Reply::Held);
    let p = input(s, d, "live", "held@example.com").await?;
    counts(s, 1, 0, 0, 0).await?;
    let id = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_u64)
        .ok_or("Held email reply missing")?;
    shot(
        s,
        d,
        n,
        "pending-disabled",
        json!({"input":p,"pending":id}),
        st,
        cap,
    )
    .await?;
    if !base::release(&mut s.write(), id, base::Reply::Accepted) {
        return Err("Held email reply did not release".into());
    }
    counts(s, 1, 1, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "accepted-no-replay",
        json!({"released":id,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Uncertain);
    let p = input(s, d, "live", "uncertain@example.com").await?;
    counts(s, 2, 2, 0, 0).await?;
    let quiet = silence(s).await?;
    let notice = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "uncertain-no-replay",
        json!({"input":p,"silence":quiet,"toastDismissal":notice}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Offline);
    let p = input(s, d, "live", "offline@example.com").await?;
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
    let p = input(s, d, "live", "fresh@example.com").await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(s, d, n, "fresh-explicit", p, st, cap).await?;
    base::configure(&mut s.write(), base::Reply::Rejected);
    let p = input(s, d, "live", "rejected@example.com").await?;
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
    let p = input(s, d, "live", "recovered@example.com").await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(s, d, n, "rejected-recovery", p, st, cap).await?;
    let index = base::admission::arm(&mut s.write());
    let p = held_input(s, d, "live", "stale-source@example.com", index).await?;
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
        return Err("Email source gate accepted stale source".into());
    }
    shot(s,d,n,"source-retired",json!({"attempt":index,"input":p,"held":held,"domRetired":dom_retired,"source":source,"silence":quiet}),st,cap).await?;
    let index = base::admission::arm(&mut s.write());
    let p = held_input(s, d, "live", "stale-owner@example.com", index).await?;
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
        return Err("Email owner gate accepted stale owner".into());
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
