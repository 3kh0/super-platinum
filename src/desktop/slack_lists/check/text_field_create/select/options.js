(()=>{
 const h=window.__listTextCreate,t=h?.selectTools();if(!t||window.__listSelectOptions)return{ok:false};
 const {q,visible,hit,reveal,rect}=t,root=()=>q('#slack-list-rename'),pending=()=>q('#slack-list-select-option-label'),rows=()=>[...root()?.querySelectorAll('[data-option-id]')||[]];
 const colors=['indigo','blue','cyan','pink','green','gray','purple','orange','brown'];
 const box=b=>({left:b.left,top:b.top,right:b.right,bottom:b.bottom,width:b.width,height:b.height});
 const bounded=(s,n)=>String(s??'').slice(0,n);
 const identify=e=>({id:bounded(e?.id,96),className:bounded(e?.className,160)});
 function ring(e,trace){
  const shown=visible(e);if(trace)trace.visible=shown;
  if(!shown)return false;
  const b=e.getBoundingClientRect(),s=getComputedStyle(e),x=s.outlineStyle==='none'?0:Math.max(0,(parseFloat(s.outlineWidth)||0)+(parseFloat(s.outlineOffset)||0));
  let l=0,u=0,r=innerWidth,d=innerHeight;
  if(trace){trace.rect=box(b);trace.outline={style:bounded(s.outlineStyle,32),width:bounded(s.outlineWidth,48),offset:bounded(s.outlineOffset,48),expand:x};trace.ancestors=[];trace.ancestorCount=0;trace.limiters={left:null,top:null,right:null,bottom:null};}
  for(let p=e.parentElement;p;p=p.parentElement){const c=p.getBoundingClientRect(),s=getComputedStyle(p);
   if(trace){const index=trace.ancestorCount++;const a={index,...identify(p),rect:box(c),overflowX:bounded(s.overflowX,32),overflowY:bounded(s.overflowY,32)};if(trace.ancestors.length<24)trace.ancestors.push(a);if(/auto|scroll|hidden|clip/.test(s.overflowX)){if(c.left>l)trace.limiters.left={index,edge:c.left};if(c.right<r)trace.limiters.right={index,edge:c.right};}if(/auto|scroll|hidden|clip/.test(s.overflowY)){if(c.top>u)trace.limiters.top={index,edge:c.top};if(c.bottom<d)trace.limiters.bottom={index,edge:c.bottom};}}
   if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,c.left);r=Math.min(r,c.right);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){u=Math.max(u,c.top);d=Math.min(d,c.bottom);}
  }
  // Fixed absolute allowance for independently reported DOM edges; never scale or accumulate it.
  const allowanceCss=2**-16,margins={left:b.left-x-l,top:b.top-x-u,right:r-(b.right+x),bottom:d-(b.bottom+x)};
  const comparisons={left:b.left-x>=l,top:b.top-x>=u,right:b.right+x<=r,bottom:b.bottom+x<=d};
  const fits=margin=>Number.isFinite(margin)&&margin>=-allowanceCss;
  const adjustedComparisons={left:fits(margins.left),top:fits(margins.top),right:fits(margins.right),bottom:fits(margins.bottom)};
  if(trace){trace.clip={left:l,top:u,right:r,bottom:d};trace.comparisons=comparisons;trace.margins=margins;trace.allowanceCss=allowanceCss;trace.adjustedComparisons=adjustedComparisons;trace.ancestorsTruncated=trace.ancestorCount>24;}
  return adjustedComparisons.left&&adjustedComparisons.top&&adjustedComparisons.right&&adjustedComparisons.bottom;
 }
 // Failure-only bounded primitive snapshot retains raw strict and adjusted comparisons.
 function pairFailure(e,trace){
  try{
   const diagnostic={version:1,branch:'pair',element:identify(e),active:identify(document.activeElement),activeIsButton:document.activeElement===e,viewport:[innerWidth,innerHeight],devicePixelRatio:typeof devicePixelRatio==='number'?devicePixelRatio:null,ring:trace,finite:true};
   let nonfinite=false;const encode=()=>JSON.stringify(diagnostic,(_,v)=>{if(typeof v==='number'&&!Number.isFinite(v)){nonfinite=true;return null;}return v;});
   let serialized=encode();diagnostic.finite=!nonfinite;serialized=encode();
   if(new TextEncoder().encode(serialized).length>16384){diagnostic.ring={...trace,ancestors:[]};diagnostic.truncated=true;serialized=encode();}
   if(new TextEncoder().encode(serialized).length>16384)return{version:1,branch:'pair',truncated:true,error:'Diagnostic exceeds 16384 bytes'};
   return JSON.parse(serialized);
  }catch(error){return{version:1,branch:'pair',diagnosticError:bounded(error,512)};}
 }
 function topFailure(e,trace,predicate){
  try{
   // These observations run only after the unchanged predicate has failed.
   if(predicate.ring===null)ring(e,trace);
   const diagnostic=pairFailure(e,trace);diagnostic.branch='top';diagnostic.activeIsElement=diagnostic.activeIsButton;delete diagnostic.activeIsButton;
   diagnostic.predicate=predicate;diagnostic.ringSample=predicate.ring===null?'after-failure':'predicate';
   const b=e.getBoundingClientRect();diagnostic.rectAfterFailure=box(b);diagnostic.hitsAfterFailure=[];
   for(const x of [.15,.5,.85])for(const y of [.15,.5,.85]){const px=b.left+b.width*x,py=b.top+b.height*y,f=document.elementFromPoint(px,py);diagnostic.hitsAfterFailure.push({x:px,y:py,target:identify(f),inside:f===e||!!e.contains(f)});}
   let nonfinite=diagnostic.finite===false;const encode=()=>JSON.stringify(diagnostic,(_,v)=>{if(typeof v==='number'&&!Number.isFinite(v)){nonfinite=true;return null;}return v;});
   let serialized=encode();diagnostic.finite=!nonfinite;serialized=encode();
   if(new TextEncoder().encode(serialized).length>16384){diagnostic.ring={...trace,ancestors:[]};diagnostic.truncated=true;serialized=encode();}
   if(new TextEncoder().encode(serialized).length>16384)return{version:1,branch:'top',predicate,truncated:true,error:'Diagnostic exceeds 16384 bytes'};
   return JSON.parse(serialized);
  }catch(error){return{version:1,branch:'top',predicate,diagnosticError:bounded(error,512)};}
 }
 function owner(p){const r=root(),kind=q('#slack-list-new-field-kind'),input=pending(),list=q('.slack-list-dialog');return !!r&&r.dataset.renameStage==='Editing'&&r.dataset.renameSerial===String(p.serial)&&kind?.value==='select'&&kind.dataset.fieldChoice===String(p.choice)&&input?.dataset.addEpoch===String(p.epoch)&&list?.dataset.dialogGeneration===String(p.visit)&&list.dataset.viewSelection===String(p.selection)&&[...document.querySelectorAll('[data-keyboard-dialog=true]')].filter(e=>e.isConnected&&e.getClientRects().length).at(-1)===r;}
 function snapshot(){const r=root(),i=pending(),list=q('.slack-list-dialog'),kind=q('#slack-list-new-field-kind'),values=rows().map(e=>({label:e.querySelector('span')?.textContent,value:e.dataset.optionId,color:e.dataset.optionColor,generation:e.dataset.optionGeneration}));return{ok:!!r&&!!i&&kind?.value==='select'&&values.length<=8&&values.every(v=>typeof v.label==='string'&&/^Opt[A-Z0-9]{8}$/.test(v.value)&&colors.includes(v.color)&&/^[1-9][0-9]*$/.test(v.generation))&&new Set(values.map(v=>v.value)).size===values.length&&new Set(values.map(v=>v.generation)).size===values.length,serial:r?.dataset.renameSerial,visit:list?.dataset.dialogGeneration,selection:list?.dataset.viewSelection,choice:kind?.dataset.fieldChoice,epoch:i?.dataset.addEpoch,pending:i?.value,rows:values};}
 function activate(e){const scrolling=reveal(e);if(!e||e.disabled||!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};e.click();return{ok:true,scrolling,rect:rect(e),focusOutlineFits:true};}
 let tabs=null;const cleanup=()=>{if(tabs?.listener)document.removeEventListener('keydown',tabs.listener,true);tabs=null;};
 window.__listSelectOptions={ring,snapshot,
  input(value,p){const i=pending();if(!owner(p)||i.disabled||typeof value!=='string'||value.length>1024)return{ok:false};const scrolling=reveal(i);i.focus();if(!hit(i)||!ring(i))return{ok:false};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,value);i.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,value:i.value,scrolling};},
  add(p,twice=false){if(!owner(p))return{ok:false};const e=q('#slack-list-select-option-add'),v=activate(e);if(v.ok&&twice)e.click();return v;},
  remove(generation,p){if(!owner(p))return{ok:false};const matches=rows().filter(e=>e.dataset.optionGeneration===String(generation));return matches.length===1?activate(matches[0].querySelector('button')):{ok:false};},
  capacity(p){const add=q('#slack-list-select-option-add'),confirm=q('#slack-list-rename-confirm'),before=snapshot();if(!owner(p)||rows().length!==8||!add?.disabled||!confirm?.disabled||!pending()?.value)return{ok:false};add.click();add.click();return{ok:JSON.stringify(before)===JSON.stringify(snapshot()),before};},
  invalid(p){return{ok:owner(p)&&!!pending()?.value&&q('#slack-list-select-option-add')?.disabled===true&&q('#slack-list-rename-confirm')?.disabled===true,snapshot:snapshot()};},
  pair(index,p){if(!owner(p)||![0,2,4,6].includes(index)||rows().length!==8)return{ok:false};const a=rows()[index],b=rows()[index+1],scrolling=reveal(b);a.scrollIntoView({block:'nearest',inline:'nearest'});const button=b.querySelector('button');button.focus();const pair=[a,b].map(e=>({id:e.dataset.optionId,generation:e.dataset.optionGeneration,label:e.querySelector('span')?.textContent,row:rect(e),button:rect(e.querySelector('button')),visible:visible(e)&&visible(e.querySelector('span')),hit:hit(e.querySelector('button'))}));const trace={};const result={ok:pair.every(v=>v.visible&&v.hit)&&document.activeElement===button&&ring(button),index,pair,scrolling,snapshot:snapshot(),viewport:[innerWidth,innerHeight],focusOutlineFits:ring(button,trace)};if(!result.ok)result.diagnostic=pairFailure(button,trace);return result;},
  top(p){const name=q('#slack-list-rename-name');if(!owner(p))return{ok:false};const scrolling=reveal(name);name.focus();const predicate={hit:hit(name),focus:null,ring:null},trace={};if(predicate.hit){predicate.focus=document.activeElement===name;if(predicate.focus)predicate.ring=ring(name,trace);}const result={ok:predicate.hit&&predicate.focus&&predicate.ring,scrolling};if(!result.ok)result.diagnostic=topFailure(name,trace,predicate);return result;},
  locked(disabled){const r=root(),i=pending(),list=rows(),add=q('#slack-list-select-option-add'),set=q('#slack-list-select-options');return{ok:!!r&&!!set&&list.length===8&&i?.disabled===disabled&&set.disabled===disabled&&list.every(e=>e.querySelector('button')?.disabled===disabled)&&!!add?.disabled&&i.value==='',disabled,rows:list.length};},
  beginTabs(p){cleanup();if(!owner(p)||rows().length!==8||pending().value!==''||!q('#slack-list-select-option-add')?.disabled)return{ok:false};const start=rows()[7].querySelector('button'),expected=[pending(),q('#slack-list-rename-leave'),q('#slack-list-rename-close'),q('#slack-list-rename-confirm')],all=[...root().querySelectorAll('button,input,select,textarea,[tabindex]')].filter(e=>!e.disabled&&e.tabIndex>=0&&!e.closest('[hidden],[inert]')&&e.getClientRects().length);const at=all.indexOf(start);if(at<0||!expected.every((e,i)=>e&&all[at+i+1]===e)||expected[1].textContent.trim()!=='Close list'||expected[2].textContent.trim()!=='Cancel'||expected[3].textContent.trim()!=='Create field')return{ok:false};reveal(start);start.focus();if(document.activeElement!==start||!ring(start))return{ok:false};tabs={proof:p,root:root(),start,expected,records:[],armed:null,error:null,listener:null};tabs.listener=e=>{const s=tabs;if(!s)return;if(s.armed===null||!e.isTrusted||e.key!=='Tab'||e.repeat||e.shiftKey||e.ctrlKey||e.altKey||e.metaKey||!owner(s.proof)||root()!==s.root){s.error='Unexpected or untrusted keyboard event';return;}const ordinal=s.armed;s.armed=null;requestAnimationFrame(()=>{if(tabs!==s)return;const target=s.expected[ordinal],ok=owner(s.proof)&&root()===s.root&&document.activeElement===target&&target.isConnected&&ring(target);s.records.push({ordinal,key:'Tab',trusted:true,repeat:false,shift:false,ctrl:false,alt:false,meta:false,id:target.id,ok,rect:rect(target),focusOutlineFits:ring(target)});if(!ok)s.error='Native Tab focus/geometry differs';});};document.addEventListener('keydown',tabs.listener,true);return{ok:true,fromGeneration:rows()[7].dataset.optionGeneration,expected:expected.map(e=>e.id),snapshot:snapshot()};},
  armTab(ordinal){const s=tabs;if(!s||s.error||s.records.length!==ordinal||s.armed!==null||ordinal<0||ordinal>3||!owner(s.proof)||root()!==s.root||document.activeElement!==(ordinal===0?s.start:s.expected[ordinal-1]))return{ok:false};s.armed=ordinal;return{ok:true,ordinal,expected:s.expected[ordinal].id};},
  tabStatus(){const s=tabs;return{ok:!!s&&!s.error,error:s?.error??null,count:s?.records.length??0,armed:s?.armed??null,events:s?.records.slice()??[]};},
  endTabs(){const s=tabs,result={ok:!!s&&!s.error&&s.records.length===4&&s.armed===null,events:s?.records.slice()??[]};cleanup();return result;},
  cleanupTabs(){cleanup();return{ok:true};}
 };return{ok:true};
})()
