// Own only actual calendar nodes. Mark pending before an event can cross IPC.
async function workflowCalendarBridge(owner, bridge) {
    const button = document.getElementById("workflow-activity-date-range");
    const form = document.getElementById("workflow-activity-filters");
    const filter = document.getElementById("workflow-activity-filter");
    const panel = document.getElementById("workflow-activity-filter-panel");
    const original = () => button?.isConnected && form?.isConnected && filter?.isConnected &&
        document.getElementById(button.id) === button && button.closest("form") === form &&
        button.dataset.visit === owner.visit && button.dataset.document === owner.document;
    if (!original()) return;
    const iso = date => `${date.getFullYear().toString().padStart(4, "0")}-${(date.getMonth()+1).toString().padStart(2,"0")}-${date.getDate().toString().padStart(2,"0")}`;
    const local = text => { const [y,m,d] = text.split("-").map(Number); const date=new Date(0); date.setFullYear(y,m-1,d); date.setHours(12,0,0,0); return date; };
    const now = new Date(), locale = document.documentElement.lang || navigator.language || "en-US";
    let weekStart = 0;
    try { const info = new Intl.Locale(locale); weekStart = (info.getWeekInfo?.() || info.weekInfo)?.firstDay % 7 || 0; } catch (_) {}
    const init = {today:iso(now), week_start:weekStart,
        months:Array.from({length:12},(_,m)=>new Intl.DateTimeFormat(locale,{month:"long"}).format(new Date(2020,m,15))),
        short_months:Array.from({length:12},(_,m)=>new Intl.DateTimeFormat(locale,{month:"short"}).format(new Date(2020,m,15))),
        weekdays:Array.from({length:7},(_,d)=>new Intl.DateTimeFormat(locale,{weekday:"short"}).format(new Date(2020,5,7+d)))};
    let sequence = Number(button.dataset.calendarAck || 0), pending = null, stopped = false, restoring = false;
    let detach;
    const detached=new Promise(resolve=>{detach=resolve;});
    function send(action, value = "") {
        if (!original() || stopped || pending || button.disabled || button.matches(":disabled") || button.dataset.calendarReadBusy==="true" || button.closest("[inert]")) return;
        // Hover is advisory: it cannot charge the event gate and swallow the
        // click immediately following pointer entry. Its original epoch still
        // prevents a late hover from changing a successor transaction.
        const event = {action,value,seq:action==="hover"?sequence:++sequence,epoch:button.dataset.calendarEpoch,
            revision:button.dataset.revision,visit:owner.visit,document:owner.document};
        if(action==="hover") {bridge.send(event);return;}
        if (action === "save") {
            const status=form.querySelector("#workflow-activity-status"),limit=form.querySelector("#workflow-activity-limit"),
                user=form.querySelector("#workflow-activity-user"),search=form.querySelector("#workflow-activity-user-search");
            if (!status || !limit || !user || !search) return;
            event.buffers={status:status.value,limit:limit.value,user:user.value,search:search.value};
        }
        pending=event;
        button.dataset.calendarPending="true";
        bridge.send(event);
    }
    function click(event) {
        if (!original()) return;
        const target=event.target instanceof Element ? event.target : null;
        if (!target) return;
        if (filter.contains(target)) { event.preventDefault(); send("filter"); return; }
        if (button.contains(target)) { event.preventDefault(); send("open"); return; }
        const calendar=document.getElementById("workflow-activity-calendar");
        if (calendar?.contains(target)) {
            const action=target.closest("[data-calendar-action]");
            if (action && !action.disabled && action.getAttribute("aria-disabled")!=="true") {
                event.preventDefault(); send(action.dataset.calendarAction,action.dataset.date || "");
            }
            return;
        }
        if (button.dataset.calendarOpen==="true") {
            event.preventDefault();event.stopPropagation();send("dismiss");
        } else if (filter.getAttribute("aria-expanded")==="true" && !panel.contains(target)) {
            event.preventDefault();event.stopPropagation();send("outer-close");
        }
    }
    function key(event) {
        if (!original()) return;
        const calendar=document.getElementById("workflow-activity-calendar"), target=event.target;
        if (target===button) {
            if (event.key==="ArrowDown") {event.preventDefault();send("focus");}
            else if (event.key==="Tab" && button.dataset.calendarOpen==="true") send("tab");
            else if (event.key==="Escape" && button.dataset.calendarOpen==="true") {event.preventDefault();event.stopPropagation();send("cancel");}
            return;
        }
        if (calendar?.contains(target)) {
            if (event.key==="Escape") {event.preventDefault();event.stopPropagation();send("cancel");return;}
            const day=target.closest("[data-date]");
            if(day && event.key==="Enter") {
                event.preventDefault();
                if(day.getAttribute("aria-disabled")!=="true")send("day",day.dataset.date);
            } else if (day && ["ArrowLeft","ArrowRight","ArrowUp","ArrowDown"].includes(event.key)) {
                event.preventDefault();
                const next=local(day.dataset.date), delta={ArrowLeft:-1,ArrowRight:1,ArrowUp:-7,ArrowDown:7}[event.key];
                next.setDate(next.getDate()+delta);send("move",iso(next));
            }
        } else if (event.key==="Escape" && filter.getAttribute("aria-expanded")==="true" && panel.contains(target)) {
            event.preventDefault();event.stopPropagation();send("outer-close");
        }
    }
    function hover(event) {
        const calendar=document.getElementById("workflow-activity-calendar");
        if (!calendar || button.dataset.calendarOpen!=="true") return;
        const day=event.target instanceof Element ? event.target.closest("[data-date]") : null;
        if (day && calendar.contains(day)) send("hover",day.dataset.date);
    }
    function leave(event) {
        if (event.target?.id==="workflow-activity-calendar") send("hover","");
    }
    function focus(event) {
        if(event.target===button && !restoring && button.dataset.calendarOpen!=="true")send("open");
    }
    function position() {
        if (!original()) return;
        function place(node, anchor) {
            if (!node || !node.getClientRects().length) return;
            const a=anchor.getBoundingClientRect(),r=node.getBoundingClientRect();
            const left=Math.max(8,Math.min(a.left,innerWidth-r.width-8));
            const top=Math.max(8,Math.min(a.bottom+4,innerHeight-r.height-8));
            const x=`${left}px`,y=`${top}px`;
            if(node.style.left!==x)node.style.left=x;
            if(node.style.top!==y)node.style.top=y;
        }
        place(panel,filter);
        place(document.getElementById("workflow-activity-calendar"),button);
    }
    function acknowledge() {
        if(!original()) {
            if(!stopped) {
                stopped=true;
                bridge.send({action:"closed",value:"",seq:sequence,epoch:button.dataset.calendarEpoch,
                    revision:button.dataset.revision,visit:owner.visit,document:owner.document});
                detach();
            }
            return;
        }
        position();
        if (!original() || !pending || button.dataset.calendarAck !== String(pending.seq)) return;
        const completed=pending;pending=null;
        button.dataset.calendarPending="false";
        if (["focus","move","day"].includes(completed.action)) {
            const calendar=document.getElementById("workflow-activity-calendar");
            calendar?.querySelector(`[data-date="${calendar.dataset.focus}"]`)?.focus({preventScroll:true});
        } else if (["cancel","save","dismiss"].includes(completed.action)) {
            restoring=true;
            button.focus({preventScroll:true});
            restoring=false;
        } else if (completed.action==="outer-close" || (completed.action==="filter" && filter.getAttribute("aria-expanded")==="false")) {
            filter.focus({preventScroll:true});
        }
    }
    const observer=new MutationObserver(acknowledge);
    observer.observe(form,{subtree:true,attributes:true,childList:true});
    if(document.body)observer.observe(document.body,{subtree:true,childList:true});
    document.addEventListener("click",click,true);
    document.addEventListener("keydown",key,true);
    document.addEventListener("pointerover",hover,true);
    document.addEventListener("pointerout",leave,true);
    document.addEventListener("focusin",focus,true);
    window.addEventListener("resize",position);
    document.addEventListener("scroll",position,true);
    try { await bridge.send(init); await Promise.race([bridge.recv(),detached]); } finally {
        stopped=true; observer.disconnect();
        document.removeEventListener("click",click,true);document.removeEventListener("keydown",key,true);
        document.removeEventListener("pointerover",hover,true);document.removeEventListener("pointerout",leave,true);
        document.removeEventListener("focusin",focus,true);
        window.removeEventListener("resize",position);document.removeEventListener("scroll",position,true);
    }
}
