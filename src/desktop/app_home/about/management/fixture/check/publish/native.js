(()=>{
 const capture=CAPTURE,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description'),config=document.getElementById('workflow-configuration');
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),s=getComputedStyle(e),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.top>=0&&r.left>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.display!=='none'&&s.visibility==='visible'&&(hit===e||e.contains(hit))))return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),v=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(v.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}return true;};
 // Each sample reveals the original element independently. Center alignment is
 // deterministic across repeated observations; nearest depends on prior scroll.
 const paint=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const r=rect(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return{id:e.id,text:e.textContent,disabled:!!e.disabled,rect:r,visible:visible(e),hit:h===e||e.contains(h)}};
 if(capture){if(!root||!title||!description||!root.__workflowManagement)return{ok:false};window.__workflowPublishOriginal={root,title,description,state:root.__workflowManagement};}
 const saved=window.__workflowPublishOriginal,originalNodes=title&&description?!!saved&&saved.root===root&&saved.title===title&&saved.description===description&&saved.state===root.__workflowManagement:null;
 const fields=title&&description?{title:{value:title.value,readonly:title.readOnly,start:title.selectionStart,end:title.selectionEnd},description:{value:description.value,readonly:description.readOnly,start:description.selectionStart,end:description.selectionEnd}}:null;
 const buttons=root?[...root.querySelectorAll('.workflow-management-actions button,#workflow-publish-retry,#workflow-publish-check,.workflow-management-filters button,.workflow-management-row')]:config?[config]:[];
 const controls=[...new Set(buttons)].map(paint);
 const feedback=root?[...root.querySelectorAll('.workflow-management-feedback p,#workflow-publish-status')].filter(e=>e.textContent.trim()).filter((e,i,a)=>a.indexOf(e)===i).map(paint):[];
 const state=root?.__workflowManagement,publish=controls.find(v=>v.id==='workflow-management-publish');
 const captionMatches=!publish||publish.text===EXPECTED_PUBLISH_CAPTION;
 return{ok:captionMatches&&controls.length>0&&controls.every(v=>v.visible)&&feedback.every(v=>v.visible)&&(!fields||originalNodes)&&document.documentElement.scrollWidth<=innerWidth+1,
 viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1,present:!!root,
 scope:root?{visit:root.dataset.managementVisit,document:root.dataset.managementDocument,revision:root.dataset.managementRevision}:null,
 originalNodes,originalDetached:saved?!saved.root.isConnected:null,fields,composing:!!state?.composing,leasePresent:!!state?.lease,
 confirmation:null,controls,feedback};
})()
