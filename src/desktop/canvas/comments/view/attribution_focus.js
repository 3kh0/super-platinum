(() => {
    const pane = document.getElementById('canvas-comment-profile');
    if (!pane || pane.__canvasFocus) return;
    pane.__canvasFocus = true;
    const active = document.activeElement;
    const original = active?.closest('#canvas-comments') ? active : document.querySelector('#canvas-comments .canvas-attribution-name');
    pane.querySelector('.profile-close')?.focus({preventScroll:true});
    const observer = new MutationObserver(() => {
        if (pane.isConnected) return;
        observer.disconnect();
        if (original?.isConnected && original.closest('#canvas-comments') &&
            !original.closest('[hidden],[inert]') && original.getClientRects().length &&
            !document.getElementById('canvas-comment-profile')) original.focus({preventScroll:true});
    });
    observer.observe(document.getElementById('shell-overlays'), {childList:true,subtree:true});
})()
