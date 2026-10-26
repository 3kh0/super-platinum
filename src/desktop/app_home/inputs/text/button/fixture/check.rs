use super::*;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::fixture::check::helpers as h;
use crate::app_home::inputs::text::fixture::{key, snapshot};
use dioxus::desktop::DesktopContext;
mod lanes;
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &name)).await {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":error,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Button sync timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-text-button-sync-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"homeOpenedFixtureAdapter":true,"buttonTransport":"actual production Attempt and prepared request; closed interceptor"}),&mut stages,&mut captures).await?;
    match case().as_str() {
        "admission" => lanes::admission(s, d, n, &mut stages, &mut captures).await?,
        "pending" => lanes::pending(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown sync case".into()),
    };
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
async fn click() -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const e=[...document.querySelectorAll('#app-home button,.app-home button')].find(e=>e.textContent.trim()==='Send form');if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});e.click();return{{ok:true,text:e.textContent,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD)).await
}
async fn counts(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
    refusals: usize,
) -> Result<(), String> {
    for _ in 0..250 {
        let v = snapshot(&s.peek());
        let b = &v["buttonSync"];
        if b["requests"].as_array().map(Vec::len) == Some(requests)
            && b["responses"].as_array().map(Vec::len) == Some(responses)
            && b["refusals"].as_array().map(Vec::len) == Some(refusals)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(12)).await;
    }
    Err(format!("Button settlement {}", snapshot(&s.peek())))
}
async fn unlocked() -> Result<Value, String> {
    for _ in 0..200 {
        let v=capture::dom(r#"(()=>{const home=document.getElementById('app-home')||document.querySelector('.app-home'),texts=[...(window.__spHomeText?.entries.values()||[])].filter(e=>!e.closed&&home?.contains(e.host)),dates=[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>!e.closed&&home?.contains(e.host));return{ok:true,text:texts.map(e=>({mount:e.mount??null,seq:e.seq,acked:e.acked,value:e.node.value,disabled:e.node.disabled,lease:!!e.lease})),calendar:dates.map(e=>({seq:e.seq,acked:e.acked,value:e.node.value,picker:e.picker?.value??null,snapshotValue:e.snapshotValue?.()??e.node.value,ackedValue:e.ackedValue,disabled:e.node.disabled,lease:!!e.lease,pending:!!e.pending})),released:texts.length===2&&dates.length===1&&texts.every(e=>!e.lease&&!e.node.disabled&&e.seq===e.acked&&!e.pending)&&dates.every(e=>e.editableDate&&e.nativeCurrent()&&!e.lease&&!e.node.disabled&&!e.node.readOnly&&!e.picker.disabled&&!e.picker.readOnly&&e.seq===e.acked&&!e.pending&&e.snapshotValue()===e.ackedValue)};})()"#).await?;
        if v["released"] == true {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Invocation leases did not release with current native acknowledgements".into())
}

async fn dismiss_uncertain(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    cap: &mut h::Captures,
) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let script = format!(
        r#"(()=>{{{}const t=document.getElementById('status-toast'),e=t?.querySelector('.toast-dismiss[aria-label="Dismiss notification"]');if(!t||!owned(e))return{{ok:false}};return{{ok:true,text:t.querySelector('.toast-text')?.textContent,control:e.getAttribute('aria-label'),visibleHitTested:true,viewport:{{width:innerWidth,height:innerHeight}}}};}})()"#,
        capture::ACTION_GUARD
    );
    let mut geometry = crate::slack_links_check::wait_dom(d, &script).await?;
    capture::capture_image(d, n, "uncertain-toast", &mut geometry, cap).await?;
    let click =
        capture::click("#status-toast .toast-dismiss[aria-label='Dismiss notification']").await?;
    let absent=crate::slack_links_check::wait_dom(d,"(()=>({ok:!document.getElementById('status-toast'),toastPresent:!!document.getElementById('status-toast')}))()").await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Notification dismissal changed Home state".into());
    }
    Ok(
        json!({"captureKey":"uncertain-toast","geometry":geometry,"click":click,"afterDom":absent,"modelUnchanged":true,"beforeModel":before,"afterModel":after}),
    )
}

// The production ordinary Button gate lasts one second, independently of HTTP.
// Capture only its settled state; the wrappers lane separately proves the
// pre-deadline disabled click and post-deadline fresh click while pending.
async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    proof: Value,
    st: &mut Vec<Value>,
    cap: &mut h::Captures,
) -> Result<(), String> {
    for _ in 0..250 {
        if actions::wrappers_fixture_processing(&s.peek())
            .as_array()
            .is_some_and(|a| a.iter().all(|v| v["processing"] == false))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    crate::slack_links_check::wait_dom(d,"(()=>{const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');return{ok:!!b&&!b.disabled};})()").await?;
    h::shot(s, d, n, stage, proof, st, cap).await
}
