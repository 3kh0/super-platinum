// Fixture-only native operations; never patches Dioxus or reads clipboard/storage.
async function draftProjectionCheck(operation, args) {
  const e = document.getElementById(args.id);
  if (operation === "absent") return {ok: !e};
  if (!e || !e.isConnected) return {ok:false, reason:"missing field"};
  const own = Object.getOwnPropertyDescriptor(e, "value");
  if (!own?.get || !own?.set || !e.dataset.projectionVisit) return {ok:false, reason:"reference/projection mount not ready"};
  if (operation === "ready") return {ok:true};
  if (operation === "set" || operation === "queued-input") {
    const edit = () => {
      e.value = args.text;
      const start = e.value.indexOf("\u{1f680}");
      e.setSelectionRange(Math.max(0,start), e.value.length, "backward");
      if (args.input) e.dispatchEvent(new InputEvent("input",{bubbles:true,inputType:"insertText",data:args.text}));
    };
    if (operation === "queued-input") await new Promise(resolve => queueMicrotask(() => {edit();resolve();}));
    else edit();
  } else if (operation === "selection") {
    e.focus({preventScroll:true});
    const start=e.value.indexOf("\u{1f680}");
    e.setSelectionRange(Math.max(0,start),e.value.length,"backward");
  } else if (operation === "selection-change") {
    e.setSelectionRange(0,0,"none");
  } else if (operation === "composition-start" || operation === "composition-end") {
    e.dispatchEvent(new CompositionEvent(operation === "composition-start" ? "compositionstart" : "compositionend", {bubbles:true,data:"\u{65e5}\u{672c}\u{8a9e}"}));
  } else if (operation === "show") {
    e.scrollIntoView({block:"nearest",inline:"nearest"});
  }
  const r=e.getBoundingClientRect();
  const display=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value").get.call(e);
  return {ok:true,text:e.value,display,scope:e.dataset.draftScope,visit:e.dataset.projectionVisit,
    codePoints:[...e.value].map(c=>c.codePointAt(0)),utf16Length:e.value.length,start:e.selectionStart,end:e.selectionEnd,direction:e.selectionDirection,
    composing:e.dataset.composing === "true",rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},
    viewport:[innerWidth,innerHeight]};
}
