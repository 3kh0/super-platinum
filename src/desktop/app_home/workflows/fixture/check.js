// Closed fixture driver. Poll read only; every action is attempted once.
if (!window.homeWorkflowCheck) {
    const used = new Set();
    const visible = node => !!node?.isConnected && node.getClientRects().length > 0;
    const one = nodes => {
        const found = [...nodes].filter(visible);
        if (found.length !== 1) throw new Error(`Expected one mounted target, got ${found.length}`);
        return found[0];
    };
    const label = node => (node.getAttribute('aria-label') || node.textContent || '').trim();
    const button = (scope, text) => one([...scope.querySelectorAll('button')].filter(n => label(n) === text));
    const rect = node => {
        const r = node.getBoundingClientRect();
        return {x:r.x,y:r.y,width:r.width,height:r.height};
    };
    // Rectangles are viewport coordinates; client metrics exclude borders and
    // scrollbars. Refuse transformed/zoomed ancestry instead of mixing spaces.
    const geometry = node => {
        const r = node.getBoundingClientRect(), clip = [0,0,innerWidth,innerHeight], ancestors = [];
        let reason = visible(node) ? null : 'not-mounted';
        for (let p = node; p; p = p.parentElement) {
            const style = getComputedStyle(p), b = p.getBoundingClientRect();
            if (style.display === 'none' || ['hidden','collapse'].includes(style.visibility)
                || Number(style.opacity) === 0 || p.hidden || p.inert) reason ??= 'not-visible';
            if (style.transform !== 'none' || !['1','normal'].includes(String(style.zoom))
                || !['none',''].includes(style.translate || '') || !['none',''].includes(style.rotate || '')
                || !['none',''].includes(style.scale || '') || style.perspective !== 'none') reason ??= 'unsupported-transform';
            if (style.clipPath !== 'none') reason ??= 'unsupported-clip-path';
            if (p === node) continue;
            const x = /^(auto|scroll|hidden|clip)$/.test(style.overflowX);
            const y = /^(auto|scroll|hidden|clip)$/.test(style.overflowY);
            if (!x && !y) continue;
            const client = [p.clientLeft,p.clientTop,p.clientWidth,p.clientHeight];
            const bounds = [b.left+client[0],b.top+client[1],b.left+client[0]+client[2],b.top+client[1]+client[3]];
            ancestors.push({id:p.id || null,x,y,client,bounds});
            if (!bounds.every(Number.isFinite) || client[2] <= 0 || client[3] <= 0) reason ??= 'invalid-client-box';
            if (x) {clip[0]=Math.max(clip[0],bounds[0]);clip[2]=Math.min(clip[2],bounds[2]);}
            if (y) {clip[1]=Math.max(clip[1],bounds[1]);clip[3]=Math.min(clip[3],bounds[3]);}
        }
        const edges = [r.left,r.top,r.right,r.bottom];
        const inside = !reason && r.width > 0 && r.height > 0 && [...edges,...clip].every(Number.isFinite)
            && edges[0] >= clip[0] && edges[1] >= clip[1] && edges[2] <= clip[2] && edges[3] <= clip[3];
        return {inside,reason:reason || (inside ? null : 'clipped'),rect:rect(node),edges,clip,ancestors};
    };
    window.homeWorkflowCheck = (operation, args = {}) => {
        try {
            const home = one(document.querySelectorAll('.app-home'));
            const dialogs = [...document.querySelectorAll('#home-workflow-dialog')].filter(visible);
            if (dialogs.length > 1) throw new Error('Duplicate workflow dialogs');
            const dialog = dialogs[0];
            if (operation === 'read') {
                const picker = dialog?.querySelector('.app-home-workflow-picker');
                const accounts = dialog ? [...dialog.querySelectorAll('.app-home-workflow-step select')].filter(visible) : [];
                if (accounts.length > 1) throw new Error('Unexpected authentication picker count');
                const account = accounts[0];
                const start = dialog && [...dialog.querySelectorAll('footer button')].find(n => label(n) === 'Start workflow');
                const results = picker ? [...picker.querySelectorAll('.app-home-workflow-results button')].filter(visible) : [];
                return {ok:true,home:true,dialog:dialog ? {
                    generation:dialog.dataset.dialogGeneration,rect:rect(dialog),inside:geometry(dialog).inside,geometry:geometry(dialog),
                    title:dialog.querySelector('.app-home-workflow-field input[type=text]')?.value ?? null,
                    startEnabled:!!start && !start.disabled,
                    account:account ? {value:account.value,disabled:account.disabled,geometry:geometry(account)} : null,
                    picker:picker ? {query:picker.querySelector('input')?.value,results:results.map(n=>({label:label(n),disabled:n.disabled,rect:rect(n),inside:geometry(n).inside,geometry:geometry(n)}))} : null
                }:null,viewport:{width:innerWidth,height:innerHeight}};
            }
            if (!['open','title','field','query','choose','start','cancel','account'].includes(operation)) throw new Error('Unknown action');
            if (typeof args.token !== 'string' || !/^workflow-check-[1-9][0-9]*$/.test(args.token) || used.has(args.token)) throw new Error('Missing or repeated action token');
            let target;
            if (operation === 'open') {
                if (dialog) throw new Error('Workflow already mounted');
                target = button(home, 'Run release workflow');
            } else {
                if (!dialog || dialog.dataset.dialogGeneration !== args.generation) throw new Error('Stale workflow generation');
                if (operation === 'title') target = one(dialog.querySelectorAll('.app-home-workflow-field input[type=text]'));
                if (operation === 'account') {
                    if (args.value !== 'xt2') throw new Error('Unknown fixture account');
                    target = one(dialog.querySelectorAll('.app-home-workflow-step select'));
                    const options = [...target.options].filter(n=>n.value===args.value && !n.disabled);
                    if(options.length!==1) throw new Error('Missing retained account option');
                }
                if (operation === 'query') target = one(dialog.querySelectorAll('#home-workflow-entity-query'));
                if (operation === 'field') {
                    if (!['Reviewer','Conversation'].includes(args.label)) throw new Error('Unknown field');
                    const field = one([...dialog.querySelectorAll('.app-home-workflow-field')].filter(n=>n.querySelector('label')?.textContent.trim() === args.label));
                    target = one(field.querySelectorAll('button'));
                }
                if (operation === 'choose') {
                    if (args.label !== 'Ada') throw new Error('Unknown fixture result');
                    target = button(one(dialog.querySelectorAll('.app-home-workflow-results')), args.label);
                }
                if (operation === 'start') target = button(one(dialog.querySelectorAll('footer')), 'Start workflow');
                if (operation === 'cancel') target = button(dialog, 'Cancel workflow');
            }
            const targetGeometry = target && geometry(target);
            if (!target || target.disabled || !targetGeometry.inside) return {ok:false,error:'Target disabled or clipped',geometry:targetGeometry};
            if ((operation === 'title' || operation === 'query') && typeof args.value !== 'string') throw new Error('Missing input');
            used.add(args.token); // retained even if dispatch or the reply fails
            target.focus({preventScroll:true});
            if (operation === 'title' || operation === 'query') {
                Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(target,args.value);
                target.dispatchEvent(new Event('input',{bubbles:true}));
            } else if (operation === 'account') {
                Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(target,args.value);
                target.dispatchEvent(new Event('change',{bubbles:true}));
            } else target.click();
            return {ok:true,action:operation,token:args.token,generation:dialog?.dataset.dialogGeneration ?? null,geometry:targetGeometry};
        } catch (error) { return {ok:false,error:String(error)}; }
    };
}
