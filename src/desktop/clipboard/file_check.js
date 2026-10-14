// Synthetic File/ClipboardEvent bridge proof. Never accesses navigator.clipboard.
await (async () => {
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (ok, label) => { if (!ok) throw Error(label); };
  const until = async (predicate, label) => {
    for (let n = 0; n < 250; n++) { if (predicate()) return; await pause(20); }
    throw Error(label);
  };
  const paste = (field, name, bytes) => {
    const data = new DataTransfer();
    data.items.add(new File([new Uint8Array(bytes)], name, {type:'application/octet-stream'}));
    const event = new ClipboardEvent('paste', {bubbles:true, cancelable:true, clipboardData:data});
    field.dispatchEvent(event);
    return {prevented:event.defaultPrevented, trusted:event.isTrusted};
  };
  const layout = (field, name) => {
    const footer = field.closest('footer');
    const chips = footer.querySelectorAll('.attachment-chip');
    const chip = chips.length === 1 ? chips[0] : null;
    const visible = e => {
      if (!e) return false;
      const r = e.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left+r.width/2, r.top+r.height/2);
      return r.width > 0 && r.height > 0 && r.left >= 0 && r.top >= 0 && r.right <= innerWidth+1 && r.bottom <= innerHeight+1 && (hit === e || e.contains(hit));
    };
    const displayName = chip?.querySelector('.attachment-name')?.textContent;
    return {ok:visible(field) && visible(chip) && displayName === name, displayName, viewport:[innerWidth,innerHeight], fieldVisible:visible(field), chipVisible:visible(chip)};
  };
  try {
    const plan = await dioxus.recv();
    for (const [ordinal, stage] of plan.entries()) {
      await until(() => document.getElementById(stage.field)?.dataset.completionReady, 'composer mounted');
      const field = document.getElementById(stage.field);
      field.focus({preventScroll:true});
      if (ordinal === 0) {
        const outside = document.createElement('textarea');
        outside.id = 'file-paste-outside-probe';
        outside.style.display = 'none';
        document.body.appendChild(outside);
        try { assert(!paste(outside, 'outside.txt', [9]).prevented, 'outside paste untouched'); }
        finally { outside.remove(); }
        const scope = field.getAttribute('data-draft-scope');
        field.setAttribute('data-draft-scope', 'stale-file-paste-probe');
        try { assert(paste(field, 'stale.txt', [8]).prevented, 'stale paste reached bridge'); }
        finally { field.setAttribute('data-draft-scope', scope); }
        await pause(100);
      }
      const event = paste(field, stage.name, stage.bytes);
      assert(event.prevented && !event.trusted, 'synthetic file paste reached bridge');
      await until(() => field.closest('footer')?.querySelectorAll('.attachment-chip').length === 1, 'attachment rendered');
      await pause(120);
      const geometry = layout(field, stage.name);
      assert(geometry.ok, 'attachment and composer visible');
      dioxus.send({kind:'stage',ordinal,stage:stage.stage,field:stage.field,synthetic:true,layout:geometry});
      assert(await dioxus.recv() === 'captured', 'capture acknowledged');
    }
    dioxus.send({kind:'done'});
  } catch (error) { dioxus.send({kind:'error',error:String(error)}); }
})();
