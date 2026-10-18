// A short entry lease. Conversation editors stay mounted while logs are shown.
async function workflowLogsEntry(key, channel) {
  const matches = [...document.querySelectorAll('[data-workflow-logs-source]')].filter(n => n.dataset.workflowLogsSource === key);
  const button = matches.length === 1 ? matches[0] : null;
  const notice = [...document.querySelectorAll('[data-workflow-logs-notice]')].find(n => n.dataset.workflowLogsNotice === key);
  const body = document.body;
  const editors = ['channel-composer','thread-composer'].map(id => document.getElementById(id)).filter(Boolean);
  const values = editors.map(n => ({node:n,value:n.value,start:n.selectionStart,end:n.selectionEnd}));
  let invalid = false, held = false, observer;
  const composing = () => !!document.querySelector('[data-composing="true"]');
  const originalButton = () => {
    const current = [...document.querySelectorAll('[data-workflow-logs-source]')].filter(n => n.dataset.workflowLogsSource === key);
    return current.length === 1 && current[0] === button;
  };
  const visible = () => button.getClientRects().length > 0
    && button.getBoundingClientRect().width > 0 && button.getBoundingClientRect().height > 0
    && getComputedStyle(button).visibility === 'visible';
  const current = () => !invalid && button?.isConnected && !button.disabled && !button.matches(':disabled')
    && originalButton() && visible() && button.dataset.workflowLogsSource === key && !composing()
    && !document.querySelector('[role="dialog"][aria-modal="true"]')
    && values.every(v => v.node.isConnected && document.getElementById(v.node.id) === v.node && v.node.value === v.value);
  const refuse = () => {if(button?.isConnected && notice?.isConnected)notice.textContent='Finish the current composition before opening workflow activity. Your draft is unchanged.';};
  const original = body.inert;
  const inspect = event => {if(event?.type === 'compositionstart' || !current())invalid=true;};
  const frame = () => new Promise(resolve => {let done=false;const finish=()=>{if(!done){done=true;clearTimeout(timer);cancelAnimationFrame(raf);resolve();}};const timer=setTimeout(finish,200);const raf=requestAnimationFrame(finish);});
  try {
    if(original || !current()){refuse();channel.send(false);await channel.recv();return;}
    body.addEventListener('compositionstart',inspect,true);
    body.addEventListener('input',inspect,true);
    observer=new MutationObserver(inspect);observer.observe(document.documentElement,{attributes:true,childList:true,subtree:true});
    body.inert=true;held=true;
    await frame();
    const ok=current();
    if(ok){if(notice)notice.textContent='';}else refuse();
    channel.send(ok);
    await channel.recv();
    await frame();
  } finally {
    observer?.disconnect();body.removeEventListener('compositionstart',inspect,true);body.removeEventListener('input',inspect,true);
    if(held)body.inert=original;
  }
}
