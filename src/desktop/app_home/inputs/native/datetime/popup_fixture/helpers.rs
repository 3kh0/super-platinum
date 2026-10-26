use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn script(body: &str) -> String {
    let fields = FIELDS
        .iter()
        .map(|b| json!({"block":b,"id":id(b)}))
        .collect::<Vec<_>>();
    format!(
        "(()=>{{{}const fields={};{}\n{} }})()",
        capture::ACTION_GUARD,
        json!(fields),
        include_str!("dom.js"),
        body
    )
}
pub(super) async fn probe(action: &str, block: &str, args: Value) -> Result<Value, String> {
    capture::dom(&script(&format!(
        "return datetimePopupFixture({},{},{});",
        json!(action),
        json!(id(block)),
        args
    )))
    .await
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
    observed(s, d).await?;
    let mut scheduler_ready = false;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        let ready = ["activity", "windows"].iter().all(|k| {
            x[*k].as_array().is_some_and(|rows| {
                rows.iter().all(|r| {
                    r["current"] == true
                        && (r["trailing"].is_null() || r["trailing"]["current"] == true)
                })
            })
        }) && x["buttonProcessing"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["processing"] == false));
        if ready {
            scheduler_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !scheduler_ready {
        return Err("Composite popup capture scheduler did not settle".into());
    }
    let before = snapshot(&s.peek());
    proof["effect"] = probe("read", block, json!({})).await?;
    let mut g = crate::slack_links_check::wait_dom(
        d,
        &script(&format!(
            "return datetimePopupGeometry({});",
            json!(id(block))
        )),
    )
    .await?;
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err(format!(
            "Datetime popup capture changed model: {}",
            json!({"before":before,"after":after})
        ));
    }
    st.push(json!({"stage":stage,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
pub(super) fn unchanged_parent(s: &ShellState, before: &Value) -> Result<(), String> {
    if snapshot(s)["formState"] != *before {
        return Err("Popup-only intent changed parent form".into());
    }
    Ok(())
}
