await (async()=>{
 const init=await dioxus.recv();
 const wait=async(fn,label)=>{for(let i=0;i<250;i++){if(fn())return;await new Promise(r=>setTimeout(r,20));}throw Error(label);};
 const assert=(v,m)=>{if(!v)throw Error(m);};
 const pre=()=>document.querySelector('.text-preview-content');
 const refresh=()=>document.getElementById('file-preview-refresh');
 const busy=()=>document.querySelector('.file-refresh')?.dataset.refreshBusy==='true';
 const status=()=>document.querySelector('.file-refresh [role=status]')?.textContent;
 const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,top:r.top,bottom:r.bottom,left:r.left,right:r.right};};
 const full=(a,b)=>a.left>=b.left-1&&a.right<=b.right+1&&a.top>=b.top-1&&a.bottom<=b.bottom+1;
 const selectedGeometry=()=>{
  if(pre()!==old)return {required:false,ok:true,rects:[]};
  const s=getSelection();if(s.rangeCount!==1)return {required:true,ok:false,rects:[]};
  const clip={left:0,top:0,right:innerWidth,bottom:innerHeight};
  let depth=0;
  for(let e=pre();e&&depth++<128;e=e.parentElement){
   const style=getComputedStyle(e),r=rect(e);
   if(/auto|scroll|hidden|clip/.test(style.overflowX)){clip.left=Math.max(clip.left,r.left);clip.right=Math.min(clip.right,r.right);}
   if(/auto|scroll|hidden|clip/.test(style.overflowY)){clip.top=Math.max(clip.top,r.top);clip.bottom=Math.min(clip.bottom,r.bottom);}
  }
  const rects=[...s.getRangeAt(0).getClientRects()].filter(r=>r.width>0&&r.height>0).map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height}));
  return {required:true,ok:depth<128&&rects.length>0&&rects.every(r=>full(r,clip)),rects,clip};
 };
 const layout=()=>{
  const dialog=document.querySelector('.file-viewer-dialog'),contents=pre(),button=refresh();
  assert(dialog&&contents&&button,'mounted refresh controls');
  const d=rect(dialog),p=rect(contents),b=rect(button),v={left:0,top:0,right:innerWidth,bottom:innerHeight};
  const hit=document.elementFromPoint(b.x+b.width/2,b.y+b.height/2);
  const hitOwns=hit===button||button.contains(hit);
  const selected=selectedGeometry();
  const visibleHeight=Math.max(0,Math.min(p.bottom,d.bottom,innerHeight)-Math.max(p.top,d.top,0));
  return {ok:full(d,v)&&full(b,d)&&visibleHeight>=60&&button.offsetWidth>0&&hitOwns&&selected.ok,
   viewport:{width:innerWidth,height:innerHeight},dialog:d,contents:p,button:{...b,disabled:button.disabled,text:button.textContent,hit:hitOwns},visibleHeight,selection:selected,
   active:{tag:document.activeElement?.tagName,id:document.activeElement?.id||'',class:document.activeElement?.className||''}};
 };
 let old,code,text,range,focus;
 const selection=()=>{const s=getSelection();return {anchorNode:s.anchorNode,anchorOffset:s.anchorOffset,focusNode:s.focusNode,focusOffset:s.focusOffset,text:s.toString()};};
 const retained=()=>{
  const s=selection();
  return {node:pre()===old,code:pre()?.querySelector('code')===code,text:pre()?.textContent===init.old,
   range:s.anchorNode===range.anchorNode&&s.anchorOffset===range.anchorOffset&&s.focusNode===range.focusNode&&s.focusOffset===range.focusOffset&&s.text===range.text,
   focus:document.activeElement===focus,selected:s.text,anchorOffset:s.anchorOffset,focusOffset:s.focusOffset};
 };
 const report=async(stage,extra={})=>{
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const geometry=layout();assert(geometry.ok,`${stage}: visible contents and Refresh control`);
  const preserve=stage!=='fresh'?retained():null;
  if(preserve)assert(preserve.node&&preserve.code&&preserve.text&&preserve.range&&preserve.focus,`${stage}: exact old node/range/focus retained`);
  dioxus.send({ok:true,stage,layout:geometry,retained:preserve,busy:busy(),status:status(),text:pre().textContent,...extra});
  const ack=await dioxus.recv();assert(ack?.ok,`${stage}: native oracle refused`);
 };
 try{
  await wait(()=>pre()?.textContent===init.old&&refresh()&&!refresh().disabled,'initial refresh preview');
  old=pre();code=old.querySelector('code');text=code.firstChild;
  assert(text?.nodeType===Node.TEXT_NODE,'typed text node');old.focus();
  const r=document.createRange();r.setStart(text,14);r.setEnd(text,37);getSelection().removeAllRanges();getSelection().addRange(r);
  range=selection();focus=document.activeElement;assert(focus===old&&range.text.length===23,'initial exact selection');
  await report('selected');
  refresh().click();await wait(()=>busy()&&refresh().disabled,'first actual Refresh loading');await report('loading');
  await wait(()=>!busy()&&!refresh().disabled&&status()?.includes('Could not refresh'),'failed refresh settles');await report('failure');
  refresh().click();await wait(()=>busy()&&refresh().disabled,'retry actual Refresh loading');await report('retry-loading');
  await wait(()=>!busy()&&!refresh().disabled&&status()?.includes('changed during refresh'),'stale response refused');await report('stale');
  refresh().click();await wait(()=>busy()&&refresh().disabled,'final actual Refresh loading');await report('final-loading');
  await wait(()=>pre()?.textContent===init.fresh&&pre()!==old&&!busy(),'fresh keyed text mount');
  assert(!old.isConnected&&pre().querySelector('code')!==code,'old text mount retired');
  const s=selection();assert(s.anchorNode!==text&&s.focusNode!==text,'old range no longer owns detached text');
  await report('fresh',{replaced:true,oldDisconnected:!old.isConnected,oldRangeDetached:s.anchorNode!==text&&s.focusNode!==text,
   focusAfterReplacement:{tag:document.activeElement?.tagName,id:document.activeElement?.id||''}});
  dioxus.send({ok:true,complete:true,stages:7});
 }catch(error){dioxus.send({ok:false,error:String(error.message),diagnostic:{busy:busy(),status:status(),text:pre()?.textContent,active:document.activeElement?.outerHTML?.slice(0,500)}});}
})();
