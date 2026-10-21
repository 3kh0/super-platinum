(() => {
 const dialog=document.querySelector('.slack-list-dialog');
 const board=dialog?.querySelector('.slack-list-board');
 const layout=board?.closest('.slack-list-layout');
 const add=layout?.querySelector('[data-board-choice-kind="add"]');
 const lanes=[...(board?.querySelectorAll(':scope > .slack-list-lane')||[])];
 const expected=[['OPT_ENGINEERING','Engineering'],['OPT_DESIGN','Design'],['OPT_OPERATIONS','Operations'],['','Ungrouped']];
 if(!dialog||!board||!layout||!add||lanes.length!==4)return {ok:false,error:'Original multi-lane Board controls missing'};
 const originalFocus=document.activeElement;
 const offsets=[board,layout,dialog].map(node=>({node,left:node.scrollLeft,top:node.scrollTop}));
 const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,left:r.left,right:r.right,top:r.top,bottom:r.bottom};};
 const inside=(a,b)=>a.left>=b.left-1&&a.right<=b.right+1&&a.top>=b.top-1&&a.bottom<=b.bottom+1;
 const intersects=(a,b)=>Math.min(a.right,b.right)-Math.max(a.left,b.left)>1&&Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)>1;
 const finite=r=>Object.values(r).every(Number.isFinite)&&r.width>0&&r.height>0;
 const text=e=>{
  const walker=document.createTreeWalker(e,NodeFilter.SHOW_TEXT),ranges=[];
  while(walker.nextNode()){
   if(!walker.currentNode.textContent.trim())continue;
   const range=document.createRange();range.selectNodeContents(walker.currentNode);
   ranges.push(...[...range.getClientRects()].filter(r=>r.width>0&&r.height>0).map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height})));
  }
  const lines=[];for(const r of ranges)if(!lines.some(y=>Math.abs(y-r.top)<=1))lines.push(r.top);
  return {text:e.textContent.trim(),ranges,lines:lines.length};
 };
 const paint=e=>{
  const r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
  let clipped=false;
  for(let parent=e.parentElement;parent;parent=parent.parentElement){
   const style=getComputedStyle(parent),p=parent.getBoundingClientRect();
   const q={left:p.left+parent.clientLeft,right:p.left+parent.clientLeft+parent.clientWidth,top:p.top+parent.clientTop,bottom:p.top+parent.clientTop+parent.clientHeight};
   if(['auto','scroll','hidden','clip'].includes(style.overflowX)&&(r.left<q.left-1||r.right>q.right+1))clipped=true;
   if(['auto','scroll','hidden','clip'].includes(style.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))clipped=true;
  }
  return {rect:r,inView:finite(r)&&inside(r,{left:0,top:0,right:innerWidth,bottom:innerHeight})&&!clipped,hit:!!hit&&(hit===e||e.contains(hit))};
 };
 const controlHeight=parseFloat(getComputedStyle(dialog).getPropertyValue('--control-height'));
 const results=[];let result;
 try {
  add.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
  const addPaint=paint(add),addText=text(add),icon=add.querySelector('svg'),style=getComputedStyle(add);
  const iconRect=icon?rect(icon):null,t=addText.ranges[0];
  const box={gap:parseFloat(style.columnGap)||0,paddingLeft:parseFloat(style.paddingLeft)||0,paddingRight:parseFloat(style.paddingRight)||0,borderLeft:parseFloat(style.borderLeftWidth)||0,borderRight:parseFloat(style.borderRightWidth)||0};
  const intrinsic=iconRect&&t?iconRect.width+t.width+Object.values(box).reduce((sum,n)=>sum+n,0):0;
  const addGroup={...addPaint,label:addText,icon:iconRect,controlHeight,intrinsic,box,disabled:add.disabled,view:add.dataset.boardChoiceView,value:add.dataset.boardChoiceValue};
  addGroup.ok=addPaint.inView&&addPaint.hit&&addText.text==='Add group'&&addText.lines===1&&addText.ranges.length===1&&!!iconRect&&finite(iconRect)
   &&inside(iconRect,addPaint.rect)&&inside(t,addPaint.rect)&&iconRect.right<=t.left+1
   &&Math.min(iconRect.bottom,t.bottom)>Math.max(iconRect.top,t.top)
   &&Math.abs(addPaint.rect.height-controlHeight)<=1&&addPaint.rect.width<=intrinsic+2
   &&add.dataset.boardChoiceView==='V_BOARD_MULTI'&&add.dataset.boardChoiceValue==='';
  for(let i=0;i<lanes.length;i++){
   const lane=lanes[i],header=lane.querySelector(':scope > .slack-list-lane-title'),title=header?.querySelector('h3'),count=header?.querySelector('.slack-list-group-count');
   if(!header||!title||!count){results.push({ok:false,key:lane.dataset.groupKey,error:'Original lane identity missing'});continue;}
   header.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
   const heading=paint(title),counter=paint(count),headerPaint=paint(header),label=text(title),countText=text(count);
   const buttons=[...header.querySelectorAll('button')],actions=buttons.map(button=>{
    const before=paint(button),label=text(button);
    let focusStable=true;
    if(!button.disabled){button.focus({preventScroll:true});const after=rect(button);focusStable=document.activeElement===button&&JSON.stringify(after)===JSON.stringify(before.rect);}
    return {...before,text:label.text,label,disabled:button.disabled,choice:button.dataset.boardChoiceKind??null,value:button.dataset.boardChoiceValue??null,view:button.dataset.boardChoiceView??button.dataset.boardGroupDeleteView??null,delete:button.dataset.boardGroupDelete??null,move:button.dataset.laneMove??null,laneKey:button.dataset.laneKey??null,add:button.classList.contains('slack-list-lane-add'),ariaLabel:button.getAttribute('aria-label'),focusStable};
   });
   const expectedNames=i<3?['Name','Color','Delete','Left','Right','']:['Left','Right',''];
   const identities=actions.every(a=>a.choice?a.view==='V_BOARD_MULTI'&&a.value===lane.dataset.groupKey:a.delete!==null?a.view==='V_BOARD_MULTI'&&a.delete===lane.dataset.groupKey:a.laneKey===lane.dataset.groupKey)
    &&actions.filter(a=>a.add).length===1&&actions.find(a=>a.add)?.ariaLabel===`Add item to ${expected[i][1]}`;
   const nonoverlap=actions.every((a,j)=>actions.every((b,k)=>j>=k||!intersects(a.rect,b.rect)));
   const actionBounds=actions.every(a=>a.inView&&(a.disabled||a.hit)&&a.focusStable&&inside(a.rect,headerPaint.rect)
    &&Math.abs(a.rect.height-controlHeight)<=1&&(!a.text||a.label.lines===1&&a.label.ranges.every(r=>inside(r,a.rect)))
    &&!intersects(a.rect,heading.rect)&&!intersects(a.rect,counter.rect)&&(!a.add||Math.abs(a.rect.width-controlHeight)<=1));
   const cards=lane.querySelector('.slack-list-lane-cards'),cardsRect=cards?rect(cards):null;
   const ok=lane.dataset.groupKey===expected[i][0]&&label.text===expected[i][1]&&label.lines===1&&label.ranges.length>0
    &&label.ranges.every(r=>inside(r,heading.rect))&&heading.inView&&heading.hit&&counter.inView&&countText.text===lane.dataset.count&&countText.lines===1
    &&headerPaint.inView&&inside(heading.rect,headerPaint.rect)&&inside(counter.rect,headerPaint.rect)&&!intersects(heading.rect,counter.rect)
    &&JSON.stringify(actions.map(a=>a.text))===JSON.stringify(expectedNames)&&identities&&nonoverlap&&actionBounds
    &&cardsRect&&cardsRect.height>0&&cardsRect.top>=headerPaint.rect.bottom-1;
   results.push({ok,key:lane.dataset.groupKey,lane:rect(lane),label,heading,counter,count:countText,header:headerPaint,actions,identities,nonoverlap,actionBounds,cards:cardsRect});
  }
  result={ok:addGroup.ok&&results.every(r=>r.ok),addGroup,lanes:results};
 } finally {
  for(const s of offsets){s.node.scrollLeft=s.left;s.node.scrollTop=s.top;}
  if(originalFocus?.isConnected&&originalFocus!==document.body)originalFocus.focus({preventScroll:true});
 }
 const restored=offsets.every(s=>s.node.isConnected&&Math.abs(s.node.scrollLeft-s.left)<=1&&Math.abs(s.node.scrollTop-s.top)<=1)
  &&(originalFocus===document.body||document.activeElement===originalFocus);
 return {...result,ok:result.ok&&restored,restored};
})()
