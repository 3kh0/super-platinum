(() => {
 const command=__COMMAND__,target=__TARGET__;
 const key='__listBoardHeaderCapture';
 const dialog=document.querySelector('.slack-list-dialog'),board=dialog?.querySelector('.slack-list-board'),layout=board?.closest('.slack-list-layout');
 const header=board?.querySelector('.slack-list-lane[data-group-key="OPT_ENGINEERING"] > .slack-list-lane-title');
 const add=layout?.querySelector('[data-board-choice-kind="add"]');
 const owner=dialog?{list:dialog.dataset.listId,generation:dialog.dataset.dialogGeneration,layout:dialog.dataset.layoutGeneration,view:document.getElementById('slack-list-view-select')?.value}:null;
 if(command==='begin'){
  if(window[key]||!header||!add||owner?.view!=='V_BOARD_MULTI')return {ok:false,error:'Original Board capture scope missing or already held'};
  window[key]={dialog,board,layout,header,add,owner,buttons:[...header.querySelectorAll('button')],focus:document.activeElement,
   offsets:[board,layout,dialog].map(node=>({node,left:node.scrollLeft,top:node.scrollTop}))};
  return {ok:true,owner};
 }
 const h=window[key];
 if(!h||h.dialog!==dialog||h.board!==board||h.layout!==layout||h.header!==header||h.add!==add||JSON.stringify(owner)!==JSON.stringify(h.owner)
  ||h.buttons.some((button,i)=>header.querySelectorAll('button')[i]!==button))
  return {ok:false,error:'Original Board capture owner or nodes replaced',owner};
 if(command==='restore'){
  for(const s of h.offsets){s.node.scrollLeft=s.left;s.node.scrollTop=s.top;}
  if(h.focus?.isConnected&&h.focus!==document.body)h.focus.focus({preventScroll:true});
  const restored=h.offsets.every(s=>s.node.isConnected&&Math.abs(s.node.scrollLeft-s.left)<=1&&Math.abs(s.node.scrollTop-s.top)<=1)
   &&(h.focus===document.body||document.activeElement===h.focus);
  delete window[key];return {ok:restored,restored};
 }
 const element=target==='header'?header:target==='addGroup'?add:null;
 if(!element)return {ok:false,error:'Unknown original Board capture target'};
 if(command==='reveal'){element.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});return {ok:true};}
 const proof=e=>{
  const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
  let contained=true;
  for(let p=e.parentElement;p;p=p.parentElement){
   const s=getComputedStyle(p),q=p.getBoundingClientRect(),left=q.left+p.clientLeft,top=q.top+p.clientTop;
   if(['auto','scroll','hidden','clip'].includes(s.overflowX))contained&&=r.left>=left-1&&r.right<=left+p.clientWidth+1;
   if(['auto','scroll','hidden','clip'].includes(s.overflowY))contained&&=r.top>=top-1&&r.bottom<=top+p.clientHeight+1;
  }
  return {rect:r.toJSON(),text:e.textContent.trim(),inView:contained&&[r.x,r.y,r.width,r.height].every(Number.isFinite)&&r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1,hit:!!hit&&(hit===e||e.contains(hit))};
 };
 const paint=proof(element),title=target==='header'?proof(header.querySelector('h3')):null;
 const actions=target==='header'?h.buttons.map(button=>({...proof(button),disabled:button.disabled,choice:button.dataset.boardChoiceKind??null,delete:button.dataset.boardGroupDelete??null,move:button.dataset.laneMove??null,add:button.classList.contains('slack-list-lane-add'),laneKey:button.dataset.laneKey??null})):[];
 const icon=target==='addGroup'?proof(add.querySelector('svg')):null;
 const valid=target==='header'?title.text==='Engineering'&&title.inView&&title.hit&&actions.length===6&&actions.every(a=>a.inView&&(a.disabled||a.hit))
  :paint.text==='Add group'&&icon.inView;
 return {ok:paint.inView&&paint.hit&&valid,target,owner,...paint,title,actions,icon,viewport:[innerWidth,innerHeight],scroll:{board:[board.scrollLeft,board.scrollTop],layout:[layout.scrollLeft,layout.scrollTop],dialog:[dialog.scrollLeft,dialog.scrollTop]}};
})()
