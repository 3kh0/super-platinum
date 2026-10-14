(()=>{
  const button=key=>[...document.querySelectorAll('.slack-list-lane-add')].find(e=>e.dataset.laneKey===key);
  const rect=e=>e?.getBoundingClientRect();
  const visible=e=>{const r=rect(e);return !!r&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;};
  const hit=e=>{if(!visible(e))return false;const r=rect(e),target=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return target===e||e.contains(target);};
  window.__boardAdd={
    ready:key=>({ok:!!button(key)&&!button(key).disabled&&document.querySelector('.slack-list-board')?.dataset.keyboardMounted==='true'}),
    reveal:key=>{const e=button(key);if(!e)return {ok:false};e.scrollIntoView({block:'nearest',inline:'nearest'});return {ok:true};},
    click:key=>{const e=button(key);if(!e||e.disabled||!hit(e))return {ok:false};e.focus();const focused=document.activeElement===e;e.click();return {ok:focused};},
    geometry:(kind,key)=>{
      const dialog=document.querySelector('.slack-list-dialog'),close=document.getElementById('slack-list-close');
      const target=kind==='lane'?button(key):document.getElementById(kind==='detail'?'slack-list-record-back':kind==='pending'?'slack-list-close':'slack-list-create-retire');
      const r=rect(dialog),t=rect(target),s=document.getElementById('slack-list-create-status');
      const bounded=!!r&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
      const compact=kind!=='lane'||(Math.abs(t?.width-32)<=1&&Math.abs(t?.height-32)<=1);
      const hitTested=hit(target)&&hit(close),status=kind==='lane'||visible(s);
      return {ok:bounded&&compact&&hitTested&&status,bounded,compact,hitTested,status,kind,
        target:t?{left:t.left,top:t.top,width:t.width,height:t.height}:null,viewport:[innerWidth,innerHeight]};
    }
  };
  return {ok:true};
})()
