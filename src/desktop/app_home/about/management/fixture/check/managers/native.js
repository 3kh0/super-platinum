(()=>{
 const capture=CAPTURE,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description'),config=document.getElementById('workflow-configuration');
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),s=getComputedStyle(e),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.top>=0&&r.left>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.display!=='none'&&s.visibility==='visible'&&(hit===e||e.contains(hit))))return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),v=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(v.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}return true;};
 // Each sample reveals the original element independently. Center alignment is
 // deterministic across repeated observations; nearest depends on prior scroll.
 const paint=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const r=rect(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return{id:e.id,text:e.textContent,disabled:!!e.disabled,rect:r,visible:visible(e),hit:h===e||e.contains(h)}};
 if(capture){if(!root||!title||!description||!root.__workflowManagement)return{ok:false};window.__workflowManagersOriginal={root,title,description,state:root.__workflowManagement};}
 const saved=window.__workflowManagersOriginal,originalNodes=title&&description?!!saved&&saved.root===root&&saved.title===title&&saved.description===description&&saved.state===root.__workflowManagement:null;
 const fields=title&&description?{title:{value:title.value,readonly:title.readOnly,start:title.selectionStart,end:title.selectionEnd},description:{value:description.value,readonly:description.readOnly,start:description.selectionStart,end:description.selectionEnd}}:null;
 const panel=document.getElementById('workflow-permissions'),select=document.getElementById('workflow-permissions-copy');
 const buttons=root?[...root.querySelectorAll('.workflow-management-actions button,#workflow-permissions-save,#workflow-permissions-cancel,#workflow-permissions-check,#workflow-permissions-target-retry,#workflow-permissions-manager-retry,#workflow-permissions-manager-agent-retry,#workflow-permissions-self-save,#workflow-permissions-self-cancel,.workflow-management-filters button,.workflow-management-row')]:config?[config]:[];
 const controls=[...new Set(buttons)].map(paint);
 const feedback=root?[...root.querySelectorAll('.workflow-management-feedback p,#workflow-permissions-status')].filter(e=>e.textContent.trim()).filter((e,i,a)=>a.indexOf(e)===i).map(paint):[];
 const label=select?root.querySelector('label[for="workflow-permissions-copy"]'):null;
 const settings=panel?{id:panel.id,title:panel.querySelector('h2,h3')?.textContent??null,label:label?paint(label):null,
 copy:select?{...paint(select),value:select.value,options:[...select.options].map(o=>({value:o.value,text:o.textContent,disabled:o.disabled}))}:null,
 actions:['workflow-permissions-cancel','workflow-permissions-save'].map(id=>controls.find(c=>c.id===id)??null)}:null;
 const run=document.getElementById('workflow-permissions-run'),box=document.getElementById('workflow-permissions-external');
 const query=id=>{const e=document.getElementById(id);return e?{...paint(e),value:e.value,readonly:e.readOnly,start:e.selectionStart,end:e.selectionEnd}:null;};
 if(settings){const l=root.querySelector('label[for="workflow-permissions-run"]');settings.run=run?{...paint(run),value:run.value,options:[...run.options].map(o=>({value:o.value,text:o.textContent,disabled:o.disabled}))}:null;settings.runLabel=l?paint(l):null;
 settings.external=box?{...paint(box),checked:box.checked}:null;
 settings.query=query('workflow-permissions-target-query');settings.externalQuery=query('workflow-permissions-external-query');
 settings.selected=[...panel.querySelectorAll('[data-run-selected]')].map(e=>({...paint(e),target:e.dataset.runSelected}));
 settings.results=[...panel.querySelectorAll('[data-run-result]')].map(e=>({...paint(e),target:e.dataset.runResult,kind:e.dataset.kind}));
 const area=panel.querySelector('#workflow-permissions-run-settings');settings.epoch=area?.dataset.settingsEpoch??null;
 const held=root.__workflowManagement?.permissionsConfirmation,witness=held?.runSettings;
 const nodes=[area,run,box,document.getElementById('workflow-permissions-target-query'),document.getElementById('workflow-permissions-external-query'),...panel.querySelectorAll('[data-run-selected]'),...panel.querySelectorAll('[data-run-result]')];
 settings.originalControls=held?.panel===panel&&held?.choice===select&&(!area||witness?.epoch===settings.epoch&&witness.nodes.length===nodes.length&&witness.nodes.every((n,i)=>n===nodes[i]));

 settings.binding={renderedEpoch:settings.epoch,heldEpoch:witness?.epoch??null,panel:held?.panel===panel,choice:held?.choice===select,renderedCount:nodes.length,heldCount:witness?.nodes?.length??null,nodeEqual:nodes.map((n,i)=>!!witness&&witness.nodes[i]===n),connected:nodes.map(n=>n===null||n.isConnected)};}
 if(settings){const area=panel.querySelector('#workflow-permissions-manager-settings'),q=area?.querySelector('#workflow-permissions-manager-query'),bump=panel.querySelector('#workflow-permissions-self-removal');
 const chosen=area?[...area.querySelectorAll('[data-manager-selected]')]:[],results=area?[...area.querySelectorAll('[data-manager-result]')]:[],nodes=[area,q,...chosen,...results,bump],held=root.__workflowManagement?.permissionsConfirmation?.managers;
 settings.managers={epoch:area?.dataset.managerEpoch??null,query:q?query(q.id):null,selected:chosen.map(e=>({...paint(e),target:e.dataset.managerSelected})),results:results.map(e=>({...paint(e),target:e.dataset.managerResult})),feedback:area?[...area.querySelectorAll('p[role="status"]')].filter(e=>e.textContent.trim()).map(paint):[],speedBump:bump?{heading:bump.querySelector('h3')?.textContent,message:bump.querySelector('p')?.textContent,actions:['workflow-permissions-self-cancel','workflow-permissions-self-save'].map(id=>controls.find(v=>v.id===id)??null)}:null,
 originalControls:!!held&&held.epoch===area?.dataset.managerEpoch&&held.nodes.length===nodes.length&&held.nodes.every((n,i)=>n===nodes[i])};
 settings.managerBinding={renderedEpoch:area?.dataset.managerEpoch??null,heldEpoch:held?.epoch??null,nodeEqual:nodes.map((n,i)=>!!held&&held.nodes[i]===n)};
 }
 const state=root?.__workflowManagement;
 return{ok:controls.length>0&&controls.every(v=>v.visible)&&feedback.every(v=>v.visible)&&(!fields||originalNodes)&&(!settings||(settings.originalControls&&(!settings.referenceFeedback||settings.referenceFeedback.every(v=>v.visible))&&settings.copy?.visible&&settings.label?.visible&&(!settings.run||(settings.run.visible&&settings.runLabel?.visible))&&(!settings.external||settings.external.visible)&&(!settings.query||settings.query.visible)&&(!settings.externalQuery||settings.externalQuery.visible)&&settings.selected.every(v=>v.visible)&&settings.results.every(v=>v.visible)&&settings.actions.every(v=>v?.visible)&&settings.managers?.originalControls&&settings.managers.query?.visible&&settings.managers.selected.every(v=>v.visible)&&settings.managers.results.every(v=>v.visible)&&settings.managers.feedback.every(v=>v.visible)))&&document.documentElement.scrollWidth<=innerWidth+1,
 viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1,present:!!root,
 scope:root?{visit:root.dataset.managementVisit,document:root.dataset.managementDocument,revision:root.dataset.managementRevision}:null,
 originalNodes,originalDetached:saved?!saved.root.isConnected:null,fields,composing:!!state?.composing,leasePresent:!!state?.lease,
 settings,controls,feedback};
})()
