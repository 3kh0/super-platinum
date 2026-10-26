use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let repeated = same_checked(s, d, "pick", 0).await?;
    let change = edit(s, d, "pick", vec![select(1)]).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "selected-current",
        json!({"sameChecked":repeated,"change":change}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |raw| {
        let gamma = raw["blocks"][0]["element"]["options"][2].clone();
        raw["blocks"][0]["element"]["initial_option"] = gamma.clone();
        raw["state"] =
            json!({"values":{"pick":{"value":{"type":"radio_buttons","selected_option":gamma}}}});
        let mut beta = base::option("beta", "Beta current");
        beta["opaque"]["revision"] = json!(2);
        raw["blocks"][0]["element"]["options"][1] = beta;
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "props-no-reset",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["options"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    });
    let quiet = edit(s, d, "quiet", vec![select(1)]).await?;
    let press = click(s, d).await?;
    counts(s, 1, 1, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 1, 1, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "hidden-selected-button",
        json!({"refresh":source,"quiet":quiet,"click":press}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |raw| {
        raw.as_object_mut().unwrap().remove("state");
        let mut beta = base::option("beta", "Beta current");
        beta["opaque"]["revision"] = json!(2);
        raw["blocks"][0]["element"]["options"]
            .as_array_mut()
            .unwrap()
            .insert(1, beta);
    });
    let unchanged = same_checked(s, d, "pick", 1).await?;
    let alpha = edit(s, d, "pick", vec![select(0)]).await?;
    counts(s, 2, 2, 1, 1).await?;
    let beta = edit(s, d, "pick", vec![select(1)]).await?;
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "current-option-metadata",
        json!({"refresh":source,"sameChecked":unchanged,"alpha":alpha,"beta":beta}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![select(3)]).await?;
    counts(s, 4, 4, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "empty-value-option",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":reset,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![select(3)]).await?;
    counts(s, 5, 5, 1, 1).await?;
    let returned = revisit(s, d).await?;
    let press = click(s, d).await?;
    counts(s, 5, 5, 2, 1).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 5, 5, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "saved-empty-return",
        json!({"change":change,"return":returned,"click":press}),
        st,
        cap,
    )
    .await
}
