(()=>{
 const h=window.__listTextCreate,t=h?.voteTools();if(!t||window.__listVote)return{ok:false};
 const {q,rect,visible,hit,reveal}=t;
 const cases=Object.freeze({default:{emoji:':sparkling_heart:',team:false},custom:{emoji:':fixture_vote:',team:true}});
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
 function owner(){const p=panel(),e=emoji(),kind=q('#slack-list-new-field-kind');return !!mode&&!!p&&kind?.value==='vote'&&!max()&&!!e&&/^[1-9][0-9]*$/.test(kind.dataset.fieldChoice||'')&&e.dataset.fieldChoice===kind.dataset.fieldChoice;}
 function activate(e){if(!owner()||!e||disabled(e))return{ok:false};const scrolling=reveal(e,'center');if(!hit(e))return{ok:false,scrolling};e.focus();if(document.activeElement!==e||!ring(e))return{ok:false,scrolling};e.click();return{ok:true,scrolling};}
 window.__listVote={
  init(which){if(mode||!Object.hasOwn(cases,which))return{ok:false};mode=cases[which];return{ok:true,case:which};},
  open(){if(picker())return{ok:false};opener=emoji();return activate(opener);},
  pickerReady(){const p=picker(),w=q('#slack-list-rating-picker'),query=q('#emoji-query');if(!p||!w||p.dataset.emojiChoiceReady!=='true'||!query||document.activeElement!==query||!hit(query)||!ring(query)||!visible(p)||q('#emoji-picker-title')?.textContent!=='Choose vote emoji')return{ok:false};pickerNode=p;token=w.dataset.ratingPicker;return{ok:/^[1-9][0-9]*$/.test(token||''),token,rect:rect(p),query:rect(query)};},
  search(name){const query=q('#emoji-query');if(picker()!==pickerNode||q('#slack-list-rating-picker')?.dataset.ratingPicker!==token||!['heart','fixture_vote'].includes(name)||!hit(query))return{ok:false};query.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(query,name);query.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true,name};},
  result(name){const p=picker(),query=q('#emoji-query'),rows=[...p?.querySelectorAll('#emoji-options [role=option]')||[]],matches=rows.filter(e=>e.getAttribute('aria-label')===name),e=matches[0];const image=e?.querySelector('img');return{ok:p===pickerNode&&q('#slack-list-rating-picker')?.dataset.ratingPicker===token&&p?.dataset.emojiChoiceReady==='true'&&query?.value===name&&matches.length===1&&!disabled(e)&&hit(e)&&visible(query)&&document.activeElement===query&&ring(query)&&(name!=='fixture_vote'||!!image&&image.complete&&image.naturalWidth>0),token,name,count:rows.length,label:e?.getAttribute('aria-label'),title:e?.getAttribute('title'),query:query?.value,focused:document.activeElement?.id,rect:rect(e),image:image?{src:image.getAttribute('src'),complete:image.complete,width:image.naturalWidth}:null};},
  dismiss(kind){const p=picker();if(p!==pickerNode||q('#slack-list-rating-picker')?.dataset.ratingPicker!==token)return{ok:false};if(kind==='close'){const e=p.querySelector('button[aria-label="Close"]');if(!hit(e))return{ok:false};e.focus();e.click();}else if(kind==='escape'){const e=q('#emoji-query');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));}else{return{ok:false};}return{ok:true,kind,syntheticCallback:true,token};},
  choose(name){const proof=this.result(name);if(!proof.ok)return proof;const e=[...picker().querySelectorAll('[role=option]')].find(e=>e.getAttribute('aria-label')===name);e.focus();e.click();return{ok:true,name,token,proof};},
  returned(){const e=emoji();return{ok:owner()&&!picker()&&e===opener&&e.isConnected&&!disabled(e)&&document.activeElement===e&&ring(e),sameNode:e===opener,focused:document.activeElement?.id,choice:e?.dataset.fieldChoice,rect:rect(e)};},
  inspect(expectedEmoji,editing){const e=emoji(),p=panel(),k=q('#slack-list-new-field-kind');return{ok:owner()&&!picker()&&e.querySelector('.slack-list-rating-shortcode')?.textContent===expectedEmoji&&disabled(e)===!editing&&ring(e)&&(!editing||hit(e)),choice:k?.dataset.fieldChoice,serial:p?.dataset.renameSerial,emoji:e?.querySelector('.slack-list-rating-shortcode')?.textContent,quantityAbsent:!max(),emojiDisabled:disabled(e),emojiRect:rect(e)};},
  emptyCell(e){if(!mode||!e)return false;const v=e.querySelector('button.slack-list-vote-action'),f=e.querySelector('.slack-list-vote-fallback');return !!v&&!v.disabled&&v.dataset.voteCount==='0'&&v.dataset.voteOwned==='false'&&v.getAttribute('aria-pressed')==='false'&&v.getAttribute('aria-label')===(mode===cases.custom?'Add your vote, 0 votes, :fixture_vote:':'Add your vote, 0 votes')&&v.title===(mode===cases.custom?'No votes yet, :fixture_vote:':'No votes yet')&&(mode===cases.custom?!!f&&f.textContent===':fixture_vote:':!f&&v.querySelectorAll('.slack-list-vote-mark .custom-emoji').length===1&&v.querySelector('.slack-list-vote-mark .custom-emoji')?.title===':sparkling_heart:'&&!v.querySelector('.slack-list-vote-mark img, .slack-list-vote-mark svg'));},
  focusName(){const e=q('#slack-list-rename-name');if(!owner()||disabled(e))return{ok:false};const scrolling=reveal(e);if(!hit(e))return{ok:false,scrolling};e.focus();return{ok:document.activeElement===e,scrolling};},
  actions(){return actions.slice();}
 };
 return{ok:true};
})()
