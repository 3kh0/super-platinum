await (async () => {
    const pause = () => new Promise(r => setTimeout(r, 40));
    const assert = (ok, why) => { if (!ok) throw Error(why); };
    const wait = async fn => { for (let n = 0; n < 750; n++) { if (fn()) return; await pause(); } throw Error('DOM wait expired'); };
    const button = () => document.querySelector('.viewer-footer button.primary');
    let originalWidth = null;
    const geometry = element => {
        assert(element && !element.disabled, 'required enabled control');
        const r = element.getBoundingClientRect();
        const inside = r.width > 0 && r.height > 0 && r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight;
        const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        assert(inside && hit && (hit === element || element.contains(hit)), 'control clipped or covered');
        return {label: element.textContent.trim(), aria: element.getAttribute('aria-label'), disabled: element.disabled,
            x:r.x,y:r.y,width:r.width,height:r.height,inside,hit:true};
    };
    const assertUnsupportedPreview = viewer => {
        assert(viewer, 'viewer missing');
        assert(!viewer.querySelector('.viewer-loading'), 'unsupported file claims loading');
        const fallback = Array.from(viewer.children).find(e => e.tagName === 'P');
        assert(fallback?.textContent === 'A preview is not available for this file type. Download it to open it.', 'unsupported preview copy');
        assert(!viewer.querySelector('img,video,audio,canvas,.text-preview,.media-unavailable'), 'unsupported file mounted preview content or failure');
    };
    const report = async stage => {
        const closed = stage === 'closed';
        const control = closed ? null : geometry(button());
        if (control) {
            if (originalWidth === null) originalWidth = control.width;
            assert(Math.abs(control.width - originalWidth) <= 1, 'Download/Cancel width changed');
            assert(control.width >= 96, 'stable action minimum width');
            const viewer = document.querySelector('.viewer');
            assertUnsupportedPreview(viewer);
            assert(viewer.scrollWidth <= viewer.clientWidth + 1, 'viewer horizontal overflow');
        }
        const busy = ['busy','retry-busy','close-busy'].includes(stage);
        if (!closed) assert(control.label === (busy ? 'Cancel' : 'Download'), 'exact action label');
        if (closed) assert(!document.querySelector('.viewer'), 'viewer remained mounted');
        const toast = stage === 'saved' ? await downloadToastGeometry('Saved original-fixture.bin') : null;
        dioxus.send({stage,control,closed,toast,viewport:[innerWidth,innerHeight]});
        const ack = await dioxus.recv();
        assert(ack === stage, 'exact stage acknowledgement');
    };
    try {
        await wait(() => button() && !button().disabled);
        await report('initial');
        button().click(); await wait(() => button()?.textContent.trim() === 'Cancel');
        await report('busy');
        button().click(); await wait(() => button()?.textContent.trim() === 'Download');
        await report('canceled');
        button().click(); await wait(() => button()?.textContent.trim() === 'Cancel');
        await report('retry-busy');
        await wait(() => button()?.textContent.trim() === 'Download');
        await report('saved');
        button().click(); await wait(() => button()?.textContent.trim() === 'Cancel');
        await report('close-busy');
        const close = document.querySelector('#shell-overlay-dialog header button[aria-label="Close"]');
        geometry(close); close.click();
        await wait(() => !document.querySelector('.viewer'));
        await report('closed');
        dioxus.send({ok:true}); await dioxus.recv();
    } catch (error) {
        dioxus.send({ok:false,error:String(error.message)}); await dioxus.recv();
    }
})();
