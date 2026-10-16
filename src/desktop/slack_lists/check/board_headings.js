(()=>{
  const lanes=[...document.querySelectorAll('.slack-list-lane')];
  const headings=lanes.map(lane=>{
    const key=lane.dataset.groupKey,h=lane.querySelector('.slack-list-lane-title h3');
    const marker=h?.querySelector('[data-checkbox-heading]'),svg=marker?.querySelector('svg.material-icon');
    const label=h?.getAttribute('aria-label'),rect=svg?.getBoundingClientRect();
    return {key,label,text:h?.textContent.trim(),state:marker?.dataset.checkboxHeading,
      decorative:svg?.getAttribute('aria-hidden')==='true',
      noControl:!h?.querySelector('input,button,[tabindex],a[href],[role=checkbox]'),
      sameSectionLabel:lane.getAttribute('aria-label')===label,
      size:rect?[rect.width,rect.height]:[],path:svg?.querySelector('path')?.getAttribute('d')};
  });
  const empty=headings.find(h=>h.key===''),checked=headings.find(h=>h.key==='true');
  const ok=headings.length===2&&empty?.label==='Ungrouped'&&empty.state==='unchecked'
    &&checked?.label==='Done: true'&&checked.state==='checked'
    &&headings.every(h=>h.text===''&&h.decorative&&h.noControl&&h.sameSectionLabel&&h.size[0]===20&&h.size[1]===20&&h.path)
    &&empty.path!==checked.path;
  return {ok,headings};
})()
