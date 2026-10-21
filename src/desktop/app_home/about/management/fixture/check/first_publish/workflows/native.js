(()=>{
 const base=BASE,registry=window.__firstPublishRegistry;
 const nodeId=e=>{if(!e)return null;if(!registry.ids.has(e))registry.ids.set(e,registry.next++);return registry.ids.get(e)};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const detailsPane=document.getElementById('channel-workflow-details');
 EXPOSURE
 const paint=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const r=rect(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(detailsPane){const exposure=exposed(e,r);return{id:e.id,nodeId:nodeId(e),text:e.textContent,rect:r,connected:e.isConnected,visible:e.isConnected&&exposure.hitPoint!==null,exposure}}return{id:e.id,nodeId:nodeId(e),text:e.textContent,rect:r,connected:e.isConnected,visible:r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&(h===e||e.contains(h))};};
 const panel=document.getElementById('channel-workflows-panel');
 if(!panel)return{...base,workflows:null};
 let product;try{product=JSON.parse(panel.getAttribute('data-fixture-readback'))}catch{return{...base,ok:false,workflows:{error:'original readback absent'}}}
 const tab=[...document.querySelectorAll('.channel-tabs button')].find(e=>e.textContent.trim()==='Workflows'),header=document.getElementById('channel-details-trigger');
 const controls=['channel-workflows-refresh','channel-workflows-retry'].map(id=>document.getElementById(id)).filter(Boolean).map(e=>({...paint(e),disabled:e.disabled}));
 const sections=['featured','bookmarked'].map(kind=>{const e=document.getElementById('channel-workflows-'+kind);return e?{kind,id:e.id,nodeId:nodeId(e),role:e.getAttribute('role'),rows:[...e.querySelectorAll('[data-trigger-id]')].map(row=>{const title=row.querySelector('.channel-workflow-title'),description=row.querySelector('.channel-workflow-description');return{...paint(row),trigger:row.dataset.triggerId,status:row.dataset.status,title:title?.textContent??null,description:description?.textContent??null,actions:row.querySelectorAll('button,a,input,select').length,opener:row.querySelector('button')?{id:row.querySelector('button').id,nodeId:nodeId(row.querySelector('button')),disabled:row.querySelector('button').disabled}:null,brokenImages:[...row.querySelectorAll('img')].filter(i=>!i.complete||i.naturalWidth===0).length}})}:null});
 const feedback=[...panel.querySelectorAll('[role=status],[role=alert]')].filter(e=>e.textContent.trim()).map(paint);
 const t=tab?{...paint(tab),pressed:tab.getAttribute('aria-pressed')}:null;
 const h=header?paint(header):null;
 const empty=[...panel.querySelectorAll('.channel-workflows-empty')].map(paint);
 const measured=e=>detailsPane?e.connected&&!!e.exposure:e.visible;
 const value={empty,panel:{id:panel.id,nodeId:nodeId(panel),connected:panel.isConnected},product,tab:t,header:h,controls,sections,feedback};
 return{...base,ok:!!t&&measured(t)&&t.pressed==='true'&&!!h&&measured(h)&&controls.every(measured)&&(['ready','partial'].includes(product.status)?sections.every(s=>s&&s.rows.every(r=>measured(r)&&r.actions===(r.status==='available'?1:0)&& (r.status!=='available'||r.opener?.disabled===false)&&r.brokenImages===0)):sections.every(s=>s===null))&&feedback.every(measured)&&base.noOverflow,workflows:value};
})()
