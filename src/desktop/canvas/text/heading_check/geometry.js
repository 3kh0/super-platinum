(()=>{
 const d=document.querySelector('.canvas-dialog'),sc=d?.querySelector('.canvas-scroll');
 if(!d||!sc)return {ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
 const dr=rect(d),sr=rect(sc),clip={left:Math.max(0,dr.x,sr.x),right:Math.min(innerWidth,dr.right,sr.right),top:Math.max(0,dr.y,sr.y),bottom:Math.min(innerHeight,dr.bottom,sr.bottom)};
 const visible=r=>r.width>0&&r.height>0&&r.x>=clip.left-1&&r.right<=clip.right+1&&r.y>=clip.top-1&&r.bottom<=clip.bottom+1;
 const hit=e=>{const r=rect(e),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return h===e||e.contains(h);};
 const rows=[...d.querySelectorAll('[data-canvas-text-section]')].map(p=>{
  const h=p.querySelector('.canvas-paragraph-content'),b=p.querySelector('.canvas-text-open');
  return {id:p.dataset.canvasTextSection,tag:h?.tagName??null,text:h?.textContent??null,nested:h?!!h.querySelector('p,div'):null,
   marks:h?[...h.querySelectorAll('.bold,.italic')].map(n=>({text:n.textContent,bold:n.classList.contains('bold'),italic:n.classList.contains('italic')})):[],
   rect:h?rect(h):null,visible:!!h&&visible(rect(h)),hit:!!h&&hit(h),editLabel:b?.getAttribute('aria-label')??null};
 });
 const root=document.getElementById('canvas-text-editor'),e=document.getElementById('canvas-text-draft');let native=null;
 if(root&&e){try{native={generation:root.dataset.editorGeneration,visit:root.dataset.canvasVisit,text:e.textContent,blocks:window.superPlatinumRichEditor.read(e.id),editable:e.isContentEditable,label:document.getElementById('canvas-text-draft-label')?.textContent,
   toolbarLabel:d.querySelector('.canvas-toolbar .canvas-status')?.textContent??null,
   saveDisabled:document.getElementById('canvas-text-save').disabled,cancelDisabled:document.getElementById('canvas-text-cancel').disabled,rect:rect(e),visible:visible(rect(e))};}catch{return {ok:false};}}
 const headingRows=rows.filter(r=>r.id.startsWith('heading-'));
 const semantic=headingRows.every(r=>r.tag===({'heading-one':'H1','heading-two':'H2','heading-three':'H3','heading-twin':'H2'}[r.id])&&!r.nested&&r.editLabel==='Edit heading');
 const noOverflow=d.scrollWidth<=d.clientWidth+1&&sc.scrollWidth<=sc.clientWidth+1;
 return {ok:noOverflow&&semantic&&(native?native.visible&&native.toolbarLabel==='Editing heading':headingRows.some(r=>r.visible&&r.hit)),viewport:[innerWidth,innerHeight],noOverflow,clip,rows,native};
})()
