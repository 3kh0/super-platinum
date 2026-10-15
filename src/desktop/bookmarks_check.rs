//! Disconnected native WebView checks; no OS keys, clipboard or Slack writes.
use crate::state::ShellState;
use dioxus::prelude::*;
mod pickers;
pub async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_BOOKMARKS_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: bookmarks native check failed: connected fixture");
        return;
    }
    if let Err(error) = pickers::seed(state) {
        eprintln!("super-platinum: bookmarks native check failed: {error}");
        return;
    }
    let mut check = document::eval(
        r#"
        await (async()=>{
            // Keep the eval alive until Rust consumes the final receipt.
            const report=async value=>{dioxus.send(value);await dioxus.recv();};
            const pause=()=>new Promise(r=>setTimeout(r,40));
            const assert=(c,m)=>{if(!c)throw Error(m);};
            try {
                for(let i=0;i<100&&!document.querySelector('.bookmark-row');i++)await pause();
                const panel=document.querySelector('.bookmarks-panel');
                const find=label=>[...panel.querySelectorAll('button')].find(b=>b.textContent===label);
                assert(panel.querySelectorAll('.bookmark-row').length===4,'loaded rows including folder');
                const rect=panel.getBoundingClientRect();assert(rect.left>=0&&rect.right<=innerWidth,'panel bounds');
                assert(panel.scrollWidth<=panel.clientWidth+1,'panel overflow');
                const editor=async()=>{for(let i=0;i<100&&!panel.querySelector('.bookmark-editor');i++)await pause();assert(panel.querySelector('.bookmark-editor'),'editor opened');};
                const cancel=async()=>{find('Cancel').click();for(let i=0;i<50&&panel.querySelector('.bookmark-editor');i++)await pause();assert(!panel.querySelector('.bookmark-editor'),'editor closed');};
                const select=(id,value)=>{const el=panel.querySelector(id);el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));};
                const picker=async(stage,parent,parentLabel,before,beforeLabel)=>{
                    panel.querySelector('.bookmark-editor').scrollIntoView({block:'nearest'});
                    await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
                    const read=id=>{const el=panel.querySelector(id);const r=el.getBoundingClientRect();return {id:el.value,label:el.selectedOptions[0]?.textContent,selectedCount:el.selectedOptions.length,visible:r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight,disabled:el.disabled};};
                    const receipt={stage,parent:read('#bookmark-destination'),before:read('#bookmark-position')};
                    assert(receipt.parent.id===parent&&receipt.parent.label===parentLabel&&receipt.before.id===before&&receipt.before.label===beforeLabel,'picker ID/visible label '+stage);
                    assert([receipt.parent,receipt.before].every(p=>p.selectedCount===1&&p.visible&&!p.disabled),'picker visibility/selection '+stage);
                    dioxus.send({picker:receipt});assert(await dioxus.recv()===true,'canonical/capture acknowledgement '+stage);
                };
                await editor();
                await picker('prepopulated','Bk2','Launch references','Bk3','Before Release checklist \u00b7 \u65e5\u672c\u8a9e');await cancel();
                const childMove=()=>panel.querySelector('[aria-label="Move bookmark Release checklist \u00b7 \u65e5\u672c\u8a9e"]').click();
                childMove();await editor();await picker('child-initial','Bk2','Launch references','','At the end');await cancel();
                childMove();await editor();await picker('child-reopened','Bk2','Launch references','','At the end');
                select('#bookmark-destination','');await pause();select('#bookmark-position','Bk1');await pause();
                await picker('child-before','','Top level','Bk1','Before Design system');
                select('#bookmark-destination','Bk2');await pause();await picker('child-reset','Bk2','Launch references','','At the end');await cancel();
                find('Add bookmark').click();for(let i=0;i<50&&!panel.querySelector('#bookmark-title');i++)await pause();
                const title=panel.querySelector('#bookmark-title'),link=panel.querySelector('#bookmark-link');
                const input=(el,value)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,value);el.dispatchEvent(new Event('input',{bubbles:true}));};
                assert(find('Save bookmark').disabled,'blank form blocked');
                input(title,'Reference 🦊');input(link,'javascript:alert(1)');await pause();assert(find('Save bookmark').disabled,'unsafe link blocked');
                input(link,'https://example.com/reference');await pause();assert(!find('Save bookmark').disabled,'valid link enabled');
                find('Save bookmark').click();for(let i=0;i<50&&!panel.querySelector('[role=alert]');i++)await pause();
                assert(panel.querySelector('[role=alert]')?.textContent.includes('Reconnect'),'disconnected save error');
                assert(title.value==='Reference 🦊'&&link.value==='https://example.com/reference','failed draft retained');
                find('Cancel').click();for(let i=0;i<50&&panel.querySelector('.bookmark-editor');i++)await pause();
                panel.querySelector('[aria-label="Edit bookmark Design system"]').click();
                for(let i=0;i<50&&!panel.querySelector('#bookmark-title');i++)await pause();
                assert(panel.querySelector('#bookmark-title').value==='Design system','edit current title');
                assert(panel.querySelector('#bookmark-link').value==='https://vercel.com/geist/introduction','edit current URL');
                find('Cancel').click();for(let i=0;i<50&&panel.querySelector('.bookmark-editor');i++)await pause();
                panel.querySelector('[aria-label="Remove bookmark Design system"]').click();
                for(let i=0;i<50&&!panel.querySelector('.bookmark-editor');i++)await pause();
                assert(panel.querySelector('.bookmark-editor').textContent.includes('Remove “Design system”'),'remove confirmation names bookmark');
                assert(panel.querySelectorAll('.bookmark-row').length===4,'remove not applied before confirmation');
                find('Cancel').click();await pause();assert(panel.querySelectorAll('.bookmark-row').length===4,'remove cancel preserves rows');
                const folder=()=>panel.querySelector('.bookmark-open[aria-expanded]');
                folder().click();await pause();assert(panel.querySelectorAll('.bookmark-row').length===3&&folder().getAttribute('aria-expanded')==='false','folder collapse hides child');
                folder().click();await pause();assert(panel.querySelectorAll('.bookmark-row').length===4,'folder expansion restores child');
                find('New folder').click();await editor();
                assert(!panel.querySelector('#bookmark-link')&&find('Save folder').disabled,'folder form excludes URL and requires name');
                input(panel.querySelector('#bookmark-title'),'Release references');await pause();
                assert(!find('Save folder').disabled,'folder name enables save');
                find('Save folder').click();await pause();assert(panel.querySelector('[role=alert]')?.textContent.includes('Reconnect'),'folder offline error');
                assert(panel.querySelector('#bookmark-title').value==='Release references','folder draft retained');await cancel();
                panel.querySelector('[aria-label="Edit bookmark Launch references"]').click();await editor();
                assert(panel.querySelector('#bookmark-title').value==='Launch references'&&!panel.querySelector('#bookmark-link'),'folder rename form');await cancel();
                panel.querySelector('[aria-label="Move bookmark Design system"]').click();await editor();
                select('#bookmark-destination','Bk2');await pause();
                assert([...panel.querySelector('#bookmark-position').options].some(o=>o.value==='Bk3'),'destination shows child position');
                select('#bookmark-position','Bk3');await pause();
                await picker('changed','Bk2','Launch references','Bk3','Before Release checklist \u00b7 \u65e5\u672c\u8a9e');
                find('Move bookmark').click();await pause();assert(panel.querySelector('[role=alert]')?.textContent.includes('Reconnect'),'move offline error');
                assert(panel.querySelector('#bookmark-destination').value==='Bk2'&&panel.querySelector('#bookmark-position').value==='Bk3','move draft retained');
                await picker('offline-retained','Bk2','Launch references','Bk3','Before Release checklist \u00b7 \u65e5\u672c\u8a9e');
                select('#bookmark-destination','');await pause();await picker('reset','','Top level','','At the end');await cancel();
                panel.querySelector('[aria-label="Move bookmark Launch references"]').click();await editor();
                assert(panel.querySelector('#bookmark-destination').options.length===1,'folder moves only at top level');await cancel();
                panel.querySelector('[aria-label="Remove bookmark Launch references"]').click();await editor();
                assert(panel.querySelector('.bookmark-editor').textContent.includes('1 contained bookmark from'),'folder removal discloses singular cascade count');
                assert(panel.querySelectorAll('.bookmark-row').length===4,'folder intact before confirmation');await cancel();
                assert(panel.querySelectorAll('.bookmark-row').length===4,'folder cancellation preserves child');
                assert(panel.scrollWidth<=panel.clientWidth+1,'final panel overflow');
                await report({ok:true});
            }catch(e){await report({ok:false,case:e.message});}
        })();
    "#,
    );
    let mut stages = Vec::new();
    let receipt = loop {
        let receipt = check.recv::<serde_json::Value>().await.unwrap_or_default();
        let Some(dom) = receipt.get("picker") else {
            break receipt;
        };
        match pickers::capture(state, stages.len(), dom.clone()).await {
            Ok(stage) => {
                eprintln!("bookmarks-picker-stage: {stage}");
                stages.push(stage);
                let _ = check.send(true);
            }
            Err(error) => {
                let _ = check.send(false);
                eprintln!("super-platinum: bookmarks native check failed: {error}");
                return;
            }
        }
    };
    let _ = check.send("received");
    if receipt["ok"] == true && stages.len() == pickers::STAGES.len() {
        eprintln!(
            "super-platinum: bookmarks native check passed: list, validation, draft retention, edit, folders, collapse, movement, cascade removal cancellation, layout"
        );
    } else {
        eprintln!(
            "super-platinum: bookmarks native check failed: {}",
            receipt["case"]
        );
    }
}
