(()=>{
 const q=s=>document.querySelector(s),rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null,mode='legacy',actionNode=null,phaseNode=null;const history=[];
 const clips=e=>{const out=[];for(let p=e?.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s.overflowX+s.overflowY))out.push({id:p.id||null,class:p.className,rect:rect(p),scroll:[p.scrollLeft,p.scrollTop]});}return out;};
 const api=window.__listViewSort={
  rect,visible,hit,
  mode(){return mode;},
  phaseFocus(stage){const panel=q('#slack-list-rename'),current=document.activeElement,live=e=>!!e&&e.isConnected&&!e.disabled&&panel?.contains(e);const retained=live(actionNode)?actionNode:live(phaseNode)?phaseNode:null;const allowed=stage.endsWith('-saving')?['slack-list-rename-leave']:['slack-list-rename-leave','slack-list-rename-check'];const ok=live(current)&&allowed.includes(current.id)&&(!retained||current===retained)&&this.focusRing(current).ok;const receipt={ok,focused:current?.id||null,action:actionNode?.id||null,actionRetained:live(actionNode),previousRetained:live(phaseNode),sameRetained:retained?current===retained:null,allowed,ring:this.focusRing(current)};if(ok)phaseNode=current;return receipt;},
  focusRing(e){
   if(!e)return{ok:false};
   const s=getComputedStyle(e),r=e.getBoundingClientRect(),width=parseFloat(s.outlineWidth),offset=parseFloat(s.outlineOffset),out=Math.max(0,width+offset),bounds=[r.left-out,r.top-out,r.right+out,r.bottom+out],ancestors=[];
   let clipped=bounds.some(v=>!Number.isFinite(v))||bounds[0]<0||bounds[1]<0||bounds[2]>innerWidth||bounds[3]>innerHeight;
   for(let p=e.parentElement;p;p=p.parentElement){const c=getComputedStyle(p),b=p.getBoundingClientRect(),x=/auto|scroll|hidden|clip/.test(c.overflowX),y=/auto|scroll|hidden|clip/.test(c.overflowY);if(x||y){const edges=[b.left,b.top,b.right,b.bottom];ancestors.push({x,y,bounds:edges});if(edges.some(v=>!Number.isFinite(v))||x&&(bounds[0]<b.left||bounds[2]>b.right)||y&&(bounds[1]<b.top||bounds[3]>b.bottom))clipped=true;}}
   return{ok:visible(e)&&Number.isFinite(width)&&width>0&&Number.isFinite(offset)&&s.outlineStyle!=='none'&&!clipped,bounds,width,offset,style:s.outlineStyle,clipped,ancestors,viewport:[innerWidth,innerHeight]};
  },
  setMode(value){if(!['legacy','ordered'].includes(value)||history.length)return{ok:false};mode=value;return{ok:true,mode};},
  history(){return{ok:true,schema:2,mode,scrolls:history.slice()};},
  reveal(e,reason){if(!e?.isConnected)return{ok:false,reason};const before={rect:rect(e),clips:clips(e)},active=document.activeElement,focused=active?.id||null;e.scrollIntoView({block:'nearest',inline:'nearest'});const after={rect:rect(e),clips:clips(e)};const record={index:history.length,reason,target:e.id||null,focusedBefore:focused,focusedAfter:document.activeElement?.id||null,sameActiveElement:document.activeElement===active,before,after};history.push(record);return{ok:visible(e)&&record.sameActiveElement,...record};},
  click(selector){const e=q(selector),reveal=this.reveal(e,'activate');if(!reveal.ok||e.disabled||!hit(e))return{ok:false,selector,reveal};e.focus();actionNode=e;phaseNode=null;e.click();return{ok:true,selector,hitPoints:9,reveal};},
  select(selector,value){const e=q(selector),reveal=this.reveal(e,'select');if(!(e instanceof HTMLSelectElement)||!reveal.ok||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value&&!o.disabled))return{ok:false,selector,reveal};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,selector,value,reveal};},
  query(){const e=find();if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find();const range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberSort(){opener=q('#slack-list-sort');return{ok:!!opener&&opener.isConnected};},
  returnedSort(){const current=q('#slack-list-sort');return{ok:!!opener&&opener===current&&opener===document.activeElement&&opener.isConnected&&!q('#slack-list-rename'),sameNode:opener===current,focused:opener===document.activeElement};},
  revealFirst(){return this.reveal(q('.slack-list-grid tr[data-row-id]'),'table-first-row');},
 };
 return{ok:true};
})()
