// Fixture-only instrumentation. It observes the real bridge without changing
// its return values, original event, focus, receipt retirement, or dispatcher.
const service = window.superPlatinumReadShortcuts;
if (!service?.capture || !service?.take) {
  dioxus.send({ok:false,error:'Reading shortcut bridge is not installed'});
} else {
  const entries = [], mutations = [];
  let watching = false;
  const identify = node => node?.id || node?.className || node?.nodeName || null;
  const record = mutation => ({type:mutation.type,target:identify(mutation.target),
    attribute:mutation.attributeName,old:mutation.oldValue,
    current:mutation.attributeName ? mutation.target.getAttribute(mutation.attributeName) : null,
    added:[...(mutation.addedNodes??[])].map(identify).slice(0,5),
    removed:[...(mutation.removedNodes??[])].map(identify).slice(0,5)});
  const observer = new MutationObserver(records => {
    if (watching) mutations.push(...records.map(record));
    if (mutations.length>40) mutations.splice(0,mutations.length-40);
  });
  observer.observe(document.documentElement,{subtree:true,childList:true,attributes:true,attributeOldValue:true,
    attributeFilter:['hidden','style','class','role','aria-hidden','data-read-shortcut-key']});
  const append = entry => {entries.push(entry);if(entries.length>16)entries.shift();};
  const capture = service.capture, take = service.take;
  service.capture = function(event) {
    const result = capture.call(this,event);
    watching=!!result;mutations.length=0;observer.takeRecords();
    append({kind:'capture',target:identify(event.target),active:identify(document.activeElement),
      prevented:event.defaultPrevented,focused:document.hasFocus(),
      payload:result ? {...result} : null});
    return result;
  };
  service.take = function(token) {
    mutations.push(...observer.takeRecords().map(record));
    const result=take.call(this,token);watching=false;
    append({kind:'take',token,result,active:identify(document.activeElement),
      key:document.querySelector('.app-shell')?.dataset.readShortcutKey,
      mutations:mutations.slice(-40)});
    return result;
  };
  window.superPlatinumReadShortcutCheckDiagnostics = {
    snapshot:()=>entries,
    dispose(){observer.disconnect();service.capture=capture;service.take=take;}
  };
  dioxus.send({ok:true});
}
await dioxus.recv();
