for(let t=0;t<200;t++){
 const home=document.getElementById('app-home'),book=window.__spHomeCalendarNative;
 const rows=expected.map(item=>{const e=book?.entries.get(item.id);return{id:item.id,expectedMount:item.mount,mount:e?.mount??null,exists:!!e,closed:!!e?.closed,connected:!!e?.host?.isConnected,parentMatches:!!e&&e.node.parentNode===e.host,visit:e?.visit??null,revision:e?.revision??null,homeVisit:home?.dataset.visit??null,homeRevision:home?.dataset.revision??null,retired:!e||e.mount!==item.mount||e.closed||!e.host?.isConnected||e.node.parentNode!==e.host||(e.date&&(home?.dataset.visit!==String(e.visit)||home?.dataset.revision!==String(e.revision)))};});
 if(rows.length&&rows.every(r=>r.retired))return{ok:true,readOnly:true,rows};
 await new Promise(r=>setTimeout(r,5));
}
return{ok:false,error:'Original calendar DOM did not retire while gate held'};
