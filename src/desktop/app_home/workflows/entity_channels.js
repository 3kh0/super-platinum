// Exact dfPN record/missing-event and h6YE ordinary English display branches.
// Known event names are supplied only by the owned calendar formatter.
function workflowChannelNames(request) {
    if (!request || typeof request.generation !== "string" || !request.inputs ||
        typeof request.inputs !== "object" || Array.isArray(request.inputs)) return null;
    const names = {};
    for (const [id, v] of Object.entries(request.inputs)) {
        if (!id || !v || (v.raw != null && typeof v.raw !== "string")) return null;
        let name;
        if (v.kind === "ordinary") name = v.raw || "unknown-channel";
        else if (v.kind === "record") {
            const raw = v.raw || "...", match = raw.match(/ZC:[0-9A-Z]+?:(.+?)$/i);
            name = match ? match[1] : raw;
        } else if (v.kind === "meeting") {
            if (v.known != null) { if (typeof v.known !== "string") return null; name = v.known; }
            else {
                let formatted = null;
                if (v.raw?.startsWith("mc:")) {
                    const [,,title] = v.raw.split(":",3);
                    if (title) formatted = title.replace(/-\d{4}-\d{2}-\d{2}-\d{4}$/, "").replace(/-/g," ").replace(/\b\w/g,c=>c.toUpperCase());
                }
                name = formatted ?? v.raw ?? "";
            }
        } else return null;
        Object.defineProperty(names,id,{value:name,enumerable:true});
    }
    return {generation:request.generation,names};
}
