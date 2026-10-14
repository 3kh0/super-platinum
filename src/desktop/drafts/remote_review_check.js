// Exact-gated fixture controller. Synthetic DOM events, never trusted keyboard/IME.
function draftRemoteReviewScrollDelta(child, viewport, center = false) {
  if (center) return (child.top + child.bottom - viewport.top - viewport.bottom) / 2;
  if (child.top < viewport.top) return child.top - viewport.top;
  if (child.bottom > viewport.bottom) return child.bottom - viewport.bottom;
  return 0;
}
async function draftRemoteReviewCheck(operation, args) {
  const id=args.target+"-draft-review", panel=document.getElementById(id);
  const frame=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const hit=e=>{const r=rect(e),x=(r.left+r.right)/2,y=(r.top+r.bottom)/2,h=document.elementFromPoint(x,y);return r.width>0&&r.height>0&&x>=0&&x<innerWidth&&y>=0&&y<innerHeight&&(h===e||e.contains(h));};
  function shellGeometry() {
    const footer=document.getElementById(id+"-open")?.closest("footer");
    const pane=footer?.closest(".thread-pane, .conversation");
    const header=pane?.querySelector(":scope > header");
    if(!footer||!pane||!header)throw Error("Missing composer or conversation header");
    const f=rect(footer),h=rect(header),p=rect(pane);
    const top=Math.max(0,p.top),bottom=Math.min(innerHeight,p.bottom);
    if(h.top<top-1||h.bottom>bottom+1||f.top<h.bottom-1||f.bottom>bottom+1)
      throw Error("Conversation header or composer is outside its pane");
    return {header:h,footer:f,pane:p};
  }
  async function reveal(e,center=false) {
    // scrollIntoView walks overflow:hidden ancestors too, concealing a header
    // if a footer overflows. Only move the actual inventory scroll container.
    const inventory=e.closest(".draft-remote-review-versions");
    if(inventory)inventory.scrollTop+=draftRemoteReviewScrollDelta(rect(e),rect(inventory),center);
    await frame();
    shellGeometry();
  }
  async function completionGeometry() {
    // Geometry-only fixture probe: use the real completion container/classes,
    // without querying people, changing input, or advancing draft serials.
    const popup=document.getElementById(args.target+"-composer-completion");
    if(!popup||!popup.hidden)throw Error("Expected an idle completion container");
    const footer=popup.closest("footer"),field=document.getElementById(args.target+"-composer");
    const oldScroll=footer.scrollTop,oldPopupScroll=popup.scrollTop,oldPanelScroll=panel.scrollTop;
    const hidden=popup.getAttribute("hidden");
    const snapshot=()=>JSON.stringify({html:popup.outerHTML,footerScroll:footer.scrollTop,popupScroll:popup.scrollTop,panelScroll:panel.scrollTop,
      input:field.value,start:field.selectionStart,end:field.selectionEnd,direction:field.selectionDirection});
    const before=snapshot();
    const saved=[...popup.childNodes],samples=[];
    try {
      const options=Array.from({length:8},(_,index)=>{
        const row=document.createElement("div");row.className="composer-completion-option";
        const label=document.createElement("span");label.textContent=`Fixture person ${index+1}`;
        const detail=document.createElement("span");detail.className="composer-completion-detail";detail.textContent="Synthetic completion geometry";
        row.append(label,detail);return row;
      });
      popup.replaceChildren(...options);popup.hidden=false;await frame();
      for(const row of [options[0],options.at(-1)]) {
        popup.scrollTop+=draftRemoteReviewScrollDelta(rect(row),rect(popup));await frame();
        const p=rect(popup),f=rect(footer),r=rect(row);
        if(p.top<Math.max(0,f.top)-1||p.bottom>Math.min(innerHeight,f.bottom)+1||!hit(row))
          throw Error("Composer suggestions are clipped while draft review is open");
        samples.push({popup:p,row:r,shell:shellGeometry()});
      }
      return {syntheticCompletionGeometry:true,prePostStateEqual:true,samples};
    } finally {
      popup.replaceChildren(...saved);
      if(hidden===null)popup.removeAttribute("hidden");else popup.setAttribute("hidden",hidden);
      await frame();footer.scrollTop=oldScroll;popup.scrollTop=oldPopupScroll;panel.scrollTop=oldPanelScroll;
      if(snapshot()!==before)throw Error("Completion geometry probe did not restore its exact DOM and input state");
    }
  }
  async function click(e) {
    if(!e||e.disabled)throw Error("Missing or disabled native control");
    await reveal(e);
    if(!hit(e))throw Error("Native control is covered or clipped");
    e.focus({preventScroll:true});e.click();await frame();
  }
  if(operation==="open"||operation==="apply"||operation==="cancel"||operation==="finish") {
    await click(document.getElementById(id+"-"+operation));
    return {ok:true};
  }
  if(operation==="feedback") {
    const opener=document.getElementById(id+"-open");
    const section=opener?.closest(".draft-remote-review"),toast=document.getElementById("status-toast");
    const success="The selected draft version is ready. Nothing was sent.";
    const acknowledged=!panel&&!section&&toast?.textContent.includes(success);
    return {ok:(!panel&&!!opener&&!opener.disabled)||!!acknowledged,text:acknowledged?success:section?.textContent||"",acknowledged:!!acknowledged};
  }
  if(!panel)return {ok:false,reason:"Review not open"};
  const rows=[...panel.querySelectorAll("[data-remote-review-choice]")];
  if(operation==="row") {
    const row=rows[args.index];if(!row)throw Error("Missing inventory row");
    await reveal(row,true);
    const r=rect(row),p=rect(panel),shell=shellGeometry();
    if(r.top<Math.max(0,p.top,shell.footer.top)-1||r.bottom>Math.min(innerHeight,p.bottom,shell.footer.bottom)+1)throw Error("Inventory row is not completely readable");
    return {ok:true,index:args.index,text:row.textContent,rect:r,panel:p,shell};
  }
  if(operation==="choose") {
    const candidates=rows.filter(row=>row.querySelector(`[data-remote-review-kind="${args.kind}"]`)&&(!args.text||row.textContent.includes(args.text)));
    if(candidates.length!==1)throw Error("Choice is not unique");
    await click(candidates[0].querySelector("input[type=radio]"));
  }
  const apply=document.getElementById(id+"-apply");
  const completion=operation==="read"?await completionGeometry():undefined;
  return {ok:true,rows:rows.map((row,index)=>({index,text:row.textContent,kind:row.querySelector("input")?.dataset.remoteReviewKind||"comparison",checked:!!row.querySelector("input")?.checked,disabled:!!row.querySelector("input")?.disabled})),
    selected:rows.filter(r=>r.querySelector("input")?.checked).length,applyDisabled:apply?.disabled,
    panel:rect(panel),shell:shellGeometry(),completion,viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};
}
