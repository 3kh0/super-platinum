use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let focus = act(s, d, "focus", "pick", json!({})).await?;
    let draft = act(s, d, "input", "pick", json!({"value":"5. November 2026"})).await?;
    let open = act(s, d, "open", "pick", json!({"child":"time","focus":false})).await?;
    // The real Time open/select transfers focus and commits Date through blur.
    counts(s, 1, 1, 0, 0).await?;
    let time = act(
        s,
        d,
        "input",
        "pick",
        json!({"child":"time","value":"18:15","focus":false}),
    )
    .await?;
    let enter = act(s, d, "enter", "pick", json!({"child":"time","focus":false})).await?;
    counts(s, 2, 2, 0, 0).await?;
    let after = effect("pick").await?;
    if after["target"]["active"] != false
        || after["target"]["time"]["active"] != true
        || after["target"]["dateSelected"] != "2026-11-05"
    {
        return Err("Time focus transfer did not commit Date then retain Time focus".into());
    }
    shot(s,d,n,"date-blur-time-commit","pick",json!({"focus":focus,"draft":draft,"open":open,"time":time,"enter":enter,"after":after,"timeFocusRequested":false,"browserFocusTransfer":true,"dateBlurBeforeTimeInput":true}),st,cap).await?;
    let blur = act(s, d, "blur", "pick", json!({"child":"time"})).await?;
    counts(s, 2, 2, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "time-blur-deduped",
        "pick",
        json!({"blur":blur,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let focus = act(s, d, "focus", "pick", json!({"child":"time"})).await?;
    let draft = act(
        s,
        d,
        "input",
        "pick",
        json!({"child":"time","value":"17:45"}),
    )
    .await?;
    let date = act(
        s,
        d,
        "input",
        "pick",
        json!({"value":"6. November 2026","focus":false}),
    )
    .await?;
    let enter = act(s, d, "enter", "pick", json!({"focus":false})).await?;
    counts(s, 3, 3, 0, 0).await?;
    let after = effect("pick").await?;
    if after["target"]["time"]["value"] != "17:45"
        || after["target"]["time"]["active"] != true
        || after["target"]["dateSelected"] != "2026-11-06"
    {
        return Err("Date callback replaced counterpart Time draft/focus".into());
    }
    shot(s,d,n,"time-draft-date-commit","pick",json!({"focus":focus,"draft":draft,"date":date,"enter":enter,"after":after,"dateFocusRequested":false}),st,cap).await?;
    let blur = act(s, d, "blur", "pick", json!({"child":"time"})).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "time-blur-latest-date",
        "pick",
        json!({"blur":blur}),
        st,
        cap,
    )
    .await?;
    let focus = act(s, d, "focus", "pick", json!({})).await?;
    let date = act(s, d, "input", "pick", json!({"value":"7. November 2026"})).await?;
    let range = act(s, d, "range", "pick", json!({})).await?;
    let before = effect("pick").await?;
    let parent = snapshot(&s.peek())["formState"].clone();
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_date_time"] = json!(1793032662i64);
        r["blocks"][0]["label"]["text"] = json!("Current release date and time");
    });
    observed(s, d).await?;
    let after = effect("pick").await?;
    if after["target"]["value"] != "7. November 2026"
        || after["target"]["time"]["value"] != before["target"]["time"]["value"]
        || after["target"]["node"] != before["target"]["node"]
        || after["target"]["time"]["node"] != before["target"]["time"]["node"]
        || after["target"]["selection"] != before["target"]["selection"]
        || after["target"]["active"] != true
        || snapshot(&s.peek())["formState"] != parent
    {
        return Err(format!(
            "Datetime source changed draft/focus/parent: {}",
            json!({"before":before,"after":after})
        ));
    }
    shot(s,d,n,"focused-source-date-draft","pick",json!({"focus":focus,"date":date,"range":range,"before":before,"source":source,"after":after,"parent":parent}),st,cap).await?;
    let before = snapshot(&s.peek());
    let before_effect = effect("pick").await?;
    let block = before["rawView"]["blocks"][0].clone();
    let removal = refresh(&mut s.write(), |r| {
        r["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["block_id"] != "pick");
    });
    observed(s, d).await?;
    let incoming = s
        .peek()
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    if incoming["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["block_id"] == "pick")
    {
        return Err("Removal source still contains pick".into());
    }
    let refused = snapshot(&s.peek());
    let retained = effect("pick").await?;
    for key in [
        "rawView",
        "formState",
        "fields",
        "events",
        "requests",
        "responses",
    ] {
        if refused[key] != before[key] {
            return Err(format!("Datetime removal rewrote retained {key}"));
        }
    }
    if refused["ready"] != false
        || retained["target"]["value"] != before_effect["target"]["value"]
        || retained["target"]["time"]["value"] != before_effect["target"]["time"]["value"]
        || retained["target"]["selection"] != before_effect["target"]["selection"]
        || retained["target"]["active"] != true
    {
        return Err(
            "Datetime removal did not retain the visible dirty draft and refuse delivery".into(),
        );
    }
    let restore = refresh(&mut s.write(), |r| {
        r["blocks"].as_array_mut().unwrap().insert(0, block);
    });
    observed(s, d).await?;
    let after = effect("pick").await?;
    if after["target"]["value"] != "7. November 2026"
        || after["target"]["time"]["value"] != before_effect["target"]["time"]["value"]
        || after["target"]["active"] != true
        || snapshot(&s.peek())["formState"] != before["formState"]
    {
        return Err("Datetime restore lost acknowledged child draft".into());
    }
    shot(s,d,n,"removal-refused","pick",json!({"before":before,"beforeEffect":before_effect,"remove":removal,"refused":refused,"incomingRaw":incoming,"retained":retained,"restore":restore,"after":after}),st,cap).await?;
    let reset = navigation::reset(s, d).await?;
    observed(s, d).await?;
    let after = effect("pick").await?;
    no_field_focus(&after)?;
    if selected(&s.peek(), "pick") != 1793032662i64
        || after["target"]["dateRawDirty"] != false
        || after["target"]["timeRawDirty"] != false
    {
        return Err("Datetime Reset did not seed current source and retire drafts".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-current",
        "pick",
        json!({"reset":reset,"after":after,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let revisit = navigation::revisit(s, d).await?;
    let after = effect("pick").await?;
    no_field_focus(&after)?;
    shot(
        s,
        d,
        n,
        "cached-revisit",
        "pick",
        json!({"revisit":revisit,"after":after}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    capture::wait(d,"!!document.querySelector('.app-home')&&!document.querySelector('.app-home input')&&[...document.querySelectorAll('.app-home button')].some(b=>b.textContent.trim()==='Load Home')").await?;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["authority"]["homeCurrent"] == false
            && x["nativeMounts"].is_null()
            && x["windows"] == json!([])
            && x["activity"] == json!([])
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let after = snapshot(&s.peek());
    if after["authority"]["homeCurrent"] != false || !after["nativeMounts"].is_null() {
        return Err("Retired Datetime owner retained current controls".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "owner-retired",
        "home-empty",
        json!({"before":before,"after":after,"silence":quiet}),
        st,
        cap,
    )
    .await
}
