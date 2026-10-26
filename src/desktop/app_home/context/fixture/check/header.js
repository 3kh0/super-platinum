(()=>{
 const e=document.querySelector('.app-modal')?null:document.querySelector('[data-home-context]');if(!e)return{ok:true,present:false};
 e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect(),s=getComputedStyle(e);let clipped=false;
 for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))clipped=true;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))clipped=true;}
 const visible=!!r.width&&!!r.height&&!clipped&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&[[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===e||e.contains(h)});
 const select=e.querySelector('select');return{ok:visible,present:true,visible,text:e.textContent,value:select?.value??null,options:select?[...select.options].map(o=>({value:o.value,label:o.textContent})):[],rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},viewport:[innerWidth,innerHeight]};
})()
