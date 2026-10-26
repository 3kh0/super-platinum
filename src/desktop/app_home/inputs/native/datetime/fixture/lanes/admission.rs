use super::*;
use crate::app_home::inputs::text::fixture::admission as observer;
async fn press() -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,visibleHitTested:true,focusRequested:false,trusted:false}};}})()"#,capture::ACTION_GUARD)).await
}
async fn wait(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
    refusals: usize,
) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        let b = &x["buttonSync"];
        if b["requests"].as_array().map(Vec::len) == Some(requests)
            && b["responses"].as_array().map(Vec::len) == Some(responses)
            && b["refusals"].as_array().map(Vec::len) == Some(refusals)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Datetime Button ledger did not settle: {}",
        snapshot(&s.peek())
    ))
}
async fn barrier(s: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..180 {
        let x = snapshot(&s.peek());
        if x["admission"]["waiting"] == true {
            return Ok(x);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Datetime calendar observation gate missing".into())
}
async fn raw_buffer(child: &str, value: &str, expected: Value) -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const id={},child={},value={},expected={},date=document.getElementById(id),e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.datetime&&e.date===date),node=child==='date'?e?.date:e?.node;node?.scrollIntoView({{block:'center'}});if(!owned(node)||!e||e.closed||e.lease||e.pending||e.seq!==e.acked||node.disabled||node.readOnly||!e.host.isConnected||!e.host.contains(node)||!e.host.contains(e.datePicker)||!e.nativeCurrent()||e.date.type!=='text'||e.datePicker.type!=='date'||e.datePicker.getAttribute('data-home-date-picker')!=='true')return{{ok:false}};if(expected&&(e.mount!==expected.mount||e.seq!==expected.seq))return{{ok:false}};const before=node.value,seq=e.seq,acked=e.acked,mount=e.mount,ackedValue=e.ackedValue,beforeSnapshotValue=e.snapshotValue(),pickerBefore=e.datePicker.value;node.value=value;if(node.value!==value||e.seq!==seq||e.acked!==acked)return{{ok:false}};return{{ok:true,child,before,value,seq,acked,mount,ackedValue,beforeSnapshotValue,pickerBefore,pickerAfter:e.datePicker.value,dateValue:e.date.value,timeValue:e.node.value,snapshotValue:e.snapshotValue(),eventDispatched:false,visibleHitTested:true,diagnosticUnseenMutation:true}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("pick"))),json!(child),json!(value),expected)).await
}
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for (i, child, value) in [(0usize, "date", "2026-11-05"), (1usize, "time", "17:45")] {
        settle(s, d).await?;
        let before = snapshot(&s.peek());
        let unseen = raw_buffer(child, value, Value::Null).await?;
        let click = press().await?;
        wait(s, 0, 0, i + 1).await?;
        let restore = raw_buffer(
            child,
            unseen["before"].as_str().ok_or("Original buffer missing")?,
            unseen.clone(),
        )
        .await?;
        shot(s,d,n,&format!("unseen-{child}-refused"),json!({"before":before,"unseen":unseen,"click":click,"restore":restore,"eventAuthorityInjected":false}),st,cap).await?;
    }
    for (i, child) in [(0usize, "date"), (1usize, "time")] {
        settle(s, d).await?;
        let before = snapshot(&s.peek());
        let index = observer::arm(&mut s.write());
        let click = press().await?;
        let attempted = barrier(s).await?;
        let mut change = if child == "date" {
            date("pick", "2026-10-26", "none")
        } else {
            op("pick", "17:45", "none")
        };
        change["focus"] = json!(false);
        let edit = edit(s, vec![change], None).await?;
        let acknowledged = snapshot(&s.peek());
        if acknowledged["formState"] != before["formState"] {
            return Err("Admission buffer observation committed parent".into());
        }
        observer::release(&mut s.write())?;
        wait(s, i + 1, i, 2).await?;
        buttons::release(&mut s.write(), true);
        wait(s, i + 1, i + 1, 2).await?;
        let delivered = snapshot(&s.peek());
        if delivered["formState"] != before["formState"]
            || delivered["nativeMounts"] != acknowledged["nativeMounts"]
        {
            return Err("Trusted progress delivery rewrote buffer or parent".into());
        }
        shot(s,d,n,&format!("trusted-{child}-progress"),json!({"index":index,"before":before,"click":click,"attempted":attempted,"edit":edit,"acknowledged":acknowledged,"noRequestReplay":true}),st,cap).await?;
    }
    let escape = edit(s, vec![commit("pick", "escape")], None).await?;
    settle(s, d).await?;
    let index = observer::arm(&mut s.write());
    let click = press().await?;
    let attempted = barrier(s).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][3]["label"]["text"] = json!("Current release notes");
    });
    observer::release(&mut s.write())?;
    wait(s, 2, 2, 3).await?;
    shot(s,d,n,"source-retired",json!({"escape":escape,"index":index,"click":click,"attempted":attempted,"refresh":source,"bothChildAttestationsRetired":true}),st,cap).await?;
    let click = super::super::helpers::click(s, d).await?;
    wait(s, 3, 2, 3).await?;
    buttons::release(&mut s.write(), true);
    wait(s, 3, 3, 3).await?;
    shot(
        s,
        d,
        n,
        "fresh-explicit-button",
        json!({"click":click,"silence":silence(s).await?}),
        st,
        cap,
    )
    .await
}
