requestAnimationFrame(()=>{
 const board=document.querySelector('.slack-list-board');if(!board||board.dataset.keyboardMounted)return;
 board.dataset.keyboardMounted='true';
 const dialog=board.closest('.slack-list-dialog');
 let retained,lastFocus,identity,interaction,pagerIntent;
 const pagerSelector='.slack-list-lane-next,.slack-list-lane-previous,#slack-list-lanes-next,#slack-list-lanes-previous';
 const laneKeys=()=>JSON.stringify([...board.querySelectorAll('.slack-list-lane')].map(lane=>lane.dataset.groupKey));
 const clearPager=()=>{if(pagerIntent)clearTimeout(pagerIntent.timeout);pagerIntent=undefined;};
 const cellOf=e=>e?.closest?.('.slack-list-card-field');
 const cardOf=e=>e?.closest?.('.slack-list-card');
 const cells=()=>[...board.querySelectorAll('.slack-list-card-field')];
 const controls=c=>[...c.querySelectorAll('a[href],button,input,select,textarea')].filter(e=>!e.matches(':disabled,input[type=checkbox]'));
 const move=c=>{if(!c)return;interaction=undefined;retained=c;c.focus({preventScroll:true});c.scrollIntoView({block:'nearest',inline:'nearest'});normalize();};
 const normalize=()=>{
   if(!board.isConnected||dialog.querySelector('#slack-list-record-detail')){clearPager();return;}
   if(settlePager())return;
   const all=cells(),active=document.activeElement,activeCell=cellOf(active);
   const chosen=all.includes(activeCell)?activeCell:all.includes(retained)?retained:
     all.find(c=>identity&&cardOf(c).dataset.instanceId===identity.instance&&c.dataset.columnId===identity.column)||
     all.find(c=>identity&&cardOf(c).dataset.rowId===identity.row&&c.dataset.columnId===identity.column)||all[0];
   if(!all.includes(interaction))interaction=undefined;
   for(const cell of all){
     const tab=cell===chosen&&!interaction?0:-1;if(cell.tabIndex!==tab)cell.tabIndex=tab;
     const local=interaction===cell;
     if(local&&!cell.hasAttribute('data-escape-local'))cell.setAttribute('data-escape-local','true');
     if(!local&&cell.hasAttribute('data-escape-local'))cell.removeAttribute('data-escape-local');
     for(const control of cell.querySelectorAll('a[href],button,input,select,textarea')){
       const value=local&&controls(cell).includes(control)?0:-1;if(control.tabIndex!==value)control.tabIndex=value;
     }
   }
   retained=chosen;
   if(chosen)identity={instance:cardOf(chosen).dataset.instanceId,row:cardOf(chosen).dataset.rowId,column:chosen.dataset.columnId};
   const unclaimed=!active||active===document.body||active===lastFocus;
   // Paging owns its deferred same-lane restoration; a disabled pager must
   // not let this observer claim focus before that callback runs.
   const lost=lastFocus&&(cellOf(lastFocus)&&(!lastFocus.isConnected||lastFocus.matches?.(':disabled')||active===document.body));
   if(!pagerIntent&&unclaimed&&lost&&!lastFocus?.closest?.('#slack-list-cell-editor')){
     chosen?.focus({preventScroll:true});chosen?.scrollIntoView({block:'nearest',inline:'nearest'});
   }
 };
 // A RAF can run before Dioxus applies a page change. Retain the explicit
 // paging intent until the old card disappears or the lane window changes.
 function settlePager(){
   const intent=pagerIntent;if(!intent)return false;
   if(!board.isConnected||dialog.querySelector('#slack-list-cell-editor,#slack-list-record-detail')){clearPager();return false;}
   const active=document.activeElement;
   if(active&&active!==document.body&&active!==intent.pager&&active!==intent.previous&&active.isConnected){clearPager();return false;}
   const changed=intent.key===undefined?laneKeys()!==intent.lanes:!intent.oldCard?.isConnected;
   if(!changed)return false;
   const lane=[...board.querySelectorAll('.slack-list-lane')].find(candidate=>candidate.dataset.groupKey===intent.key);
   const cell=(lane||board).querySelector('.slack-list-card-field');
   const fallback=lane?.querySelector('.slack-list-lane-previous:not(:disabled),.slack-list-lane-next:not(:disabled)')||
     dialog.querySelector('#slack-list-lanes-previous:not(:disabled),#slack-list-lanes-next:not(:disabled)')||dialog.querySelector('#slack-list-close');
   clearPager();
   if(cell)move(cell);else fallback?.focus({preventScroll:true});
   return true;
 }
 const focus=event=>{
   if(pagerIntent&&event.target!==pagerIntent.pager&&event.target!==pagerIntent.previous)clearPager();
   lastFocus=event.target;const cell=cellOf(event.target);interaction=cell&&controls(cell).includes(event.target)?cell:undefined;if(cell)retained=cell;normalize();
 };
 const click=event=>{
   const pager=event.target.closest(pagerSelector);
   if(!pager||pager.disabled){clearPager();return;}
   clearPager();
   const lane=pager.closest('.slack-list-lane');
   pagerIntent={pager,key:lane?.dataset.groupKey,oldCard:lane?.querySelector('.slack-list-card'),lanes:laneKeys(),previous:document.activeElement};
   pagerIntent.timeout=setTimeout(()=>{clearPager();},2000);
   // Usually the mutation observer settles first. This also handles synchronous
   // renderer patches without assuming that one frame is enough for async ones.
   requestAnimationFrame(()=>{if(board.isConnected)settlePager();});
 };
 const keydown=event=>{
   const cell=cellOf(event.target);if(!cell||!board.contains(cell))return;
   if(event.key==='Escape'&&interaction===cell){event.preventDefault();event.stopPropagation();interaction=undefined;move(cell);return;}
   if(interaction===cell&&event.target!==cell)return;
   if(event.key==='Tab'&&!event.ctrlKey&&!event.altKey&&!event.metaKey){
     const fields=[...cardOf(cell).querySelectorAll('.slack-list-card-field')],index=fields.indexOf(cell)+(event.shiftKey?-1:1);
     if(index>=0&&index<fields.length){event.preventDefault();event.stopPropagation();move(fields[index]);}return;
   }
   if(event.key==='Enter'&&event.altKey){
     const open=cardOf(cell)?.querySelector('.slack-list-record-open');
     if(open){event.preventDefault();event.stopPropagation();open.click();}return;
   }
   if(event.key==='F2'){
     const edit=cell.querySelector('.slack-list-edit-cell:not(:disabled)');
     if(edit){event.preventDefault();event.stopPropagation();edit.click();return;}
   }
   if(event.key==='Enter'){
     const control=controls(cell)[0];if(control){event.preventDefault();event.stopPropagation();interaction=cell;normalize();control.focus({preventScroll:true});return;}
   }
   if(event.key===' '||event.key==='Enter'){
     const checkbox=cell.querySelector('input[type=checkbox]');if(checkbox&&event.target!==checkbox){event.preventDefault();event.stopPropagation();if(!checkbox.disabled)checkbox.click();}return;
   }
   if(!['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End'].includes(event.key)||event.altKey||event.metaKey)return;
   const lanes=[...board.querySelectorAll('.slack-list-lane')],lane=cell.closest('.slack-list-lane'),cards=[...lane.querySelectorAll('.slack-list-card')];
   let laneIndex=lanes.indexOf(lane),cardIndex=cards.indexOf(cardOf(cell));
   if(event.key==='ArrowLeft')laneIndex--;if(event.key==='ArrowRight')laneIndex++;
   if(event.key==='ArrowUp')cardIndex--;if(event.key==='ArrowDown')cardIndex++;
   const horizontal=event.key==='ArrowLeft'||event.key==='ArrowRight';
   if(horizontal){
     const step=event.key==='ArrowLeft'?-1:1;
     while(laneIndex>=0&&laneIndex<lanes.length&&!lanes[laneIndex].querySelector('.slack-list-card'))laneIndex+=step;
     if(laneIndex<0||laneIndex>=lanes.length){event.preventDefault();event.stopPropagation();return;}
   }
   const nextLane=lanes[Math.max(0,Math.min(lanes.length-1,laneIndex))],nextCards=[...nextLane.querySelectorAll('.slack-list-card')];
   if(event.key==='Home')cardIndex=0;if(event.key==='End')cardIndex=nextCards.length-1;
   const card=nextCards[Math.max(0,Math.min(nextCards.length-1,cardIndex))];
   const fields=card?[...card.querySelectorAll('.slack-list-card-field')]:[];
   const target=fields.find(c=>c.dataset.columnId===cell.dataset.columnId)||fields[0];
   if(target){event.preventDefault();event.stopPropagation();move(target);}
 };
 const observer=new MutationObserver(normalize);observer.observe(dialog,{childList:true,subtree:true,attributes:true,attributeFilter:['tabindex','disabled']});
 dialog.addEventListener('focusin',focus);dialog.addEventListener('keydown',keydown);dialog.addEventListener('click',click,true);
 const removal=new MutationObserver(()=>{if(!board.isConnected){observer.disconnect();removal.disconnect();dialog.removeEventListener('focusin',focus);dialog.removeEventListener('keydown',keydown);dialog.removeEventListener('click',click,true);clearPager();}});
 removal.observe(dialog,{childList:true,subtree:true});normalize();
});
