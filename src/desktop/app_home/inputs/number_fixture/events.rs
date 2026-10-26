use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = blur(s, d, "empty", Value::Null, json!({"type":"number_input"})).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "pristine-empty-blur",
        json!({"blur":p,"silence":quiet,"boundedOriginalExceptionNotReproduced":true}),
        st,
        cap,
    )
    .await?;
    let p = input(
        s,
        d,
        "enter",
        "0012.500",
        json!("0012.500"),
        wire(json!("12.5")),
    )
    .await?;
    counts(s, 0, 0, 0, 0).await?;
    shot(s, d, n, "normalized-parent-literal-draft", p, st, cap).await?;
    let started = Instant::now();
    let p = enter(s, d, "enter", json!("0012.500"), wire(json!("0012.500"))).await?;
    let sibling = notes(s, "Latest number sibling").await?;
    let before = trailing(&s.peek(), started, 0)?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "literal-enter",
        json!({"enter":p,"notes":sibling,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    let p = blur(s, d, "enter", json!("0012.5"), wire(json!("0012.5"))).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "blur-normalization", p, st, cap).await?;
    let started = Instant::now();
    let a = input(s, d, "live", "8", json!("8"), wire(json!("8"))).await?;
    let b = input(s, d, "live", "9.0", json!("9.0"), wire(json!("9"))).await?;
    let before = trailing(&s.peek(), started, 2)?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "character-trailing",
        json!({"first":a,"second":b,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    let p = blur(s, d, "live", json!("9"), wire(json!("9"))).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(s, d, n, "character-blur", p, st, cap).await?;
    let a = input(s, d, "live", "9.00", json!("9.00"), wire(json!("9"))).await?;
    let b = input(s, d, "live", "9.00", json!("9.00"), wire(json!("9"))).await?;
    counts(s, 5, 5, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "same-parent-new-spelling",
        json!({"changed":a,"equal":b,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let p = input(s, d, "integer", "5.5", json!("4"), wire(json!("5.5"))).await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(s, d, n, "integer-local-rejection", p, st, cap).await?;
    let p = enter(s, d, "empty", Value::Null, wire(Value::Null)).await?;
    counts(s, 7, 7, 0, 0).await?;
    shot(s, d, n, "optional-empty-enter", p, st, cap).await?;
    let a = input(s, d, "quiet", "06.00", json!("06.00"), wire(json!("6"))).await?;
    let b = blur(s, d, "quiet", json!("06"), wire(json!("06"))).await?;
    let quiet = silence(s).await?;
    let pressed = button().await?;
    counts(s, 7, 7, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Number Button reply missing".into());
    }
    counts(s, 7, 7, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "quiet-button-state",
        json!({"input":a,"blur":b,"silence":quiet,"button":pressed}),
        st,
        cap,
    )
    .await?;
    let p = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":p,"silence":quiet}),
        st,
        cap,
    )
    .await
}
