// Capture exact owner and successor nodes before Rust removes one selected default.
async function entityDefaultRemoveFocus(proof){
 const root=document.getElementById('slack-list-rename'),set=root?.querySelector('#slack-list-entity-settings');
 const top=()=>Array.from(document.querySelectorAll('[role="dialog"]')).filter(n=>n.isConnected&&n.getClientRects().length).at(-1);
 const base=()=>root?.isConnected&&document.getElementById(root.id)===root&&top()===root&&root.dataset.renameSerial===proof.serial&&root.dataset.renameStage==='Editing'&&set?.isConnected&&root.querySelector('#slack-list-entity-settings')===set&&!set.disabled&&set.dataset.entityVisit===proof.visit&&set.dataset.entitySelection===proof.selection&&set.dataset.entityChoice===proof.choice;
 const buttons=set?Array.from(set.querySelectorAll('[data-entity-remove]')):[],index=buttons.findIndex(n=>n.dataset.entityRemove===proof.id),active=document.activeElement;
 const removed=buttons[index],query=set?.querySelector('#slack-list-entity-default-query');
 if(!base()||root.dataset.entityFocus||root.dataset.filterFocus||root.dataset.filterCapture||set.dataset.entityEpoch!==proof.epoch||index<0||buttons.filter(n=>n.dataset.entityRemove===proof.id).length!==1||active!==removed||removed.disabled||!query?.isConnected){dioxus.send(false);return;}
 const target=buttons[index+1]||buttons[index-1]||query;
 root.dataset.entityFocus=proof.epoch;
 let moved=false;const track=e=>{if(e.target!==active)moved=true;};document.addEventListener('focusin',track,true);
 let timer;const expired=new Promise(resolve=>{timer=setTimeout(()=>resolve(null),1000);});
 try{
  dioxus.send(true);
  const epoch=await Promise.race([dioxus.recv(),expired]);
  if(typeof epoch!=='string'||epoch===proof.epoch)return;
  for(let i=0;i<30;i++){
   if(!await Promise.race([new Promise(resolve=>requestAnimationFrame(()=>resolve(true))),expired]))return;
   if(!base()||moved||root.dataset.entityFocus!==proof.epoch)return;
   if(set.dataset.entityEpoch===proof.epoch)continue;
   if(set.dataset.entityEpoch!==epoch||removed.isConnected||!target.isConnected||!set.contains(target)||target.disabled||!target.getClientRects().length)return;
   if(document.activeElement!==active&&document.activeElement!==document.body)return;
   target.focus({preventScroll:true});target.scrollIntoView({block:'nearest',inline:'nearest'});return;
  }
 }finally{clearTimeout(timer);document.removeEventListener('focusin',track,true);if(root.dataset.entityFocus===proof.epoch)delete root.dataset.entityFocus;}
}
