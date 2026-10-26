use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    base::arm_gate(&mut s.write());
    let change = edit(s, d, "pick", vec![select(1)]).await?;
    gate(s).await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][2]["label"]["text"] = json!("Current release notes")
    });
    base::release_gate(&mut s.write());
    refused(s, 1).await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"change":change,"refresh":source}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Held);
    let change = edit(s, d, "pick", vec![select(0)]).await?;
    counts(s, 1, 0, 0, 0).await?;
    shot(s, d, n, "pending", json!({"change":change}), st, cap).await?;
    let id = snapshot(&s.peek())["requests"][0]["id"].as_u64().unwrap();
    base::release(&mut s.write(), id, base::Reply::Uncertain);
    counts(s, 1, 1, 0, 0).await?;
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
    base::configure(&mut s.write(), base::Reply::Offline);
    let change = edit(s, d, "pick", vec![select(1)]).await?;
    counts(s, 2, 2, 0, 0).await?;
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
    let change = edit(s, d, "pick", vec![select(2)]).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s, d, n, "fresh-explicit", json!({"change":change}), st, cap).await?;
    base::arm_gate(&mut s.write());
    let change = edit(s, d, "required", vec![select(1)]).await?;
    gate(s).await?;
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    base::release_gate(&mut s.write());
    refused(s, 2).await?;
    shot(s, d, n, "owner-retired", json!({"change":change}), st, cap).await
}
