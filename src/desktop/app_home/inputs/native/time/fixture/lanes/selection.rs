use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let before = snapshot(&s.peek());
    let open = edit(s, vec![commit("pick", "enter")], None).await?;
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "opened",
        json!({"closedEnter":open,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let escape = edit(
        s,
        vec![
            op("pick", "discard this draft", "none"),
            commit("pick", "escape"),
        ],
        None,
    )
    .await?;
    let quiet = no_callback(s, &before).await?;
    let change = edit(s, vec![op("pick", "9", "blur")], None).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "typed-nine",
        json!({"escape":escape,"escapeSilence":quiet,"change":change}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![op("pick", "930", "blur")], None).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "compact-half", json!({"change":change}), st, cap).await?;
    let before = snapshot(&s.peek());
    let typing = edit(
        s,
        vec![op("pick", "5pm", "none"), commit("pick", "composing-enter")],
        None,
    )
    .await?;
    let quiet = no_callback(s, &before).await?;
    let change = edit(s, vec![commit("pick", "select-enter")], None).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s,d,n,"meridiem-enter",json!({"typingAndComposing":typing,"compositionSilence":quiet,"change":change,"buttonRequests":0,"enterPreventDefaultIsRendererFormSafetyAdaptation":true}),st,cap).await?;
    let change = edit(s, vec![choose("pick", "18:00")], None).await?;
    counts(s, 4, 4, 0, 0).await?;
    let before = snapshot(&s.peek());
    let same = edit(s, vec![commit("pick", "blur")], None).await?;
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "option-equality",
        json!({"change":change,"sameSelectedBlur":same,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let draft = edit(s, vec![op("pick", "garbage", "none")], None).await?;
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "invalid-draft",
        json!({"draft":draft,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![commit("pick", "blur")], None).await?;
    counts(s, 5, 5, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "optional-invalid-clear",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let first = edit(s, vec![op("pick", "", "blur")], None).await?;
    counts(s, 6, 6, 0, 0).await?;
    let repeated = edit(s, vec![op("pick", "", "blur")], None).await?;
    counts(s, 7, 7, 0, 0).await?;
    let before = snapshot(&s.peek());
    let invalid = edit(s, vec![op("pick", "garbage", "blur")], None).await?;
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "blank-repeated",
        json!({"first":first,"repeated":repeated,"invalidAlreadyEmpty":invalid,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let refused = edit(
        s,
        vec![
            op("required", "garbage", "blur"),
            op("required", "", "blur"),
        ],
        None,
    )
    .await?;
    let quiet = no_callback(s, &before).await?;
    let required = edit(s, vec![op("required", "930", "blur")], None).await?;
    counts(s, 8, 8, 0, 0).await?;
    let before = snapshot(&s.peek());
    let retained = edit(
        s,
        vec![
            op("required", "garbage", "blur"),
            op("required", "", "blur"),
        ],
        None,
    )
    .await?;
    let quiet_retained = no_callback(s, &before).await?;
    let sibling = edit(s, vec![op("quiet", "5pm", "blur")], Some("Latest sibling")).await?;
    let press = click(s, d).await?;
    counts(s, 8, 8, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 8, 8, 1, 1).await?;
    shot(s,d,n,"required-quiet-button",json!({"refused":refused,"silence":quiet,"required":required,"retained":retained,"retainedSilence":quiet_retained,"quietChange":sibling,"click":press}),st,cap).await
}
