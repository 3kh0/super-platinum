(() => {
    const panel=document.getElementById('canvas-comments');
    const scroll=panel?.querySelector('.canvas-comments-scroll'),ts=__TARGET__;
    const row=ts?panel?.querySelector('.canvas-comment[data-comment-ts="'+ts+'"]'):null;
    const body=ts?row?.querySelector(':scope > .message-body'):panel?.querySelector('[role=alert]');
    if(!scroll||!body)return {ok:false};
    const button=row?.querySelector(':scope > .canvas-comment-open');
    const raw=body.getBoundingClientRect(),b=button?.getBoundingClientRect()||raw;
    const a=body.textContent===''?b:raw;
    const viewport=scroll.getBoundingClientRect();
    // The full row includes metadata and can exceed a narrow viewport. Center
    // the body and its actual action together, which the next proof validates.
    const top=Math.min(a.top,b.top),bottom=Math.max(a.bottom,b.bottom);
    scroll.scrollTop+=(top+bottom-viewport.top-viewport.bottom)/2;
    return {ok:true};
})()
