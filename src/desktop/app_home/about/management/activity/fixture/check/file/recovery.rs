//! Actual cold metadata failures, original Retry controls and held-read retirement.
use super::*;
const LOADING: &str = "Loading file details\u{2026}";
const OFFLINE: &str = "Connect to Slack to load file details. Retry file details when connected.";
const FAILED: &str = "File details could not load. Retry file details to try again.";

fn expected(stage: &str) -> Option<Value> {
    let (canvas, list, retries, notice) = match stage {
        "metadata-failed" => (
            false,
            false,
            vec![endpoint::LIST, endpoint::LIST, endpoint::CANVAS],
            Some(OFFLINE),
        ),
        "metadata-held" | "retry-canvas-held" => (false, false, vec![], Some(LOADING)),
        "canvas-recovered" => (
            true,
            false,
            vec![endpoint::LIST, endpoint::LIST],
            Some(FAILED),
        ),
        "retry-list-held" => (true, false, vec![], Some(LOADING)),
        "recovered" => (true, true, vec![], None),
        _ => return None,
    };
    Some(
        json!({"canvasReady":canvas,"listReady":list,"retries":retries,"notice":notice,
        "list":endpoint::LIST,"canvas":endpoint::CANVAS,"listLabel":endpoint::LIST_LABEL,
        "canvasLabel":endpoint::CANVAS_LABEL,"url":endpoint::file()["permalink"],
        "listPath":crate::icons::Icon::TableRows.path(),"canvasPath":crate::icons::Icon::Description.path()}),
    )
}
async fn measure(d: &DesktopContext, label: &str) -> Result<Option<Value>, String> {
    let Some(expected) = expected(label) else {
        return Ok(None);
    };
    wait_dom(
        d,
        &include_str!("recovery.js").replace("EXPECTED", &expected.to_string()),
    )
    .await
    .map(Some)
}
fn isolated(s: &ShellState, original: &Value) -> Result<(), String> {
    let target = destination::snapshot(s);
    if endpoint::custody(s) != *original
        || target["requests"].as_array().is_none_or(|v| !v.is_empty())
        || !target["loaded"].is_null()
        || target["overlay"] != false
    {
        return Err(format!(
            "Metadata recovery lost original custody or opened destination: original={original},actual={},destination={target}",
            endpoint::custody(s)
        ));
    }
    Ok(())
}
async fn quiet(s: Signal<ShellState>, original: &Value) -> Result<Value, String> {
    isolated(&s.peek(), original)?;
    let files = endpoint::snapshot(&s.peek());
    let catalog = canonical(&s.peek());
    tokio::time::sleep(Duration::from_millis(500)).await;
    isolated(&s.peek(), original)?;
    if endpoint::snapshot(&s.peek()) != files || canonical(&s.peek()) != catalog {
        return Err(
            "Metadata acquisition changed without explicit action during quiet observation".into(),
        );
    }
    Ok(json!({"observedMs":500,"noAutomaticRead":true,"files":files,"canonical":catalog}))
}
async fn record(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    original: Option<&Value>,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if let Some(original) = original {
        isolated(&s.peek(), original)?;
    }
    let model = snapshot(&s.peek());
    if label != "about" && (!exact(&model) || model["metadata"]["writes"] != 0) {
        return Err(format!(
            "Recovery stage lost dirty original workflow: {label}"
        ));
    }
    let file_labels = measure(d, label).await?;
    if label == "recovered" {
        labels(d, false).await?;
    }
    if matches!(label, "metadata-held" | "retired" | "late-refused") {
        let catalog = canonical(&s.peek());
        if catalog["entries"].as_array().is_none_or(|rows| {
            rows.len() != 1
                || rows[0]["id"] != endpoint::CANVAS
                || rows[0]["current"] != false
                || !rows[0]["source"].is_null()
                || !rows[0]["effective"].is_null()
                || !rows[0]["receipt"].is_null()
                || !rows[0]["response"].is_null()
        }) {
            return Err(format!(
                "Held or retired original metadata published canonical facts: {catalog}"
            ));
        }
    }
    let proof = json!({"files":endpoint::snapshot(&s.peek()),"canonical":canonical(&s.peek()),
        "destination":destination::snapshot(&s.peek()),"custody":endpoint::custody(&s.peek()),
        "exercise":exercise,"fileLabels":file_labels});
    capture(s, d, name, label, proof.clone(), stages, captures).await?;
    if endpoint::snapshot(&s.peek()) != proof["files"]
        || canonical(&s.peek()) != proof["canonical"]
        || endpoint::custody(&s.peek()) != proof["custody"]
        || destination::snapshot(&s.peek()) != proof["destination"]
    {
        return Err(format!(
            "Recovery capture changed original evidence: {label}"
        ));
    }
    Ok(())
}
async fn open_detail(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    click(d, "workflow-activity-run-E_REVIEW").await?;
    wait(s, "original cold file detail", |v| {
        v["activity"]["detail"]["workflow_event"]["workflow_execution_id"] == FIRST
    })
    .await?;
    Ok(())
}
async fn retry(d: &DesktopContext, id: &str) -> Result<Value, String> {
    let script=r#"(()=>{
 const id=EXPECTED,detail=document.getElementById('workflow-activity-detail'),buttons=[...detail?.querySelectorAll('button[data-file-retry]')||[]].filter(e=>e.dataset.fileRetry===id);
 const count=id==='FCANON003'?2:1;
 if(!detail?.isConnected||buttons.length!==count)return{ok:false};
 const b=buttons[0],occurrence=b.previousElementSibling;
 b.scrollIntoView({block:'nearest'});
 const r=b.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
 if(b.disabled||!b.isConnected||b.textContent!=='Retry file details'||!occurrence?.classList.contains('workflow-activity-file')||occurrence.dataset.file!==id||r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||(h!==b&&!b.contains(h)))return{ok:false};
 const proof={ok:true,id,text:b.textContent,part:occurrence.dataset.part,originalConnected:true,buttonCount:count,detailRevision:detail.dataset.detailRevision,activation:'original-dom-click',duplicateActivation:true};
 // Both invocations use retained actual nodes in this one renderer turn; no model callback.
 b.click();(buttons[1]||b).click();
 return proof;
})()"#.replace("EXPECTED",&json!(id).to_string());
    wait_dom(d, &script).await
}
async fn ready(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..100 {
        let v = canonical(&s.peek());
        if v["pendingFile"].is_null()
            && v["entries"].as_array().is_some_and(|v| {
                v.len() == 2
                    && v.iter()
                        .all(|e| e["phase"] == "ready" && e["current"] == true)
            })
        {
            labels(d, false).await?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Canonical metadata did not recover: {}",
        canonical(&s.peek())
    ))
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    record(
        s,
        d,
        name,
        "about",
        Value::Null,
        None,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(d, "workflow-configuration").await?;
    wait(s, "metadata editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(d, "workflow-management-title", TITLE).await?;
    input(d, "workflow-management-description", DESCRIPTION).await?;
    wait(s, "original dirty workflow", exact).await?;
    let draft = endpoint::custody(&s.peek());
    record(
        s,
        d,
        name,
        "draft",
        Value::Null,
        Some(&draft),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(d, "workflow-management-activity").await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('workflow-activity-run-E_REVIEW')})",
    )
    .await?;
    open_detail(s, d).await?;
    let original = endpoint::custody(&s.peek());
    let entry = entry_custody(&draft, &original)?;
    if case() == "file-recovery-retirement" {
        file_wait(s, "original cold Canvas held", |v| {
            v["held"] == 1 && v["completed"] == 0
        })
        .await?;
        measure(d, "metadata-held").await?;
        let q = quiet(s, &original).await?;
        record(
            s,
            d,
            name,
            "metadata-held",
            json!({"entry":entry,"quiet":q}),
            Some(&original),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(d, "workflow-activity-close-detail").await?;
        wait(s, "cold detail closed", |v| {
            v["activity"]["detail"].is_null()
        })
        .await?;
        file_wait(s, "original metadata future dropped", |v| {
            v["completed"] == 1 && v["cancelled"] == 1
        })
        .await?;
        let q = quiet(s, &original).await?;
        record(
            s,
            d,
            name,
            "retired",
            json!({"quiet":q}),
            Some(&original),
            &mut stages,
            &mut captures,
        )
        .await?;
        let release = endpoint::release(&s.peek())?;
        if release["serial"] != 1
            || release["receiverRetired"] != true
            || release["delivered"] != false
        {
            return Err(format!(
                "Original retired metadata accepted late response: {release}"
            ));
        }
        let q = quiet(s, &original).await?;
        record(
            s,
            d,
            name,
            "late-refused",
            json!({"release":release,"quiet":q}),
            Some(&original),
            &mut stages,
            &mut captures,
        )
        .await?;
        open_detail(s, d).await?;
        file_wait(s, "fresh detail metadata completed", |v| {
            v["requests"].as_array().map(Vec::len) == Some(3)
                && v["completed"] == 3
                && v["cancelled"] == 1
        })
        .await?;
        ready(s, d).await?;
        let q = quiet(s, &original).await?;
        record(
            s,
            d,
            name,
            "recovered",
            json!({"quiet":q}),
            Some(&original),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        file_wait(s, "both cold metadata failures", |v| {
            v["requests"].as_array().map(Vec::len) == Some(2) && v["completed"] == 2
        })
        .await?;
        measure(d, "metadata-failed").await?;
        let q = quiet(s, &original).await?;
        record(
            s,
            d,
            name,
            "metadata-failed",
            json!({"entry":entry,"quiet":q}),
            Some(&original),
            &mut stages,
            &mut captures,
        )
        .await?;
        for (id, held_label, settled_label, serial) in [
            (endpoint::CANVAS, "retry-canvas-held", "canvas-recovered", 3),
            (endpoint::LIST, "retry-list-held", "recovered", 4),
        ] {
            let attempt = retry(d, id).await?;
            file_wait(s, "explicit original Retry held", |v| {
                v["held"] == serial && v["requests"].as_array().map(Vec::len) == Some(serial)
            })
            .await?;
            measure(d, held_label).await?;
            let q = quiet(s, &original).await?;
            record(
                s,
                d,
                name,
                held_label,
                json!({"attempt":attempt,"quiet":q}),
                Some(&original),
                &mut stages,
                &mut captures,
            )
            .await?;
            let release = endpoint::release(&s.peek())?;
            if release["serial"] != serial
                || release["receiverRetired"] != false
                || release["delivered"] != true
            {
                return Err(format!(
                    "Original current metadata release failed: {release}"
                ));
            }
            file_wait(s, "released original metadata completed", |v| {
                v["completed"] == serial
            })
            .await?;
            measure(d, settled_label).await?;
            if settled_label == "recovered" {
                ready(s, d).await?;
            }
            let q = quiet(s, &original).await?;
            record(
                s,
                d,
                name,
                settled_label,
                json!({"release":release,"quiet":q}),
                Some(&original),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
    }
    click(d, "workflow-activity-back").await?;
    wait_dom(d,&format!("({{ok:document.getElementById('workflow-management-title')?.value==={}&&document.getElementById('workflow-management-description')?.value==={}&&!document.getElementById('workflow-activity')}})",json!(TITLE),json!(DESCRIPTION))).await?;
    record(
        s,
        d,
        name,
        "back",
        json!({"originalCustody":original}),
        Some(&original),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"actualActivityEntry":true,
        "aboutConfiguration":true,"metadataOnly":true,"noDestination":true,"draftRetained":true,
        "readOnly":true,"networkCalls":0,"noLiveNetwork":true,"viewport":stages.first().map(|s|s["geometry"]["viewport"].clone()),
        "final":snapshot(&s.peek()),"files":endpoint::snapshot(&s.peek()),"destination":destination::snapshot(&s.peek()),
        "stages":stages,"captures":captures}),
    )
}
