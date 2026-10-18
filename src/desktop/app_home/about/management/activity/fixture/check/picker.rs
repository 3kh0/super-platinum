//! Stateful native acceptance of the real member picker pipeline.
use super::*;
use crate::app_home::about::management::activity::members::fixture as people;

const SEARCH: &str = "workflow-activity-user-search";
const SELECT: &str = "workflow-activity-user";
const FIND: &str = "workflow-activity-find-people";
const FAILURE: &str = "People could not be loaded. Try again.";
const OFFLINE: &str = "People are unavailable while offline. Try again when connected.";

fn member(state: Signal<ShellState>) -> Value {
    people::snapshot(&state.peek())
}
async fn until(
    state: Signal<ShellState>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..160 {
        let v = member(state);
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("Picker {label}: {}", member(state)))
}
fn searches(v: &Value) -> u64 {
    v["ledger"]["searches"].as_u64().unwrap_or_default()
}
async fn done(state: Signal<ShellState>, after: u64) -> Result<Value, String> {
    until(state, "settled search", |v| {
        searches(v) > after && v["picker"]["pending"] == false
    })
    .await
}
async fn dom(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop, r#"(()=>{const q=document.getElementById('workflow-activity-user-search'),s=document.getElementById('workflow-activity-user');return{ok:!!q&&!!s,query:q?.value,selected:s?.value,options:s?[...s.options].map(o=>[o.value,o.textContent]):[]}})()"#).await
}
async fn options(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    let expected = member(state)["picker"]["rows"].clone();
    wait_dom(desktop, &format!(r#"(()=>{{const s=document.getElementById('workflow-activity-user');return{{ok:!!s&&JSON.stringify([...s.options].slice(1).map(o=>[o.value,o.textContent]))===JSON.stringify({})}}}})()"#, expected)).await?;
    Ok(())
}
async fn feedback(desktop: &DesktopContext, label: &str) -> Result<Value, String> {
    let expected = match label {
        "malformed" => FAILURE,
        "failure" => OFFLINE,
        _ => "",
    };
    let script = r#"(()=>{
        const expected=EXPECTED,returned=RETURNED,n=document.getElementById('workflow-activity-member-notice'),body=document.querySelector('.workflow-activity-content');
        const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
        const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e);if(r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;}return [[.5,.5],[.15,.5],[.85,.5]].every(([x,y])=>{const h=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return h===e||e.contains(h)});};
        const globalToastCount=document.querySelectorAll('#status-toast').length,notice=n?.textContent??'',outsideScroll=!!n&&!n.closest('.workflow-activity-content'),samples=[];
        if(n&&body){const old=body.scrollTop;for(const edge of ['start','end']){body.scrollTop=edge==='start'?0:body.scrollHeight;samples.push({edge,scrollTop:body.scrollTop,visible:visible(n),rect:rect(n)})}body.scrollTop=old;}
        const stable=samples.length===2&&samples.every(x=>x.visible)&&Math.abs(samples[0].rect.top-samples[1].rect.top)<1&&Math.abs(samples[0].rect.bottom-samples[1].rect.bottom)<1;
        const ids=returned?['workflow-management-activity','workflow-management-managed','workflow-management-check','workflow-management-save']:[];
        const actions=ids.map(id=>{const e=document.getElementById(id);if(!e)return{id,visible:false};e.scrollIntoView({block:'nearest'});return{id,visible:visible(e),disabled:e.disabled,rect:rect(e),scrollSample:true}});
        return{ok:globalToastCount===0&&(expected?notice===expected&&outsideScroll&&stable:!n)&&actions.every(x=>x.visible&&!x.disabled),notice,globalToastCount,outsideScroll,stable,samples,actions,actionMeasurement:'individual-scroll-samples'};
    })()"#
    .replace("EXPECTED", &json!(expected).to_string())
    .replace("RETURNED", if label == "returned" { "true" } else { "false" });
    wait_dom(desktop, &script).await
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let members = member(state);
    let native = if snapshot(&state.peek())["activity"]["active"] == true {
        options(state, desktop).await?;
        dom(desktop).await?
    } else {
        Value::Null
    };
    let feedback = if matches!(
        label,
        "malformed" | "failure" | "retried" | "empty" | "returned"
    ) {
        feedback(desktop, label).await?
    } else {
        Value::Null
    };
    capture(
        state,
        desktop,
        name,
        label,
        json!({"members":members,"native":native,"exercise":proof,"feedback":feedback}),
        stages,
        captures,
    )
    .await
}
async fn restore_select(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const s=document.getElementById('workflow-activity-user');s.querySelectorAll('[data-fixture-probe]').forEach(e=>e.remove());s.value='';return{ok:s.value===''}})()"#).await.map(|_|())
}
async fn choose_probe(desktop: &DesktopContext, id: &str) -> Result<Value, String> {
    let attempted = wait_dom(desktop,&format!(r#"(()=>{{const value={};const s=document.getElementById('workflow-activity-user'),q=document.getElementById('workflow-activity-user-search');if(!s||!q)return{{ok:false}};if(![...s.options].some(o=>o.value===value)){{const o=new Option('Retired option',value);o.dataset.fixtureProbe='1';s.add(o)}}s.value=value;const attempted={{ok:s.value===value,selected:s.value,query:q.value,options:[...s.options].map(o=>[o.value,o.textContent])}};s.dispatchEvent(new Event('change',{{bubbles:true}}));return attempted;}})()"#,json!(id))).await?;
    // Observe the asynchronous native snapshot refusal, without dispatching a
    // second input event that could restore the expected buffer first.
    tokio::time::sleep(Duration::from_millis(350)).await;
    Ok(attempted)
}
async fn refused(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    kind: &str,
) -> Result<Value, String> {
    let before = member(state);
    if kind == "raw-refused" {
        wait_dom(desktop,r#"(()=>{const q=document.getElementById('workflow-activity-user-search');q.value='Uncommitted native query';return{ok:q.value==='Uncommitted native query'}})()"#).await?;
    } else if kind == "composition-refused" {
        wait_dom(desktop,r#"(()=>{document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'a'}));return{ok:true}})()"#).await?;
        until(state, "composition retires options", |v| {
            v["picker"]["generation"] == 0
        })
        .await?;
    }
    let baseline = member(state);
    let id = if kind == "option-refused" {
        "UACTSTALE"
    } else {
        people::ADA
    };
    let attempted = choose_probe(desktop, id).await?;
    let observed = dom(desktop).await?;
    let after = member(state);
    if after["picker"]["filterUser"] != ""
        || searches(&after) != searches(&baseline)
        || after["picker"]["filterRevision"] != baseline["picker"]["filterRevision"]
    {
        return Err(format!("{kind} changed actor admission: {after}"));
    }
    restore_select(desktop).await?;
    if kind == "raw-refused" {
        wait_dom(desktop,r#"(()=>{document.getElementById('workflow-activity-user-search').value='a';return{ok:true}})()"#).await?;
    }
    Ok(
        json!({"kind":kind,"before":before["picker"],"after":after["picker"],"nativeAttempt":attempted,"nativeAfter":observed,"searchesBefore":searches(&baseline),"searchesAfter":searches(&after)}),
    )
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    wait_dom(
        desktop,
        "({ok:!!document.getElementById('workflow-configuration')})",
    )
    .await?;
    stage(
        state,
        desktop,
        name,
        "about",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "owned editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "dirty metadata", exact).await?;
    stage(
        state,
        desktop,
        name,
        "draft",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-management-activity").await?;
    wait(state, "initial Activity list", |v| {
        v["ledger"]["requests"]
            .as_array()
            .is_some_and(|r| r.len() == 1)
            && v["metadata"]["busy"] == false
            && !v["activity"]["query"].is_null()
            && v["activity"]["notice"].is_null()
    })
    .await?;
    people::connection(&mut state.write(), false)?;
    input(desktop, SEARCH, "zz-no-match").await?;
    until(state, "local empty", |v| {
        v["picker"]["generation"].as_u64().is_some_and(|g| g > 0)
            && v["picker"]["pending"] == false
            && v["picker"]["rows"] == json!([])
    })
    .await?;
    if searches(&member(state)) != 0 {
        return Err("Local path sent remote search".into());
    }
    stage(
        state,
        desktop,
        name,
        "local-empty",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    people::connection(&mut state.write(), true)?;
    input(desktop, SEARCH, "a").await?;
    done(state, 0).await?;
    until(state, "ranked options", |v| {
        v["picker"]["rows"].as_array().is_some_and(|r| r.len() == 2)
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "remote",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    for label in ["raw-refused", "option-refused", "composition-refused"] {
        let proof = refused(state, desktop, label).await?;
        stage(
            state,
            desktop,
            name,
            label,
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    let before = searches(&member(state));
    wait_dom(desktop,r#"(()=>{document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'a'}));return{ok:true}})()"#).await?;
    done(state, before).await?;
    // A person can only select an option once the refreshed result has been
    // rendered. Settled Rust acquisition alone is not a DOM-ready receipt.
    options(state, desktop).await?;
    input(desktop, SELECT, people::ADA).await?;
    until(state, "actor selected", |v| {
        v["picker"]["filterUser"] == people::ADA
    })
    .await?;
    click(desktop, "workflow-activity-apply").await?;
    wait(state, "actor query", |v| {
        v["activity"]["query"]["filters"]["user_id"] == people::ADA
            && v["metadata"]["busy"] == false
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "selected",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    input(desktop, SELECT, "").await?;
    until(state, "actor cleared", |v| v["picker"]["filterUser"] == "").await?;
    for label in ["query-retired", "filter-retired"] {
        people::mode(&state.peek(), people::Mode::Held)?;
        input(
            desktop,
            SEARCH,
            if label == "query-retired" { "ad" } else { "a" },
        )
        .await?;
        until(state, "held member read", |v| v["ledger"]["held"] == true).await?;
        let before = member(state);
        if label == "query-retired" {
            wait_dom(desktop,r#"(()=>{document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'z'}));return{ok:true}})()"#).await?;
            input(desktop, SEARCH, "zz-no-match").await?;
        } else {
            input(desktop, "workflow-activity-status", "SUCCESS").await?;
        }
        until(state, "old callback cancelled", |v| {
            v["ledger"]["cancelled"].as_u64() > before["ledger"]["cancelled"].as_u64()
        })
        .await?;
        let release = people::release(&state.peek())?;
        if release["delivered"] != false {
            return Err("Retired member response remained deliverable".into());
        }
        stage(
            state,
            desktop,
            name,
            label,
            json!({"before":before["picker"],"release":release}),
            &mut stages,
            &mut captures,
        )
        .await?;
        if label == "query-retired" {
            people::mode(&state.peek(), people::Mode::Empty)?;
            let after = searches(&member(state));
            wait_dom(desktop,r#"(()=>{document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'z'}));return{ok:true}})()"#).await?;
            done(state, after).await?;
        }
    }
    for (label, mode) in [
        ("malformed", people::Mode::Malformed),
        ("failure", people::Mode::Failure),
        ("retried", people::Mode::Success),
    ] {
        people::mode(&state.peek(), mode)?;
        let before = searches(&member(state));
        click(desktop, FIND).await?;
        done(state, before).await?;
        until(state, label, |v| {
            if label == "retried" {
                v["picker"]["notice"].is_null()
                    && v["picker"]["rows"].as_array().is_some_and(|r| r.len() == 2)
            } else {
                v["picker"]["notice"] == if label == "failure" { OFFLINE } else { FAILURE }
            }
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            label,
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    people::mode(&state.peek(), people::Mode::Empty)?;
    let before = searches(&member(state));
    input(desktop, SEARCH, "zz-no-match").await?;
    done(state, before).await?;
    stage(
        state,
        desktop,
        name,
        "empty",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-activity-back").await?;
    wait(state, "retained metadata", |v| {
        exact(v) && v["activity"]["active"] == false && v["metadata"]["writes"] == 0
    })
    .await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
    stage(
        state,
        desktop,
        name,
        "returned",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":"picker","readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
