//! Real Activity occurrence -> List read -> Close -> retained dirty metadata.
use super::super::file as endpoint;
use super::*;
use crate::slack_lists::fixture::activity_navigation as destination;
mod emoji;
mod recovery;
mod refusal;
const FIRST_LINK: &str = "workflow-activity-file-trigger-1";
const SECOND_LINK: &str = "workflow-activity-file-trigger-3";

fn entry_custody(draft: &Value, admitted: &Value) -> Result<Value, String> {
    let before = draft["documents"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("Original single workflow document missing")?;
    let after = admitted["documents"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("Admitted single workflow document missing")?;
    let original_revision = before[0]["revision"]
        .as_u64()
        .ok_or("Original revision missing")?;
    let admitted_revision = after[0]["revision"]
        .as_u64()
        .ok_or("Admitted revision missing")?;
    let mut expected = draft.clone();
    expected["documents"][0]["revision"] = json!(admitted_revision);
    if admitted_revision <= original_revision || &expected != admitted {
        return Err(format!(
            "Activity entry changed facts beyond its original buffer revision: draft={draft},admitted={admitted}"
        ));
    }
    Ok(json!({"draft":draft,"admitted":admitted,"revisionAdvanced":true}))
}

fn canonical(s: &ShellState) -> Value {
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .map(|h| crate::app_home::about::management::activity::file_snapshot(s, h.id))
        .unwrap_or(Value::Null)
}
async fn file_wait(
    s: Signal<ShellState>,
    label: &str,
    p: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..100 {
        let v = endpoint::snapshot(&s.peek());
        if p(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{label}: {}", endpoint::snapshot(&s.peek())))
}
async fn labels(d: &DesktopContext, inert: bool) -> Result<Value, String> {
    let probe = if endpoint::emoji_requested() {
        include_str!("file/emoji.js")
    } else {
        include_str!("file/labels.js")
    };
    let script = probe.replace(
        "EXPECTED",
        &json!({"list":endpoint::LIST,"canvas":endpoint::CANVAS,
            "listLabel":endpoint::list_label(),"canvasLabel":endpoint::canvas_label(),
            "url":endpoint::file()["permalink"],"inert":inert,"cold":endpoint::cold_requested(),"encoded":endpoint::encoded_requested(),
            "listPath":crate::icons::Icon::TableRows.path(),
            "canvasPath":crate::icons::Icon::Description.path()})
        .to_string(),
    );
    wait_dom(d, &script).await
}
async fn detail(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    click(d, "workflow-activity-run-E_REVIEW").await?;
    wait(s, "original file detail", |v| {
        v["activity"]["detail"]["workflow_event"]["workflow_execution_id"] == FIRST
    })
    .await?;
    labels(d, false).await
}
async fn stage(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let model = snapshot(&s.peek());
    if label != "about" && (!exact(&model) || model["metadata"]["writes"] != 0) {
        return Err(format!("File stage lost original dirty workflow: {label}"));
    }
    let file_labels = if model["activity"]["active"] == true
        && model["activity"]["detail"].is_object()
        && !matches!(label, "destination" | "calendar-refused")
    {
        Some(labels(d, label == "composition-refused").await?)
    } else {
        None
    };
    let proof = json!({"files":endpoint::snapshot(&s.peek()),"canonical":canonical(&s.peek()),
        "destination":destination::snapshot(&s.peek()),"custody":endpoint::custody(&s.peek()),
        "exercise":exercise,"fileLabels":file_labels});
    if label != "destination" {
        return capture(s, d, name, label, proof, stages, captures).await;
    }
    let target_probe = if endpoint::emoji_requested() {
        include_str!("file/header-emoji.js")
    } else {
        include_str!("file/target.js")
    };
    let mut geometry = wait_dom(
        d,
        target_probe
            .replace(
                "EXPECTED",
                &json!({"id":endpoint::LIST,"title":endpoint::destination_label(),"row":endpoint::ROW,"cold":endpoint::cold_requested()})
                    .to_string(),
            )
            .as_str(),
    )
    .await?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture worker")?
        .map_err(|_| "Capture path")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, w, h) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    geometry["pngDimensions"] = json!([w, h]);
    if endpoint::custody(&s.peek()) != proof["custody"]
        || destination::snapshot(&s.peek()) != proof["destination"]
    {
        return Err("Destination capture changed original custody or reader".into());
    }
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}
async fn navigated(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..100 {
        let v = destination::snapshot(&s.peek());
        if v["loaded"]["id"] == endpoint::LIST
            && v["loaded"]["loading"] == false
            && v["completed"] == 1
            && v["loaded"]["records"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
        {
            wait_dom(d, "({ok:!!document.getElementById('slack-list-close')})").await?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Actual List reader did not settle: {}",
        destination::snapshot(&s.peek())
    ))
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    if endpoint::recovery_requested() {
        return recovery::run(s, d, name).await;
    }
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    stage(s, d, name, "about", Value::Null, &mut stages, &mut captures).await?;
    click(d, "workflow-configuration").await?;
    wait(s, "metadata editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(d, "workflow-management-title", TITLE).await?;
    input(d, "workflow-management-description", DESCRIPTION).await?;
    wait(s, "original dirty workflow", exact).await?;
    let draft_custody = endpoint::custody(&s.peek());
    stage(s, d, name, "draft", Value::Null, &mut stages, &mut captures).await?;
    click(d, "workflow-management-activity").await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('workflow-activity-run-E_REVIEW')})",
    )
    .await?;
    let proof = detail(s, d).await?;
    file_wait(s, "both original metadata reads completed", |v| {
        v["requests"].as_array().is_some_and(|v| v.len() == 2) && v["completed"] == 2
    })
    .await?;
    let original = endpoint::custody(&s.peek());
    let entry = entry_custody(&draft_custody, &original)?;
    stage(
        s,
        d,
        name,
        "detail",
        json!({"labels":proof,"entry":entry}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if matches!(case().as_str(), "file-retirement" | "file-emoji-retirement") {
        endpoint::mode(&s.peek(), endpoint::Mode::Held)?;
        click(d, SECOND_LINK).await?;
        file_wait(s, "original resolver held", |v| v["held"] == 3).await?;
        click(d, FIRST_LINK).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        if endpoint::snapshot(&s.peek())["requests"]
            .as_array()
            .map(Vec::len)
            != Some(3)
        {
            return Err("Duplicate held occurrence dispatched twice".into());
        }
        stage(
            s,
            d,
            name,
            "held",
            json!({"duplicateClick":true,"observedMs":150}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(d, "workflow-activity-close-detail").await?;
        wait(s, "original detail retired", |v| {
            v["activity"]["detail"].is_null()
        })
        .await?;
        file_wait(s, "original resolver dropped", |v| {
            v["cancelled"] == 1 && v["completed"] == 3
        })
        .await?;
        stage(
            s,
            d,
            name,
            "retired",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let released = endpoint::release(&s.peek())?;
        if released["receiverRetired"] != true || released["delivered"] != false {
            return Err("Retired original resolver accepted late response".into());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        if destination::snapshot(&s.peek())["requests"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
        {
            return Err("Retired resolver opened destination".into());
        }
        stage(
            s,
            d,
            name,
            "late-refused",
            released,
            &mut stages,
            &mut captures,
        )
        .await?;
        detail(s, d).await?;
    } else {
        if endpoint::emoji_requested() && !endpoint::cold_requested() {
            let proof = emoji::refusal(s, d).await?;
            stage(
                s,
                d,
                name,
                "label-refused",
                proof,
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        for kind in ["filter-refused", "search-refused"] {
            let proof = refusal::buffer(s, d, kind).await?;
            stage(s, d, name, kind, proof, &mut stages, &mut captures).await?;
            refusal::restore(d, kind).await?;
        }
        let proof = refusal::composition(s, d).await?;
        stage(
            s,
            d,
            name,
            "composition-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        refusal::composition_end(d).await?;
        let proof = refusal::calendar(s, d).await?;
        stage(
            s,
            d,
            name,
            "calendar-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        refusal::calendar_end(d).await?;
        for (label, mode) in [
            ("malformed", endpoint::Mode::Malformed),
            ("wrong-id", endpoint::Mode::WrongId),
            ("unsupported", endpoint::Mode::Unsupported),
            ("deleted", endpoint::Mode::Deleted),
            ("offline", endpoint::Mode::Offline),
        ] {
            endpoint::mode(&s.peek(), mode)?;
            let before = endpoint::snapshot(&s.peek())["completed"].as_u64().unwrap();
            click(d, SECOND_LINK).await?;
            file_wait(s, label, |v| v["completed"] == before + 1).await?;
            let notice = if label == "offline" {
                "Connect to Slack to open this List."
            } else {
                "Could not open this List. It may no longer be accessible. Try again."
            };
            wait_dom(d,&format!("({{ok:document.getElementById('workflow-activity-navigation-notice')?.textContent==={}&&!document.getElementById('slack-list-close')}})",json!(notice))).await?;
            stage(
                s,
                d,
                name,
                label,
                json!({"notice":notice}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
    }
    let navigation_detail = wait_dom(d,"(()=>{const e=document.getElementById('workflow-activity-detail'),n=document.getElementById('workflow-activity-navigation-notice');return {ok:!!(e?.isConnected&&n?.isConnected&&/^\\d+$/.test(e.dataset.detailRevision)&&e.dataset.detailRevision===n.dataset.detailRevision),revision:e?.dataset.detailRevision}})()").await?;
    click(d, SECOND_LINK).await?;
    navigated(s, d).await?;
    if endpoint::custody(&s.peek()) != original {
        return Err(format!(
            "List navigation replaced original workflow draft or editor: before={original},after={}",
            endpoint::custody(&s.peek())
        ));
    }
    stage(
        s,
        d,
        name,
        "destination",
        json!({"button":SECOND_LINK,"originalCustody":original}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(d, "slack-list-close").await?;
    wait_dom(d,"({ok:!document.getElementById('shell-overlay-dialog')&&!!document.getElementById('workflow-activity-detail')})").await?;
    let returned_notice = wait_dom(
        d,
        &format!("(()=>{{const e=document.getElementById('workflow-activity-detail'),n=document.getElementById('workflow-activity-navigation-notice');return {{ok:!!(e?.isConnected&&n?.isConnected&&e.dataset.detailRevision==={}&&n.dataset.detailRevision===e.dataset.detailRevision&&n.textContent===''),navigationNotice:n?.textContent}}}})()",navigation_detail["revision"]),
    )
    .await?;
    if endpoint::custody(&s.peek()) != original {
        return Err(format!(
            "Closing List changed original workflow: before={original},after={}",
            endpoint::custody(&s.peek())
        ));
    }
    stage(
        s,
        d,
        name,
        "returned",
        json!({"originalCustody":original,"navigationNotice":returned_notice["navigationNotice"]}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(d, "workflow-activity-back").await?;
    wait_dom(d,&format!("({{ok:document.getElementById('workflow-management-title')?.value==={}&&document.getElementById('workflow-management-description')?.value==={}&&!document.getElementById('workflow-activity')}})",json!(TITLE),json!(DESCRIPTION))).await?;
    if endpoint::custody(&s.peek()) != original {
        return Err(format!(
            "Activity Back changed original workflow: before={original},after={}",
            endpoint::custody(&s.peek())
        ));
    }
    stage(
        s,
        d,
        name,
        "back",
        json!({"originalCustody":original}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,
        "actualActivityEntry":true,"aboutConfiguration":true,"nativeListDestination":true,
        "draftRetained":true,"readOnly":true,"networkCalls":0,"noLiveNetwork":true,
        "viewport":stages.first().map(|s|s["geometry"]["viewport"].clone()),
        "final":snapshot(&s.peek()),"files":endpoint::snapshot(&s.peek()),
        "destination":destination::snapshot(&s.peek()),"stages":stages,"captures":captures}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_admission_allows_only_the_one_original_revision_advance() {
        let before = json!({"documents":[{"id":3,"revision":7,"title":"Draft","description":"Original",
            "pending":null,"workflow":{"opaque":[false,0,null]}}],"account":"original","epoch":1});
        let mut after = before.clone();
        after["documents"][0]["revision"] = json!(9);
        assert!(entry_custody(&before, &after).is_ok());
        for key in ["id", "title", "description", "pending", "workflow"] {
            let mut bad = after.clone();
            bad["documents"][0][key] = json!("changed");
            assert!(entry_custody(&before, &bad).is_err());
        }
        for key in ["account", "epoch"] {
            let mut bad = after.clone();
            bad[key] = json!("foreign");
            assert!(entry_custody(&before, &bad).is_err());
        }
        assert!(entry_custody(&before, &before).is_err());
        after["documents"][0]["revision"] = json!(6);
        assert!(entry_custody(&before, &after).is_err());
    }
}
