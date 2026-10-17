(()=>{
 const q=s=>document.querySelector(s),select=()=>q('#slack-list-new-number-precision');
 const expected=[['0','None'],['1','.0'],['2','.00'],['3','.000'],['4','.0000'],['5','.00000']];
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height,left:r.left,top:r.top,right:r.right,bottom:r.bottom}:null;};
 function fits(e,outline=false){
  if(!e||e.closest('[hidden],[inert]'))return false;const r=e.getBoundingClientRect(),s=getComputedStyle(e);
  if(r.width<=0||r.height<=0||s.display==='none'||s.visibility==='hidden')return false;
  const expand=outline&&s.outlineStyle!=='none'?Math.max(0,parseFloat(s.outlineWidth)+parseFloat(s.outlineOffset)):0;
  let l=0,t=0,right=innerWidth,bottom=innerHeight;
  for(let p=e.parentElement;p;p=p.parentElement){const style=getComputedStyle(p),b=p.getBoundingClientRect();if(style.display==='none'||style.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(style.overflowX)){l=Math.max(l,b.left);right=Math.min(right,b.right);}if(/auto|scroll|hidden|clip/.test(style.overflowY)){t=Math.max(t,b.top);bottom=Math.min(bottom,b.bottom);}}
  return [r.left-expand-l,r.top-expand-t,right-r.right-expand,bottom-r.bottom-expand].every(m=>Number.isFinite(m)&&m>=-Math.pow(2,-16));
 }
 function hit(e){if(!fits(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const t=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return t===e||e.contains(t);}));}
 function inspect(){const e=select(),kind=q('#slack-list-new-field-kind'),name=q('#slack-list-rename-name'),label=q('label[for="slack-list-new-number-precision"]'),panel=q('#slack-list-rename'),a=document.activeElement;
  const options=e?[...e.options].map(o=>[o.value,o.textContent]):null;
  const owned=!!panel&&panel.contains(a)&&fits(a,true);
  return{ok:!!e&&kind?.value==='number'&&label?.textContent==='Decimals'&&JSON.stringify(options)===JSON.stringify(expected)&&fits(e)&&fits(label)&&(e.disabled||hit(e))&&e.dataset.fieldChoice===kind.dataset.fieldChoice&&e.dataset.fieldChoice===name?.dataset.fieldChoice&&e.disabled===kind.disabled&&e.disabled===name.disabled&&owned,value:e?.value,options,label:label?.textContent,disabled:e?.disabled,choice:e?.dataset.fieldChoice,rect:rect(e),labelRect:rect(label),visible:fits(e),hit:hit(e),focus:{id:a?.id,rect:rect(a),owned,ringFits:fits(a,true)}};
 }
 window.__listNumberPrecision={inspect,choose(){const e=select(),scrolling=window.__listTextCreate.numberTools().reveal(e),before=inspect();if(!before.ok||before.value!=='0'||before.disabled)return{ok:false,before};e.focus();if(document.activeElement!==e||!fits(e,true))return{ok:false,before};e.value='3';e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,before,scrolling,value:e.value,focused:document.activeElement===e};}};
 return{ok:true};
})()
