(()=>{
 const field=()=>document.getElementById('slack-list-find-input');
 const visible=e=>!!e?.getClientRects().length;
 const click=selector=>{const button=document.querySelector(selector);if(!visible(button)||button.disabled)return {ok:false,selector};button.focus({preventScroll:true});button.click();return {ok:true};};
 const input=(value,focus=true)=>{const node=field();if(!node)return {ok:false};if(focus)node.focus({preventScroll:true});node.value=value;node.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};};
 const choose=value=>{const select=document.getElementById('slack-list-view-select');if(!select)return {ok:false};select.focus({preventScroll:true});select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};};
 const shortcut=()=>{const node=document.activeElement||document.querySelector('.slack-list-dialog');node.dispatchEvent(new KeyboardEvent('keydown',{key:'f',ctrlKey:true,bubbles:true,cancelable:true}));const input=field();return {ok:!!input&&document.activeElement===input&&input.selectionStart===0&&input.selectionEnd===input.value.length};};
 const clipped=e=>{
  if(!e||!e.getClientRects().length)return null;
  const r=e.getBoundingClientRect();let left=Math.max(0,r.left),top=Math.max(0,r.top),right=Math.min(innerWidth,r.right),bottom=Math.min(innerHeight,r.bottom);
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){left=Math.max(left,q.left+p.clientLeft);right=Math.min(right,q.left+p.clientLeft+p.clientWidth);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){top=Math.max(top,q.top+p.clientTop);bottom=Math.min(bottom,q.top+p.clientTop+p.clientHeight);}}
  return {width:Math.max(0,right-left),height:Math.max(0,bottom-top),fullWidth:r.width,fullHeight:r.height};
 };
 const full=r=>!!r&&r.fullWidth>0&&r.fullHeight>0&&r.width>=r.fullWidth-1&&r.height>=r.fullHeight-1;
 const boardGeometry=()=>{
  const board=document.querySelector('.slack-list-board');if(!board)return null;
  const lane=board.querySelector('.slack-list-lane'),card=lane?.querySelector('.slack-list-card');
  const heading=clipped(lane?.querySelector('.slack-list-lane-title')),body=clipped(board),first=clipped(card),primary=clipped(card?.querySelector('.slack-list-card-primary')),action=clipped(card?.querySelector('.slack-list-record-open')),empty=clipped(lane?.querySelector('.slack-list-lane-empty'));
  const readableCard=!!first&&first.width>=Math.min(first.fullWidth,180)&&first.height>=Math.min(first.fullHeight,80)&&full(primary)&&full(action);
  return {ok:full(heading)&&(card?readableCard:full(empty)),kind:card?'card':'empty',heading,body,card:first,primary,action,empty,completeCard:full(first)};
 };
 const boardReachability=()=>{
  const dialog=document.querySelector('.slack-list-dialog'),layout=dialog.querySelector('.slack-list-layout'),help=dialog.querySelector('.slack-list-board-shortcuts'),notices=dialog.querySelector('.slack-list-notices');
  if(!help)return {ok:false,error:'Missing Board disclosure'};
  const active=document.activeElement,range=active===field()?[active.selectionStart,active.selectionEnd,active.selectionDirection]:null,open=help.open;
  const scroll=[...dialog.querySelectorAll('*'),dialog,document.documentElement,document.body].map(e=>[e,e.scrollLeft,e.scrollTop]);
  const controls=[];let helpBottom=false,warningBottom=false;
  try{
   for(const selector of ['#slack-list-view-select','#slack-list-rename-view','#slack-list-refresh','#slack-list-close','.slack-list-card-primary','.slack-list-lane-next:not(:disabled)','#slack-list-lanes-next','.slack-list-board-shortcuts summary']){
    const e=dialog.querySelector(selector);if(!e){controls.push({selector,ok:false});continue;}e.focus({preventScroll:true});e.scrollIntoView({block:'nearest',inline:'nearest'});controls.push({selector,ok:document.activeElement===e&&!e.disabled&&e.tabIndex>=0&&full(clipped(e)),visible:clipped(e)});
   }
   help.open=true;const text=help.querySelector('p');text.scrollIntoView({block:'end'});const t=text.getBoundingClientRect(),l=layout.getBoundingClientRect();helpBottom=t.bottom<=l.bottom+1&&clipped(text)?.height>0;
   if(notices){notices.scrollTop=notices.scrollHeight;warningBottom=notices.scrollTop+notices.clientHeight>=notices.scrollHeight-1&&clipped(notices)?.height>0;}else warningBottom=true;
  }finally{help.open=open;active?.focus({preventScroll:true});if(range)active.setSelectionRange(...range);for(const [e,x,y] of scroll){e.scrollLeft=x;e.scrollTop=y;}}
  return {ok:controls.every(c=>c.ok)&&helpBottom&&warningBottom,controls,helpBottom,warningBottom,restored:document.activeElement===active&&(!range||(active.selectionStart===range[0]&&active.selectionEnd===range[1]&&active.selectionDirection===range[2]))&&scroll.every(([e,x,y])=>e.scrollLeft===x&&e.scrollTop===y)};
 };
 const geometry=stage=>{
  const dialog=document.querySelector('.slack-list-dialog'),input=field(),r=dialog?.getBoundingClientRect(),f=input?.getBoundingClientRect();
  const bounded=!!r&&r.left>=-1&&r.top>=-1&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&dialog.scrollWidth<=dialog.clientWidth+1;
  const panel=document.querySelector('#slack-list-cell-editor,#slack-list-delete-confirm,#slack-list-record-detail,#slack-list-rename');
  const findVisible=visible(input),findFits=!findVisible||f.left>=r.left&&f.right<=r.right&&f.top>=r.top&&f.bottom<=r.bottom;
  const active=document.activeElement,focusOwned=!!active&&dialog.contains(active);
  const inspectBoard=['initial','filtered','empty','page-reset','metadata-change','refresh-change','final'].includes(stage)&&!panel;
  const board=inspectBoard?boardGeometry():null,reachability=stage==='initial'&&board?boardReachability():null;
  if(board){board.findVisible=findVisible;board.findFits=full(clipped(input));board.ok=board.ok&&board.findVisible&&board.findFits;}
  return {ok:bounded&&findFits&&focusOwned&&(!board||(board.ok&&(!['initial','page-reset','final'].includes(stage)||board.completeCard)))&&(!reachability||(reachability.ok&&reachability.restored)),board,reachability,stage,bounded,findFits,findVisible,focusOwned,focus:active?.id||active?.getAttribute('aria-label')||active?.tagName,viewport:[innerWidth,innerHeight],query:input?.value??null,panel:panel?.id??null,matched:document.querySelector('.slack-list-pagination')?.textContent,visibleRows:[...document.querySelectorAll('.slack-list-layout [data-row-id]')].map(e=>e.dataset.rowId)};
 };
 const beginDraft=()=>{const cell=document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_NAME"]');if(!cell)return {ok:false};cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return {ok:true};};
 const selection=()=>{const s=getSelection();return s?.rangeCount===1?{anchor:s.anchorNode,anchorOffset:s.anchorOffset,focus:s.focusNode,focusOffset:s.focusOffset,text:s.toString()}:null;};
 const sameSelection=old=>{const now=selection();return !!old&&!!now&&Object.keys(old).every(k=>old[k]===now[k]);};
 const changeDraft=()=>{
  const root=document.getElementById('slack-list-rich-draft'),editor=document.getElementById('slack-list-cell-editor');if(!root||root.contentEditable!=='true')return {ok:false};
  const original=JSON.stringify(window.superPlatinumRichEditor.read(root.id)),originalText=root.textContent;
  root.focus();const range=document.createRange();range.selectNodeContents(root);range.collapse(false);getSelection().removeAllRanges();getSelection().addRange(range);
  if(!document.execCommand('insertText',false,' Unsaved Find draft Café'))return {ok:false};
  const caret=getSelection().getRangeAt(0);if(caret.startContainer.nodeType!==3||caret.startOffset<4)return {ok:false};
  getSelection().setBaseAndExtent(caret.startContainer,caret.startOffset-4,caret.startContainer,caret.startOffset);
  window.__findDraft={root,editor,original,originalText,text:originalText+' Unsaved Find draft Café',blocks:JSON.stringify(window.superPlatinumRichEditor.read(root.id)),selection:selection()};
  return {ok:root.textContent===window.__findDraft.text&&window.__findDraft.blocks!==original&&window.__findDraft.selection?.text==='Café'};
 };
 const draft=hidden=>{const old=window.__findDraft,root=document.getElementById('slack-list-rich-draft'),save=document.getElementById('slack-list-editor-save');return {ok:!!old&&root===old.root&&document.getElementById('slack-list-cell-editor')===old.editor&&root.textContent===old.text&&JSON.stringify(window.superPlatinumRichEditor.read(root.id))===old.blocks&&sameSelection(old.selection)&&save?.disabled===hidden,retainedRoot:root===old?.root,selectionRetained:sameSelection(old?.selection),disabled:save?.disabled};};
 const undoDraft=()=>{const old=window.__findDraft;if(!draft(false).ok)return {ok:false};old.root.focus();const changed=document.execCommand('undo');return {ok:changed&&old.root.textContent===old.originalText&&JSON.stringify(window.superPlatinumRichEditor.read(old.root.id))===old.original};};
 const redoDraft=()=>{const old=window.__findDraft;if(!old||document.getElementById('slack-list-rich-draft')!==old.root)return {ok:false};const changed=document.execCommand('redo');return {ok:changed&&old.root.textContent===old.text&&JSON.stringify(window.superPlatinumRichEditor.read(old.root.id))===old.blocks};};
 window.__listFind={input,choose,click,shortcut,geometry,beginDraft,changeDraft,draft,undoDraft,redoDraft};
 return {ok:true};
})()
