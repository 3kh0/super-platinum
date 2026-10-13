//! Disconnected native interaction checks; never reads a session or touches the OS clipboard.
use crate::state::ShellState;
use dioxus::prelude::*;

pub fn fixture(shell: &mut ShellState) {
    use super_platinum_core::files::FilesPage;
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.main_view = crate::state::MainView::Files;
    shell.toast = None;
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .deleted_files
        .clear();
    shell.core.files = Default::default();
    shell
        .core
        .files
        .ensure_scope(shell.core.account_epoch, &team);
    let files=serde_json::from_value::<Vec<super_platinum_core::slack::models::File>>(serde_json::json!([
        {"id":"FACTIONS","title":"Release notes · 日本語","name":"release-notes.txt","user":user,"filetype":"text","mimetype":"text/plain","url_private":"https://files.slack.com/fixture/actions","permalink":"https://fixture.slack.com/files/USELF/FACTIONS","size":42},
        {"id":"FOTHER","title":"Design references","name":"references.pdf","user":"UOTHER","filetype":"pdf","permalink":"https://fixture.slack.com/files/UOTHER/FOTHER","size":1024},
        {"id":"FNOLINK","title":"A very long filename that should remain readable without making the file action controls overflow the narrowest supported window.txt","name":"notes.txt","user":"UOTHER","permalink":"javascript:alert(1)"}
    ])).unwrap();
    let id = shell.media.register_deferred(
        super_platinum_core::MediaAssetKind::Attachment,
        files[0].full_url().unwrap(),
        "text/plain",
    );
    shell
        .media
        .insert(id, "text/plain", b"File action fixture".as_slice());
    let gen_id = shell.core.files.begin(Default::default(), 1);
    shell.core.files.apply(
        gen_id,
        FilesPage {
            items: files,
            ..Default::default()
        },
    );
    if matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("file-actions-channel-narrow" | "file-rename-channel-narrow")
    ) {
        shell.main_view = crate::state::MainView::Home;
        shell.channel_tab = crate::state::ChannelTab::Files;
        shell.core.channel_files = std::mem::take(&mut shell.core.files);
        shell.core.channel_files.query.channel = shell.core.active_channel.clone();
    }
}

pub async fn run(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_FILE_ACTIONS_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("file actions check failed: connected fixture");
        return;
    }
    let mut eval = document::eval(
        r#"
    await (async()=>{
        const report=async value=>{dioxus.send(value);await dioxus.recv();};
        const pause=()=>new Promise(r=>setTimeout(r,40));
        const assert=(c,m)=>{if(!c)throw Error(m);};
        const wait=async(fn,label)=>{for(let i=0;i<100;i++){if(fn())return;await pause();}throw Error(label);};
        const rows=()=>[...document.querySelectorAll('.file-result')];
        const more=i=>rows()[i].querySelector('.file-more');
        const button=(i,label)=>[...rows()[i].querySelectorAll('button')].find(b=>b.textContent===label);
        const expanded=i=>rows()[i].querySelector('.file-action-panel');
        const original=Object.getOwnPropertyDescriptor(navigator,'clipboard');
        try {
            await wait(()=>rows().length===3,'file rows');
            assert(!document.querySelector('.file-result button button'),'no nested buttons');
            more(1).click();await wait(()=>expanded(1),'other menu');assert(!button(1,'Delete…'),'no delete for another uploader');more(1).click();
            more(2).click();await wait(()=>expanded(2),'missing link menu');assert(button(2,'Copy link').disabled&&button(2,'Open in Slack').disabled,'unsafe link disabled');more(2).click();
            more(0).click();await wait(()=>expanded(0),'owned menu');assert(button(0,'Delete…'),'owned delete available');
            let copied=null;
            Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async value=>{copied=value;}}});
            button(0,'Copy link').click();await wait(()=>copied!==null,'clipboard called');assert(copied==='https://fixture.slack.com/files/USELF/FACTIONS','copies exact private permalink');
            await wait(()=>document.body.textContent.includes('File link copied.'),'copy receipt');
            Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw Error('denied');}}});
            button(0,'Copy link').click();await wait(()=>document.body.textContent.includes('clipboard could not be updated'),'clipboard error');
            button(0,'Delete…').click();await wait(()=>button(0,'Cancel'),'confirmation');
            assert(expanded(0).textContent.includes('Release notes · 日本語')&&rows().length===3,'confirmation names target and preserves row');
            button(0,'Cancel').click();await wait(()=>button(0,'Delete…'),'cancel');assert(rows().length===3,'cancel preserves file');
            button(0,'Delete…').click();await wait(()=>button(0,'Delete file'),'confirm again');button(0,'Delete file').click();
            await wait(()=>expanded(0).querySelector('[role=alert]'),'offline error');assert(expanded(0).textContent.includes('Reconnect'),'offline message');assert(rows().length===3&&!button(0,'Delete file').disabled,'failed deletion preserves file and controls');
            button(0,'Cancel').focus();button(0,'Cancel').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
            await wait(()=>!expanded(0),'escape closes');assert(document.activeElement===more(0),'escape restores trigger focus');
            more(0).click();await wait(()=>expanded(0),'reopen for scope change');
            await report({stage:'account'});await wait(()=>!expanded(0),'account change clears old action');
            rows()[0].querySelector('.files-row').click();await wait(()=>document.querySelector('.viewer'),'preview opened');
            await report({stage:'delete'});await wait(()=>rows().length===2&&!document.querySelector('.viewer'),'deletion removes row and closes its viewer');
            await report({stage:'restore'});await wait(()=>rows().length===3,'restored capture fixture');
            more(0).click();await wait(()=>expanded(0),'capture menu');button(0,'Delete…').click();await wait(()=>button(0,'Cancel'),'capture confirmation');
            await wait(()=>document.activeElement===button(0,'Cancel'),'confirmation focus');
            await pause();
            const visible=document.querySelector('.files-results').getBoundingClientRect(),confirmation=expanded(0).getBoundingClientRect();
            assert(confirmation.top>=visible.top-1&&confirmation.bottom<=visible.bottom+1,'whole confirmation visible in file list');
            const panel=document.querySelector('.files-panel');assert(panel.scrollWidth<=panel.clientWidth+1,'panel has no horizontal overflow');
            for(const b of expanded(0).querySelectorAll('button')){const r=b.getBoundingClientRect();assert(r.left>=0&&r.right<=innerWidth,'action within window');}
            await report({ok:true});
        } catch(error) {await report({ok:false,case:error.message});}
        finally {if(original)Object.defineProperty(navigator,'clipboard',original);else delete navigator.clipboard;}
    })();
    "#,
    );
    loop {
        let receipt = eval.recv::<serde_json::Value>().await.unwrap_or_default();
        match receipt["stage"].as_str() {
            Some("account") => {
                let mut shell = state.write();
                shell.core.account_epoch += 1;
                fixture(&mut shell);
            }
            Some("delete") => {
                let mut shell = state.write();
                let team = shell.core.active_team.clone().unwrap();
                shell.core.remove_file(&team, "FACTIONS");
                shell.refresh_from_core();
            }
            Some("restore") => fixture(&mut state.write()),
            _ => {
                let _ = eval.send("received");
                eprintln!(
                    "file actions check {}: {}",
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
