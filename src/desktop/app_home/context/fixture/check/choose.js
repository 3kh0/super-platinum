(()=>{
 const e=document.querySelector('select[aria-label="App Home workspace"]'),value=__SP_TEAM__;
 if(!e||e.disabled||![...e.options].some(o=>o.value===value))return{ok:false};
 e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect(),s=getComputedStyle(e);
 if(!r.width||!r.height||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return{ok:false};
 for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))return{ok:false};if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))return{ok:false};}
 if(![[r.left+r.width/2,r.top+r.height/2],[r.left+1,r.top+r.height/2],[r.right-1,r.top+r.height/2],[r.left+r.width/2,r.top+1],[r.left+r.width/2,r.bottom-1]].every(([x,y])=>{const h=document.elementFromPoint(x,y);return h===e||e.contains(h)}))return{ok:false};
 e.focus({preventScroll:true});Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,value:e.value,visibleHitTested:true};
})()
