use crate::state::ShellState;
use dioxus::prelude::*;
pub async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_PDF_CHECK").is_none() {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("super-platinum: PDF check failed: connected fixture");
        return;
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    let config = serde_json::json!({"fixture":fixture,"capture":std::env::var("SUPER_PLATINUM_PDF_CAPTURE").ok()});
    let mut eval = document::eval(
        r#"
    await (async()=>{
            // Keep the eval alive until Rust consumes the final receipt.
            const report=async value=>{dioxus.send(value);await dioxus.recv();};
        const config=await dioxus.recv();
        const errorFixture=config.fixture==='pdf-preview-error';
        const pause=()=>new Promise(r=>setTimeout(r,50));
        const assert=(c,m)=>{if(!c)throw Error(m);};
        const wait=async(f,label='condition')=>{for(let i=0;i<300&&!f();i++)await pause();assert(f(),`wait timed out: ${label}`);};
        try{
            await wait(()=>document.querySelector('.pdf-preview'));
            const root=document.querySelector('.pdf-preview');
            const button=name=>root.querySelector(`[data-pdf="${name}"]`);
            await wait(()=>root.dataset.ready==='true'||root.dataset.error);
            if(errorFixture){assert(root.dataset.error==='open','invalid PDF error');assert(!document.querySelector('[aria-label^="Download"]').disabled,'invalid PDF original downloadable');await report({ok:true});return;}
            if(root.dataset.error){throw Error(button('status').textContent);}
            assert(root.dataset.worker==='thread','PDF parsing uses a worker');
            const input=button('find');
            const query=value=>{input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));};
            const found=async count=>{await wait(()=>root.dataset.findState==='ready'&&root.dataset.findCount===String(count)&&root.dataset.ready==='true',`search count ${count}`);};
            const beginSearch=value=>new Promise((resolve,reject)=>{
                const observer=new MutationObserver(()=>{if(button('find-status').textContent.startsWith('Searching page')){clearTimeout(timer);observer.disconnect();resolve();}});
                const timer=setTimeout(()=>{observer.disconnect();reject(Error('search did not start'));},15000);
                observer.observe(button('find-status'),{childList:true,characterData:true,subtree:true});query(value);
            });
            const close=async()=>{const key=root.id;document.querySelector('#shell-overlay-dialog [aria-label="Close"]').click();await wait(()=>!document.querySelector('.pdf-preview')&&!window.superPlatinumPdf.has(key));};
            if(config.fixture==='pdf-preview-search-long'){
                assert(root.dataset.pages==='120','long document fixture');
                await beginSearch('page');
                const started=performance.now();query('LANDSCAPE');await found(40);
                const searchMs=Math.round(performance.now()-started);
                await wait(()=>root.dataset.page==='2'&&root.querySelector('mark.pdf-find-active'),'replacement query selected');
                assert(root.querySelector('mark.pdf-find-active').textContent==='Landscape','superseded search cannot paint old query');
                button('find-previous').click();await wait(()=>root.dataset.page==='119'&&root.dataset.ready==='true','last landscape page');
                button('find-next').click();await wait(()=>root.dataset.page==='2'&&root.dataset.ready==='true');
                await beginSearch('local');
                await close();await report({ok:true,searchPages:120,searchMs});return;
            }
            assert(root.dataset.pages==='3' &&root.dataset.page==='1','three page PDF loaded');
            assert(root.querySelector('.textLayer').textContent.includes('First page'),'selectable first-page text');
            const range=document.createRange();range.selectNodeContents(root.querySelector('.textLayer'));const selection=getSelection();selection.removeAllRanges();selection.addRange(range);assert(selection.toString().includes('First page'),'PDF text selection');selection.removeAllRanges();
            const canvas=root.querySelector('canvas');
            assert(canvas.width>0&&canvas.height>0,'page rasterized');
            const pixels=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;
            assert(pixels.some((v,i)=>i%4!==3&&v<200),'page contains rendered content');
            assert(button('previous').disabled&&!button('next').disabled,'first page navigation bounds');
            button('next').click();await wait(()=>root.dataset.ready==='true'&&root.dataset.page==='2');
            assert(root.querySelector('.textLayer').textContent.includes('Second page'),'second page text');
            const width=canvas.width,height=canvas.height;
            button('rotate').click();await wait(()=>root.dataset.ready==='true'&&canvas.height!==height);
            assert(canvas.width<canvas.height,'rotated landscape is portrait');
            const zoom=button('zoom');zoom.value='2';zoom.dispatchEvent(new Event('change',{bubbles:true}));
            await wait(()=>root.dataset.ready==='true'&&canvas.width>width);assert(canvas.width*canvas.height<=16020000,'bounded raster');
            button('next').click();await wait(()=>root.dataset.ready==='true'&&root.dataset.page==='3');assert(button('next').disabled,'last page bounds');
            const page=button('page');page.value='1';page.dispatchEvent(new Event('change',{bubbles:true}));await wait(()=>root.dataset.ready==='true'&&root.dataset.page==='1');
            page.value='999';page.dispatchEvent(new Event('change',{bubbles:true}));assert(page.value==='1','invalid page rejected');
            button('next').click();button('next').click();await wait(()=>root.dataset.ready==='true'&&root.dataset.page==='3');
            assert(root.querySelector('.textLayer').textContent.includes('Third page'),'rapid navigation keeps latest page');
            window.dispatchEvent(new KeyboardEvent('keydown',{key:'f',metaKey:true,bubbles:true,cancelable:true}));
            assert(document.activeElement===input,'Cmd+F focuses PDF search');
            assert(document.querySelector('.pdf-preview')===root,'PDF find does not open workspace search');
            query('page');await found(3);assert(root.dataset.findIndex==='3','search starts from current page');
            button('find-next').click();await wait(()=>root.dataset.page==='1'&&root.dataset.ready==='true');
            assert(root.querySelector('mark.pdf-find-active').textContent==='page','active match highlighted');
            input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true,cancelable:true}));
            await wait(()=>root.dataset.page==='3'&&root.dataset.ready==='true');
            input.dispatchEvent(new KeyboardEvent('keydown',{key:'g',ctrlKey:true,bubbles:true,cancelable:true}));
            await wait(()=>root.dataset.page==='1'&&root.dataset.ready==='true');
            query('a+b');await found(6);assert(root.querySelector('mark.pdf-find-active').textContent==='a+b','literal regex characters');
            assert(root.querySelectorAll('mark').length===2,'all same-page occurrences highlighted');
            button('find-next').click();assert(root.dataset.findIndex==='2'&&root.dataset.page==='1','next match on same page');
            assert(root.querySelectorAll('mark')[1].classList.contains('pdf-find-active'),'second occurrence selected');
            input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true,cancelable:true}));
            await pause();assert(root.dataset.findIndex==='2','IME confirmation does not advance matches');
            query('no such phrase');await found(0);assert(button('find-next').disabled&&!root.querySelector('mark'),'no-match state clears highlights');
            query('Platinum Selectable');await found(1);
            assert(root.querySelectorAll('mark.pdf-find-active').length>=2,'phrase spanning PDF lines highlights both fragments');
            const selectedText=root.querySelector('.textLayer').textContent;
            button('rotate').click();await wait(()=>root.dataset.ready==='true'&&root.querySelector('mark.pdf-find-active'));
            assert(root.querySelector('.textLayer').textContent===selectedText,'highlighting preserves text through rotation');
            input.focus();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
            assert(input.value===''&&root.dataset.findState==='idle'&&!root.querySelector('mark'),'Escape clears search without closing PDF');
            query('[ready]');await found(6);button('find-clear').click();assert(!root.querySelector('mark'),'clear button removes marks');
            const download=document.querySelector('[aria-label^="Download"]');assert(download.getBoundingClientRect().bottom<=innerHeight,'download remains visible');
            const rect=root.getBoundingClientRect();assert(rect.left>=0&&rect.right<=innerWidth,'PDF viewport bounds');
            zoom.value='1';zoom.dispatchEvent(new Event('change',{bubbles:true}));
            button('rotate').click();button('rotate').click();await wait(()=>root.dataset.ready==='true');
            query('Platinum Selectable');await found(1);
            const area=root.querySelector('.pdf-scroll').getBoundingClientRect();
            const fragments=Array.from(root.querySelectorAll('mark.pdf-find-active')).map(mark=>mark.getBoundingClientRect());
            const matchTop=Math.min(...fragments.map(rect=>rect.top)),matchBottom=Math.max(...fragments.map(rect=>rect.bottom));
            if(matchBottom-matchTop<=area.height-8)assert(matchTop>=area.top&&matchBottom<=area.bottom,'entire multi-line match scrolled into view');
            if(config.capture){
                if(config.capture==='empty'){query('no such phrase');await found(0);}
                await report({ok:true});return;
            }
            zoom.value='1.5';zoom.dispatchEvent(new Event('change',{bubbles:true}));
            await close();
            window.dispatchEvent(new KeyboardEvent('keydown',{key:'f',metaKey:true,bubbles:true,cancelable:true}));
            await wait(()=>document.querySelector('#shell-overlay-dialog')?.getAttribute('aria-label')==='Search messages');
            await report({ok:true});
        }catch(error){await report({ok:false,case:String(error?.message||error)});}
    })();
    "#,
    );
    let _ = eval.send(config);
    let receipt = eval.recv::<serde_json::Value>().await;
    let _ = eval.send("received");
    match receipt {
        Ok(value) if value["ok"] == true => eprintln!("super-platinum: PDF check passed: {value}"),
        other => eprintln!("super-platinum: PDF check failed: {other:?}"),
    }
}
