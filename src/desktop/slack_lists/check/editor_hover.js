(() => {
    const action = __ACTION__, domain = __DOMAIN__;
    const slot = window.__listEditorHover ??= {};
    const column = domain === "user" ? "C_EDIT_TASK_USER" : "C_USER";
    const source = () => document.querySelector(`tr[data-row-id="R_LIST_000"] [data-column-id="${column}"] button.mention-chip`);
    const rect = e => {
        const r = e.getBoundingClientRect();
        return {x:r.x,y:r.y,left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};
    };
    const hit = e => {
        const r = rect(e), top = document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
        return !!top && (top === e || e.contains(top));
    };
    const enter = e => {
        const r = rect(e);
        e.dispatchEvent(new MouseEvent("mouseenter",{bubbles:false,clientX:r.x+r.width/2,clientY:r.y+r.height/2}));
    };
    const snapshot = () => {
        const root = document.getElementById("slack-list-cell-editor");
        const input = document.getElementById(`slack-list-${domain}-query`);
        if (!root || !input) throw Error("Original entity editor missing");
        const control = name => {
            const e = root.querySelector(`#slack-list-editor-${name}`);
            if (!e) throw Error("Original editor control missing");
            return {id:e.id,text:e.textContent.trim(),disabled:e.disabled,rect:rect(e),centerHit:hit(e)};
        };
        return {
            generation:root.dataset.editorGeneration,
            dialogGeneration:root.closest(".slack-list-dialog").dataset.dialogGeneration,
            query:input.value,selectionStart:input.selectionStart,selectionEnd:input.selectionEnd,
            selected:[...root.querySelectorAll(`[data-selected-${domain}]`)].map(e=>e.getAttribute(`data-selected-${domain}`)),
            sameRoot:root===slot.root,sameInput:input===slot.input,
            viewport:{width:innerWidth,height:innerHeight},root:rect(root),
            controls:{cancel:control("cancel"),save:control("save")}
        };
    };
    if (action === "hover") {
        const e = source();
        if (!e || document.getElementById("slack-list-cell-editor")) return {ok:false,reason:"No ordinary List hover source",column,editorPresent:!!document.getElementById("slack-list-cell-editor")};
        e.scrollIntoView({block:"center",inline:"center"});
        if (!hit(e)) return {ok:false,reason:"List hover source is occluded",rect:rect(e)};
        slot.source=e;
        enter(e);
        return {ok:true};
    }
    if (action === "visible") {
        const e=document.querySelector(".profile-hover");
        return {ok:!!e,visible:!!e,label:source()?.textContent.trim().replace(/^@/, ""),cardLabel:e?.getAttribute("aria-label")};
    }
    if (action === "dismiss") {
        const e=source();
        if (!e) return {ok:false};
        e.dispatchEvent(new MouseEvent("mouseleave",{bubbles:false}));
        return {ok:true};
    }
    if (action === "remember") {
        slot.root=document.getElementById("slack-list-cell-editor");
        slot.input=document.getElementById(`slack-list-${domain}-query`);
        return {ok:!!slot.root && !!slot.input && !document.querySelector(".profile-hover")};
    }
    if (action === "blocked") {
        const e=slot.source;
        if (!e?.isConnected || e!==source()) return {ok:false,reason:"Original lower-layer hover source retired"};
        enter(e);
        return {ok:true};
    }
    const evidence=snapshot();
    return {ok:evidence.sameRoot && evidence.sameInput && evidence.controls.cancel.centerHit && evidence.controls.save.centerHit && !document.querySelector(".profile-hover"),
        evidence,hoverAbsent:!document.querySelector(".profile-hover"),sourceConnected:!!slot.source?.isConnected};
})()
