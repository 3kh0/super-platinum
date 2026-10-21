// Actual native picker buffers, including badInput, are checked before reads.
function workflowActivitySnapshot(expected) {
  const root=document.getElementById('workflow-management');
  const form=document.getElementById('workflow-activity-filters');
  if (!root?.isConnected || !form || !root.contains(form)
      || root.dataset.managementVisit!==String(expected.visit)
      || root.dataset.managementDocument!==String(expected.document)
      || form.dataset.revision!==String(expected.filters.revision)
      || root.__workflowManagement?.composing
      || document.querySelector('[role="dialog"][aria-modal="true"]')) return null;
  if (!workflowActivityDateWitness(expected, form)) return null;
  for (const name of ['status','limit','user']) {
    const node=form.querySelector('#workflow-activity-'+name);
    if (!node || node.disabled || node.validity?.badInput || node.value!==expected.filters[name]) return null;
  }
  function localDay(text,end) {
    if (!text) return null;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(text)) throw Error('date');
    const [y,m,d]=text.split('-').map(Number);
    const date=new Date(0); date.setFullYear(y,m-1,d); date.setHours(end?23:0,end?59:0,end?59:0,0);
    if (date.getFullYear()!==y || date.getMonth()!==m-1 || date.getDate()!==d || !Number.isFinite(date.getTime())) throw Error('date');
    return Math.floor(date.getTime()/1000);
  }
  try {
    const from=localDay(expected.filters.from,false),to=localDay(expected.filters.to,true);
    if (from!==null&&to!==null) {
      // tePt uses strict calendar bounds at +/-90 days: up to 90 inclusive
      // dates. Count calendar days here; local timestamps still preserve DST.
      const calendar = text => {
        const [y,m,d]=text.split('-').map(Number), date=new Date(0);
        date.setUTCFullYear(y,m-1,d);date.setUTCHours(0,0,0,0);return date.getTime();
      };
      const days=(calendar(expected.filters.to)-calendar(expected.filters.from))/86400000;
      const initialDefault=expected.filters.date_origin==='default'
        && expected.filters.default_from===expected.filters.from
        && expected.filters.default_to===expected.filters.to;
      // The source initializes a 90-day-difference default pair without the
      // new-selection bound. Only that exact committed provenance permits it.
      if (from>to || days>(initialDefault?90:89)) return null;
    }
    return {from,to};
  } catch (_) {return null;}
}

// A sent message is not a lifetime acknowledgment in the pinned desktop
// bridge: its evaluator may be collected once this function returns.
async function workflowActivityRead(expected, channel) {
  channel.send(workflowActivitySnapshot(expected));
  await channel.recv();
}
