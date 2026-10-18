// Closed fixture driver. Actual DOM events, explicitly synthetic/untrusted.
async function hostedComposerCheck(operation, args = {}) {
  const one = (selector, root = document) => {
    const nodes = [...root.querySelectorAll(selector)].filter(e => e.isConnected);
    if (nodes.length !== 1) throw Error(`expected one ${selector}, got ${nodes.length}`);
    return nodes[0];
  };
  const visible = e => {
    const r = e.getBoundingClientRect(), s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.display !== "none" && s.visibility === "visible";
  };
  const button = (text, root = document) => {
    const nodes = [...root.querySelectorAll("button")].filter(e => visible(e) && e.textContent.trim() === text);
    if (nodes.length !== 1) throw Error(`expected one button ${text}, got ${nodes.length}`);
    return nodes[0];
  };
  const click = e => {
    if (!visible(e) || e.disabled || e.getAttribute("aria-disabled") === "true") throw Error("disabled or hidden control");
    e.focus({preventScroll:true}); e.click();
  };
  const rect = e => {
    const r = e.getBoundingClientRect();
    return {left:r.left, top:r.top, right:r.right, bottom:r.bottom, width:r.width, height:r.height};
  };
  if (operation === "attach") click(one("#hosted-attach-trigger"));
  else if (operation === "open") click(button("Attach from Slack", one(".hosted-attach-menu")));
  else if (operation === "choose") {
    const rows = [...one("#hosted-file-picker").querySelectorAll('[role="option"]')];
    if (!Number.isInteger(args.index) || args.index < 0 || args.index >= rows.length) throw Error("row index");
    click(rows[args.index]);
  } else if (operation === "insert") click(button("Insert", one("#hosted-file-picker")));
  else if (operation === "reorder") {
    const chips = [...document.querySelectorAll(".hosted-composer-file")];
    if (chips.length !== 2) throw Error("exact two-file set missing");
    click(button("Move earlier", chips[1]));
  } else if (operation === "send") {
    const field = one("#channel-composer");
    click(one("button.composer-send-btn", field.parentElement));
  } else if (operation === "retry-result") click(button("Save returned result"));
  else if (operation === "review-retirement") click(button("Review local removal"));
  else if (operation === "retire") click(button("Remove local share record"));
  else if (operation !== "read") throw Error("unknown operation");
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const field = one("#channel-composer"), picker = document.querySelector("#hosted-file-picker");
  const active = document.activeElement;
  const rows = picker ? [...picker.querySelectorAll('[role="option"]')].map(e => ({
    text:e.textContent.trim(), selected:e.getAttribute("aria-selected"), disabled:e.disabled, rect:rect(e)
  })) : [];
  return {ok:true, operation, trustedInput:false, viewport:[innerWidth,innerHeight],
    field:{text:field.value, readOnly:field.readOnly, disabled:field.disabled, rect:rect(field),
      start:field.selectionStart,end:field.selectionEnd,direction:field.selectionDirection,
      visit:field.dataset.projectionVisit,scope:field.dataset.draftScope},
    picker:picker ? {rect:rect(picker), rows,
      insertDisabled:one("footer button.primary",picker).disabled,
      query:one("#hosted-picker-query",picker).value} : null,
    chips:[...document.querySelectorAll(".hosted-composer-file")].map(e => ({
      title:one("span",e).textContent,rect:rect(e),buttons:[...e.querySelectorAll("button")].map(b=>({label:b.getAttribute("aria-label"),disabled:b.disabled}))})),
    review:[...document.querySelectorAll('[aria-label="Accepted file share"]')].map(e=>({text:e.textContent,rect:rect(e)})),
    buttons:[...document.querySelectorAll(".hosted-composer-files button")].filter(visible).map(e=>({text:e.textContent.trim(),disabled:e.disabled})),
    focus:{id:active?.id||"",tag:active?.tagName||"",label:active?.getAttribute("aria-label")||"",text:active?.tagName === "BUTTON" ? active.textContent.trim() : ""}
  };
}
