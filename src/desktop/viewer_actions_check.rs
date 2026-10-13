//! Viewer action checks use disconnected native fixtures and a stub clipboard.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;

pub fn fixture(shell: &mut ShellState, name: &str) {
    crate::file_actions_check::fixture(shell);
    shell.viewer = None;
    shell.overlay = None;
    if name.ends_with("pdf") {
        crate::pdf_preview::fixture::apply(shell, "pdf-preview");
        let team = shell.core.active_team.as_ref().unwrap();
        let user = shell.core.workspaces[team].self_user_id.clone();
        shell.viewer.as_mut().unwrap().file =
            Some(super_platinum_core::file_actions::FileIdentity {
                id: "FVIEWERPDF".into(),
                permalink: Some("https://fixture.slack.com/files/USELF/FVIEWERPDF".into()),
                uploader: Some(user),
                renameable: true,
            });
    } else if name.ends_with("message") || name.ends_with("narrow") {
        shell.main_view = crate::state::MainView::Home;
        let file = shell.core.files.items[0].clone();
        let node = crate::message_vm::file_node(&shell.media, &file);
        let message = shell.messages.last_mut().unwrap();
        message.body = vec![crate::state::RichNode::Paragraph(vec![node])];
        shell.sync_fixture_messages();
    }
}

pub async fn run(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_VIEWER_ACTIONS_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("viewer actions check failed: connected fixture");
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    let mut eval = document::eval(
        r#"
    await (async()=>{
        const name=await dioxus.recv(),pdf=name.endsWith('pdf');
        const report=async value=>{dioxus.send(value);await dioxus.recv();};
        const pause=()=>new Promise(r=>setTimeout(r,40));
        const assert=(c,m)=>{if(!c)throw Error(m);};
        const wait=async(fn,label)=>{for(let i=0;i<120;i++){if(fn())return;await pause();}throw Error(label);};
        const viewer=()=>document.querySelector('.viewer');
        const footer=()=>viewer()?.querySelector('.viewer-footer');
        const more=()=>footer()?.querySelector('.file-more');
        const panel=()=>footer()?.querySelector('.file-action-panel');
        const button=label=>[...footer().querySelectorAll('button')].find(b=>b.textContent===label);
        const open=async()=>{
            if(!pdf){const selector=name.endsWith('files')?'.files-row':'.file-link';
                const entry=()=>[...document.querySelectorAll(selector)].find(e=>e.textContent.includes('Release notes'));
                await wait(entry,'file entry');entry().click();}
            await wait(()=>more(),'viewer file controls');
            if(pdf)await wait(()=>document.querySelector('.pdf-preview[data-page="1"]'),'PDF page');
            else await wait(()=>viewer().querySelector('pre'),'text preview');
        };
        const original=Object.getOwnPropertyDescriptor(navigator,'clipboard');
        try {
            await open();
            const originalPreview=pdf?viewer().querySelector('canvas'):viewer().querySelector('pre');
            const expected=pdf?'Project brief.pdf':'release-notes.txt';
            assert(viewer().querySelector(`[aria-label="Download ${expected}"]`),'original download filename retained');
            more().click();await wait(panel,'file menu');
            const ids=[...document.querySelectorAll('[id]')].map(e=>e.id);assert(new Set(ids).size===ids.length,'row and viewer have distinct IDs');
            assert(button('Delete…')&&!button('Copy link').disabled&&!button('Open in Slack').disabled,'owner controls available from attachment');
            let copied=null;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async value=>{copied=value;}}});
            button('Copy link').click();await wait(()=>copied!==null,'clipboard receipt');assert(copied===`https://fixture.slack.com/files/USELF/${pdf?'FVIEWERPDF':'FACTIONS'}`,'exact private permalink');
            await wait(()=>document.body.textContent.includes('File link copied.'),'copy success');
            button('Delete…').click();await wait(()=>button('Cancel'),'confirmation');button('Cancel').click();await wait(()=>button('Delete…'),'cancel');
            assert((pdf?viewer().querySelector('canvas'):viewer().querySelector('pre'))===originalPreview,'actions do not remount preview');
            button('Delete…').click();await wait(()=>button('Cancel'),'confirmation again');
            button('Cancel').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            await wait(()=>viewer()&&!panel(),'Escape closes local menu');await pause();assert(viewer()&&document.activeElement===more(),'Escape preserves viewer and restores focus');
            more().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));await wait(()=>!viewer(),'second Escape closes viewer');
            await report({stage:'restore'});await open();more().click();await wait(panel,'reopened menu');
            button('Delete…').click();await wait(()=>button('Delete file'),'confirm delete');button('Delete file').click();
            await wait(()=>panel()?.querySelector('[role=alert]'),'offline failure');assert(panel().textContent.includes('Reconnect')&&viewer(),'offline error preserves preview');
            await report({stage:'account'});await wait(()=>viewer()&&!panel(),'account epoch clears confirmation');
            await report({stage:'delete'});await wait(()=>!viewer(),'confirmed deletion closes preview');
            await report({stage:'restore'});await open();more().click();await wait(panel,'capture menu');button('Delete…').click();await wait(()=>button('Cancel')===document.activeElement,'confirmation focused');await pause();
            const dialog=document.querySelector('.file-viewer-dialog'),d=viewer().getBoundingClientRect(),p=panel().getBoundingClientRect();
            assert(dialog.scrollWidth<=dialog.clientWidth+1,'dialog has no horizontal overflow');
            assert(p.top>=d.top-1&&p.bottom<=Math.min(d.bottom,innerHeight)+1,'confirmation fully inside viewer viewport');
            const close=dialog.querySelector('[aria-label=Close]').getBoundingClientRect();
            assert(close.top>=dialog.getBoundingClientRect().top&&close.bottom<=innerHeight,'Close stays visible above scrolled preview');
            assert(!document.querySelector('.toast'),'clean restored fixture');
            await report({ok:true});
        }catch(error){await report({ok:false,case:error.message});}
        finally{if(original)Object.defineProperty(navigator,'clipboard',original);else delete navigator.clipboard;}
    })();
    "#,
    );
    let _ = eval.send(name.clone());
    loop {
        let receipt = eval.recv::<serde_json::Value>().await.unwrap_or_default();
        match receipt["stage"].as_str() {
            Some("restore") => fixture(&mut state.write(), &name),
            Some("account") => {
                state.write().core.account_epoch += 1;
            }
            Some("delete") => {
                let mut shell = state.write();
                let team = shell.core.active_team.clone().unwrap();
                let id = shell
                    .viewer
                    .as_ref()
                    .and_then(|v| v.file.as_ref())
                    .unwrap()
                    .id
                    .clone();
                shell.core.remove_file(&team, &id);
                shell.refresh_from_core();
                assert_ne!(shell.overlay, Some(Overlay::Viewer));
            }
            _ => {
                let _ = eval.send("received");
                eprintln!(
                    "viewer actions check {}: {}",
                    if receipt["ok"] == true {
                        "passed"
                    } else {
                        "failed"
                    },
                    receipt
                );
                break;
            }
        }
        let _ = eval.send("received");
    }
}
