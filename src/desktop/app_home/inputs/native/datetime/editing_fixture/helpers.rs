use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn script(body: &str) -> String {
    let fields = FIELDS
        .iter()
        .map(|b| json!({"block":b,"id":id(b)}))
        .collect::<Vec<_>>();
    // Observation only: retain all geometry/hit checks while allowing a disabled
    // button to be read. Mutating gestures continue using the unchanged owned guard.
    let disabled_guard = capture::ACTION_GUARD
        .replace("const owned=", "const visibleDisabled=")
        .replace("if(!e||e.disabled)", "if(!e)");
    format!(
        "(()=>{{{}{}const fields={};{}\n{} }})()",
        capture::ACTION_GUARD,
        disabled_guard,
        json!(fields),
        include_str!("dom.js"),
        body
    )
}
pub(super) async fn probe(action: &str, block: &str, args: Value) -> Result<Value, String> {
    capture::dom(&script(&format!(
        "return datetimeEditingFixture({},{},{});",
        json!(action),
        json!(id(block)),
        args
    )))
    .await
    .map_err(|error| {
        format!(
            "Datetime {action} for {block} child {}: {error}",
            args["child"].as_str().unwrap_or("date")
        )
    })
}
pub(super) async fn effect(block: &str) -> Result<Value, String> {
    Ok(probe("read", block, json!({})).await?["effect"].clone())
}
pub(super) async fn observed(s: Signal<ShellState>, _d: &DesktopContext) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..300 {
        let expected = snapshot(&s.peek());
        let body = format!(
            r#"const expected={},home=document.getElementById('app-home'),a=expected.authority,widgets=expected.datetimeWidgets||[];
const source=home&&a.homeCurrent===true&&home.dataset.visit===String(a.homeVisit)&&home.dataset.revision===String(a.homeRevision);
const entries=[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>home?.contains(e.date));
const exact=source&&entries.length===widgets.length&&widgets.every(w=>{{const rows=(expected.nativeMounts||[]).filter(m=>m.block===w.block&&m.action===w.action);if(w.current!==true||rows.length!==1)return false;const m=rows[0],e=window.__spHomeCalendarNative?.entries.get(m.id),f=fields.find(f=>f.block===m.block),n=f&&document.getElementById(f.id);return e&&e.datetime&&e.date===n&&home.contains(n)&&e.nativeCurrent()&&e.mount===m.mount&&e.seq===m.seq&&e.acked===m.seq&&!e.pending&&!m.pending&&e.snapshotValue()===m.value&&e.ackedValue===m.value;}});
return{{ok:true,settled:!!exact,homeRevision:home?.dataset.revision??null,expectedRevision:a.homeRevision,count:entries.length,expectedCount:widgets.length}};"#,
            expected
        );
        last = capture::dom(&script(&body)).await?;
        if last["settled"] == true {
            let after = snapshot(&s.peek());
            if ["authority", "datetimeWidgets", "nativeMounts", "fields"]
                .iter()
                .all(|k| expected[*k] == after[*k])
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Datetime current triple did not settle: {}",
        json!({"dom":last,"model":snapshot(&s.peek())})
    ))
}
pub(super) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    observed(s, d).await
}
pub(super) fn no_field_focus(e: &Value) -> Result<(), String> {
    if e["fields"].as_array().is_none_or(|rows| {
        rows.iter().any(|r| {
            r["field"]["active"] == true
                || r["field"]["time"]["active"] == true
                || r["field"]["picker"]["active"] == true
        })
    }) {
        return Err("Datetime requested unforwarded automatic focus".into());
    }
    Ok(())
}
pub(super) fn selected_all(e: &Value) -> Result<(), String> {
    let f = &e["target"];
    let len = f["value"]
        .as_str()
        .ok_or("Date literal missing")?
        .encode_utf16()
        .count() as u64;
    if f["active"] != true || f["selection"]["start"] != 0 || f["selection"]["end"] != len {
        return Err(format!("Date editing label was not selected: {f}"));
    }
    Ok(())
}
pub(super) async fn act(
    s: Signal<ShellState>,
    d: &DesktopContext,
    action: &str,
    block: &str,
    args: Value,
) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let intent = probe(action, block, args).await?;
    observed(s, d).await?;
    Ok(
        json!({"before":before,"intent":intent,"after":snapshot(&s.peek()),"effect":effect(block).await?}),
    )
}
pub(super) async fn silence(s: Signal<ShellState>) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let after = snapshot(&s.peek());
    for k in ["requests", "responses", "events", "buttonSync"] {
        if before[k] != after[k] {
            return Err(format!("Quiet Datetime interval changed {k}"));
        }
    }
    Ok(json!({"before":before,"after":after,"waitMs":1100}))
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    sr: usize,
    ss: usize,
    br: usize,
    bs: usize,
) -> Result<(), String> {
    for _ in 0..350 {
        let x = snapshot(&s.peek());
        if x["requests"].as_array().map(Vec::len) == Some(sr)
            && x["responses"].as_array().map(Vec::len) == Some(ss)
            && x["buttonSync"]["requests"].as_array().map(Vec::len) == Some(br)
            && x["buttonSync"]["responses"].as_array().map(Vec::len) == Some(bs)
            && x["windows"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Datetime request ledger did not settle: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    block: &str,
    mut proof: Value,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    if snapshot(&s.peek())["authority"]["homeCurrent"] == true {
        observed(s, d).await?;
    }
    let mut scheduler = false;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        scheduler = ["activity", "windows"].iter().all(|k| {
            x[*k].as_array().is_some_and(|rows| {
                rows.iter().all(|r| {
                    r["current"] == true
                        && (r["trailing"].is_null() || r["trailing"]["current"] == true)
                })
            })
        });
        scheduler = scheduler
            && x["buttonProcessing"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["processing"] == false));
        if scheduler {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !scheduler {
        return Err("Datetime capture scheduler did not settle".into());
    }
    let before = snapshot(&s.peek());
    proof["effect"] = effect(block).await?;
    let mut g = crate::slack_links_check::wait_dom(
        d,
        &script(&format!(
            "return datetimeEditingGeometry({});",
            json!(id(block))
        )),
    )
    .await?;
    if g["before"]["active"] != g["after"]["active"]
        || g["before"]["activeField"] != g["after"]["activeField"]
        || g["before"]["fields"] != g["after"]["fields"]
    {
        return Err("Datetime capture scrolling changed focus/range/buffers".into());
    }
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err(format!(
            "Datetime capture changed model: {}",
            json!({"before":before,"after":after})
        ));
    }
    st.push(json!({"stage":stage,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
pub(super) fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    raw["hash"] = json!(format!("datetime-editing-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
pub(super) fn selected(s: &ShellState, block: &str) -> Value {
    snapshot(s)["datetimeWidgets"]
        .as_array()
        .and_then(|rows| rows.iter().find(|w| w["block"] == block))
        .map(|w| w["visible"].clone())
        .unwrap_or(Value::Null)
}
