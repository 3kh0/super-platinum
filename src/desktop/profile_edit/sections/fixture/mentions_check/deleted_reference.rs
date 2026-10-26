//! Native existing-token status acceptance using the actual receipt/editor path.
use super::super::{deleted_reference as source, files as file};
use super::*;
use crate::profile_edit::sections::rich;
mod retirement;

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
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Deleted reference timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("profile-canvas-deleted-check: {result}")
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
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed deleted-reference owner missing".into());
    }
    select_values(state, desktop).await?;
    let case = source::case();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    source::reply(if case == "failure" {
        source::Reply::Malformed
    } else {
        source::Reply::Deleted
    });
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
    if case == "cancel" {
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        for _ in 0..100 {
            if file::status()["cancelled"] == 1 && file::status()["active"] == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if file::status()["cancelled"] != 1 || file::status()["active"] != 0 {
            return Err("Cancelled read lease remained active".into());
        }
        let release = file::release();
        files_settled(1, false).await?;
        if release["cancelledBeforeReply"] != true || release["delivered"] != false {
            return Err("Cancelled file read accepted late carrier".into());
        }
        if !rich::files::fixture_status(&state.peek(), file::ID).is_null() {
            return Err("Cancelled file carrier installed source".into());
        }
        shot(
            state,
            desktop,
            name,
            "cancelled",
            json!({"release":release}),
            &mut stages,
            &mut captures,
        )
        .await?;
        file::mode(file::Mode::Normal);
        click(desktop, "Edit description", false).await?;
        files_settled(2, false).await?;
    } else {
        let release = file::release();
        files_settled(1, false).await?;
        if release["delivered"] != true {
            return Err("Original status carrier not delivered".into());
        }
    }
    if case == "failure" {
        unavailable(desktop).await?;
        if !rich::files::fixture_status(&state.peek(), file::ID).is_null() {
            return Err("Malformed carrier minted status".into());
        }
        shot(
            state,
            desktop,
            name,
            "malformed",
            json!({"statusRefused":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        file::mode(file::Mode::Offline);
        click(desktop, "Retry description", false).await?;
        files_settled(2, false).await?;
        unavailable(desktop).await?;
        if !rich::files::fixture_status(&state.peek(), file::ID).is_null() {
            return Err("Offline minted status".into());
        }
        shot(
            state,
            desktop,
            name,
            "offline",
            json!({"statusRefused":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        source::reply(source::Reply::Deleted);
        file::mode(file::Mode::Normal);
        click(desktop, "Retry description", false).await?;
        files_settled(3, false).await?;
    }
    editor_label(desktop, "Working with ", true, source::DELETED).await?;
    let token = token_view(desktop, true).await?;
    let tamper = tamper(desktop).await?;
    shot(
        state,
        desktop,
        name,
        "opened-deleted",
        json!({"token":token,"tamper":tamper}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        retirement::run(state, desktop, name, &mut stages, &mut captures).await?;
    } else {
        if matches!(case.as_str(), "recovery" | "partial") {
            edit_label(desktop, "Retained draft with ", source::DELETED).await?;
            source::reply(source::Reply::Normal);
            file::mode(file::Mode::Normal);
            rich::files::fixture_refresh(state)
                .await
                .map_err(|e| e.to_string())?;
            files_settled(2, false).await?;
            editor_label(desktop, "Retained draft with ", true, source::DELETED).await?;
            let token = token_view(desktop, true).await?;
            shot(
                state,
                desktop,
                name,
                "refreshed-mounted",
                json!({"mountedDeletedRetained":true,"token":token}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        edit_label(desktop, "Saved with ", source::DELETED).await?;
        use_description(state, desktop, "Saved with ").await?;
        shot(
            state,
            desktop,
            name,
            "used",
            json!({"rawTokensPreserved":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        let deleted = case != "recovery";
        let label = if deleted {
            source::DELETED
        } else {
            source::RECOVERED
        };
        editor_label(desktop, "Saved with ", true, label).await?;
        let token = token_view(desktop, deleted).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"savedAndReadBack":true,"label":label,"token":token}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"deletedCanvas":true,"mentionKind":"canvas","titleBlocks":true,"readiness":ready,"stages":stages,"captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"files":file::status(),"visualReview":"PNG inspection required separately"}),
    )
}
