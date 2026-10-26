use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let mut changes = vec![];
    for (i, index) in [1, 0, 1, 1].into_iter().enumerate() {
        changes.push(edit(s, d, "pick", vec![toggle(index)]).await?);
        counts(s, i + 1, i + 1, 0, 0).await?;
    }
    shot(
        s,
        d,
        n,
        "ordered-remove-readd",
        json!({"changes":changes}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |raw| {
        let gamma = raw["blocks"][0]["element"]["options"][2].clone();
        raw["blocks"][0]["element"]["initial_options"] = json!([gamma.clone()]);
        raw["state"] =
            json!({"values":{"pick":{"value":{"type":"checkboxes","selected_options":[gamma]}}}});
        let mut beta = base::option("beta", "Beta current");
        beta["opaque"]["revision"] = json!(2);
        raw["blocks"][0]["element"]["options"][1] = beta;
    });
    let change = edit(s, d, "pick", vec![toggle(2)]).await?;
    counts(s, 5, 5, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "props-retained-survivors",
        json!({"refresh":source,"change":change}),
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
    let change = edit(s, d, "pick", vec![toggle(0)]).await?;
    counts(s, 6, 6, 0, 0).await?;
    let quiet = edit(s, d, "quiet", vec![toggle(1), toggle(0)]).await?;
    let press = click(s, d).await?;
    counts(s, 6, 6, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 6, 6, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "hidden-survivor-button",
        json!({"refresh":source,"change":change,"quiet":quiet,"click":press}),
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
    let removed = edit(s, d, "pick", vec![toggle(1)]).await?;
    counts(s, 7, 7, 1, 1).await?;
    let added = edit(s, d, "pick", vec![toggle(1)]).await?;
    counts(s, 8, 8, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "current-option-rechecked",
        json!({"refresh":source,"removed":removed,"added":added}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, d, "pick", vec![toggle(2), toggle(1)]).await?;
    counts(s, 10, 10, 1, 1).await?;
    shot(s, d, n, "optional-empty", json!({"change":change}), st, cap).await?;
    let change = edit(s, d, "pick", vec![toggle(3)]).await?;
    counts(s, 11, 11, 1, 1).await?;
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
    let before = snapshot(&s.peek());
    let reset=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(e=>e.textContent.trim()==='Reset inputs');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD)).await?;
    let mut changed = false;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            changed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !changed {
        return Err("Checkbox Reset did not replace form revision".into());
    }
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
    .await
}
