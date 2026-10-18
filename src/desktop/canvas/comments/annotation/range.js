// One original native range is retained until Rust acknowledges local custody.
// No selection, focus, scroll, editor content, or request is changed here.
async function captureCanvasAnnotationRange(options, channel) {
    const root = document.getElementById("shell-overlay-dialog");
    const selection = window.getSelection();
    if (!root || !root.isConnected || root.dataset.dialogKind !== "other" ||
        root.dataset.dialogGeneration !== options.visit || !selection ||
        selection.rangeCount !== 1 || selection.isCollapsed) throw Error("range unavailable");
    const range = selection.getRangeAt(0).cloneRange();
    const originalSelection = [selection.anchorNode, selection.anchorOffset, selection.focusNode, selection.focusOffset];
    const element = n => n?.nodeType === 1 ? n : n?.parentElement;
    const content = element(range.startContainer)?.closest(".canvas-annotation-text");
    const row = content?.parentElement;
    if (!content || !row || !root.contains(content) ||
        !content.contains(range.endContainer) || row.dataset.canvasTextVisit !== options.visit ||
        row.dataset.canvasTextRead !== options.read || !row.dataset.canvasTextSection ||
        row.querySelectorAll(".canvas-annotation-text").length !== 1 ||
        [...root.querySelectorAll("[data-canvas-text-section]")].filter(n => n.dataset.canvasTextSection === row.dataset.canvasTextSection).length !== 1)
        throw Error("range owner changed");
    const tree = [], annotations = [], controlNodes = [], leaves = [];
    let text = "", start = null, end = null;
    const boundary = (node, offset, position) => {
        if (range.startContainer === node && range.startOffset === offset) start = position;
        if (range.endContainer === node && range.endOffset === offset) end = position;
    };
    const scan = (node, depth) => {
        if (++count > 16384 || depth > 40) throw Error("range limit");
        if (node.nodeType === 3) {
            tree.push([node, node.data]);
            const initial = text.length;
            for (let i = 0; i <= node.data.length; i++) boundary(node, i, initial + i);
            text += node.data;
            leaves.push({node, start:initial, end:text.length});
            if (text.length > 65536) throw Error("range limit");
        } else if (node.nodeType === 1 && (node === content || ["SPAN", "A"].includes(node.tagName))) {
            if (node.isContentEditable || node.hidden || node.getAttribute("aria-hidden") === "true") throw Error("range hidden or editable");
            const style = getComputedStyle(node);
            if (style.display === "none" || style.visibility !== "visible") throw Error("range hidden");
            const annotationStart = text.length;
            const control = node.dataset?.canvasCommentControl;
            if (control !== undefined && (node.tagName !== "SPAN" || !control)) throw Error("invalid annotation node");
            const archived = node.dataset?.canvasCommentArchived;
            if (archived !== undefined && (control === undefined || !["true", "false"].includes(archived))) throw Error("invalid archived annotation state");
            const hidden = node.dataset?.canvasCommentHidden;
            if (control !== undefined && (!["true", "false"].includes(hidden) || archived === "true" && hidden !== "true")) throw Error("invalid hidden annotation state");
            if (control !== undefined) controlNodes.push(node);
            tree.push([node, [...node.childNodes]]);
            boundary(node, 0, text.length);
            [...node.childNodes].forEach((child, i) => { scan(child, depth + 1); boundary(node, i + 1, text.length); });
            if (control !== undefined) {
                if (annotationStart === text.length) throw Error("empty annotation node");
                annotations.push({control, start:annotationStart, end:text.length, archived:archived === "true", hidden:hidden === "true"});
            }
        } else if (node.nodeType !== 8) throw Error("unsupported range node");
    };
    let count = 0;
    scan(content, 0);
    annotations.sort((a,b) => a.start-b.start);
    const merged = [];
    for (const annotation of annotations) {
        const last = merged.at(-1);
        if (last && last.end > annotation.start) throw Error("nested annotation node");
        if (last && last.end === annotation.start && last.control === annotation.control && last.archived === annotation.archived && last.hidden === annotation.hidden) last.end = annotation.end;
        else merged.push({...annotation});
    }
    const scalarBoundary = n => Number.isSafeInteger(n) && n >= 0 && n <= text.length &&
        !(n > 0 && n < text.length && /[\uD800-\uDBFF]/.test(text[n - 1]) && /[\uDC00-\uDFFF]/.test(text[n]));
    if (!scalarBoundary(start) || !scalarBoundary(end) || start >= end ||
        ![...range.getClientRects()].some(r => r.width > 0 && r.height > 0)) throw Error("invalid native range");
    // Retained active-thread routing checks a containing openable annotation,
    // then non-hidden descendants with Selection.containsNode(node, true).
    // Range.intersectsNode and the separate hidden-removal rangeContainsNode
    // differ at text edges and must not substitute for this actual result.
    const common = element(range.commonAncestorContainer);
    if (!common || !content.contains(common) || typeof selection.containsNode !== "function")
        throw Error("native control selection unavailable");
    const selectedControl = canvasAnnotationHiddenRoute({content, range, selection, leaves, start, end, exactEmpty:true,
        controls:controlNodes.map(node => ({node,id:node.dataset.canvasCommentControl,hidden:node.dataset.canvasCommentHidden === "true",archived:node.dataset.canvasCommentArchived === "true"}))});
    // Separate retained hidden-removal branch, not active-thread routing.
    // Keep the exact original common ancestor and native boundary predicate:
    // an ancestor-wide selection can remove a disjoint hidden descendant.
    function rangeContainsNode(e,t,n){let i=document.createRange();i.selectNode(t);let r=e.compareBoundaryPoints(Range.START_TO_START,i),s=e.compareBoundaryPoints(Range.START_TO_END,i),a=e.compareBoundaryPoints(Range.END_TO_START,i),o=e.compareBoundaryPoints(Range.END_TO_END,i);return r<=0&&o>=0||!!n&&(r>=0&&s>=0||o<=0&&a<=0)}
    const removed = [];
    if (selectedControl === null) {
        for (let node = common; node && node !== content; node = node.parentElement) {
            if (controlNodes.includes(node)) {
                if (node.dataset.canvasCommentHidden === "true") removed.push(node.dataset.canvasCommentControl);
                break;
            }
        }
        // The retained $(text).find selector has no descendants. Do not widen
        // that query to its element parent while enumerating hidden wrappers.
        const ancestor = range.commonAncestorContainer;
        if (ancestor.nodeType === 1) for (const node of controlNodes) {
            if (node !== ancestor && ancestor.contains(node) && node.dataset.canvasCommentHidden === "true" && rangeContainsNode(range, node, true)) {
                const id = node.dataset.canvasCommentControl;
                if (!removed.includes(id)) removed.push(id);
            }
        }
    }
    let changed = false, timer;
    const observer = new MutationObserver(() => { changed = true; });
    observer.observe(row, {subtree:true, childList:true, characterData:true, attributes:true});
    const composing = () => { changed = true; };
    document.addEventListener("compositionstart", composing, true);
    const result = {token:options.token, section:row.dataset.canvasTextSection, visit:options.visit,
        read:options.read, text, start, end, annotations:merged, selectedControl, removed};
    try {
        await channel.send(result);
        const ack = await Promise.race([channel.recv(), new Promise((_, reject) => {
            timer = setTimeout(() => reject(Error("range custody timeout")), 1500);
        })]);
        const current = window.getSelection();
        if (ack?.token !== options.token || ack.accepted !== true || changed || observer.takeRecords().length ||
            document.getElementById("shell-overlay-dialog") !== root || !root.contains(row) ||
            row.parentElement === null || row.dataset.canvasTextVisit !== options.visit ||
            row.dataset.canvasTextRead !== options.read || content.parentElement !== row ||
            !current || current.rangeCount !== 1 || current.isCollapsed ||
            ![current.anchorNode, current.anchorOffset, current.focusNode, current.focusOffset].every((v,i) => v === originalSelection[i]) ||
            tree.some(([node, value]) => node.nodeType === 3 ? node.data !== value :
                node.childNodes.length !== value.length || [...node.childNodes].some((n,i) => n !== value[i])))
            throw Error("original range changed");
        return result;
    } finally {
        clearTimeout(timer);
        observer.disconnect();
        document.removeEventListener("compositionstart", composing, true);
    }
}
