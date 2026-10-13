//! Native, disconnected checks; uses DOM events rather than OS input.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
static STARTED: AtomicBool = AtomicBool::new(false);

pub async fn run(mut state: Signal<ShellState>, mut action: Signal<ActionState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_FILE_RENAME_CHECK").is_none()
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("file rename check failed: connected fixture");
        return;
    }
    let mut eval = document::eval(
        r#"
    await (async()=>{
        const capture=await dioxus.recv();
        const report=async value=>{dioxus.send(value);await dioxus.recv();};
        const pause=()=>new Promise(r=>setTimeout(r,40));
        const assert=(c,m)=>{if(!c)throw Error(m);};
        const wait=async(fn,label)=>{for(let i=0;i<120;i++){if(fn())return;await pause();}throw Error(label);};
        const rows=()=>[...document.querySelectorAll('.file-result')];
        let host=()=>rows()[0];
        const button=label=>[...host().querySelectorAll('button')].find(b=>b.textContent===label);
        const more=()=>host().querySelector('.file-more');
        const input=()=>host().querySelector('.file-rename input');
        const fill=value=>{input().value=value;input().dispatchEvent(new Event('input',{bubbles:true}));};
        const open=async()=>{more().click();await wait(()=>button('Rename…'),'rename action');button('Rename…').click();await wait(input,'rename form');await wait(()=>document.activeElement===input(),'name focus');};
        try {
            await wait(()=>rows().length===3,'rows');
            rows()[1].querySelector('.file-more').click();await pause();
            assert(![...rows()[1].querySelectorAll('button')].some(b=>b.textContent==='Rename…'),'other uploader hidden');
            rows()[1].querySelector('.file-more').click();
            await open();assert(input().value==='Release notes · 日本語','prefilled title');assert(button('Save name').disabled,'unchanged disabled');
            fill('   ');await pause();assert(button('Save name').disabled,'empty name disabled');
            fill('Updated notes · 日本語');await pause();assert(!button('Save name').disabled,'valid name enabled');
            input().closest('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
            await wait(()=>host().textContent.includes('Reconnect to Slack'),'offline error');assert(input().value==='Updated notes · 日本語','draft retained');
            await report({stage:'uncertain'});await wait(()=>button('Check status'),'uncertain action');
            assert(input().disabled,'uncertain name frozen');button('Check status').click();
            await wait(()=>button('Check status')&&!button('Check status').disabled,'offline check finishes');
            assert(input().disabled&&!button('Save name'),'offline check cannot resend');
            input().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            await wait(()=>!input(),'escape closes');assert(document.activeElement===more(),'trigger focus restored');
            await open();button('Cancel').click();await wait(()=>button('Rename…'),'cancel returns menu');more().click();
            await wait(()=>!host().querySelector('.file-action-panel'),'menu closed');
            if(capture==='list'){
                await open();fill('Updated release notes · 日本語');await pause();
                const panel=host().querySelector('.file-action-panel').getBoundingClientRect(),list=document.querySelector('.files-results').getBoundingClientRect();
                assert(panel.top>=list.top-1&&panel.bottom<=list.bottom+1,'list rename form visible');
                assert(host().scrollWidth<=host().clientWidth+1,'list has no horizontal overflow');
                await report({ok:true});return;
            }
            rows()[0].querySelector('.files-row').click();await wait(()=>document.querySelector('.viewer pre'),'preview');
            host=()=>document.querySelector('.viewer-footer');const preview=document.querySelector('.viewer pre');
            await open();fill('Viewer name · 日本語');await pause();
            await report({stage:'rename'});
            await wait(()=>document.querySelector('.file-viewer-dialog')?.textContent.includes('Viewer name · 日本語'),'viewer updated');
            assert(document.querySelector('.viewer pre')===preview,'rename preserves preview node');
            assert(input().value==='Viewer name · 日本語','open draft preserved');
            input().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            await wait(()=>!input(),'nested escape');assert(document.querySelector('.viewer pre')===preview,'nested escape keeps viewer');
            await open();assert(input().value==='Viewer name · 日本語','reopened name refreshed');
            fill('Unsent draft · 日本語');await pause();
            await report({stage:'remote_rename'});
            await wait(()=>document.querySelector('.file-viewer-dialog')?.textContent.includes('Remote name · 日本語'),'remote viewer updated');
            assert(document.querySelector('.viewer pre')===preview,'remote rename preserves preview node');
            assert(input().value==='Unsent draft · 日本語','remote rename preserves unsent draft');
            input().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            await wait(()=>!input(),'close stale draft');await open();
            assert(input().value==='Remote name · 日本語','reopened remote name refreshed');
            await pause();const panel=host().querySelector('.file-action-panel').getBoundingClientRect(),view=document.querySelector('.viewer').getBoundingClientRect();
            assert(panel.top>=view.top-1&&panel.bottom<=view.bottom+1,'form visible');
            assert(host().scrollWidth<=host().clientWidth+1,'no horizontal overflow');
            await report({ok:true});
        }catch(error){await report({ok:false,case:error.message});}
    })();
    "#,
    );
    let _ = eval.send(std::env::var("SUPER_PLATINUM_FILE_RENAME_CAPTURE").unwrap_or_default());
    loop {
        let receipt = eval.recv::<serde_json::Value>().await.unwrap_or_default();
        if receipt["stage"] == "uncertain" {
            action.write().uncertain = true;
        } else if receipt["stage"] == "rename" {
            let mut shell = state.write();
            let viewer = shell.viewer.clone().unwrap();
            let target = Target::for_viewer(&shell, &viewer).unwrap();
            let file = File {
                id: Some(target.id.clone()),
                title: Some("Viewer name · 日本語".into()),
                name: Some("Viewer_name.txt".into()),
                ..Default::default()
            };
            rename::reconcile(&mut shell, &target, &file);
            assert_eq!(
                shell.viewer.as_ref().unwrap().download_name,
                "Viewer_name.txt"
            );
        } else if receipt["stage"] == "remote_rename" {
            let mut shell = state.write();
            let viewer = shell.viewer.clone().unwrap();
            let target = Target::for_viewer(&shell, &viewer).unwrap();
            let file = File {
                id: Some(target.id.clone()),
                title: Some("Remote name · 日本語".into()),
                name: Some("Remote_name.txt".into()),
                ..Default::default()
            };
            assert!(crate::file_events::reconcile_metadata(
                &mut shell,
                &target.team,
                &file,
            ));
            assert_eq!(
                shell.viewer.as_ref().unwrap().download_name,
                "Remote_name.txt"
            );
        } else {
            let _ = eval.send("received");
            eprintln!(
                "file rename check {}: {}",
                if receipt["ok"] == true {
                    "passed"
                } else {
                    "failed"
                },
                receipt
            );
            break;
        }
        let _ = eval.send("received");
    }
}
