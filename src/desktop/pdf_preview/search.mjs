// Search extracted PDF text, never document HTML or scripting. Keep only per-page
// match counts across the document; offsets and DOM marks belong to the visible page.
export function pageText(items) {
  let text = '';
  const segments = [];
  for (const item of items) {
    if (typeof item.str !== 'string') continue;
    segments.push({start:text.length, end:text.length + item.str.length, text:item.str});
    text += item.str;
    if (item.hasEOL) text += '\n';
  }
  return {text, segments};
}

export function patternFor(query) {
  const words = query.trim().split(/\s+/u).filter(Boolean);
  if (!words.length) return null;
  return new RegExp(words.map(word => word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('\\s+'), 'giu');
}

export function findMatches(text, query) {
  const pattern = patternFor(query);
  return pattern ? [...text.matchAll(pattern)].map(match => [match.index, match.index + match[0].length]) : [];
}

export function createPdfSearch({root, signal, getDocument, getRendered, showPage}) {
  const control = name => root.querySelector(`[data-pdf="${name}"]`);
  const input = control('find');
  const previous = control('find-previous'), next = control('find-next'), clear = control('find-clear');
  const status = control('find-status');
  let revision = 0, timer, queued, scanning = false, destroyed = false;
  let query = '', entries = [], total = 0, selected = -1, failed = 0, busy = false;
  let lastPainted;
  const alive = version => !destroyed && root.isConnected && version === revision;

  function controls() {
    input.disabled = !getDocument();
    previous.disabled = next.disabled = busy || !total;
    clear.disabled = !input.value;
    root.dataset.findCount = total;
    root.dataset.findIndex = selected + 1;
  }
  function selection() {
    const entry = entries.find(entry => selected >= entry.start && selected < entry.start + entry.count);
    return entry && {page:entry.page, index:selected - entry.start};
  }
  function announce() {
    status.textContent = !query ? '' : total ? `${selected + 1} of ${total}${failed ? ` · ${failed} pages unavailable` : ''}`
      : failed ? `Search incomplete · ${failed} pages unavailable` : 'No matches';
    controls();
  }
  function paint(scrollToMatch = false) {
    const rendered = getRendered();
    if (!rendered) return;
    const {page, items, textLayer} = rendered;
    const extracted = pageText(items);
    const matches = findMatches(extracted.text, query);
    const chosen = selection();
    let matchIndex = 0;
    const activeMarks = [];
    extracted.segments.forEach((segment, index) => {
      const div = textLayer.textDivs[index];
      if (!div) return;
      // Rebuild from the original text, so clearing/changing a query never
      // accumulates marks or alters selection/copy contents.
      div.replaceChildren();
      let offset = segment.start;
      while (matchIndex < matches.length && matches[matchIndex][1] <= segment.start) matchIndex++;
      for (let at = matchIndex; at < matches.length && matches[at][0] < segment.end; at++) {
        const start = Math.max(segment.start, matches[at][0]), end = Math.min(segment.end, matches[at][1]);
        if (start > offset) div.append(document.createTextNode(segment.text.slice(offset - segment.start, start - segment.start)));
        const mark = document.createElement('mark');
        mark.textContent = segment.text.slice(start - segment.start, end - segment.start);
        if (chosen?.page === page && chosen.index === at) {mark.className = 'pdf-find-active';activeMarks.push(mark);}
        div.append(mark); offset = end;
      }
      if (offset < segment.end) div.append(document.createTextNode(segment.text.slice(offset - segment.start)));
    });
    lastPainted = {page, query, selected};
    if (scrollToMatch && activeMarks.length) {
      const scroll = root.querySelector('.pdf-scroll');
      const area = scroll.getBoundingClientRect(), rects = activeMarks.map(mark => mark.getBoundingClientRect());
      let {top, bottom, left, right} = rects[0];
      for (const rect of rects) {
        top = Math.min(top, rect.top); bottom = Math.max(bottom, rect.bottom);
        left = Math.min(left, rect.left); right = Math.max(right, rect.right);
      }
      // Include every fragment of a multi-line match when it fits. Larger matches
      // start at their first fragment. Scroll only the PDF pane, never its modal.
      if (bottom - top > scroll.clientHeight - 8) scroll.scrollTop += rects[0].top - area.top - 4;
      else if (top < area.top + 4) scroll.scrollTop += top - area.top - 4;
      else if (bottom > area.bottom - 4) scroll.scrollTop += bottom - area.bottom + 4;
      if (right - left > scroll.clientWidth - 8) scroll.scrollLeft += rects[0].left - area.left - 4;
      else if (left < area.left + 4) scroll.scrollLeft += left - area.left - 4;
      else if (right > area.right - 4) scroll.scrollLeft += right - area.right + 4;
    }
  }
  function showSelection() {
    announce();
    const target = selection();
    if (!target) return;
    if (getRendered()?.page === target.page) paint(true);
    else showPage(target.page);
  }
  function move(direction) {
    if (busy || !total) return;
    selected = (selected + direction + total) % total;
    showSelection();
  }
  async function scan(task) {
    const pdf = getDocument();
    if (!pdf || !alive(task.version)) return;
    const results = [];
    let count = 0, failures = 0;
    for (let number = 1; number <= pdf.numPages; number++) {
      if (!alive(task.version)) return;
      status.textContent = `Searching page ${number} of ${pdf.numPages}…`;
      let page;
      try {
        page = await pdf.getPage(number);
        if (!alive(task.version)) return;
        const content = await page.getTextContent();
        if (!alive(task.version)) return;
        const pattern = patternFor(task.query);
        let found = 0;
        for (const match of pageText(content.items).text.matchAll(pattern)) { void match; found++; }
        if (found) {results.push({page:number, start:count, count:found});count += found;}
      } catch {
        if (!alive(task.version)) return;
        failures++;
      } finally {
        if (!destroyed && page && getRendered()?.page !== number) page.cleanup();
      }
      // Let typing, navigation and close events interrupt long document scans.
      await new Promise(resolve => setTimeout(resolve, 0));
    }
    if (!alive(task.version)) return;
    entries = results; total = count; failed = failures; busy = false;
    selected = count ? (results.find(entry => entry.page >= task.startPage) ?? results[0]).start : -1;
    root.dataset.findState = 'ready';
    if (count) showSelection(); else {announce();paint();}
  }
  async function pump() {
    if (scanning) return;
    scanning = true;
    try {while (queued && !destroyed) {const task = queued;queued = undefined;await scan(task);}}
    finally {scanning = false;}
  }
  function change() {
    const version = ++revision;
    clearTimeout(timer); queued = undefined;
    query = input.value.trim(); entries = []; total = 0; selected = -1; failed = 0;
    busy = !!query; root.dataset.findState = query ? 'searching' : 'idle';
    paint(); controls();
    if (!query) {announce();return;}
    status.textContent = 'Searching…';
    const startPage = getRendered()?.page ?? 1;
    timer = setTimeout(() => {queued = {version, query, startPage};void pump();}, 180);
  }
  function reset() {input.value = '';change();input.focus({preventScroll:true});}
  input.addEventListener('input', event => {if (!event.isComposing) change();}, {signal});
  input.addEventListener('compositionend', change, {signal});
  previous.addEventListener('click', () => move(-1), {signal});
  next.addEventListener('click', () => move(1), {signal});
  clear.addEventListener('click', reset, {signal});
  window.addEventListener('keydown', event => {
    if (destroyed || !root.isConnected || event.isComposing || event.keyCode === 229 || event.altKey) return;
    const command = event.metaKey || event.ctrlKey, key = event.key.toLowerCase();
    if (command && key === 'f' && !event.shiftKey) {
      event.preventDefault();event.stopImmediatePropagation();input.focus({preventScroll:true});input.select();
    } else if ((event.target === input && event.key === 'Enter') || (command && key === 'g')) {
      event.preventDefault();event.stopImmediatePropagation();move(event.shiftKey ? -1 : 1);
    } else if (event.target === input && event.key === 'Escape' && input.value) {
      event.preventDefault();event.stopImmediatePropagation();reset();
    }
  }, {signal, capture:true});
  controls(); root.dataset.findState = 'idle';
  return {
    loaded: controls,
    rendered() {
      const rendered = getRendered();
      if (rendered && (query || lastPainted)) paint(selection()?.page === rendered.page);
    },
    destroy() {
      destroyed = true; revision++;clearTimeout(timer);queued = undefined;
      entries = [];lastPainted = undefined;
    }
  };
}
