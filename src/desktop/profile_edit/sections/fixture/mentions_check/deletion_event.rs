//! Native event lifecycle through the shared source, actual editor and save path.
use super::super::{deletion_event as source, files as file};
use super::*;
use crate::profile_edit::sections::rich;
mod lifecycle;
pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":error,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Deletion event timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("profile-canvas-deletion-event-check: {result}")
        })
        .await;
    });
}
async fn files_settled(count: usize, held: bool) -> Result<(), String> {
    for _ in 0..100 {
        let v = file::status();
        if v["requests"].as_array().map(Vec::len) == Some(count)
            && v["held"] == held
            && v["active"] == if held { 1 } else { 0 }
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("File receipt did not settle: {}", file::status()))
}
async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'})").await.map(|_|())
}
async fn no_editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn token_view(desktop: &DesktopContext, deleted: bool) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich');if(!e)return{{ok:false}};const c=window.superPlatinumRichEditor.resolved(e),expected={{kind:'deleted-canvas',icon:'delete',disabled:true}},tokens=[...e.querySelectorAll('.rich-editor-token')];const icons=tokens.map(t=>{{const i=t.querySelector(':scope > .rich-editor-deleted-symbol'),r=i?.getBoundingClientRect();return{{deleted:t.classList.contains('rich-editor-deleted-canvas'),disabled:t.getAttribute('aria-disabled'),tag:t.tagName,contentEditable:t.contentEditable,href:t.getAttribute('href'),tabindex:t.getAttribute('tabindex'),icon:i?{{tag:i.tagName,hidden:i.getAttribute('aria-hidden'),paths:[...i.querySelectorAll('path')].map(p=>p.getAttribute('d')),width:r.width,height:r.height}}:null}};}});return{{ok:e.contentEditable==='true'&&tokens.length===2&&c.tokens.every(t=>{deleted}?JSON.stringify(t.presentation)===JSON.stringify(expected):t.presentation===undefined)&&icons.every(t=>t.contentEditable==='false'&&t.href===null&&({deleted}?t.deleted&&t.disabled==='true'&&t.icon?.width>0&&t.icon?.height>0:!t.deleted&&!t.icon)),editorEditable:e.contentEditable,tokens:icons,capture:c}};}})()"#)).await
}
async fn tamper(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('profile-section-rich'),token=e?.querySelector('.rich-editor-deleted-canvas'),path=token?.querySelector('.rich-editor-deleted-symbol path');if(!path)return{ok:false};const reject=()=>{try{window.superPlatinumRichEditor.resolved(e);return false;}catch{return true;}};const d=path.getAttribute('d');path.setAttribute('d','M0 0');const iconMutationRefused=reject();path.setAttribute('d',d);token.setAttribute('aria-disabled','false');const disabledMutationRefused=reject();token.setAttribute('aria-disabled','true');token.classList.remove('rich-editor-deleted-canvas');const classRemovalRefused=reject();token.classList.add('rich-editor-deleted-canvas');const restored=!!window.superPlatinumRichEditor.resolved(e);return{ok:iconMutationRefused&&disabledMutationRefused&&classRemovalRefused&&restored,iconMutationRefused,disabledMutationRefused,classRemovalRefused,restored};})()"#).await
}

async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture(state,desktop,name,stage,json!({"source":source::snapshot(&state.peek()),"cache":rich::files::fixture_status(&state.peek(),file::ID),"exercise":exercise}),stages,captures).await
}
fn current_event(mut state: Signal<ShellState>, raw: Value) -> Result<Value, String> {
    let owner = source::owner(&state.peek())?;
    source::event(&mut state.write(), &owner, raw)
}
fn deletion() -> Value {
    json!({"type":"file_deleted","file_id":file::ID,"fixture_event":"exact owned frame"})
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed deletion event owner missing".into());
    }
    if source::case() == "known" {
        source::seed_navigation(&mut state.write())?;
    }
    select_values(state, desktop).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    let case = source::case();
    file::mode(file::Mode::Held);
    click(desktop, "Edit description", false).await?;
    files_settled(1, true).await?;
    shot(
        state,
        desktop,
        name,
        "cold-loading",
        json!({"held":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let release = file::release();
    files_settled(1, false).await?;
    editor_label(desktop, "Working with ", true, source::NORMAL).await?;
    shot(
        state,
        desktop,
        name,
        "known-opened",
        json!({"release":release,"token":token_view(desktop,false).await?}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        lifecycle::retirement(state, desktop, name, &mut stages, &mut captures).await?;
    } else {
        if case == "known" {
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            {
                let mut shell = state.write();
                crate::profile_edit::close_shell(&mut shell);
                crate::profile_edit::close_shell(&mut shell);
            }
            wait_dom(desktop, "({ok:!document.querySelector('.profile-edit')})").await?;
            let event = current_event(state, deletion())?;
            if event["after"]["rows"][0]["current"] != true {
                return Err("Closed-pane event not retained".into());
            }
            shot(
                state,
                desktop,
                name,
                "event-closed",
                json!({"event":event}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let navigation_owner = source::owner(&state.peek())?;
            let away = source::navigate(&mut state.write(), &navigation_owner, true)?;
            wait_dom(desktop,"({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})").await?;
            shot(
                state,
                desktop,
                name,
                "team-away",
                json!({"navigation":away}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let returned = source::navigate(&mut state.write(), &navigation_owner, false)?;
            shot(
                state,
                desktop,
                name,
                "team-returned",
                json!({"navigation":returned}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let reopened = source::reopen(&mut state.write())?;
            select_values(state, desktop).await?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::DELETED).await?;
            shot(
                state,
                desktop,
                name,
                "reopened-deleted",
                json!({"reopened":reopened,"token":token_view(desktop,true).await?}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            let socket = source::retire_socket(state)?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::DELETED).await?;
            shot(
                state,
                desktop,
                name,
                "reconnected-deleted",
                json!({"socket":socket,"token":token_view(desktop,true).await?}),
                &mut stages,
                &mut captures,
            )
            .await?;
        } else if case == "mounted" {
            edit_label(desktop, "Retained draft with ", source::NORMAL).await?;
            let event = current_event(state, deletion())?;
            editor_label(desktop, "Retained draft with ", true, source::NORMAL).await?;
            shot(state,desktop,name,"event-mounted",json!({"event":event,"mountedNormalRetained":true,"token":token_view(desktop,false).await?}),&mut stages,&mut captures).await?;
        } else if case == "held" {
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            file::mode(file::Mode::Held);
            let (send, receive) = tokio::sync::oneshot::channel();
            dioxus::core::spawn_forever(async move {
                let _ = send.send(rich::files::fixture_refresh(state).await.is_ok());
            });
            files_settled(2, true).await?;
            shot(
                state,
                desktop,
                name,
                "refresh-held",
                json!({"held":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let event = current_event(state, deletion())?;
            let release = file::release();
            let accepted = tokio::time::timeout(Duration::from_secs(5), receive)
                .await
                .map_err(|_| "Late read timeout")?
                .map_err(|_| "Late read missing")?;
            files_settled(2, false).await?;
            if accepted {
                return Err("Late normal response replaced event".into());
            }
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::DELETED).await?;
            shot(state,desktop,name,"late-read-refused",json!({"event":event,"release":release,"accepted":accepted,"token":token_view(desktop,true).await?}),&mut stages,&mut captures).await?;
        } else if case == "plural" {
            let proof = lifecycle::plural(state)?;
            editor_label(desktop, "Working with ", true, source::NORMAL).await?;
            shot(
                state,
                desktop,
                name,
                "plural-mounted",
                json!({"proof":proof,"token":token_view(desktop,false).await?}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::DELETED).await?;
            shot(
                state,
                desktop,
                name,
                "plural-deleted",
                json!({"token":token_view(desktop,true).await?}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        let label = if case == "mounted" {
            source::NORMAL
        } else {
            source::DELETED
        };
        let tamper = if case != "mounted" {
            tamper(desktop).await?
        } else {
            Value::Null
        };
        edit_label(desktop, "Saved with ", label).await?;
        use_description(state, desktop, "Saved with ").await?;
        shot(
            state,
            desktop,
            name,
            "used",
            json!({"rawTokensPreserved":true,"tamper":tamper}),
            &mut stages,
            &mut captures,
        )
        .await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Saved with ", true, source::DELETED).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"savedAndReadBack":true,"token":token_view(desktop,true).await?}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    let expected = if case == "held" { 2 } else { 1 };
    files_settled(expected, false).await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"deletionEvent":true,"mentionKind":"canvas","titleBlocks":true,"readiness":ready,"stages":stages,"captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"files":file::status(),"visualReview":"PNG inspection required separately"}),
    )
}
