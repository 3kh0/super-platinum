(() => {
    if (window.superPlatinumCanvasCommentFocus) return;
    let current=null;
    function release(lease) {
        if (current?.lease!==lease) return;
        current.abort.abort(); current=null;
    }
    function sync(lease,generation,opened) {
        const dialog=document.querySelector('.canvas-dialog');
        if (!dialog || dialog.getAttribute('data-dialog-generation')!==generation) return;
        if (!current || current.dialog!==dialog || current.lease!==lease || current.generation!==generation) {
            current?.abort.abort();
            const owner={lease,generation,dialog,abort:new AbortController(),armed:null,serial:0,placing:false};
            current=owner;
            const observe=event=>{
                if(current!==owner||owner.placing)return;
                owner.serial++;
                const source=event.target?.closest?.('[data-canvas-comment-action]');
                owner.armed=event.type!=='focusin'&&source&&dialog.contains(source)?{source,serial:owner.serial}:null;
            };
            for(const type of ['pointerdown','keydown','click','change','focusin'])
                document.addEventListener(type,observe,{capture:true,signal:owner.abort.signal});
        }
        const owner=current,armed=owner.armed;
        if(!armed)return;
        requestAnimationFrame(()=>{
            if(current!==owner||!dialog.isConnected||owner.armed!==armed||owner.serial!==armed.serial
                ||dialog.getAttribute('data-dialog-generation')!==generation)return;
            const target=dialog.querySelector(opened?'#canvas-comments-back':'#canvas-comments-open');
            const active=document.activeElement;
            if(!target||target.disabled||!target.isConnected
                ||!(active===document.body||active===armed.source||!active?.isConnected))return;
            owner.armed=null;owner.placing=true;
            target.focus({preventScroll:true});
            owner.placing=false;
        });
    }
    window.superPlatinumCanvasCommentFocus={sync,release};
})()
