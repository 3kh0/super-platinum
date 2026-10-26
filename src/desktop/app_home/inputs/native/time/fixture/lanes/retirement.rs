use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let started = Instant::now();
    let first = edit(s, vec![op("pick", "10", "blur")], None).await?;
    let leading = prepared(s, 1, started).await?;
    let burst = edit(
        s,
        vec![op("pick", "11", "blur"), op("pick", "5pm", "none")],
        Some("Latest sibling"),
    )
    .await?;
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 450 {
        return Err("Time leading/trailing edits missed 450ms bound".into());
    }
    let before_delivery = snapshot(&s.peek());
    if before_delivery["requests"].as_array().map(Vec::len) != Some(1) {
        return Err("Time trailing action prepared before draft proof".into());
    }
    counts(s, 2, 2, 0, 0).await?;
    let after_delivery = snapshot(&s.peek());
    if before_delivery["nativeMounts"] != after_delivery["nativeMounts"]
        || before_delivery["formState"] != after_delivery["formState"]
    {
        return Err("Time trailing delivery mutated draft or parent".into());
    }
    shot(s,d,n,"trailing-with-draft",json!({"first":first,"leadingPrepared":leading,"burst":burst,"elapsedMs":elapsed,"beforeDelivery":before_delivery,"draftUnchanged":true}),st,cap).await?;
    // Close the observed draft through a new genuine edit, then exercise held final custody.
    base::arm_gate(&mut s.write());
    let change = edit(s, vec![op("pick", "12", "blur")], None).await?;
    gate(s).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][3]["label"]["text"] = json!("Current release notes")
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
    let change = edit(s, vec![op("pick", "13", "blur")], None).await?;
    counts(s, 3, 2, 0, 0).await?;
    shot(s, d, n, "pending", json!({"change":change}), st, cap).await?;
    let id = snapshot(&s.peek())["requests"][2]["id"].as_u64().unwrap();
    base::release(&mut s.write(), id, base::Reply::Uncertain);
    counts(s, 3, 3, 0, 0).await?;
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
    let change = edit(s, vec![op("pick", "14", "blur")], None).await?;
    counts(s, 4, 4, 0, 0).await?;
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
    let change = edit(s, vec![op("pick", "15", "blur")], None).await?;
    counts(s, 5, 5, 0, 0).await?;
    shot(s, d, n, "fresh-explicit", json!({"change":change}), st, cap).await?;
    base::arm_gate(&mut s.write());
    let change = edit(s, vec![op("required", "10", "blur")], None).await?;
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
