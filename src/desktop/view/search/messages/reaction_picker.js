// Node custody/focus only: never a policy, membership or mutation receipt.
function mountSearchReactionPicker(options,environment=globalThis){
 const {document,MutationObserver}=environment;
 const parent=document.getElementById('shell-overlay-dialog');
 const host=document.getElementById('search-reaction-picker');
 if(!parent||!host||host.dataset.searchReactionPicker!==String(options.id))return false;
 const opener=document.getElementById(options.opener);
 const frame=parent.dataset.searchReactionFrame;
 const visible=n=>n?.isConnected&&!n.disabled&&!n.closest('[hidden],[inert]')&&n.getClientRects().length>0;
 let last=null;
 const focus=()=>{
  if(!host.isConnected||document.getElementById('search-reaction-picker')!==host
    ||document.getElementById('shell-overlay-dialog')!==parent||parent.dataset.searchReactionFrame!==frame)return;
  const dialog=document.getElementById('search-reaction-dialog');
  if(!dialog||!host.contains(dialog)||dialog===last)return;
  last=dialog;
  const active=document.activeElement;
  if(active&&active!==document.body&&active.isConnected&&host.contains(active))return;
  if(active&&active!==document.body&&active.isConnected&&!parent.contains(active))return;
  const target=dialog.querySelector('input:not([disabled]),button:not([disabled])')||dialog;
  if(visible(target))target.focus({preventScroll:true});
 };
 const observer=new MutationObserver(()=>{
  if(host.isConnected){focus();return;}
  observer.disconnect();
  if(document.getElementById('shell-overlay-dialog')!==parent||!parent.isConnected||parent.dataset.searchReactionFrame!==frame
    ||document.getElementById('search-reaction-picker'))return;
  const active=document.activeElement;
  if(active&&active!==document.body&&active.isConnected)return;
  if(document.getElementById(options.opener)===opener&&parent.contains(opener)&&visible(opener))opener.focus({preventScroll:true});
 });
 observer.observe(document.body,{childList:true,subtree:true});focus();return true;
}
