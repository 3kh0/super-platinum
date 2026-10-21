//! Original overlapping callbacks, with a newer native notice owner.
use super::super::{channel as channels, file as files};
use super::*;
use crate::slack_lists::fixture::activity_navigation as destination;

const LIST_BUTTON: &str = "workflow-activity-file-trigger-1";
const CHANNEL_BUTTON: &str = "workflow-activity-channel-step-0-1";
const LIST_OFFLINE: &str = "Connect to Slack to open this List.";
const CHANNEL_OFFLINE: &str = "Connect to Slack to open this conversation.";

fn canonical(s: &ShellState) -> Value {
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .map(|h| crate::app_home::about::management::activity::file_snapshot(s, h.id))
        .unwrap_or(Value::Null)
}

async fn native(d: &DesktopContext, capture_original: bool) -> Result<Value, String> {
    wait_dom(
        d,
        &include_str!("mixed_navigation/native.js")
            .replace("CAPTURE", &capture_original.to_string()),
    )
    .await
}

async fn labels(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let fetched = channels::snapshot(&s.peek())["missingPresent"] == true;
    wait_dom(
        d,
        &include_str!("mixed_navigation/labels.js").replace(
            "EXPECTED",
            &json!({"list":files::list_label(),"canvas":files::canvas_label(),
                "channel":if fetched {"activity-fetched"} else {"Channel unavailable"},
                "url":files::file()["permalink"]})
            .to_string(),
        ),
    )
    .await
}

async fn observed(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let model = snapshot(&s.peek());
    let dom = if model["activity"]["active"] == true && model["activity"]["detail"].is_object() {
        native(d, false).await?
    } else {
        Value::Null
    };
    let shell = s.peek();
    Ok(json!({"model":model,"channel":channels::snapshot(&shell),
        "files":files::snapshot(&shell),"canonical":canonical(&shell),
        "custody":files::custody(&shell),"native":dom}))
}

async fn settled(
    s: Signal<ShellState>,
    label: &str,
    predicate: impl Fn(&Value, &Value) -> bool,
) -> Result<(), String> {
    for _ in 0..100 {
        let (f, c) = {
            let shell = s.peek();
            (files::snapshot(&shell), channels::snapshot(&shell))
        };
        if predicate(&f, &c) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "{label}: files={},channel={}",
        files::snapshot(&s.peek()),
        channels::snapshot(&s.peek())
    ))
}

fn entry(draft: &Value, admitted: &Value) -> Result<Value, String> {
    let revisions = |v: &Value| {
        v["documents"]
            .as_array()
            .filter(|rows| rows.len() == 1)
            .and_then(|rows| rows[0]["revision"].as_u64())
    };
    let before = revisions(draft).ok_or("Original workflow revision missing")?;
    let after = revisions(admitted).ok_or("Admitted workflow revision missing")?;
    let mut expected = draft.clone();
    expected["documents"][0]["revision"] = json!(after);
    if after <= before || &expected != admitted {
        return Err(format!(
            "Activity entry changed original custody: {draft} -> {admitted}"
        ));
    }
    Ok(json!({"draft":draft,"admitted":admitted,"revisionAdvanced":true}))
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
        return Err(format!(
            "Mixed stage lost the original dirty workflow: {label}"
        ));
    }
    let painted = if model["activity"]["active"] == true && model["activity"]["detail"].is_object()
    {
        labels(s, d).await?
    } else {
        Value::Null
    };
    let mut proof = observed(s, d).await?;
    let target = destination::snapshot(&s.peek());
    if target["requests"].as_array().is_none_or(|v| !v.is_empty())
        || !target["loaded"].is_null()
        || target["overlay"] != false
        || target["commentsOpen"] != false
        || target["mutations"] != 0
    {
        return Err(format!(
            "Superseded navigation opened a destination: {target}"
        ));
    }
    proof["destination"] = target;
    proof["labels"] = painted;
    proof["exercise"] = exercise;
    capture(s, d, name, label, proof, stages, captures).await
}

async fn notice(d: &DesktopContext, text: &str, phase: &str) -> Result<Value, String> {
    let script = format!(
        "(()=>{{const v={};return {{...v,ok:v.ok&&v.notice.text==={}&&v.owner?.phase==={}}}}})()",
        include_str!("mixed_navigation/native.js").replace("CAPTURE", "false"),
        json!(text),
        json!(phase)
    );
    wait_dom(d, &script).await
}

pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let reverse = case() == "mixed-navigation-reverse";
    let success = case() == "mixed-navigation-success";
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
    let draft = files::custody(&s.peek());
    stage(s, d, name, "draft", Value::Null, &mut stages, &mut captures).await?;
    click(d, "workflow-management-activity").await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('workflow-activity-run-E_REVIEW')})",
    )
    .await?;
    click(d, "workflow-activity-run-E_REVIEW").await?;
    wait(s, "original mixed detail", |v| {
        v["activity"]["detail"]["workflow_event"]["workflow_execution_id"] == FIRST
    })
    .await?;
    settled(s, "both deduplicated metadata reads", |f, c| {
        f["requests"].as_array().is_some_and(|v| v.len() == 2)
            && f["completed"] == 2
            && c["ledger"]["requests"]
                .as_array()
                .is_some_and(Vec::is_empty)
    })
    .await?;
    labels(s, d).await?;
    native(d, true).await?;
    let original = files::custody(&s.peek());
    let entry = entry(&draft, &original)?;
    stage(
        s,
        d,
        name,
        "detail",
        json!({"entry":entry}),
        &mut stages,
        &mut captures,
    )
    .await?;

    if reverse {
        files::mode(&s.peek(), files::Mode::HeldMalformed)?;
        click(d, LIST_BUTTON).await?;
        settled(s, "original List sender held", |f, _| f["held"] == 3).await?;
        notice(d, "", "admitted").await?;
    } else {
        channels::mode(&s.peek(), channels::Mode::Held)?;
        click(d, CHANNEL_BUTTON).await?;
        settled(s, "original channel sender held", |_, c| {
            c["ledger"]["held"] == true
        })
        .await?;
        notice(d, "Opening conversation\u{2026}", "pending").await?;
    }
    let held_before = observed(s, d).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let held_after = observed(s, d).await?;
    if held_before != held_after {
        return Err(format!(
            "Held original changed during quiet observation: {held_before} -> {held_after}"
        ));
    }
    stage(
        s,
        d,
        name,
        if reverse { "list-held" } else { "channel-held" },
        json!({"before":held_before,"after":held_after,"observedMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    if reverse {
        channels::mode(&s.peek(), channels::Mode::Offline)?;
        click(d, CHANNEL_BUTTON).await?;
        settled(s, "newer channel failed", |_, c| {
            c["ledger"]["completed"] == 1
        })
        .await?;
    } else {
        files::mode(&s.peek(), files::Mode::Offline)?;
        click(d, LIST_BUTTON).await?;
        settled(s, "newer List failed", |f, _| f["completed"] == 3).await?;
    }
    let latest = notice(
        d,
        if reverse {
            CHANNEL_OFFLINE
        } else {
            LIST_OFFLINE
        },
        "failure",
    )
    .await?;
    if latest["owner"]["chain"].as_u64() <= held_after["native"]["owner"]["chain"].as_u64() {
        return Err("Newer action did not claim a later original chain".into());
    }
    stage(
        s,
        d,
        name,
        if reverse {
            "channel-failed"
        } else {
            "list-failed"
        },
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;

    let before = observed(s, d).await?;
    let release = if reverse {
        files::release(&s.peek())?
    } else {
        channels::release_mixed(&s.peek(), success)?
    };
    if release["delivered"] != true
        || release["receiverRetired"] != false
        || release["serial"] != if reverse { json!(3) } else { json!(1) }
    {
        return Err(format!(
            "Original held response was not delivered: {release}"
        ));
    }
    settled(s, "both original responses settled once", |f, c| {
        f["completed"] == 3
            && f["cancelled"] == 0
            && c["ledger"]["completed"] == 1
            && c["ledger"]["cancelled"] == 0
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observed(s, d).await?;
    for field in ["model", "custody", "native"] {
        if before[field] != after[field] {
            return Err(format!(
                "Late response changed current {field}: before={},after={}",
                before[field], after[field]
            ));
        }
    }
    if before["channel"]["selected"] != after["channel"]["selected"]
        || before["channel"]["generation"] != after["channel"]["generation"]
    {
        return Err("Old channel result navigated after the newer List failure".into());
    }
    let publication = json!({"before":before["channel"]["sourceRows"],
        "after":after["channel"]["sourceRows"],
        "beforeMissing":before["channel"]["missingPresent"],"afterMissing":after["channel"]["missingPresent"]});
    stage(s, d, name, if reverse {"late-list-failed"} else if success {"late-channel-success"} else {"late-channel-failed"},
        json!({"before":before,"release":release,"after":after,"observedMs":500,"cachePublication":publication}),
        &mut stages, &mut captures).await?;
    click(d, "workflow-activity-back").await?;
    wait_dom(d,&format!("({{ok:document.getElementById('workflow-management-title')?.value==={}&&document.getElementById('workflow-management-description')?.value==={}&&!document.getElementById('workflow-activity')}})",json!(TITLE),json!(DESCRIPTION))).await?;
    if files::custody(&s.peek()) != original {
        return Err("Mixed navigation Back replaced the original admitted draft".into());
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
        "actualActivityEntry":true,"aboutConfiguration":true,"draftRetained":true,
        "readOnly":true,"networkCalls":0,"noLiveNetwork":true,
        "viewport":stages.first().map(|s|s["geometry"]["viewport"].clone()),
        "final":snapshot(&s.peek()),"files":files::snapshot(&s.peek()),
        "channel":channels::snapshot(&s.peek()),"destination":destination::snapshot(&s.peek()),
        "stages":stages,"captures":captures}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_entry_preserves_every_fact_except_original_revision_advance() {
        let before = json!({"documents":[{"id":3,"revision":7,"title":"Draft",
            "description":"Original","opaque":[false,0,null]}],"account":"original"});
        let mut after = before.clone();
        after["documents"][0]["revision"] = json!(9);
        assert!(entry(&before, &after).is_ok());
        assert!(entry(&before, &before).is_err());
        for key in ["id", "title", "description", "opaque"] {
            let mut changed = after.clone();
            changed["documents"][0][key] = json!("replaced");
            assert!(entry(&before, &changed).is_err());
        }
        after["account"] = json!("foreign");
        assert!(entry(&before, &after).is_err());
    }
}
