use super::*;
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let focus = act(s, d, "focus", "pick", json!({})).await?;
    selected_all(&focus["effect"])?;
    if focus["effect"]["target"]["type"] != "text"
        || focus["effect"]["target"]["dateSelected"] != "2026-10-25"
    {
        return Err("Datetime did not expose localized editable Date".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "manual-focus",
        "pick",
        json!({"focus":focus,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let input = act(s, d, "input", "pick", json!({"value":"5. November 2026"})).await?;
    let composing = act(s, d, "composing-enter", "pick", json!({})).await?;
    if snapshot(&s.peek())["formState"] != before["formState"]
        || snapshot(&s.peek())["events"] != before["events"]
        || input["effect"]["target"]["value"] != "5. November 2026"
    {
        return Err("Date raw/composing edit changed committed parent".into());
    }
    shot(
        s,
        d,
        n,
        "raw-localized",
        "pick",
        json!({"before":before,"input":input,"composing":composing}),
        st,
        cap,
    )
    .await?;
    let enter = act(s, d, "enter", "pick", json!({})).await?;
    counts(s, 1, 1, 0, 0).await?;
    let after = effect("pick").await?;
    if after["target"]["value"] != "5. November 2026"
        || after["target"]["dateSelected"] != "2026-11-05"
        || after["target"]["date"]["active"] != true
    {
        return Err("Date Enter lost raw or selected Date".into());
    }
    shot(
        s,
        d,
        n,
        "enter-commit",
        "pick",
        json!({"enter":enter,"after":after}),
        st,
        cap,
    )
    .await?;
    let blur = act(s, d, "blur", "pick", json!({})).await?;
    counts(s, 1, 1, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "blur-deduped",
        "pick",
        json!({"blur":blur,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let focus = act(s, d, "focus", "pick", json!({})).await?;
    let input = act(s, d, "input", "pick", json!({"value":"6. November 2026"})).await?;
    let escape = act(s, d, "escape", "pick", json!({})).await?;
    selected_all(&escape["effect"])?;
    if escape["effect"]["target"]["dateParsed"] != "2026-11-05" {
        return Err("Date Escape did not restore current selection".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "escape-restored",
        "pick",
        json!({"focus":focus,"input":input,"escape":escape,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let input = act(s, d, "input", "pick", json!({"value":"not-a-date"})).await?;
    let enter = act(s, d, "enter", "pick", json!({})).await?;
    counts(s, 2, 2, 0, 0).await?;
    if selected(&s.peek(), "pick") != 0 || enter["effect"]["target"]["value"] != "not-a-date" {
        return Err("Optional invalid Date did not clear epoch while retaining literal".into());
    }
    shot(
        s,
        d,
        n,
        "optional-invalid-cleared",
        "pick",
        json!({"input":input,"enter":enter}),
        st,
        cap,
    )
    .await?;
    let enter = act(s, d, "enter", "pick", json!({})).await?;
    counts(s, 3, 3, 0, 0).await?;
    let blur = act(s, d, "blur", "pick", json!({})).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "optional-null-repeat",
        "pick",
        json!({"enter":enter,"blur":blur,"originalCompositeNullCallbacks":3}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let focus = act(s, d, "focus", "quiet", json!({})).await?;
    let input = act(s, d, "input", "quiet", json!({"value":"invalid-required"})).await?;
    let enter = act(s, d, "enter", "quiet", json!({})).await?;
    if snapshot(&s.peek())["formState"] != before["formState"]
        || enter["effect"]["target"]["dateBufferValid"] != true
    {
        return Err("Required Date fallback changed parent or lost resolved custody".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "required-fallback",
        "quiet",
        json!({"before":before,"focus":focus,"input":input,"enter":enter,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let focus = act(s, d, "focus", "required", json!({})).await?;
    let input = act(
        s,
        d,
        "input",
        "required",
        json!({"value":"invalid-empty-required"}),
    )
    .await?;
    let enter = act(s, d, "enter", "required", json!({})).await?;
    if selected(&s.peek(), "required") != 0
        || enter["effect"]["target"]["dateBufferValid"] != false
        || snapshot(&s.peek())["ready"] != false
    {
        return Err("Missing required Date became a valid parent/custody".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "required-empty",
        "required",
        json!({"focus":focus,"input":input,"enter":enter,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let input = act(
        s,
        d,
        "input",
        "required",
        json!({"value":"7. November 2026"}),
    )
    .await?;
    let enter = act(s, d, "enter", "required", json!({})).await?;
    counts(s, 5, 5, 0, 0).await?;
    if enter["effect"]["target"]["dateSelected"] != "2026-11-07"
        || snapshot(&s.peek())["ready"] != true
    {
        return Err("Required Date recovery did not restore current custody".into());
    }
    shot(
        s,
        d,
        n,
        "required-recovered",
        "required",
        json!({"input":input,"enter":enter}),
        st,
        cap,
    )
    .await?;
    let focus = act(s, d, "focus", "quiet", json!({})).await?;
    let open = act(s, d, "open", "quiet", json!({})).await?;
    let input = act(
        s,
        d,
        "input",
        "quiet",
        json!({"child":"picker","value":"2026-11-08","focus":false}),
    )
    .await?;
    let change = act(
        s,
        d,
        "change",
        "quiet",
        json!({"child":"picker","focus":false}),
    )
    .await?;
    if change["effect"]["target"]["dateSelected"] != "2026-11-08" {
        return Err("Explicit chooser event did not update Date selection".into());
    }
    counts(s, 5, 5, 0, 0).await?;
    shot(s,d,n,"picker-adapter","quiet",json!({"focus":focus,"open":open,"input":input,"change":change,"syntheticChooserEvent":true,"physicalPopupClaim":false}),st,cap).await
}
