// Synthetic disconnected native controls. No clipboard, session or network APIs.
async function scheduledAttachmentCheck(operation,args){
  const dialog=()=>document.getElementById("draft-attachments-dialog");
  const frame=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
  const hit=e=>{const r=rect(e),x=(r.left+r.right)/2,y=(r.top+r.bottom)/2,h=document.elementFromPoint(x,y);return r.width>0&&r.height>0&&x>=0&&x<innerWidth&&y>=0&&y<innerHeight&&(h===e||e.contains(h))};
  const exact=(nodes,text)=>{const found=[...nodes].filter(e=>e.textContent.trim()===text);if(found.length!==1)throw Error("Expected one control: "+text);return found[0]};
  async function click(e){if(!e||e.disabled)throw Error("Control unavailable");e.scrollIntoView({block:"nearest",inline:"nearest"});await frame();if(!hit(e))throw Error("Control clipped or covered");e.focus({preventScroll:true});e.click();await frame();}
  if(operation==="open")await click(document.getElementById("draft-attachments-"+args.id));
  else if(operation==="resume")await click(exact(document.querySelectorAll(".scheduled-body button"),"Review attachment changes"));
  else if(operation==="tab"){const tabs=[...document.querySelectorAll("[role=tab]")].filter(e=>e.textContent.trim().startsWith(args.label));if(tabs.length!==1)throw Error("Unknown tab");await click(tabs[0]);}
  else if(operation==="remove"||operation==="undo"){
    const row=dialog()?.querySelectorAll("article[role=listitem]")[args.index];
    if(!row)throw Error("Attachment row missing");
    await click(exact(row.querySelectorAll("button"),operation==="remove"?"Remove from draft":"Undo removal"));
  }else if(operation==="action")await click(exact(dialog()?.querySelectorAll("footer button")||[],args.label));
  else if(operation==="legacy-open"){
    const row=document.getElementById("draft-attachments-"+args.id)?.closest("article");
    await click(exact(row?.querySelectorAll("button")||[],"Edit or reschedule"));
    return {ok:document.getElementById("schedule-title")?.textContent==="Edit message"};
  }else if(operation==="legacy-close"){
    const modal=document.getElementById("schedule-title")?.closest("section");
    await click(exact(modal?.querySelectorAll("footer button")||[],"Cancel"));
    return {ok:!document.getElementById("schedule-title")};
  }
  const d=dialog();
  if(operation==="absent")return {ok:!d};
  if(operation==="blocked"){
    const row=document.getElementById("draft-attachments-"+args.id)?.closest("article");
    const edit=row&&[...row.querySelectorAll("button")].find(e=>e.textContent.trim()==="Edit or reschedule");
    return {ok:!!edit&&edit.disabled===true,editDisabled:edit?.disabled};
  }
  if(!d)return {ok:operation==="action",closed:true,viewport:[innerWidth,innerHeight]};
  const rows=[...d.querySelectorAll("article[role=listitem]")];
  const footer=d.querySelector("footer");
  if(operation==="layout"){
    footer.scrollIntoView({block:"nearest"});await frame();
    const r=rect(d),controls=[...footer.querySelectorAll("button")];
    if(r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||!controls.every(hit))throw Error("Dialog/footer not bounded and hit-testable");
  }
  return {ok:true,rows:rows.map((r,index)=>({index,title:r.querySelector("strong")?.textContent,text:r.textContent,removed:r.textContent.includes("Will be removed from this draft")})),
    text:d.textContent,body:d.querySelector('[aria-label="Message, read only"]')?.textContent,
    buttons:[...footer.querySelectorAll("button")].map(e=>({text:e.textContent.trim(),disabled:e.disabled})),
    dialog:rect(d),viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};
}
