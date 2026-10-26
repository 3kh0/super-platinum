use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let change = edit(s, d, "required", vec![toggle(1), toggle(2)]).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "queued-valid-latest-overmax",
        json!({"change":change,"oldActionSurvivesInvalidCallback":true}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][1]["element"]["max_selected_items"] = json!(4)
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "raised-max",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "required", vec![remove(2)]).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "corrected", json!({"change":change}), st, cap).await?;
    let change = edit(s, d, "required", vec![remove(0), remove(0)]).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "queued-valid-latest-required-empty",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![toggle(0)]).await?;
    let sibling = sibling(s, "Latest sibling").await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "latest-sibling",
        json!({"change":change,"sibling":sibling}),
        st,
        cap,
    )
    .await?;
    let press = click(s, d).await?;
    counts(s, 4, 4, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 4, 4, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-required-empty",
        json!({"click":press}),
        st,
        cap,
    )
    .await
}
