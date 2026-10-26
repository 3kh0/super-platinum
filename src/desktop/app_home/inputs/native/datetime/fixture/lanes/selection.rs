use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let before = snapshot(&s.peek());
    let composition = edit(s, vec![commit("pick", "composing-enter")], None).await?;
    let opened = edit(s, vec![commit("pick", "enter")], None).await?;
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "opened",
        json!({"gesture":opened,"composition":composition,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let escape = edit(s, vec![commit("pick", "escape")], None).await?;
    let change = edit(s, vec![op("pick", "9", "blur")], None).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "time-preserves-seconds",
        json!({"escape":escape,"change":change,"seedEpoch":1792946262i64}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![date("pick", "2026-10-26", "change")], None).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "date-resets-seconds",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![choose("pick", "17:00")], None).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s, d, n, "time-option", json!({"change":change}), st, cap).await?;
    let change = edit(s, vec![date("pick", "", "change")], None).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(s, d, n, "optional-clear", json!({"change":change}), st, cap).await?;
    let before_clock = clock().await?;
    let change = edit(s, vec![date("pick", "1969-12-30", "change")], None).await?;
    let after_clock = clock().await?;
    counts(s, 5, 5, 0, 0).await?;
    shot(s,d,n,"pre1970",json!({"change":change,"clockBefore":before_clock,"clockAfter":after_clock,"clockMutation":false}),st,cap).await?;
    let reset_proof = reset(s, d).await?;
    settle(s, d).await?;
    let epoch_clock = clock().await?;
    let time = epoch_clock["epochZero"]["time"]
        .as_str()
        .ok_or("Epoch local Time missing")?
        .to_owned();
    let day = epoch_clock["epochZero"]["date"]
        .as_str()
        .ok_or("Epoch local Date missing")?
        .to_owned();
    let time_change = edit(s, vec![op("pick", &time, "blur")], None).await?;
    counts(s, 6, 6, 0, 0).await?;
    let date_change = edit(s, vec![date("pick", &day, "change")], None).await?;
    counts(s, 7, 7, 0, 0).await?;
    shot(s,d,n,"epoch-zero-null",json!({"reset":reset_proof,"localEpoch":epoch_clock,"timeChange":time_change,"dateChange":date_change,"epochAuthorityInjected":false}),st,cap).await?;
    // Preserve the real rapid focus transfer: the focused Date draft commits
    // against the null composite before the required Time callback.
    let mut required = op("required", "930", "blur");
    required["observeFocusedDate"] = json!(true);
    let change = edit(
        s,
        vec![
            required,
            date("required", "", "change"),
            op("required", "", "blur"),
            op("quiet", "5pm", "blur"),
        ],
        Some("Latest sibling"),
    )
    .await?;
    counts(s, 9, 9, 0, 0).await?;
    let press = click(s, d).await?;
    counts(s, 9, 9, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 9, 9, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "required-quiet-button",
        json!({"change":change,"click":press}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date_time");
    });
    settle(s, d).await?;
    let reset_proof = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-missing",
        json!({"refresh":source,"reset":reset_proof,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![date("pick", "", "change")], None).await?;
    counts(s, 10, 10, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "blank-callback-null",
        json!({"change":change}),
        st,
        cap,
    )
    .await
}
