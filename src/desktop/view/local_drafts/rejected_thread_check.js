// The named offline fixture drives production Return/Continue writing handlers.
await (async () => {
  const pause = () => new Promise(resolve => setTimeout(resolve, 40));
  const assert = (value, message) => { if (!value) throw Error(message); };
  const wait = async (test, message) => {
    for (let i = 0; i < 200 && !test(); i++) await pause();
    assert(test(), message);
  };
  const text = 'Rejected thread reply restored with its channel broadcast choice.';
  const uncertain = 'Unconfirmed sibling thread must not become another send.';
  const delivery = value => [...document.querySelectorAll('[data-managed-draft-recovery="delivery"]')].find(row => row.textContent.includes(value));
  const local = () => [...document.querySelectorAll('[data-local-draft-key]')].find(row => row.textContent.includes(text));
  const returnButton = row => [...(row?.querySelectorAll('button') || [])].find(button => button.textContent.trim() === 'Return to draft');
  const reachable = async element => {
    assert(element, 'required control exists');
    element.scrollIntoView({block:'center', behavior:'instant'}); await pause();
    const r = element.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    assert(r.width > 0 && r.height > 0 && r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight, 'control fully visible');
    assert(hit === element || element.contains(hit), 'control hit area not overlapped');
  };
  const stage = async name => { dioxus.send({ok:true,stage:name}); await dioxus.recv(); };
  const rect = element => {
    if (!element) return null;
    const r = element.getBoundingClientRect();
    return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};
  };
  const noticeGeometry = () => ({toast:rect(document.getElementById('status-toast')),
    focused:rect(document.activeElement),focusedTag:document.activeElement?.tagName,
    threadField:rect(document.getElementById('thread-composer')),
    composers:[...document.querySelectorAll('.composer-wrap,.thread-composer')].map(rect)});
  const notices = [];
  const clearNotice = async label => {
    const separated = () => {
      const notice = document.getElementById('status-toast'), g = noticeGeometry();
      return notice?.dataset.positioned === 'true' && g.toast && g.toast.width>0 && g.toast.height>0
        && g.toast.left>=0 && g.toast.top>=0 && g.toast.right<=innerWidth && g.toast.bottom<=innerHeight
        && g.composers.every(c => c.width<=0 || c.height<=0 || g.toast.bottom<=c.top || g.toast.top>=c.bottom || g.toast.right<=c.left || g.toast.left>=c.right);
    };
    await wait(separated, 'visible notice stays outside both composers: '+label);
    await reachable(document.querySelector('#status-toast .toast-dismiss'));
    notices.push({label,...noticeGeometry()});
  };
  try {
    await wait(() => delivery(uncertain), 'uncertain sibling remains visible');
    assert(!returnButton(delivery(uncertain)), 'uncertain sibling cannot return');
    if (phase === 'restore') {
      await wait(() => returnButton(delivery(text)), 'thread rejection exposes Return to draft');
      const button = returnButton(delivery(text)); await reachable(button);
      await stage('before'); button.click();
      await wait(() => local()?.querySelector('.draft-resume') && !delivery(text), 'thread becomes ordinary local draft');
      await wait(() => document.activeElement === local().querySelector('.draft-resume'), 'return focuses Continue writing');
    } else {
      await wait(() => local()?.querySelector('.draft-resume'), 'restart loads ordinary thread draft');
      assert(!delivery(text), 'restart has no rejected thread consumption');
    }
    const resume = local().querySelector('.draft-resume');
    await reachable(resume); await stage('restored'); await reachable(resume); resume.click();
    await wait(() => document.getElementById('thread-composer')?.value === text, 'exact thread text restored');
    const field = document.getElementById('thread-composer');
    await wait(() => document.activeElement === field, 'thread composer receives focus');
    assert(!field.disabled && !field.readOnly, 'thread composer editable');
    const broadcast = document.getElementById('thread-reply-broadcast');
    assert(broadcast?.checked && !broadcast.disabled, 'broadcast intent restored and editable');
    if (phase === 'restore') {
      await clearNotice('restored');
      const originalHeight = field.style.height;
      const originalRect = field.getBoundingClientRect();
      field.style.height = '160px';
      await wait(() => field.getBoundingClientRect().height > originalRect.height + 1,
        'textarea actually expands, not just its style attribute');
      await clearNotice('expanded-thread');
      field.style.height = originalHeight;
      await wait(() => Math.abs(field.getBoundingClientRect().height - originalRect.height) < 1,
        'textarea returns to original measured height');
      await clearNotice('restored-height');
    }
    await reachable(field); await reachable(broadcast);
    field.focus();
    // A synthetic DOM input event checks DOM→Rust editability, then reverts.
    // Fixture routing does not persist these later edits; the original restored
    // snapshot is verified independently through SQLite in both processes.
    field.value = text + ' Edited locally.';
    field.dispatchEvent(new Event('input', {bubbles:true}));
    dioxus.send({ok:true,stage:'edited'}); await dioxus.recv();
    field.value = text; field.dispatchEvent(new Event('input', {bubbles:true}));
    dioxus.send({ok:true,editable:true,focused:document.activeElement===field,broadcastChecked:broadcast.checked,
      eachVisible:true,hitTested:true,resumeHitBeforeClick:true,noticeClearance:notices,viewport:[innerWidth,innerHeight]});
  } catch(error) { dioxus.send({ok:false,error:error.message,geometry:noticeGeometry()}); }
  await dioxus.recv();
})();
