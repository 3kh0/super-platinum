use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let change = edit(s, d, "required", vec![select(3)]).await?;
    counts(s, 1, 1, 0, 0).await?;
    let repeated = same_checked(s, d, "required", 3).await?;
    shot(
        s,
        d,
        n,
        "required-empty-value-selected",
        json!({"change":change,"sameChecked":repeated}),
        st,
        cap,
    )
    .await?;
    let change = edit_sibling(
        s,
        d,
        "required",
        vec![select(1), select(0), select(1)],
        "Latest sibling",
    )
    .await?;
    if change["elapsedMs"].as_f64().is_none_or(|ms| ms >= 450.0) {
        return Err("Radio burst missed bounded debounce interval".into());
    }
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "leading-trailing-latest-state",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let press = click(s, d).await?;
    counts(s, 3, 3, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-latest-state",
        json!({"click":press}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![select(1), select(2)]).await?;
    if change["elapsedMs"].as_f64().is_none_or(|ms| ms >= 450.0) {
        return Err("Radio second burst missed bounded debounce interval".into());
    }
    counts(s, 5, 5, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "independent-radio-burst",
        json!({"change":change}),
        st,
        cap,
    )
    .await
}
