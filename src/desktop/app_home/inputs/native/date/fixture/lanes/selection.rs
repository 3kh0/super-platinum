use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_date"] = json!("2026-10-26")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "pristine-props",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let observation = edit(s, vec![op("pick", "2026-10-27", "none")], None).await?;
    if observation["before"]["formState"] != observation["after"]["formState"]
        || observation["before"]["events"] != observation["after"]["events"]
    {
        return Err("Date input observation committed parent or emitted callback".into());
    }
    let change = edit(s, vec![commit("pick", "change")], None).await?;
    counts(s, 1, 1, 0, 0).await?;
    let duplicate = edit(s, vec![commit("pick", "change")], None).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "change-once",
        json!({"observation":observation,"change":change,"sameDate":duplicate,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![op("pick", "2026-10-28", "enter")], None).await?;
    counts(s, 2, 2, 0, 0).await?;
    let duplicate = edit(s, vec![commit("pick", "blur")], None).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "enter-blur-once",
        json!({"change":change,"sameDate":duplicate,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![op("quiet", "2026-11-02", "blur")], None).await?;
    let press = click(s, d).await?;
    counts(s, 2, 2, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 2, 2, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "quiet-button",
        json!({"change":change,"click":press}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![op("required", "2026-11-01", "change")], None).await?;
    counts(s, 3, 3, 1, 1).await?;
    let fallback = edit(s, vec![op("required", "", "change")], None).await?;
    let quiet = silence(s).await?;
    shot(s,d,n,"required-fallback",json!({"change":change,"fallback":fallback,"silence":quiet,"syntheticEmptyBuffer":true,"nativeClearAffordanceClaim":false}),st,cap).await?;
    let burst_started = Instant::now();
    let burst = edit(
        s,
        vec![
            op("pick", "2026-10-29", "change"),
            op("pick", "2026-10-30", "change"),
            op("pick", "2026-10-31", "none"),
        ],
        Some("Latest sibling"),
    )
    .await?;
    if burst["dom"]["elapsedMs"]
        .as_f64()
        .is_none_or(|v| v >= 450.0)
        || burst["modelAckElapsedMs"].as_u64().is_none_or(|v| v >= 450)
    {
        return Err("Date burst exceeded controlled debounce interval".into());
    }
    let immediate = &burst["after"];
    if !matches!(immediate["requests"].as_array().map(Vec::len), Some(3 | 4))
        || !immediate["responses"].as_array().is_some_and(|responses| {
            (3..=immediate["requests"].as_array().unwrap().len()).contains(&responses.len())
                && responses.starts_with(burst["before"]["responses"].as_array().unwrap())
        })
        || !immediate["requests"]
            .as_array()
            .unwrap()
            .starts_with(burst["before"]["requests"].as_array().unwrap())
        || immediate["refusals"] != burst["before"]["refusals"]
    {
        return Err("Date C observation missed the leading preparation interval".into());
    }
    // Observe asynchronous trusted-progress retry without delaying any mutation.
    // Only delivery ledgers and scheduling activity may progress after C is acked.
    let leading_prepared = loop {
        let x = snapshot(&s.peek());
        let elapsed = burst_started.elapsed().as_millis();
        if elapsed >= 450 {
            return Err("Date leading A was not prepared before the 450ms bound".into());
        }
        for (key, value) in immediate.as_object().unwrap() {
            if key == "buttonSync" {
                let mut expected = value.clone();
                expected["correlationCount"] = json!(
                    (x["requests"].as_array().unwrap().len()
                        + x["buttonSync"]["requests"].as_array().unwrap().len())
                    .min(10)
                );
                if x[key] != expected {
                    return Err("Date leading observation changed Button custody".into());
                }
                continue;
            }
            if !matches!(
                key.as_str(),
                "requests" | "responses" | "activity" | "windows"
            ) && x[key] != *value
            {
                return Err(format!("Date leading observation changed model key {key}"));
            }
        }
        let requests = x["requests"].as_array().unwrap();
        let responses = x["responses"].as_array().unwrap();
        if !(3..=4).contains(&requests.len())
            || !(3..=requests.len()).contains(&responses.len())
            || !requests.starts_with(immediate["requests"].as_array().unwrap())
            || !responses.starts_with(immediate["responses"].as_array().unwrap())
            || !responses.starts_with(burst["before"]["responses"].as_array().unwrap())
        {
            return Err("Date leading observation changed the prepared ledger prefix".into());
        }
        if requests.len() == 4 {
            if requests[3]["edge"] != "leading"
                || requests[3]["args"]["actions"][0]["selected_date"] != "2026-10-29"
            {
                return Err("Date fourth preparation was not leading A".into());
            }
            break json!({"model":x,"elapsedMs":elapsed});
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    counts(s, 5, 5, 1, 1).await?;
    let buffer_at_delivery = snapshot(&s.peek());
    let native_pick = |x: &Value| {
        x["nativeMounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["block"] == "pick")
            .unwrap()
            .clone()
    };
    let widget_pick = |x: &Value| {
        x["dateWidgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["block"] == "pick")
            .unwrap()
            .clone()
    };
    if native_pick(&burst["after"]) != native_pick(&buffer_at_delivery)
        || widget_pick(&buffer_at_delivery)["raw"] != "2026-10-31"
        || widget_pick(&buffer_at_delivery)["parsed"] != "2026-10-31"
        || native_pick(&buffer_at_delivery)["value"]
            != json!(serde_json::to_string(&("2026-10-31", "2026-10-30")).unwrap())
        || widget_pick(&buffer_at_delivery)["visible"] != "2026-10-30"
        || burst["after"]["formState"] != buffer_at_delivery["formState"]
        || buffer_at_delivery["formState"]["values"]["pick"]["value"]["selected_date"]
            != "2026-10-30"
        || buffer_at_delivery["requests"][4]["args"]["actions"][0]["selected_date"] != "2026-10-30"
        || buffer_at_delivery["requests"][4]["args"]["state"]["values"]["pick"]["value"]["selected_date"]
            != "2026-10-30"
    {
        return Err("Queued B did not preserve parent B and acknowledged buffer C".into());
    }
    shot(
        s,
        d,
        n,
        "leading-trailing-latest-state",
        json!({"burst":burst,"leadingPrepared":leading_prepared,"bufferAtDelivery":buffer_at_delivery}),
        st,
        cap,
    )
    .await?;
    // Capture scroll never focuses or commits the input. Commit the retained C
    // buffer only after the immutable screenshot/model proof has completed.
    let explicit_commit = edit(s, vec![commit("pick", "change")], None).await?;
    counts(s, 6, 6, 1, 1).await?;
    st.last_mut().unwrap()["proof"]["afterCaptureCommit"] =
        json!({"change":explicit_commit,"model":snapshot(&s.peek())});
    let change = edit(s, vec![op("pick", "", "change")], None).await?;
    counts(s, 7, 7, 1, 1).await?;
    shot(s, d, n, "optional-clear", json!({"change":change}), st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_date"] = json!("2026-11-03")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "dirty-props",
        json!({"refresh":source,"silence":quiet}),
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
    let change = edit(s, vec![op("pick", "", "change")], None).await?;
    counts(s, 8, 8, 1, 1).await?;
    let returned = revisit(s, d).await?;
    let press = click(s, d).await?;
    counts(s, 8, 8, 2, 1).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 8, 8, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "cached-return",
        json!({"change":change,"return":returned,"click":press}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date");
    });
    let reset = navigation::reset(s, d).await?;
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    for block in ["pick", "required"] {
        let widget = before["dateWidgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["block"] == block)
            .unwrap();
        if !widget["visible"].is_null()
            || !before["formState"]["values"][block]["value"]["selected_date"].is_null()
        {
            return Err("Fresh blank date was not missing in both widget and parent".into());
        }
    }
    let required = edit(s, vec![commit("required", "blur")], None).await?;
    if required["before"]["events"] != required["after"]["events"]
        || required["before"]["formState"] != required["after"]["formState"]
    {
        return Err("Required untouched blank emitted a callback".into());
    }
    let first = edit(s, vec![commit("pick", "blur")], None).await?;
    counts(s, 9, 9, 2, 2).await?;
    let repeated = edit(
        s,
        vec![commit("pick", "enter"), commit("pick", "blur")],
        None,
    )
    .await?;
    let quiet = silence(s).await?;
    let after = snapshot(&s.peek());
    let visible = after["dateWidgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["block"] == "pick")
        .unwrap();
    if visible["visible"] != ""
        || after["events"].as_array().map(Vec::len) != Some(10)
        || before["formState"] != after["formState"]
    {
        return Err("First optional blank did not retain empty local identity exactly once".into());
    }
    shot(s,d,n,"blank-first-commit",json!({"refresh":source,"reset":reset,"before":before,"required":required,"first":first,"repeated":repeated,"silence":quiet,"after":after,"nativeClearAffordanceClaim":false}),st,cap).await
}
