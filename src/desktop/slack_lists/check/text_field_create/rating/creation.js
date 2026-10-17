(()=>{
 const h=window.__listTextCreate,t=h?.ratingTools();if(!t||window.__listRating)return{ok:false};
 const {q,rect,visible,hit,reveal}=t;
 const cases=Object.freeze({default:{max:1,value:1,emoji:':star:',team:false},standard:{max:5,value:4,emoji:':heart:',team:true},custom:{max:3,value:2,emoji:':fixture_quality:',team:true}});
 let mode=null,opener=null,pickerNode=null,token=null;const actions=[];
 const panel=()=>q('#slack-list-rename'),max=()=>q('#slack-list-new-rating-max'),emoji=()=>q('#slack-list-new-rating-emoji');
 const picker=()=>q('#slack-list-rating-picker .emoji-picker');
 const disabled=e=>!!e&&(e.disabled||e.matches(':disabled'));
 function ring(e,trace){
  const shown=visible(e);if(trace)trace.visible=shown;
  if(!shown)return false;const r=e.getBoundingClientRect(),s=getComputedStyle(e);
  const expand=s.outlineStyle==='none'?0:Math.max(0,(parseFloat(s.outlineWidth)||0)+(parseFloat(s.outlineOffset)||0));
  let l=0,top=0,right=innerWidth,bottom=innerHeight;
  if(trace){trace.rect=r;trace.outline={style:s.outlineStyle,width:s.outlineWidth,offset:s.outlineOffset,expand};trace.ancestors=[];trace.ancestorCount=0;trace.limiters={left:null,top:null,right:null,bottom:null};}
  for(let p=e.parentElement;p;p=p.parentElement){const b=p.getBoundingClientRect(),s=getComputedStyle(p);
   if(trace){const index=trace.ancestorCount++;if(trace.ancestors.length<24)trace.ancestors.push({node:p,rect:b,overflowX:s.overflowX,overflowY:s.overflowY});if(/auto|scroll|hidden|clip/.test(s.overflowX)){if(b.left>l)trace.limiters.left=index;if(b.right<right)trace.limiters.right=index;}if(/auto|scroll|hidden|clip/.test(s.overflowY)){if(b.top>top)trace.limiters.top=index;if(b.bottom<bottom)trace.limiters.bottom=index;}}
   if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,b.left);right=Math.min(right,b.right);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){top=Math.max(top,b.top);bottom=Math.min(bottom,b.bottom);}
  }
  // Fixed absolute envelope for independently reported DOM edges; never scale or accumulate it.
  const allowanceCss=2**-16,margins={left:r.left-expand-l,top:r.top-expand-top,right:right-(r.right+expand),bottom:bottom-(r.bottom+expand)};
  const comparisons={left:r.left-expand>=l,top:r.top-expand>=top,right:r.right+expand<=right,bottom:r.bottom+expand<=bottom};
  const fits=margin=>Number.isFinite(margin)&&margin>=-allowanceCss;
  const adjustedComparisons={left:fits(margins.left),top:fits(margins.top),right:fits(margins.right),bottom:fits(margins.bottom)};
  if(trace){trace.clip={left:l,top,right,bottom};trace.comparisons=comparisons;trace.margins=margins;trace.allowanceCss=allowanceCss;trace.adjustedComparisons=adjustedComparisons;}
  return adjustedComparisons.left&&adjustedComparisons.top&&adjustedComparisons.right&&adjustedComparisons.bottom;
 }
 function owner(){const p=panel(),m=max(),e=emoji(),kind=q('#slack-list-new-field-kind');return !!mode&&!!p&&kind?.value==='rating'&&m instanceof HTMLSelectElement&&!!e&&/^[1-9][0-9]*$/.test(kind.dataset.fieldChoice||'')&&m.dataset.fieldChoice===kind.dataset.fieldChoice&&e.dataset.fieldChoice===kind.dataset.fieldChoice;}
 function activate(e){if(!owner()||!e||disabled(e))return{ok:false};const scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};e.click();return{ok:true,scrolling};}
 window.__listRating={
  init(which){if(mode||!Object.hasOwn(cases,which))return{ok:false};mode=cases[which];return{ok:true,case:which};},
  quantity(n){const e=max();if(!owner()||disabled(e)||!Number.isInteger(n)||n<1||n>5||JSON.stringify([...e.options].map(o=>[o.value,o.textContent]))!==JSON.stringify([1,2,3,4,5].map(n=>[String(n),String(n)])))return{ok:false};const scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,String(n));e.dispatchEvent(new Event('change',{bubbles:true}));const a={ok:true,kind:'quantity',value:n,scrolling};actions.push(a);return a;},
  quantityReady(n,choice){return{ok:owner()&&max().value===String(n)&&max().dataset.fieldChoice===String(choice)&&!disabled(max())&&document.activeElement===max()&&hit(max())&&ring(max()),value:max()?.value,choice:max()?.dataset.fieldChoice,rect:rect(max())};},
  open(){if(picker())return{ok:false};opener=emoji();return activate(opener);},
  pickerReady(){const p=picker(),w=q('#slack-list-rating-picker'),query=q('#emoji-query');if(!p||!w||p.dataset.emojiChoiceReady!=='true'||!query||document.activeElement!==query||!hit(query)||!ring(query)||!visible(p)||q('#emoji-picker-title')?.textContent!=='Choose rating emoji')return{ok:false};pickerNode=p;token=w.dataset.ratingPicker;return{ok:/^[1-9][0-9]*$/.test(token||''),token,rect:rect(p),query:rect(query)};},
  search(name){const query=q('#emoji-query');if(picker()!==pickerNode||q('#slack-list-rating-picker')?.dataset.ratingPicker!==token||!['heart','fixture_quality'].includes(name)||!hit(query))return{ok:false};query.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(query,name);query.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,name};},
  result(name){const p=picker(),query=q('#emoji-query'),rows=[...p?.querySelectorAll('#emoji-options [role=option]')||[]],matches=rows.filter(e=>e.getAttribute('aria-label')===name),e=matches[0];const image=e?.querySelector('img');return{ok:p===pickerNode&&q('#slack-list-rating-picker')?.dataset.ratingPicker===token&&p?.dataset.emojiChoiceReady==='true'&&query?.value===name&&matches.length===1&&!disabled(e)&&hit(e)&&visible(query)&&document.activeElement===query&&ring(query)&&(name!=='fixture_quality'||!!image&&image.complete&&image.naturalWidth>0),token,name,count:rows.length,label:e?.getAttribute('aria-label'),title:e?.getAttribute('title'),query:query?.value,focused:document.activeElement?.id,rect:rect(e),image:image?{src:image.getAttribute('src'),complete:image.complete,width:image.naturalWidth}:null};},
  dismiss(kind){const p=picker();if(p!==pickerNode||q('#slack-list-rating-picker')?.dataset.ratingPicker!==token)return{ok:false};if(kind==='close'){const e=p.querySelector('button[aria-label="Close"]');if(!hit(e))return{ok:false};e.focus();e.click();}else if(kind==='escape'){const e=q('#emoji-query');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));}else{return{ok:false};}return{ok:true,kind,syntheticCallback:true,token};},
  choose(name){const proof=this.result(name);if(!proof.ok)return proof;const e=[...picker().querySelectorAll('[role=option]')].find(e=>e.getAttribute('aria-label')===name);e.focus();e.click();return{ok:true,name,token,proof};},
  returned(){const e=emoji();return{ok:owner()&&!picker()&&e===opener&&e.isConnected&&!disabled(e)&&document.activeElement===e&&ring(e),sameNode:e===opener,focused:document.activeElement?.id,choice:e?.dataset.fieldChoice,rect:rect(e)};},
  inspect(expectedMax,expectedEmoji,editing){const m=max(),e=emoji(),p=panel(),k=q('#slack-list-new-field-kind');const inventory=m?[...m.options].map(o=>[o.value,o.textContent]):null;return{ok:owner()&&!picker()&&JSON.stringify(inventory)===JSON.stringify([1,2,3,4,5].map(n=>[String(n),String(n)]))&&m.value===String(expectedMax)&&e.querySelector('.slack-list-rating-shortcode')?.textContent===expectedEmoji&&disabled(m)===!editing&&disabled(e)===!editing&&ring(m)&&ring(e)&&(!editing||hit(m)&&hit(e)),choice:k?.dataset.fieldChoice,serial:p?.dataset.renameSerial,max:m?.value,emoji:e?.querySelector('.slack-list-rating-shortcode')?.textContent,inventory,maxDisabled:disabled(m),emojiDisabled:disabled(e),maxRect:rect(m),emojiRect:rect(e)};},
  emptyCell(e){if(!mode||!e)return false;const r=e.querySelector('.slack-list-rating'),f=e.querySelector('.slack-list-rating-fallback');if(mode===cases.custom)return !r&&!!f&&f.dataset.ratingValue==='0'&&f.dataset.ratingMax===String(mode.max)&&f.textContent===`0 of ${mode.max} :fixture_quality:`;return !f&&!!r&&r.dataset.ratingValue==='0'&&r.dataset.ratingMax===String(mode.max)&&r.querySelectorAll('.slack-list-rating-mark').length===mode.max&&r.querySelectorAll('.slack-list-rating-mark svg').length===mode.max;},
  focusName(){const e=q('#slack-list-rename-name');if(!owner()||disabled(e))return{ok:false};const scrolling=reveal(e);if(!hit(e))return{ok:false,scrolling};e.focus();return{ok:document.activeElement===e,scrolling};},
  actions(){return actions.slice();}
 };
 return{ok:true};
})()
