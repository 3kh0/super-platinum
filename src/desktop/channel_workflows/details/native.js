// Original row and pane references. No workflow execution or input mutation.
function workflowDetailsNodes(e) {
    const panel = document.getElementById('channel-workflows-panel');
    const button = document.getElementById(e.button);
    if (!panel || !button || !panel.contains(button) || button.tagName !== 'BUTTON' || button.disabled ||
        panel.dataset.owner !== e.owner || panel.dataset.serial !== e.browseSerial ||
        panel.dataset.current !== 'true' || !['ready','partial'].includes(panel.dataset.status) ||
        button.closest('[data-trigger-id]')?.dataset.triggerId !== e.trigger ||
        button.closest('[data-trigger-id]')?.dataset.status !== 'available' ||
        !button.isConnected || !button.getClientRects().length) return null;
    return {panel,button};
}
function workflowDetailsCapture(e) {
    const n = workflowDetailsNodes(e);
    const modal = [...document.querySelectorAll('[aria-modal="true"],.modal')]
        .some(node=>node.isConnected && node.getClientRects().length && getComputedStyle(node).display!=='none' && getComputedStyle(node).visibility!=='hidden');
    if (!n || modal) return false;
    const old = window.__workflowDetailsOrigin;
    if (old && BigInt(old.id) >= BigInt(e.id)) return false;
    window.__workflowDetailsOrigin = {...n,id:e.id,expected:JSON.stringify(e),text:n.button.textContent,pane:null};
    return true;
}
function workflowDetailsOrigin(e) {
    const old=window.__workflowDetailsOrigin,n=workflowDetailsNodes(e);
    return !!old && !!n && old.id===e.id && old.expected===JSON.stringify(e) &&
        old.panel===n.panel && old.button===n.button && old.text===n.button.textContent;
}
async function workflowDetailsBind(e) {
    for(let i=0;i<90;i++) {
        if(!workflowDetailsOrigin(e)) return false;
        const old=window.__workflowDetailsOrigin,pane=document.getElementById('channel-workflow-details');
        if(pane && pane.dataset.selection===e.id && pane.dataset.current==='true') {
            if(old.pane && old.pane!==pane) return false;
            old.pane=pane;
            return true;
        }
        await new Promise(resolve=>requestAnimationFrame(resolve));
    }
    return false;
}
function workflowDetailsCurrent(e,action) {
    if(!workflowDetailsOrigin(e)) return false;
    const old=window.__workflowDetailsOrigin,pane=document.getElementById('channel-workflow-details');
    if(!pane || old.pane!==pane || !pane.isConnected || pane.dataset.selection!==e.id || pane.dataset.current!=='true') return false;
    if(action) {
        const button=document.getElementById('channel-workflow-details-'+action);
        if(!button || !pane.contains(button) || button.disabled || !button.getClientRects().length) return false;
    }
    return true;
}
async function workflowDetailsRestore(e) {
    for(let i=0;i<90;i++) {
        if(!workflowDetailsOrigin(e)) return false;
        const old=window.__workflowDetailsOrigin;
        if(!old.pane?.isConnected) {old.button.focus({preventScroll:true}); return document.activeElement===old.button;}
        await new Promise(resolve=>requestAnimationFrame(resolve));
    }
    return false;
}
