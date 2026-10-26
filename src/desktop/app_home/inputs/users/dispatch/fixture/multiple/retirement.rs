use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    base::arm_gate(&mut s.write());
    let choice = toggle(s, d, "pick", BLAIR, &[ADA, BLAIR]).await?;
    gate(s).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][4]["label"]["text"] = json!("Current release notes")
    });
    base::release_gate(&mut s.write());
    refused(s, 1).await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"choice":choice,"refresh":source}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Held);
    let choice = toggle(s, d, "pick", ADA, &[BLAIR]).await?;
    counts(s, 1, 0, 0, 0).await?;
    shot(s, d, n, "pending", choice, st, cap).await?;
    let id = snapshot(&s.peek())["requests"][0]["id"].as_u64().unwrap();
    if !base::release(&mut s.write(), id, base::Reply::Uncertain) {
        return Err("Multi-user held reply lost".into());
    }
    counts(s, 1, 1, 0, 0).await?;
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
    let choice = toggle(s, d, "pick", CASEY, &[BLAIR, CASEY]).await?;
    counts(s, 2, 2, 0, 0).await?;
    let quiet = silence(s).await?;
    let notice = dismiss(s, d, n, cap).await?;
    shot(
        s,
        d,
        n,
        "offline-no-replay",
        json!({"choice":choice,"silence":quiet,"toastDismissal":notice}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Accepted);
    let choice = toggle(s, d, "pick", BLAIR, &[CASEY]).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s, d, n, "fresh-explicit", choice, st, cap).await?;
    base::arm_gate(&mut s.write());
    let choice = toggle(s, d, "required", BLAIR, &[BLAIR]).await?;
    gate(s).await?;
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    base::release_gate(&mut s.write());
    refused(s, 2).await?;
    shot(s, d, n, "owner-retired", json!({"choice":choice}), st, cap).await
}
