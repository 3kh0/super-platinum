//! Disconnected restart checks with an explicitly injected, task-owned database.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::operation_receipts::{FileScope, Journal, Operation, Status};
static STARTED: AtomicBool = AtomicBool::new(false);
const ACCOUNT: &str = "00000000-0000-4000-8000-000000009999";

pub async fn run(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_FILE_RECOVERY_CHECK").is_none()
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("file recovery check failed: connected fixture");
        return;
    }
    let Some(path) = std::env::var_os("SUPER_PLATINUM_FILE_RECOVERY_DB") else {
        eprintln!("file recovery check failed: missing isolated storage");
        return;
    };
    let mode = std::env::var("SUPER_PLATINUM_FILE_RECOVERY_CHECK").unwrap();
    let (scope, title) = {
        let mut shell = state.write();
        shell.core.active_account = Some(ACCOUNT.into());
        let team = shell.core.active_team.clone().unwrap();
        let file = &shell.core.files.items[0];
        (
            FileScope {
                team_id: team.clone(),
                user_id: shell.core.workspaces[&team].self_user_id.clone(),
                file_id: file.id.clone().unwrap(),
            },
            file_title(file),
        )
    };
    let seed_scope = scope.clone();
    let seed = mode == "seed";
    let prepared = tokio::task::spawn_blocking(move || {
        let mut journal = Journal::open(path, ACCOUNT).unwrap();
        if seed {
            assert!(
                journal
                    .unresolved(&seed_scope.team_id, &seed_scope.user_id)
                    .unwrap()
                    .is_empty()
            );
            journal
                .begin(
                    seed_scope.clone(),
                    Operation::Rename {
                        original_title: title,
                        requested_title: "Recovered name · 日本語".into(),
                    },
                    recovery::now(),
                )
                .unwrap();
            let missing = FileScope {
                file_id: "F_RECOVERY_MISSING".into(),
                ..seed_scope.clone()
            };
            journal
                .begin(
                    missing,
                    Operation::Delete {
                        original_title: "Missing recovery file".into(),
                    },
                    recovery::now(),
                )
                .unwrap();
        }
        let rows = journal
            .unresolved(&seed_scope.team_id, &seed_scope.user_id)
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.status == Status::Uncertain));
    })
    .await;
    if prepared.is_err() {
        eprintln!("file recovery check failed: storage setup");
        return;
    }
    let mut eval = document::eval(
        r#"
    await(async()=>{
        const mode=await dioxus.recv();
        const pause=()=>new Promise(r=>setTimeout(r,40));
        const assert=(ok,label)=>{if(!ok)throw Error(label);};
        const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
        const button=(host,label)=>[...host.querySelectorAll('button')].find(b=>b.textContent===label);
        const visible=element=>{
            if(!element)return false;
            const r=element.getBoundingClientRect();
            if(r.top<0||r.bottom>innerHeight||r.left<0||r.right>innerWidth)return false;
            for(let parent=element.parentElement;parent;parent=parent.parentElement){
                if(/auto|scroll|hidden|clip/.test(getComputedStyle(parent).overflowY)){
                    const bounds=parent.getBoundingClientRect();
                    if(r.top<bounds.top||r.bottom>bounds.bottom)return false;
                }
            }
            return r.height>0;
        };
        const report=async(value)=>{dioxus.send(value);await dioxus.recv();};
        try {
            await wait(()=>document.querySelector('.file-recovery-controls'),'account mounted');
            const row=document.querySelector('.files-results .file-result'),more=row.querySelector('.file-more');
            const restored=()=>row.querySelector('.file-rename input')?.value==='Recovered name · 日本語'&&button(row,'Check status');
            more.click();await wait(restored,'restored rename');
            assert(row.querySelector('input').disabled,'recovered rename frozen');
            button(row,'Check status').click();await wait(()=>row.textContent.includes('Reconnect to Slack'),'offline check');
            assert(button(row,'Check status')&&!button(row,'Save name'),'offline does not enable resend');
            row.querySelector('input').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            await wait(()=>!row.querySelector('.file-action-panel'),'close recovered rename');
            more.click();await wait(restored,'reopen restores receipt');more.click();
            document.querySelector('.file-recovery-controls button').click();
            await wait(()=>document.querySelectorAll('.file-recovery-list .file-result').length===2,'recovery inventory');
            const ids=[...document.querySelectorAll('.file-more')].map(b=>b.id);assert(new Set(ids).size===ids.length,'unique action IDs');
            more.click();await wait(restored,'ordinary row remains accessible with inventory expanded');
            await wait(()=>visible(button(row,'Check status')),'ordinary file action visible with recovery expanded');
            more.click();
            const missing=[...document.querySelectorAll('.file-recovery-list .file-result')].find(r=>r.textContent.includes('Missing recovery file'));
            assert(missing,'missing file remains accessible');missing.querySelector('.file-more').click();
            await wait(()=>button(missing,'Check status'),'restored deletion');
            button(missing,'Check status').click();await wait(()=>missing.textContent.includes('Reconnect to Slack'),'offline deletion check');
            assert(button(missing,'Check status')&&!button(missing,'Delete file'),'deletion does not resend');
            await wait(()=>visible(missing.querySelector('.file-action-error'))&&visible(button(missing,'Check status')),'offline status and next action visible');
            assert(missing.textContent.includes('is not confirmed'),'uncertain deletion copy');
            if(mode==='retire'){
                button(missing,'Stop tracking…').focus();button(missing,'Stop tracking…').click();await wait(()=>button(missing,'Stop tracking'),'retirement confirmation');
                await wait(()=>document.activeElement===button(missing,'Keep checking'),'retirement focus');
                assert(missing.textContent.includes('does not undo'),'retirement copy');button(missing,'Keep checking').click();
                await wait(()=>button(missing,'Check status'),'retirement cancel');
                await wait(()=>missing.querySelector('.file-action-panel').contains(document.activeElement),'retirement cancel focus');
                button(missing,'Stop tracking…').click();await wait(()=>button(missing,'Stop tracking'),'retirement reopened');button(missing,'Stop tracking').click();
                await wait(()=>!button(missing,'Stop tracking')&&!button(missing,'Check status'),'retired result');
                button(document.querySelector('.file-recovery-controls'),'Refresh changes').click();
                await wait(()=>document.querySelectorAll('.file-recovery-list .file-result').length===1,'retired receipt removed');
            }
            assert(document.documentElement.scrollWidth<=innerWidth+1,'no page overflow');
            await report({ok:true,mode});
        }catch(error){await report({ok:false,case:error.message});}
    })();
    "#,
    );
    let _ = eval.send(mode);
    let receipt = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    let _ = eval.send("received");
    eprintln!(
        "file recovery check {}: {}",
        if receipt["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        receipt
    );
}
