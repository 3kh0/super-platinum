use super::*;
type Captures = serde_json::Map<String, Value>;
pub(super) async fn continuity(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let before = setup(s, d, "enter").await?;
    shot(
        s,
        d,
        n,
        "input-focused",
        "enter",
        json!({"setup":before}),
        st,
        cap,
    )
    .await?;
    let held = acquire_held(s, "enter").await?;
    let events_before = snapshot(&s.peek())["events"].clone();
    let guards = read("guarded-events", "enter", json!({"quietId":id("quiet")})).await?;
    let events_after = snapshot(&s.peek())["events"].clone();
    if events_before != events_after {
        return Err("Lease synthetic events reached model".into());
    }
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 1, 0).await?;
    let after = wait_release("enter").await?;
    if !preserved(&before, &after) {
        return Err("Input focus/range lost through lease".into());
    }
    shot(s,d,n,"input-released","enter",json!({"held":held,"guardedEvents":guards,"eventsBefore":events_before,"eventsAfter":events_after,"after":after,"focusPreserved":true}),st,cap).await?;
    let continued = continue_at(s, "enter", false).await?;
    wait_release("enter").await?;
    finish_replies(s, 1).await?;
    shot(
        s,
        d,
        n,
        "input-continued",
        "enter",
        json!({"continuation":continued}),
        st,
        cap,
    )
    .await?;
    let before = setup(s, d, "characters").await?;
    shot(
        s,
        d,
        n,
        "textarea-focused",
        "characters",
        json!({"setup":before}),
        st,
        cap,
    )
    .await?;
    let held = acquire_held(s, "characters").await?;
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 2, 1).await?;
    let after = wait_release("characters").await?;
    if !preserved(&before, &after) {
        return Err("Textarea focus/range lost through lease".into());
    }
    shot(
        s,
        d,
        n,
        "textarea-released",
        "characters",
        json!({"held":held,"after":after,"focusPreserved":true}),
        st,
        cap,
    )
    .await?;
    let continued = continue_at(s, "characters", false).await?;
    check::helpers::counts(s, 3, 1).await?;
    let after = wait_release("characters").await?;
    let range = &continued["continuation"]["after"];
    if after["observation"]["activeIsTarget"] != true
        || after["observation"]["selectionStart"] != range["selectionStart"]
        || after["observation"]["selectionEnd"] != range["selectionEnd"]
    {
        return Err("Character dispatch lost continuation caret".into());
    }
    finish_replies(s, 3).await?;
    shot(
        s,
        d,
        n,
        "textarea-continued",
        "characters",
        json!({"continuation":continued,"afterCharacterDispatch":after}),
        st,
        cap,
    )
    .await?;
    Ok(())
}
pub(super) async fn ownership(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let before = setup(s, d, "enter").await?;
    shot(
        s,
        d,
        n,
        "focused-before",
        "enter",
        json!({"setup":before}),
        st,
        cap,
    )
    .await?;
    let held = acquire_held(s, "enter").await?;
    let away = read("away", "enter", json!({"id":id("quiet")})).await?;
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 1, 0).await?;
    let after = wait_release("enter").await?;
    if after["observation"]["activeElement"]["id"] != id("quiet")
        || after["observation"]["activeIsTarget"] != false
    {
        return Err("Lease stole explicit quiet focus".into());
    }
    shot(
        s,
        d,
        n,
        "focus-away-preserved",
        "quiet",
        json!({"held":held,"intent":away,"after":after}),
        st,
        cap,
    )
    .await?;
    let continued = continue_at(s, "enter", true).await?;
    finish_replies(s, 1).await?;
    shot(
        s,
        d,
        n,
        "quiet-continued",
        "quiet",
        json!({"continuation":continued}),
        st,
        cap,
    )
    .await?;
    let before = setup(s, d, "enter").await?;
    let held = acquire_held(s, "enter").await?;
    let intent = read("blur", "enter", json!({})).await?;
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 2, 1).await?;
    let after = wait_release("enter").await?;
    if after["observation"]["activeElement"]["tag"] != "BODY"
        || after["observation"]["activeIsTarget"] != false
    {
        return Err("Lease reversed explicit blur".into());
    }
    finish_replies(s, 2).await?;
    shot(
        s,
        d,
        n,
        "explicit-blur",
        "enter",
        json!({"setup":before,"held":held,"intent":intent,"after":after}),
        st,
        cap,
    )
    .await?;
    let before = setup(s, d, "enter").await?;
    let held = acquire_held(s, "enter").await?;
    let intent = read("select", "enter", json!({})).await?;
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 3, 2).await?;
    let after = wait_release("enter").await?;
    if after["observation"]["activeIsTarget"] != true
        || after["observation"]["selectionStart"] != 0
        || after["observation"]["selectionEnd"] != 2
        || after["observation"]["selectionDirection"] != "forward"
    {
        return Err("Lease restored obsolete selection".into());
    }
    finish_replies(s, 3).await?;
    shot(
        s,
        d,
        n,
        "selection-moved",
        "enter",
        json!({"setup":before,"held":held,"intent":intent,"after":after}),
        st,
        cap,
    )
    .await?;
    let before = setup(s, d, "enter").await?;
    let held = acquire_held(s, "enter").await?;
    let refresh = check::helpers::refresh(&mut s.write());
    release_gate(&mut s.write())?;
    check::helpers::counts(s, 3, 3).await?;
    check::helpers::settle(s, d).await?;
    let after = read("read", "enter", json!({})).await?;
    if after["observation"]["current"] != false
        || after["observation"]["connected"] != false
        || after["replacement"]["sameOriginal"] != false
        || after["replacement"]["activeIsTarget"] != false
        || after["replacement"]["readOnly"] != false
    {
        return Err("Retired lease affected replacement node".into());
    }
    let no_continue = read("continue", "enter", json!({})).await?;
    if no_continue["continuation"]["performed"] != false {
        return Err("Retired node admitted continuation".into());
    }
    shot(s,d,n,"view-retired","enter",json!({"setup":before,"held":held,"refresh":refresh,"after":after,"refusedContinuation":no_continue}),st,cap).await?;
    Ok(())
}
