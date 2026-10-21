//! Native acceptance through About's Configuration entry; synthetic server only.
use super::*;
use crate::slack_links_check::wait_dom;
use dioxus::desktop::DesktopContext;
use std::time::Duration;
mod delete;
mod first_publish;
mod managers;
mod permissions;
mod publish;
mod run_access;
mod unpublish;

const TITLE: &str = "Révision de projet";
const DESCRIPTION: &str = "Keep the proposed details.\nReview 中🙂 e\u{301} together.";
const CHANGED_TITLE: &str = "Retained changed preflight 日本語";
const FLAG: &str = "SUPER_PLATINUM_WORKFLOW_MANAGEMENT_CHECK";

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_NONCE")
        .map_err(|_| "Missing workflow readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid workflow readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute workflow readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("workflow-management-{nonce}").as_str())
    {
        return Err("Wrong workflow readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect workflow readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid workflow readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect workflow readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid workflow readiness marker".into());
            }
            let bytes = std::fs::read(path).map_err(|_| "Read workflow readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse workflow readiness marker".into())
        })
        .await
        .map_err(|_| "Workflow readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Workflow readiness marker must be fresh".into());
    }
    eprintln!("workflow-management-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong workflow readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Workflow readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !crate::fixture::is_fixture() || std::env::var(FLAG).as_deref() != Ok("1") {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(
        name.as_str(),
        "workflow-managed"
            | "workflow-managed-narrow"
            | "workflow-editor"
            | "workflow-editor-narrow"
            | "workflow-reply-loss"
            | "workflow-reply-loss-narrow"
    ) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(150), async {
            if !std::env::var(super::first_publish::CASE_ENV)
                .unwrap_or_default()
                .is_empty()
            {
                if !super::first_publish::requested() {
                    return Err("Invalid or overlapping first-Publish flags".into());
                }
                first_publish::run(state, &desktop, &name).await
            } else if !std::env::var(super::publish::CASE_ENV)
                .unwrap_or_default()
                .is_empty()
            {
                if !super::publish::requested() {
                    return Err("Invalid or overlapping Publish fixture flags".into());
                }
                publish::run(state, &desktop, &name).await
            } else if !std::env::var(super::managers::CASE_ENV)
                .unwrap_or_default()
                .is_empty()
            {
                if !super::managers::requested() {
                    return Err("Invalid or overlapping Managers fixture flags".into());
                }
                managers::run(state, &desktop, &name).await
            } else if !std::env::var(super::run_access::CASE_ENV)
                .unwrap_or_default()
                .is_empty()
            {
                if !super::run_access::requested() {
                    return Err("Invalid or overlapping Run access fixture flags".into());
                }
                run_access::run(state, &desktop, &name).await
            } else if !std::env::var(super::permissions::CASE_ENV)
                .unwrap_or_default()
                .is_empty()
            {
                if !super::permissions::requested() {
                    return Err("Invalid or overlapping Permissions fixture flags".into());
                }
                permissions::run(state, &desktop, &name).await
            } else if super::unpublish::requested() {
                unpublish::run(state, &desktop, &name).await
            } else if super::delete::requested() {
                delete::run(state, &desktop, &name).await
            } else {
                run(state, &desktop, &name).await
            }
        })
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"fixture":name,"error":error}),
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Workflow management timed out"})
            }
        };
        if super::first_publish::requested()
            || super::publish::requested()
            || super::unpublish::requested()
            || super::permissions::requested()
            || super::run_access::requested()
            || super::managers::requested()
        {
            // The retained raw Unpublish evidence is large. Format and write
            // this owned receipt off the renderer so native ownership probes
            // can still receive window messages after the final capture.
            let _ = tokio::task::spawn_blocking(move || {
                let line = format!("workflow-management-check: {result}");
                eprintln!("{line}");
            })
            .await;
        } else {
            eprintln!("workflow-management-check: {result}");
        }
    });
}

async fn wait(
    state: Signal<ShellState>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..100 {
        let value = snapshot(&state.peek());
        if predicate(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{label}: {}", snapshot(&state.peek())))
}
async fn click(desktop: &DesktopContext, id: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled)return {{ok:false}};e.scrollIntoView({{block:'nearest'}});e.focus();const r=e.getBoundingClientRect(),a=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(a!==e&&!e.contains(a))return {{ok:false}};e.click();return {{ok:true}};}})()"#,json!(id))).await.map(|_|()).map_err(|e|format!("Click {id}: {e}"))
}
async fn input(desktop: &DesktopContext, id: &str, value: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled||e.readOnly)return {{ok:false}};e.scrollIntoView({{block:'nearest'}});e.focus();const r=e.getBoundingClientRect(),a=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(a!==e&&!e.contains(a))return {{ok:false}};Object.getOwnPropertyDescriptor(e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()"#,json!(id),json!(value))).await.map(|_|()).map_err(|e|format!("Input {id}: {e}"))
}
async fn edit(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    title: &str,
) -> Result<(), String> {
    input(desktop, "workflow-management-title", title).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "accepted exact proposed text", |v| {
        v["title"] == title && v["description"] == DESCRIPTION && v["dirty"] == true
    })
    .await?;
    Ok(())
}
fn isolated(s: &ShellState) -> bool {
    s.core.session.as_ref().is_some_and(|session| {
        session.d_cookie == "synthetic"
            && session.workspaces.len() == 1
            && session.workspaces.values().all(|workspace| {
                workspace.url == "https://example.test" && workspace.token == "synthetic-home-token"
            })
    }) && snapshot(s)["networkCalls"] == 0
}

async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !isolated(&state.peek()) {
        return Err("Synthetic workflow isolation changed".into());
    }
    let model = snapshot(&state.peek());
    let expected: Vec<String> = model["notice"]
        .as_str()
        .into_iter()
        .map(str::to_owned)
        .chain(
            model["outcomes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        )
        .collect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut geometry=wait_dom(desktop,&format!(r#"(()=>{{
      const root=document.getElementById('workflow-management'),config=document.getElementById('workflow-configuration');
      const rect=e=>{{const r=e.getBoundingClientRect();return {{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}};
      const visible=e=>{{const r=rect(e),s=getComputedStyle(e);const hits=[[r.left+1,r.top+1],[r.right-1,r.bottom-1]].every(([x,y])=>{{const a=document.elementFromPoint(x,y);return a===e||e.contains(a)}});return r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&hits&&e.scrollWidth<=e.clientWidth+1;}};
      const controls=root?[...root.querySelectorAll('.workflow-management-actions button')].filter(e=>!e.disabled):config?[config]:[];
      const feedback=root?[...root.querySelectorAll('.workflow-management-feedback p')].filter(e=>e.textContent.trim()):[];
      const texts=feedback.map(e=>e.textContent),expected={},locked={};
      const allVisible=controls.length>0&&controls.every(visible)&&feedback.every(visible);
      const title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');
      const save=document.getElementById('workflow-management-save'),saveStyle=save?getComputedStyle(save):null;
      const list=root&&!title; if(list){{controls.push(...root.querySelectorAll('.workflow-management-filters button,.workflow-management-row'));}}
      return {{ok:(list?controls.length>0&&controls.every(visible):allVisible)&&expected.every(t=>texts.includes(t))&&(!title||(title.readOnly===locked&&description?.readOnly===locked&&save&&visible(save)&&(!locked||(save.disabled&&Number(saveStyle.opacity)===0.55))))&&document.documentElement.scrollWidth<=innerWidth+1,
        viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1,
        controls:controls.map(e=>({{id:e.id,rect:rect(e),visible:visible(e)}})),feedback:feedback.map(e=>({{text:e.textContent,rect:rect(e),visible:visible(e)}})),
        save:save?{{disabled:save.disabled,opacity:Number(saveStyle.opacity),visible:visible(save),rect:rect(save)}}:null,
        fields:title&&description?{{title:title.value,description:description.value,titleReadonly:title.readOnly,descriptionReadonly:description.readOnly}}:null}};
    }})()"#,json!(expected),model["pending"])).await.map_err(|e|format!("Layout {stage}: {e}"))?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Inspect capture path failed")?
        .map_err(|_| "Inspect capture path failed")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let managed = name.starts_with("workflow-managed");
    let reply_loss = name.starts_with("workflow-reply-loss");
    let scenario = if managed {
        "managed"
    } else if reply_loss {
        "reply-loss"
    } else {
        "editor"
    };
    wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
    if snapshot(&state.peek())["route"] != "about" || !isolated(&state.peek()) {
        return Err("Expected isolated About entry".into());
    }
    let mut captures = serde_json::Map::new();
    let mut stages = Vec::new();
    let mut replacement = Value::Null;
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-configuration');e.scrollIntoView({block:'nearest'});return {ok:true}})()"#).await?;
    stage(state, desktop, name, "about", &mut captures, &mut stages).await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "Configuration destination", |v| {
        v["busy"] == false && v["route"] == if managed { "managed" } else { "editor" }
    })
    .await?;
    if managed {
        stage(state, desktop, name, "managed", &mut captures, &mut stages).await?;
        input(desktop, "workflow-management-query", "no matching workflow").await?;
        click(desktop, "workflow-management-search").await?;
        wait(state, "filtered managed list", |v| v["busy"] == false).await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('workflow-management-open-WF_REVIEW')&&document.getElementById('workflow-management')?.textContent.includes('No matching workflows.')})"#).await?;
        stage(
            state,
            desktop,
            name,
            "empty-search",
            &mut captures,
            &mut stages,
        )
        .await?;
        input(desktop, "workflow-management-query", "Review").await?;
        click(desktop, "workflow-management-search").await?;
        click(desktop, "workflow-management-open-WF_REVIEW").await?;
        wait(state, "managed workflow editor", |v| {
            v["route"] == "editor" && v["busy"] == false
        })
        .await?;
    }
    stage(state, desktop, name, "editor", &mut captures, &mut stages).await?;
    edit(state, desktop, TITLE).await?;
    stage(state, desktop, name, "draft", &mut captures, &mut stages).await?;
    if !managed && !reply_loss {
        set_mode(&mut state.write(), Mode::TriggerRejected);
    }
    click(desktop, "workflow-management-save").await?;
    wait(
        state,
        "save settled and canonical observation complete",
        |v| {
            v["writes"] == 2
                && v["busy"] == false
                && v["pending"] == !managed
                && v["observed"] == !managed
        },
    )
    .await?;
    let saved = snapshot(&state.peek());
    if saved["title"] != TITLE || saved["description"] != DESCRIPTION {
        return Err("Save lost proposed text".into());
    }
    let result_stage = if managed {
        "saved"
    } else if reply_loss {
        "unconfirmed"
    } else {
        "partial"
    };
    stage(
        state,
        desktop,
        name,
        result_stage,
        &mut captures,
        &mut stages,
    )
    .await?;
    if managed {
        if saved["dirty"] != false || saved["notice"] != "Workflow details saved." {
            return Err("Success lacked acknowledged matching readback".into());
        }
    } else {
        let expected = if reply_loss {
            json!(["Workflow: Unconfirmed", "Trigger: Unconfirmed"])
        } else {
            json!(["Workflow: Acknowledged", "Trigger: Rejected"])
        };
        if saved["outcomes"] != expected || saved["dirty"] != true {
            return Err("Independent slot outcomes or proposal were lost".into());
        }
        let reads = saved["reads"].as_u64().unwrap();
        click(desktop, "workflow-management-check").await?;
        wait(state, "read-only Check retains delivery uncertainty", |v| {
            v["busy"] == false
                && v["reads"].as_u64().is_some_and(|n| n > reads)
                && v["pending"] == true
                && v["observed"] == true
        })
        .await?;
        if snapshot(&state.peek())["writes"] != 2 {
            return Err("Check replayed an update".into());
        }
        stage(state, desktop, name, "checked", &mut captures, &mut stages).await?;
        let reads = snapshot(&state.peek())["reads"].as_u64().unwrap();
        click(desktop, "workflow-management-check").await?;
        wait(state, "repeated Check retains unresolved attempt", |v| {
            v["busy"] == false
                && v["reads"].as_u64().is_some_and(|n| n > reads)
                && v["pending"] == true
                && v["observed"] == true
        })
        .await?;
        if snapshot(&state.peek())["writes"] != 2 {
            return Err("Repeated Check replayed an update".into());
        }
        stage(
            state,
            desktop,
            name,
            "checked-again",
            &mut captures,
            &mut stages,
        )
        .await?;
        if !reply_loss {
            click(desktop, "workflow-management-use-current").await?;
            wait(state, "explicit current workflow adoption", |v| {
                v["pending"] == false && v["dirty"] == false && v["observed"] == false
            })
            .await?;
            stage(state, desktop, name, "adopted", &mut captures, &mut stages).await?;
            edit(state, desktop, CHANGED_TITLE).await?;
            set_mode(&mut state.write(), Mode::ChangedPreflight);
            click(desktop, "workflow-management-save").await?;
            wait(state,"changed preflight refusal",|v|v["busy"]==false&&v["pending"]==false&&v["notice"]=="Workflow or permissions changed. No update was sent; your draft is retained.").await?;
            if snapshot(&state.peek())["writes"] != 2 {
                return Err("Changed preflight dispatched an update".into());
            }
            stage(
                state,
                desktop,
                name,
                "preflight-refused",
                &mut captures,
                &mut stages,
            )
            .await?;
        } else {
            replacement = replace_account(state)?;
            let retained_exact = state
                .peek()
                .app_home
                .about
                .management
                .documents
                .iter()
                .any(|d| {
                    d.pending.is_some()
                        && d.title == TITLE
                        && d.description == DESCRIPTION
                        && d.dirty()
                });
            replacement["retainedDraftExact"] = json!(retained_exact);
            if !retained_exact
                || replacement["foreignDocumentHidden"] != true
                || replacement["retainedPending"] != 1
                || replacement["after"]["writes"] != 2
                || replacement["after"]["retainedAttempts"]
                    != replacement["before"]["retainedAttempts"]
            {
                return Err(
                    "Actual account replacement lost unresolved custody or replayed".into(),
                );
            }
            wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-configuration');if(!e||document.getElementById('workflow-management'))return {ok:false};e.scrollIntoView({block:'nearest'});return {ok:true}})()"#).await?;
            stage(
                state,
                desktop,
                name,
                "account-replaced",
                &mut captures,
                &mut stages,
            )
            .await?;
        }
    }
    let final_model = snapshot(&state.peek());
    Ok(
        json!({"ok":true,"fixture":name,"scenario":scenario,"viewport":stages.last().unwrap()["geometry"]["viewport"],
        "aboutConfiguration":true,"syntheticSession":true,"noLiveNetwork":true,"readiness":readiness,"stages":stages,"captures":captures,"final":final_model,"replacement":replacement}),
    )
}
