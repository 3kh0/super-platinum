function listReminderRoot(serial) {
    const root=document.getElementById('slack-list-reminder-editor');
    return root?.isConnected && root.dataset.reminderGeneration===String(serial) ? root : null;
}
function mountListReminder(serial) {
    const root=listReminderRoot(serial);
    if(!root || root.__reminderMounted) return;
    const opener=document.activeElement;
    const dialog=root.closest('.slack-list-dialog');
    const visit=dialog?.dataset.dialogGeneration;
    const detail=dialog?.querySelector('#slack-list-record-detail');
    const detailOpen=detail?.dataset.detailOpenGeneration;
    root.__reminderMounted=true;
    root.addEventListener('compositionstart',()=>{root.__reminderComposing=true;});
    root.addEventListener('compositionend',()=>{root.__reminderComposing=false;});
    const focus=()=>{
        if(listReminderRoot(serial)!==root) return;
        const active=document.activeElement;
        if(root.contains(active) && !active.disabled && active!==root) return;
        if(active && active!==document.body && active!==root && !active.disabled && !root.contains(active)) return;
        const next=root.querySelector('#slack-list-reminder-time:not(:disabled),#slack-list-reminder-check:not(:disabled),#slack-list-reminder-close');
        (next||root).focus({preventScroll:true});
    };
    root.focus({preventScroll:true});
    focus();
    const observer=new MutationObserver(focus);
    observer.observe(root,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled']});
    const cleanup=new MutationObserver(()=>{if(!root.isConnected){
        observer.disconnect();cleanup.disconnect();
        const active=document.activeElement;
        if(active!==document.body&&!root.contains(active)) return;
        const actionableOpener=opener&&opener!==document.body&&opener!==root&&!root.contains(opener)&&
            opener.isConnected&&!opener.disabled&&!opener.matches(':disabled,[aria-disabled="true"]')&&
            opener.matches('button,a[href],input:not([type="hidden"]),select,textarea,[tabindex],[contenteditable="true"]')&&
            !opener.closest('[hidden],[inert],[aria-hidden="true"]');
        if(actionableOpener) {opener.focus({preventScroll:true});return;}
        // The row opener is unmounted when this route opens record details.
        // Return only into the same detail object and exact List visit.
        if(dialog?.isConnected&&dialog.dataset.dialogGeneration===visit&&detail?.isConnected&&
            dialog.querySelector('#slack-list-record-detail')===detail&&detail.dataset.detailOpenGeneration===detailOpen) {
            const target=detail.querySelector('#slack-list-record-reminder:not(:disabled),#slack-list-record-back');
            target?.focus({preventScroll:true});
        }
    }});
    cleanup.observe(document.body,{subtree:true,childList:true});
}
function initializeListReminder(serial,due) {
    const root=listReminderRoot(serial),input=root?.querySelector('#slack-list-reminder-time');
    if(!input || input.__reminderInitialized) return;
    input.__reminderInitialized=true;
    if(due>0) {
        const d=new Date(due*1000),p=n=>String(n).padStart(2,'0');
        if(Number.isFinite(d.getTime())) {
            input.value=`${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
            input.__reminderInitial={value:input.value,seconds:due};
        }
    }
}
function captureListReminder(serial) {
    const root=listReminderRoot(serial),input=root?.querySelector('#slack-list-reminder-time');
    if(!input || root.__reminderComposing || input.type!=='datetime-local' || !input.value || !input.validity.valid || input.validity.badInput) return null;
    const value=input.value;
    if(input.__reminderInitial?.value===value) return input.__reminderInitial.seconds;
    if(!/^\d{4}-\d\d-\d\dT\d\d:\d\d$/.test(value)) return null;
    const date=new Date(value),n=date.getTime();
    // Date.parse normalizes nonexistent DST wall times. Refuse that silent
    // change. Ambiguous times follow the native Date/local-zone convention.
    const p=n=>String(n).padStart(2,'0');
    const round=`${date.getFullYear()}-${p(date.getMonth()+1)}-${p(date.getDate())}T${p(date.getHours())}:${p(date.getMinutes())}`;
    return Number.isFinite(n)&&round===value&&n>0&&n/1000<=253402300799 ? n/1000 : null;
}
