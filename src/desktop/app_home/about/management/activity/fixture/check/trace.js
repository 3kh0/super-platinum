// Fixture-only bounded observations. Never changes product attributes or input state.
function workflowFixtureObservation(id) {
 const get=id=>document.getElementById(id),box=e=>{if(!e)return null;const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const identity=e=>e?{id:e.id,tag:e.tagName,classes:typeof e.className==='string'?e.className:null}:null;
 const node=e=>{if(!e)return null;const r=box(e),s=getComputedStyle(e);return{...identity(e),connected:e.isConnected,disabled:!!e.disabled,ariaDisabled:e.getAttribute('aria-disabled'),rect:r,display:s.display,visibility:s.visibility,inlineStyle:e.getAttribute('style'),clientRects:e.getClientRects().length,hit:identity(document.elementFromPoint(r.left+r.width/2,r.top+r.height/2))}};
 const target=get(id),filter=get('workflow-activity-filter'),button=get('workflow-activity-date-range');
 return{requestedId:id,target:node(target),closestPanel:identity(target?.closest('#workflow-activity-filter-panel')),filter:{...node(filter),expanded:filter?.getAttribute('aria-expanded')??null},date:{...node(button),epoch:button?.dataset.calendarEpoch,ack:button?.dataset.calendarAck,pending:button?.dataset.calendarPending,open:button?.dataset.calendarOpen,readBusy:button?.dataset.calendarReadBusy},panel:node(get('workflow-activity-filter-panel')),calendar:node(get('workflow-activity-calendar'))};
}
function workflowFixtureTrace(event,id) {
 const observation=workflowFixtureObservation(id),key=JSON.stringify({event,observation});
 const journal=window.__workflowFixtureActionTrace??(window.__workflowFixtureActionTrace={next:1,last:null,rows:[]});
 if(journal.last!==key){journal.rows.push({sequence:journal.next++,event,observation});journal.last=key;if(journal.rows.length>32)journal.rows.shift();}
 return observation;
}
