(()=>{
 const requested=__SP_FOCUS__,home=document.getElementById('app-home')||document.querySelector('.app-home'),field=home?.querySelector('[data-modal-users="home-choice"]'),modal=document.querySelector('.app-modal');
 const target=modal||field||home;if(!target)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),s=getComputedStyle(e);let clipped=false;for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=rect(p);if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))clipped=true;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))clipped=true;}return r.width>0&&r.height>0&&!clipped&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const hit=document.elementFromPoint(x,y);return hit===e||e.contains(hit);});};
 const rowNodes=field&&!modal?[...field.querySelectorAll('[data-entity-id]')]:[];
 const headerNodes=field&&!modal?[...field.querySelectorAll('button,input,.message-multi-selected-option > span,[role=status],[role=alert],.muted')].filter(e=>!rowNodes.some(r=>r.contains(e))&&(!e.matches('[role=status],[role=alert],.muted')||e.textContent.trim())):[];
 if(field&&!modal)headerNodes.push(...[...field.closest('.app-modal-field').querySelectorAll(':scope > .app-modal-error')].filter(e=>e.textContent.trim()));
 const keyed=[...headerNodes.map((e,i)=>({key:'header-'+i,e})),...rowNodes.map((e,i)=>({key:'row-'+i,e}))];
 const scoped=!!field&&!modal;
 const focus=requested?keyed.find(x=>x.key===requested)?.e:scoped?field.querySelector('input[type=search]'):target;
 if(!focus)return{ok:false};focus.scrollIntoView({block:'center',inline:'nearest'});
 const controls=[...target.querySelectorAll('button,input')].map(e=>({text:e.textContent,label:e.getAttribute('aria-label'),type:e.type,value:e.value,disabled:e.disabled,visible:visible(e),rect:rect(e)}));
 const selected=field?[...field.querySelectorAll('[data-selected-user]')].map(e=>({id:e.dataset.selectedUser,label:e.querySelector('span')?.textContent,removable:!!e.querySelector('[data-remove-user]')})):[];
 const backgroundHome=modal?{scope:'occluded-background-dom-only',homeVisit:home?.dataset.visit,homeRevision:home?.dataset.revision,selected,controlsDisabled:!!field&&[...field.querySelectorAll('button,input')].every(e=>e.disabled),query:field?.querySelector('input')?.value??null,expanded:field?.querySelector('input')?.getAttribute('aria-expanded')??null}:null;
 const rows=rowNodes.map(e=>({id:e.dataset.entityId,label:e.textContent,role:e.getAttribute('role'),checked:e.getAttribute('aria-checked'),visible:visible(e),disabled:e.disabled}));
 const statuses=[...target.querySelectorAll('[role=status],[role=alert],.muted')].filter(e=>e.textContent.trim()).map(e=>({text:e.textContent,visible:visible(e)}));
 const fieldErrors=field?[...field.closest('.app-modal-field').querySelectorAll(':scope > .app-modal-error')].filter(e=>e.textContent.trim()).map(e=>({text:e.textContent,visible:visible(e)})):[];
 const limit=field?.querySelector('[data-home-users-limit]');
 const selectionStatus=field?{text:limit?.textContent??null,role:limit?.getAttribute('role')??null,invalid:field.querySelector('input')?.getAttribute('aria-invalid'),visible:limit?visible(limit):null}:null;
 const inventory=keyed.map(({key,e})=>({key,text:e.textContent,label:e.getAttribute('aria-label'),type:e.type||null,value:e.value??null,disabled:!!e.disabled}));
 const coverage=keyed.map(({key,e})=>({key,visible:visible(e),rect:rect(e)}));
 const requiredViews=scoped&&!requested?[...coverage.filter(x=>x.key.startsWith('header-')&&!x.visible).map(x=>x.key),...rowNodes.map((_,i)=>'row-'+i)]:[];
 const noOverflow=document.documentElement.scrollWidth<=innerWidth+1&&target.scrollWidth<=target.clientWidth+1,focusVisible=visible(focus);
 const ownControls=[...focus.querySelectorAll('button,input')].every(visible);
 return{ok:noOverflow&&focusVisible&&ownControls&&(!scoped||!!field.querySelector('input'))&&(scoped||controls.every(e=>e.visible)&&statuses.every(e=>e.visible)),scope:requested?'element':scoped?'search-control':'whole',focusKey:requested,focusVisible,inventory,coverage,requiredViews,viewport:[innerWidth,innerHeight],noOverflow,target:modal?'modal':field?'users':'home',targetRect:rect(focus),controls,selected,backgroundHome,selectionStatus,rows,statuses,fieldErrors,focus:document.activeElement?.id,query:field?.querySelector('input[type=search]')?.value??null,expanded:field?.querySelector('input')?.getAttribute('aria-expanded')??null,homeVisit:home?.dataset.visit,homeRevision:home?.dataset.revision};
})()
