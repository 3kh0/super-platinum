await (async()=>{
 const init=await dioxus.recv();
 const button=()=>document.querySelector('.viewer-footer button.primary');
 const pre=()=>document.querySelector('.text-preview-content');
 const refresh=()=>document.getElementById('file-preview-refresh');
 const label=()=>button()?.textContent.trim();
 const assert=(v,m)=>{if(!v)throw Error(m);};
 let phase='initial',postHeldDeadline=null;
 const wait=async f=>{for(let n=0;n<4500;n++){if(f())return;if(postHeldDeadline!==null&&performance.now()>=postHeldDeadline)throw Error('Cached post-held DOM deadline');await new Promise(r=>setTimeout(r,40));}throw Error('Cached DOM deadline');};
 const control=(b,wanted)=>{
   const r=b?.getBoundingClientRect(),hit=r&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
   assert(b&&!b.disabled&&b.textContent.trim()===wanted&&r.width>=96&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&(hit===b||b.contains(hit)),'Visible exact cached control');
   return {label:wanted,disabled:b.disabled,hit:true,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 };
 let oldPre,oldButton;
 const diagnostic=()=>({phase,refresh:{present:!!refresh(),disabled:refresh()?.disabled??null,busy:document.querySelector('.file-refresh')?.getAttribute('data-refresh-busy')??null,status:document.querySelector('.file-refresh [role="status"]')?.textContent??null},text:pre()?.textContent??null,label:label()??null,oldPreConnected:oldPre?.isConnected??null,oldButtonConnected:oldButton?.isConnected??null,samePre:pre()===oldPre,sameButton:button()===oldButton,toastPresent:!!document.getElementById('status-toast')});
 const report=async(stage,wanted)=>{
   if(stage!=='saved')await wait(()=>!document.getElementById('status-toast'));
   const fresh=stage==='refreshed'||stage==='retired';
   assert(pre()?.textContent===(fresh?init.fresh:init.old),'Cached visible contents');
   const r=pre().getBoundingClientRect();
   assert(r.height>=40&&r.width>0&&r.top>=0&&r.bottom<=innerHeight,'Cached text fully in viewport');
   const toast=stage==='saved'?await downloadToastGeometry('Saved release-notes.txt'):null;
   if(fresh)assert(pre()!==oldPre&&!oldPre.isConnected&&button()!==oldButton&&!oldButton.isConnected,'Actual old viewer/button replaced');
   dioxus.send({ok:true,stage,viewport:[innerWidth,innerHeight],button:control(button(),wanted),text:pre().textContent,toast,replaced:fresh});
   assert(await dioxus.recv()===stage,'Cached exact stage ACK');
 };
 try {
   await wait(()=>pre()?.textContent===init.old&&label()==='Download');oldPre=pre();oldButton=button();
   await report('initial','Download');button().click();await wait(()=>label()==='Cancel');await wait(()=>label()==='Download');
   await report('canceled','Download');button().click();await wait(()=>label()==='Cancel');await wait(()=>label()==='Download');
   await report('saved','Download');button().click();await wait(()=>label()==='Cancel');
   await report('held','Cancel');
   postHeldDeadline=performance.now()+8000;phase='refresh-control';
   control(refresh(),'Refresh contents');refresh().click();
   phase='await-fresh-viewer';
   await wait(()=>pre()?.textContent===init.fresh&&label()==='Download');
   phase='report-refreshed';await report('refreshed','Download');postHeldDeadline=null;
   phase='report-retired';await report('retired','Download');dioxus.send({ok:true});
 }catch(e){dioxus.send({ok:false,error:String(e.message),diagnostic:diagnostic()});}
})();
