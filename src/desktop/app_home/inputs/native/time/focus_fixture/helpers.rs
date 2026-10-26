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
        "return timeFocusFixture({},{},{});",
        json!(action),
        json!(id(block)),
        args
    )))
    .await
}
pub(super) async fn effect(block: &str) -> Result<Value, String> {
    Ok(probe("read", block, json!({})).await?["effect"].clone())
}
pub(super) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..300 {
        if inputs::ready(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !inputs::ready(&s.peek()) {
        return Err("Time focus inputs did not settle".into());
    }
    observed(s, d).await
}
/// Exact acknowledged buffers, including deliberately unresolved editing text.
/// This is observation only; it grants no form readiness or lease authority.
pub(super) async fn observed(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    let _ = d;
    let mut last = Value::Null;
    for _ in 0..300 {
        // A source update advances Rust before the WebView replaces its hosts.
        // Refresh this snapshot on every observation; never accept old same-visit
        // history or an empty native-entry set as evidence of the replacement.
        let expected = snapshot(&s.peek());
        let code = format!(
            r#"const expected={},home=document.getElementById('app-home'),a=expected.authority;
const same=!!home&&a.homeCurrent===true&&home.dataset.visit===String(a.homeVisit)&&home.dataset.revision===String(a.homeRevision);
const history=same&&expected.focusWidgets.every(w=>{{const h=window.__spHomeText?.focus?.get(w.instance);return h&&h.started&&h.current()&&h.config.flag===w.flag&&h.config.visit===w.visit&&h.config.revision===a.homeRevision;}});
const entries=[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>home?.contains(e.node)),widgets=expected.timeWidgets||[],focus=expected.focusWidgets.filter(w=>w.kind==='time');
// Native custody retains closed entries. Only current form widgets name live
// controls; every one must still have exactly one matching Rust/DOM mount.
const mounts=widgets.map(w=>(expected.nativeMounts||[]).filter(m=>m.block===w.block&&m.action===w.action));
const inventory=widgets.length===focus.length&&widgets.every(w=>w.current===true&&focus.some(f=>f.block===w.block&&f.action===w.action)&&expected.fields.some(f=>f.block===w.block&&f.action===w.action));
const exact=history&&inventory&&entries.length===widgets.length&&mounts.every(rows=>{{if(rows.length!==1)return false;const m=rows[0],e=window.__spHomeCalendarNative?.entries.get(m.id),f=fields.find(f=>f.block===m.block),node=f&&document.getElementById(f.id);return e&&node&&e.node===node&&home.contains(node)&&e.nativeCurrent()&&e.mount===m.mount&&e.seq===m.seq&&e.acked===m.seq&&!e.pending&&!m.pending&&!e.focusSelection?.pending&&e.node.value===m.value&&e.ackedValue===m.value;}});
return{{ok:true,settled:!!exact,homeVisit:home?.dataset.visit??null,homeRevision:home?.dataset.revision??null,expectedVisit:a.homeVisit,expectedRevision:a.homeRevision,nativeCount:entries.length,expectedCount:mounts.length}};"#,
            expected
        );
        last = capture::dom(&script(&code)).await?;
        if last["settled"] == true {
            let after = snapshot(&s.peek());
            if ["authority", "focusWidgets", "nativeMounts", "timeWidgets"]
                .iter()
                .all(|key| expected[*key] == after[*key])
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Time current revision/mount observation timed out: {}",
        json!({"dom":last,"model":snapshot(&s.peek())})
    ))
}
pub(super) async fn active(d: &DesktopContext, block: &str, wanted: bool) -> Result<Value, String> {
    let code = format!(
        "const p=timeFocusFixture('read',{},{{}});return{{ok:p.effect.target?.active==={},effect:p.effect}};",
        json!(id(block)),
        wanted
    );
    Ok(crate::slack_links_check::wait_dom(d, &script(&code)).await?["effect"].clone())
}
pub(super) fn no_field_focus(e: &Value) -> Result<(), String> {
    if e["fields"]
        .as_array()
        .is_none_or(|rows| rows.iter().any(|r| r["field"]["active"] == true))
    {
        return Err("An unrequested Time fixture field took focus".into());
    }
    Ok(())
}
pub(super) fn selected_all(e: &Value) -> Result<(), String> {
    let f = &e["target"];
    let length = f["value"]
        .as_str()
        .ok_or("Time raw text missing")?
        .encode_utf16()
        .count() as u64;
    if f["active"] != true || f["selection"]["start"] != 0 || f["selection"]["end"] != length {
        return Err(format!(
            "Time focus did not select full editing label : {f}"
        ));
    }
    Ok(())
}
pub(super) fn flags(s: &mut ShellState, target: Option<&str>) -> Value {
    refresh(s, |raw| {
        for b in raw["blocks"].as_array_mut().unwrap() {
            if b["type"] == "input" {
                b["element"]["focus_on_load"] =
                    json!(Some(b["block_id"].as_str().unwrap_or_default()) == target);
            }
        }
    })
}
pub(super) fn widget(s: &ShellState, block: &str) -> Value {
    snapshot(s)["focusWidgets"]
        .as_array()
        .and_then(|a| a.iter().find(|w| w["block"] == block))
        .cloned()
        .unwrap_or(Value::Null)
}
pub(super) async fn quiet(s: Signal<ShellState>) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let after = snapshot(&s.peek());
    for k in ["events", "requests", "responses"] {
        if before[k] != after[k] {
            return Err(format!("Time focus-only interval changed {k}"));
        }
    }
    if before["buttonSync"] != after["buttonSync"] {
        return Err("Time focus interval changed Button ledger".into());
    }
    Ok(json!({"before":before,"after":after,"waitMs":1100}))
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
    // Source replacement retires scheduler rows on the normal production tick.
    // Observe that retirement before freezing the full capture model, including
    // deliberately unresolved raw buffers which cannot use form-ready settling.
    let mut scheduler_current = false;
    for _ in 0..200 {
        let model = snapshot(&s.peek());
        scheduler_current = ["activity", "windows"].iter().all(|key| {
            model[*key].as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row["current"] == true
                        && (row["trailing"].is_null() || row["trailing"]["current"] == true)
                })
            })
        });
        if scheduler_current {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !scheduler_current {
        return Err(format!(
            "Time capture scheduler did not settle: {}",
            snapshot(&s.peek())
        ));
    }
    let before = snapshot(&s.peek());
    proof["effect"] = effect(block).await?;
    let code = script(&format!("return timeFocusGeometry({});", json!(id(block))));
    let mut g = crate::slack_links_check::wait_dom(d, &code).await?;
    if g["before"]["active"] != g["after"]["active"]
        || g["before"]["activeField"]["selection"] != g["after"]["activeField"]["selection"]
        || g["before"]["fields"] != g["after"]["fields"]
    {
        return Err("Time capture scrolling changed actual focus/range/buffer".into());
    }
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        let changed = before
            .as_object()
            .into_iter()
            .flat_map(|o| o.keys())
            .chain(after.as_object().into_iter().flat_map(|o| o.keys()))
            .filter(|key| before[*key] != after[*key])
            .collect::<std::collections::BTreeSet<_>>();
        return Err(format!(
            "Time capture changed model: {}",
            json!({"changedKeys":changed,"before":before,"after":after})
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
    raw["hash"] = json!(format!("time-focus-refresh-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
pub(super) async fn mount_flag(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
) -> Result<Value, String> {
    let before = widget(&s.peek(), block);
    let raw = snapshot(&s.peek())["rawView"].clone();
    let rows = raw["blocks"].as_array().ok_or("Source blocks missing")?;
    let index = rows
        .iter()
        .position(|b| b["block_id"] == block)
        .ok_or("Mount source block missing")?;
    let mut source = rows[index].clone();
    source["element"]["focus_on_load"] = json!(true);
    let removed = refresh(&mut s.write(), |r| {
        for b in r["blocks"].as_array_mut().unwrap().iter_mut() {
            if b["type"] == "input" {
                b["element"]["focus_on_load"] = json!(false);
            }
        }
        r["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["block_id"] != block);
    });
    capture::wait(
        d,
        &format!("!document.getElementById({})", json!(id(block))),
    )
    .await?;
    let absent = effect(block).await?;
    if !absent["target"].is_null() || !widget(&s.peek(), block).is_null() {
        return Err("Removed pristine Time retained focus identity".into());
    }
    let added = refresh(&mut s.write(), |r| {
        r["blocks"].as_array_mut().unwrap().insert(index, source)
    });
    observed(s, d).await?;
    if widget(&s.peek(), block)["instance"] == before["instance"] {
        return Err("Fresh Time mount reused logical instance".into());
    }
    Ok(
        json!({"beforeWidget":before,"remove":removed,"absent":absent,"add":added,"widget":widget(&s.peek(),block),"noFixtureFocus":true}),
    )
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
) -> Result<(), String> {
    for _ in 0..350 {
        let x = snapshot(&s.peek());
        if x["requests"].as_array().map(Vec::len) == Some(requests)
            && x["responses"].as_array().map(Vec::len) == Some(responses)
            && x["windows"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Time action ledger did not settle: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn silence(s: Signal<ShellState>) -> Result<Value, String> {
    quiet(s).await
}
