//! Late facts are released through the original server handle after retirement.
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let original = endpoint::capture(&state.peek())?;
    let before = observe(state, desktop, visit).await?;
    let document = before["permissions"]["document"]["id"]
        .as_u64()
        .ok_or("Original document missing")?;
    let original_document = retained(state, document)?;
    let account = endpoint::case() == "managers-agents-account-retirement";
    let replacement = if account {
        let replacement = replace_account(state)?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
        replacement
    } else {
        query(state, desktop, visit, "ordinary").await?;
        until(
            state,
            visit,
            "actual replacement ordinary query settled",
            |d, _| {
                let m = &d["confirmation"]["managerDraft"];
                m["query"] == "ordinary"
                    && m["pending"] == false
                    && m["agentFacts"].is_null()
                    && m["results"]
                        .as_array()
                        .is_some_and(|r| r.iter().any(|id| id == "UMANLOCAL"))
            },
        )
        .await?;
        Value::Null
    };
    let mut retired = false;
    for _ in 0..100 {
        if original.agent_receiver_retired() {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !retired {
        return Err("Original app fact receiver did not retire".into());
    }
    let successor = observe(state, desktop, visit).await?;
    let retained_after = retained(state, document)?;
    if original_document != retained_after {
        return Err(format!(
            "Query/account retirement changed original draft: before={original_document} after={retained_after}"
        ));
    }
    if account {
        if successor["server"]["ownerCurrent"] != false
            || successor["permissions"]["visible"] != false
        {
            return Err("Replacement retained original manager fact authority".into());
        }
    } else if !draft(&successor)
        || successor["permissions"]["confirmation"]["managerDraft"]["ids"]
            != before["permissions"]["confirmation"]["managerDraft"]["ids"]
    {
        return Err("Query retirement changed selections or original draft".into());
    }
    stage(state,desktop,name,if account{"successor"}else{"superseded"},visit,
        json!({"before":before,"after":successor,"replacement":replacement,"originalDocument":original_document,"retainedDocument":retained_after,"receiverRetired":true}),captures,stages).await?;
    let before = observe(state, desktop, visit).await?;
    let release = original.release()?;
    if release["delivered"] != false || release["receiverRetired"] != true {
        return Err(format!(
            "Retired original app fact was delivered: {release}"
        ));
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observe(state, desktop, visit).await?;
    let mut expected_server = before["server"].clone();
    expected_server["held"] = Value::Null;
    expected_server["releases"]
        .as_array_mut()
        .ok_or("Original release ledger missing")?
        .push(release.clone());
    let mut expected = before.clone();
    expected["server"] = expected_server;
    if expected != after || retained(state, document)? != original_document {
        return Err(format!(
            "Late original fact changed current source/native/draft: before={before} after={after}"
        ));
    }
    stage(state,desktop,name,"late-settled",visit,
        json!({"before":before,"release":release,"after":after,"observedMs":500,"originalDocument":original_document,"retainedDocument":retained(state,document)?}),captures,stages).await?;
    if !account {
        let exercise =
            actions::close_local(state, desktop, visit, "workflow-permissions-cancel").await?;
        stage(
            state,
            desktop,
            name,
            "cancelled",
            visit,
            exercise,
            captures,
            stages,
        )
        .await?;
    }
    Ok(replacement)
}

fn retained(state: Signal<ShellState>, id: u64) -> Result<Value, String> {
    let s = state.peek();
    let d = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == id)
        .ok_or("Original retained manager draft missing")?;
    Ok(
        json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger()}),
    )
}
