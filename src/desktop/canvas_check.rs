//! Passive native DOM checks. No real account, network, OS keys, or pasteboard.
use crate::state::ShellState;
use dioxus::prelude::*;
pub async fn run(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_CANVAS_CHECK").is_none() {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: canvas native check failed: connected fixture");
        return;
    }
    state.write().dismiss_overlay();
    crate::files::open(
        state,
        serde_json::from_value(
            serde_json::json!({"id":"Ffixture","title":"Project handbook","filetype":"quip"}),
        )
        .unwrap(),
    );
    let script = format!(
        "const expectedGeneration = {};\n",
        state.peek().shortcut_generation
    ) + r#"
        return await (async()=>{
            const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
            const assert=(condition,message)=>{if(!condition)throw Error(message);};
            try {
                for(let i=0;i<100;i++){const current=document.querySelector('.canvas-dialog');if(current?.dataset.dialogGeneration===String(expectedGeneration)&&current.querySelector('.canvas-document'))break;await pause();}
                const dialog=document.querySelector('.canvas-dialog');
                assert(dialog?.dataset.dialogGeneration===String(expectedGeneration),'native file open');
                assert(dialog.querySelector('article h1')?.textContent==='Project handbook','heading');
                assert(dialog.querySelector('strong')?.textContent==='clear','bold');
                assert(dialog.querySelectorAll('[role=checkbox][aria-checked=true]').length===1,'completed checklist');
                assert(dialog.querySelectorAll('[role=checkbox][aria-checked=false]').length===2,'incomplete checklist');
                assert(dialog.querySelectorAll('tbody tr').length===2,'table');
                assert(dialog.querySelector('pre').textContent.includes('cargo fmt --check'),'code');
                assert(dialog.textContent.includes('日本語'),'Unicode');
                const rect=dialog.getBoundingClientRect();
                assert(rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight,'dialog bounds');
                const scroll=dialog.querySelector('.canvas-scroll');
                assert(scroll.scrollWidth<=scroll.clientWidth+1,'horizontal overflow');
                const headerTop=dialog.querySelector('header').getBoundingClientRect().top;
                scroll.scrollTop=scroll.scrollHeight;await pause();
                assert(dialog.querySelector('header').getBoundingClientRect().top===headerTop,'stable toolbar');
                const button=label=>[...dialog.querySelectorAll('button')].find(b=>b.textContent===label);
                button('Rename').click();
                for(let i=0;i<50&&!dialog.querySelector('#canvas-name');i++)await pause();
                let name=dialog.querySelector('#canvas-name');
                assert(name?.value==='Project handbook','rename initial title');
                const input=value=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(name,value);name.dispatchEvent(new Event('input',{bubbles:true}));};
                input('   ');await pause();
                assert(button('Save').disabled,'empty rename disabled');
                input('Native rename draft 🦊');await pause();
                button('Save').click();
                for(let i=0;i<50&&!dialog.querySelector('.canvas-rename [role=alert]');i++)await pause();
                assert(name.value==='Native rename draft 🦊'&&dialog.querySelector('.canvas-rename [role=alert]'),'offline rename preserves draft');
                assert(dialog.querySelector('header h2').textContent==='Project handbook','unconfirmed rename does not change title');
                button('Cancel').click();
                for(let i=0;i<50&&dialog.querySelector('#canvas-name');i++)await pause();
                assert(!dialog.querySelector('#canvas-name'),'rename cancellation');
                dialog.querySelector('[aria-label="Close canvas"]').click();
                for(let i=0;i<50&&document.querySelector('.canvas-dialog');i++)await pause();
                assert(!document.querySelector('.canvas-dialog'),'dismiss');
                dioxus.send({ok:true});
            }catch(error){dioxus.send({ok:false,case:error.message});}
        })();
    "#;
    let mut check = document::eval(&script);
    if let Ok(receipt) = check.recv::<serde_json::Value>().await {
        if receipt["ok"] == true && state.peek().overlay.is_none() {
            if check_creation(state).await {
                eprintln!(
                    "super-platinum: canvas native check passed: reader, rename, create validation, retained draft, dismiss"
                );
            }
        } else {
            eprintln!(
                "super-platinum: canvas native check failed: {}",
                receipt["case"]
            );
        }
    } else {
        eprintln!("super-platinum: canvas native check failed: evaluator ended early");
    }
}

async fn check_creation(state: Signal<ShellState>) -> bool {
    crate::canvas_create::open(state);
    let mut check = document::eval(
        r#"
        (async()=>{
            const pause=()=>new Promise(r=>setTimeout(r,40));
            const assert=(c,m)=>{if(!c)throw Error(m)};
            try {
                for(let i=0;i<100&&!document.querySelector('#canvas-create-title');i++)await pause();
                const dialog=document.querySelector('.canvas-create-dialog');
                const title=dialog.querySelector('input'),body=dialog.querySelector('textarea');
                const submit=dialog.querySelector('[type=submit]');
                assert(submit.disabled,'blank title blocked');
                const input=(el,value)=>{Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el),'value').set.call(el,value);el.dispatchEvent(new Event('input',{bubbles:true}));};
                input(title,'Native creation draft 🦊');input(body,'## Heading\n\n- [ ] 日本語');await pause();
                assert(!submit.disabled,'named draft enabled');submit.click();
                for(let i=0;i<50&&!dialog.querySelector('[role=alert]');i++)await pause();
                assert(dialog.querySelector('[role=alert]')?.textContent.includes('Reconnect'),'offline error');
                assert(body.value==='## Heading\n\n- [ ] 日本語','body retained');
                const rect=dialog.getBoundingClientRect();
                assert(rect.left>=0&&rect.top>=0&&rect.right<=innerWidth&&rect.bottom<=innerHeight,'create dialog bounds');
                assert(dialog.scrollWidth<=dialog.clientWidth+1,'creation overflow');
                dialog.querySelector('[aria-label="Close new canvas"]').click();
                for(let i=0;i<50&&document.querySelector('.canvas-create-dialog');i++)await pause();
                assert(!document.querySelector('.canvas-create-dialog'),'creation dismiss');
                dioxus.send({ok:true});
            }catch(e){dioxus.send({ok:false,case:e.message});}
        })();
    "#,
    );
    let first = check.recv::<serde_json::Value>().await.unwrap_or_default();
    if first["ok"] != true {
        eprintln!(
            "super-platinum: canvas native check failed: create {}",
            first["case"]
        );
        return false;
    }
    crate::canvas_create::open(state);
    let mut check = document::eval(
        r#"
        (async()=>{
            const pause=()=>new Promise(r=>setTimeout(r,40));
            try {
                for(let i=0;i<100&&!document.querySelector('#canvas-create-title');i++)await pause();
                const dialog=document.querySelector('.canvas-create-dialog');
                if(dialog.querySelector('input').value!=='Native creation draft 🦊'||dialog.querySelector('textarea').value!=='## Heading\n\n- [ ] 日本語')throw Error('reopened draft lost');
                [...dialog.querySelectorAll('button')].find(b=>b.textContent==='Discard draft').click();
                for(let i=0;i<50&&document.querySelector('.canvas-create-dialog');i++)await pause();
                dioxus.send({ok:!document.querySelector('.canvas-create-dialog')});
            }catch(e){dioxus.send({ok:false,case:e.message});}
        })();
    "#,
    );
    let second = check.recv::<serde_json::Value>().await.unwrap_or_default();
    if second["ok"] != true || state.peek().canvas_create.is_some() {
        eprintln!(
            "super-platinum: canvas native check failed: reopened create {}",
            second["case"]
        );
        return false;
    }
    true
}
