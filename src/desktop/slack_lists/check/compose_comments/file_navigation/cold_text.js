// Cold title/status/retry text only; inline roots need not own a client box.
(editor => {
    const box = (left, top, right, bottom) => ({left, top, right, bottom, width: Math.max(0, right - left), height: Math.max(0, bottom - top)});
    let clip = box(0, 0, innerWidth, innerHeight), shown = !!editor?.isConnected;
    for (let node = editor; node; node = node.parentElement) {
        const css = getComputedStyle(node), rect = node.getBoundingClientRect();
        shown &&= css.display !== 'none' && css.visibility === 'visible' && !node.hidden;
        // A non-replaced inline root can paint Range fragments while both
        // client dimensions are zero. Its ancestors still supply real clips.
        const inlineRoot = node === editor && css.display === 'inline' && node.clientWidth === 0 && node.clientHeight === 0;
        const x = !inlineRoot && (node === editor || /^(auto|scroll|hidden|clip)$/.test(css.overflowX));
        const y = !inlineRoot && (node === editor || /^(auto|scroll|hidden|clip)$/.test(css.overflowY));
        const left = rect.left + node.clientLeft, top = rect.top + node.clientTop;
        clip = box(x ? Math.max(clip.left, left) : clip.left,
            y ? Math.max(clip.top, top) : clip.top,
            x ? Math.min(clip.right, left + node.clientWidth) : clip.right,
            y ? Math.min(clip.bottom, top + node.clientHeight) : clip.bottom);
    }
    const fragments = [];
    if (editor) {
        const walker = document.createTreeWalker(editor, NodeFilter.SHOW_TEXT);
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
            if (!node.textContent.trim()) continue;
            const range = document.createRange();
            range.selectNodeContents(node);
            for (const rect of range.getClientRects()) {
                if (rect.width <= 0 || rect.height <= 0) continue;
                const r = box(rect.left, rect.top, rect.right, rect.bottom);
                const point = document.elementFromPoint((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                fragments.push({rect: r, hit: !!point && (point === node.parentElement || node.parentElement.contains(point)) && editor.contains(point)});
            }
        }
    }
    fragments.sort((a, b) => a.rect.top - b.rect.top || a.rect.left - b.rect.left);
    const lines = [];
    for (const fragment of fragments) {
        const r = fragment.rect;
        let line = lines.find(line => Math.min(line.rect.bottom, r.bottom) - Math.max(line.rect.top, r.top) > Math.min(line.rect.height, r.height) / 2);
        if (!line) {
            line = {rect: r, fragments: [], fullVisible: false, hit: false};
            lines.push(line);
        }
        line.fragments.push(fragment);
        line.rect = box(Math.min(line.rect.left, r.left), Math.min(line.rect.top, r.top), Math.max(line.rect.right, r.right), Math.max(line.rect.bottom, r.bottom));
    }
    for (const line of lines) {
        const r = line.rect;
        line.fullVisible = shown && clip.width > 0 && clip.height > 0 && r.left >= clip.left - 1 && r.top >= clip.top - 1 && r.right <= clip.right + 1 && r.bottom <= clip.bottom + 1;
        line.hit = line.fragments.every(fragment => fragment.hit);
    }
    const visibleLines = lines.filter(line => line.fullVisible && line.hit).length;
    return {clip, lines, visibleLines, ok: lines.length > 0 && visibleLines === lines.length};
})
