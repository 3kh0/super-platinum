function executionSurfaceFixture(p) {
  const panel=document.querySelector(p.thread?'.thread-pane':'.pins-panel');
  const body=panel?.querySelector(p.thread?'#thread-body':'.pins-results');
  const card=p.thread?null:body?.querySelector('.pin-card');
  const actions=[...body?.querySelectorAll('[data-workflow-logs-source]')??[]].filter(e=>e.getAttribute('aria-label')===p.label);
  const action=actions.length===1?actions[0]:null;
  const header=p.thread?panel?.querySelector('header'):document.getElementById('channel-details-trigger')?.closest('.conversation-header');
  const ids=p.thread?(p.focused?['thread-composer']:['channel-composer','thread-composer']):[];
  const nodes=ids.map(id=>document.getElementById(id));
  if(!panel||!body||!action||!header||nodes.some(n=>!n)||nodes.some(n=>!n.dataset.projectionVisit||!n.dataset.completionReady))return{ok:false};
  const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
  const visible=e=>{const r=rect(e),s=getComputedStyle(e);return r.width>0&&r.height>0&&s.display!=='none'&&s.visibility==='visible';};
  if(p.seed&&!window.__executionSurfaceFixture){
    const editors=nodes.map((node,i)=>{
      const value=node.id==='channel-composer'?p.channelDraft:p.threadDraft;
      node.focus();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(node,value);
      node.setSelectionRange(3,10,'forward');
      return{node,value,start:3,end:10,visit:node.dataset.projectionVisit,scope:node.dataset.draftScope};
    });
    window.__executionSurfaceFixture={panel,body,card,action,header,key:action.dataset.workflowLogsSource,editors};
  }
  const saved=window.__executionSurfaceFixture;
  if(!saved)return{ok:false};
  const editors=nodes.map((node,i)=>({id:node.id,sameNode:node===saved.editors[i]?.node,
    value:node.value,start:node.selectionStart,end:node.selectionEnd,
    visit:node.dataset.projectionVisit,scope:node.dataset.draftScope,visible:visible(node),
    clientHeight:node.clientHeight,scrollHeight:node.scrollHeight,
    fits:visible(node)&&node.scrollHeight<=node.clientHeight+1}));
  const hrect=rect(header),hit=document.elementFromPoint(hrect.left+hrect.width/2,hrect.top+hrect.height/2);
  const hvisible=visible(header)&&hrect.left>=0&&hrect.top>=0&&hrect.right<=innerWidth+1&&hrect.bottom<=innerHeight+1&&(hit===header||header.contains(hit));
  const result={samePanel:panel===saved.panel,sameBody:body===saved.body,sameCard:p.thread?null:card===saved.card,
    sameAction:action===saved.action,actionKey:action.dataset.workflowLogsSource,actionCount:actions.length,
    visible:visible(panel),display:getComputedStyle(panel).display,
    header:{sameNode:header===saved.header,text:p.thread?header.querySelector('h2')?.textContent:document.getElementById('channel-details-trigger')?.textContent,
      channelLabel:p.thread?header.querySelector('.thread-channel')?.textContent:null,
      visible:hvisible,display:getComputedStyle(header).display,rect:hrect},editors,
    channelComposerCount:document.querySelectorAll('#channel-composer').length,
    threadComposerCount:document.querySelectorAll('#thread-composer').length};
  result.ok=result.samePanel&&result.sameBody&&result.sameAction&&result.sameCard!==false&&result.header.sameNode
    &&result.actionKey===saved.key&&result.visible===!p.hidden&&result.header.visible===!p.hidden
    &&result.header.text===(p.thread?'Thread':'#ship')&&(!p.thread||result.header.channelLabel==='#ship')
    &&result.channelComposerCount===(p.thread&&!p.focused?1:0)&&result.threadComposerCount===(p.thread?1:0)
    &&editors.every((e,i)=>e.sameNode&&e.value===saved.editors[i].value&&e.start===3&&e.end===10
      &&e.visit===saved.editors[i].visit&&e.scope===saved.editors[i].scope&&e.visible===!p.hidden
      &&(p.hidden||e.id!=='thread-composer'||e.fits));
  return result;
}
