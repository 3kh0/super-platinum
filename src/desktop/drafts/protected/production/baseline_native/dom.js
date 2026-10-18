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
  if(operation==="disabled-current") {
    const e=panel?.querySelector('[data-remote-review-kind="current"]');
    const before=panel?.querySelectorAll('input:checked').length;
    if(!e||!e.disabled)return {ok:false,reason:"Current choice must refuse"};
    e.click();await frame();
    return {ok:e.disabled&&panel.querySelectorAll('input:checked').length===before,disabled:e.disabled,checked:e.checked};
  }
  if(operation==="disabled-apply") {
    const e=document.getElementById(id+"-apply");
    if(!e||!e.disabled)return {ok:false,reason:"Stale apply must refuse"};
    e.click();await frame();return {ok:e.disabled&&e.isConnected,disabled:e.disabled};
  }
  if(!panel)return {ok:false,reason:"Review not open"};
  const rows=[...panel.querySelectorAll("[data-remote-review-choice]")];
  if(operation==="row") {
    const row=rows[args.index];if(!row)throw Error("Missing inventory row");
    await reveal(row,true);
    const r=rect(row),p=rect(panel),shell=shellGeometry();
    // Actual RSX: the panel itself is the versions scroll container. Use its
    // client area, excluding border/scrollbar, on both axes.
    const inventory=row.closest(".draft-remote-review-versions");
    if(inventory!==panel)throw Error("Wrong inventory owner");
    const left=p.left+inventory.clientLeft,top=p.top+inventory.clientTop;
    const scrollport={left,top,right:left+inventory.clientWidth,bottom:top+inventory.clientHeight,
      width:inventory.clientWidth,height:inventory.clientHeight};
    if(!Object.values(scrollport).every(Number.isFinite)||scrollport.width<=0||scrollport.height<=0
      ||r.left<Math.max(0,p.left,shell.footer.left,scrollport.left)-1
      ||r.right>Math.min(innerWidth,p.right,shell.footer.right,scrollport.right)+1
      ||r.top<Math.max(0,p.top,shell.footer.top,scrollport.top)-1
      ||r.bottom>Math.min(innerHeight,p.bottom,shell.footer.bottom,scrollport.bottom)+1)
      throw Error("Inventory row is not completely readable: "+JSON.stringify({index:args.index,row:r,panel:p,shell,scrollport}));
    return {ok:true,index:args.index,text:row.textContent,rect:r,panel:p,shell,scrollport};
  }
  if(operation==="choose") {
    const candidates=rows.filter(row=>row.querySelector(`[data-remote-review-kind="${args.kind}"]`)&&(!args.text||row.textContent.includes(args.text)));
    if(candidates.length!==1)throw Error("Choice is not unique");
    await click(candidates[0].querySelector("input[type=radio]"));
  }
  const apply=document.getElementById(id+"-apply");
  return {ok:true,rows:rows.map((row,index)=>({index,text:row.textContent,kind:row.querySelector("input")?.dataset.remoteReviewKind||"comparison",checked:!!row.querySelector("input")?.checked,disabled:!!row.querySelector("input")?.disabled})),
    selected:rows.filter(r=>r.querySelector("input")?.checked).length,applyDisabled:apply?.disabled,
    panel:rect(panel),shell:shellGeometry(),viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};
}
