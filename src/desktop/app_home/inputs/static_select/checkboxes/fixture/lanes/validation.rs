use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let change = edit_sibling(
        s,
        d,
        "required",
        vec![toggle(1), toggle(0), toggle(1)],
        "Latest sibling",
    )
    .await?;
    if change["elapsedMs"].as_f64().is_none_or(|ms| ms >= 450.0) {
        return Err("Checkbox burst missed bounded debounce interval".into());
    }
    let sibling = change["sibling"].clone();
    counts(s, 2, 2, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "queued-valid-latest-required-empty",
        json!({"change":change,"sibling":sibling}),
        st,
        cap,
    )
    .await?;
    let press = click(s, d).await?;
    counts(s, 2, 2, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 2, 2, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-required-empty",
        json!({"click":press}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "required", vec![toggle(3)]).await?;
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "required-empty-value-selected",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "required", vec![toggle(3)]).await?;
    let quiet = silence(s).await?;
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "required-empty-no-new-action",
        json!({"change":change,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![toggle(1), toggle(0)]).await?;
    if change["elapsedMs"].as_f64().is_none_or(|ms| ms >= 450.0) {
        return Err("Checkbox trailing burst missed bounded debounce interval".into());
    }
    counts(s, 5, 5, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "leading-and-trailing",
        json!({"change":change}),
        st,
        cap,
    )
    .await
}
