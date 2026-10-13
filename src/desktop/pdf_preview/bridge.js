// Only bundled PDF.js and current-viewer bytes enter this renderer. No document
// actions, links, annotations, HTML, attachments or scripting are activated.
const config = await dioxus.recv();
const root = document.getElementById(config.id);
if (!root) { dioxus.send({ closed: true }); return; }
window.superPlatinumPdf ??= new Map();
window.superPlatinumPdf.get(config.id)?.destroy();
const controls = name => root.querySelector(`[data-pdf="${name}"]`);
const scroll = root.querySelector('.pdf-scroll');
const sheet = root.querySelector('.pdf-page');
const canvas = sheet.querySelector('canvas');
const layer = sheet.querySelector('.textLayer');
let documentTask, pdf, rendering, textLayer, pageObject, search, rendered;
let pageNumber = 1, rotation = 0, zoom = 1, generation = 0, disposed = false;
let renderQueue = Promise.resolve(), passwordCallback, resize;
const events = new AbortController();
const signal = events.signal;
let resolveClosed;
const closed = new Promise(resolve => { resolveClosed = resolve; });
const session = { destroy() {
  if (disposed) return;
  disposed = true; generation++; search?.destroy();
  events.abort(); resize?.disconnect(); observer.disconnect();
  rendering?.cancel(); textLayer?.cancel();
  documentTask?.destroy().catch(() => {});
  canvas.width = canvas.height = 0;
  layer.replaceChildren(); passwordCallback = undefined; rendered = undefined;
  if (window.superPlatinumPdf.get(config.id) === session) window.superPlatinumPdf.delete(config.id);
  resolveClosed();
}};
window.superPlatinumPdf.set(config.id, session);
const observer = new MutationObserver(() => { if (!root.isConnected) session.destroy(); });
observer.observe(document.body, {childList:true,subtree:true});
const status = text => { controls('status').textContent = text; };
const active = g => !disposed && root.isConnected && g === generation;
function buttons() {
  controls('previous').disabled = !pdf || pageNumber === 1;
  controls('next').disabled = !pdf || pageNumber === pdf.numPages;
  for (const name of ['page','zoom','rotate']) controls(name).disabled = !pdf;
  controls('page').value = pageNumber;
  if (pdf) { controls('page').max = pdf.numPages; controls('count').textContent = `of ${pdf.numPages}`; }
}
let library;
function render() {
  if (!pdf || disposed) return;
  const g = ++generation, number = pageNumber;
  rendering?.cancel(); textLayer?.cancel(); buttons(); status(`Loading page ${number}…`);
  root.dataset.ready = 'false'; delete root.dataset.error;
  renderQueue = renderQueue.catch(() => {}).then(async () => {
    if (!active(g)) return;
    const page = await pdf.getPage(number);
    if (!active(g)) return;
    pageObject?.cleanup(); pageObject = page;
    const base = page.getViewport({ scale:1, rotation:(page.rotate + rotation) % 360 });
    const scale = Math.max(0.1, (scroll.clientWidth - 24) / base.width) * zoom;
    const viewport = page.getViewport({scale,rotation:(page.rotate + rotation) % 360});
    const ratio = Math.max(0.1,Math.min(devicePixelRatio || 1,2,Math.sqrt(16_000_000 / (viewport.width * viewport.height))));
    canvas.width = Math.max(1,Math.ceil(viewport.width * ratio));
    canvas.height = Math.max(1,Math.ceil(viewport.height * ratio));
    canvas.style.width = sheet.style.width = `${viewport.width}px`;
    canvas.style.height = sheet.style.height = `${viewport.height}px`;
    layer.replaceChildren();
    sheet.style.setProperty('--total-scale-factor',scale);
    rendering = page.render({canvasContext:canvas.getContext('2d'),viewport,transform:[ratio,0,0,ratio,0,0],background:'rgb(255,255,255)'});
    await rendering.promise;
    if (!active(g)) return;
    const text = await page.getTextContent();
    if (!active(g)) return;
    textLayer = new library.TextLayer({textContentSource:text,container:layer,viewport});
    await textLayer.render();
    if (!active(g)) return;
    root.dataset.page = number; root.dataset.pages = pdf.numPages;
    root.dataset.ready = 'true'; status(`Page ${number} of ${pdf.numPages}`);
    rendered = {page:number,items:text.items,textLayer};search?.rendered();
  }).catch(error => {
    if (active(g) && error?.name !== 'RenderingCancelledException' && error?.name !== 'AbortException') {
      status('This page could not be rendered. Try another page or download the original.');
      root.dataset.error = 'render';
    }
  });
}
controls('previous').addEventListener('click',()=>{if(pdf&&pageNumber>1){pageNumber--;scroll.scrollTo(0,0);render();}},{signal});
controls('next').addEventListener('click',()=>{if(pdf&&pageNumber<pdf.numPages){pageNumber++;scroll.scrollTo(0,0);render();}},{signal});
controls('page').addEventListener('change',()=>{if(!pdf)return;const n=Number(controls('page').value);if(Number.isInteger(n)&&n>=1&&n<=pdf.numPages){pageNumber=n;scroll.scrollTo(0,0);render();}else buttons();},{signal});
controls('zoom').addEventListener('change',()=>{zoom=Number(controls('zoom').value);render();},{signal});
controls('rotate').addEventListener('click',()=>{rotation=(rotation+90)%360;render();},{signal});
controls('password-form').addEventListener('submit',event=>{event.preventDefault();const input=controls('password');const value=input.value;input.value='';controls('password-form').hidden=true;status('Unlocking PDF…');passwordCallback?.(value);},{signal});
try {
  const searchModule = await import('/pdf/search.mjs');
  if (!disposed) search = searchModule.createPdfSearch({root,signal,getDocument:()=>pdf,getRendered:()=>root.dataset.ready === 'true' ? rendered : undefined,showPage:number=>{pageNumber=number;scroll.scrollTo(0,0);render();}});
  library = await import('/pdf/pdf.mjs');
  if (!disposed) {
    library.GlobalWorkerOptions.workerSrc = new URL('/pdf/pdf.worker.mjs',location.href).href;
    const response = await fetch(config.source,{cache:'no-store',signal});
    if (!response.ok) throw Error('unavailable');
    const data = new Uint8Array(await response.arrayBuffer());
    if (!disposed) {
      documentTask = library.getDocument({data,isEvalSupported:false,enableScripting:false,enableXfa:false,useWorkerFetch:false,cMapUrl:new URL('/pdf/cmaps/',location.href).href,cMapPacked:true,standardFontDataUrl:new URL('/pdf/standard_fonts/',location.href).href,wasmUrl:new URL('/pdf/wasm/',location.href).href});
      documentTask.onPassword = (update,reason)=>{
        if(disposed)return;
        passwordCallback=update;
        controls('password-form').hidden=false;
        status(reason===library.PasswordResponses.INCORRECT_PASSWORD?'Incorrect password. Try again.':'This PDF requires a password.');
      };
      pdf = await documentTask.promise;
      root.dataset.worker = documentTask._worker?.port instanceof Worker ? 'thread' : 'fallback';
      if(!disposed) {
        let width=scroll.clientWidth;
        resize=new ResizeObserver(()=>{const next=scroll.clientWidth;if(Math.abs(width-next)>1){width=next;render();}});
        search.loaded();resize.observe(scroll); render();
      }
    }
  }
} catch(error) {
  if(!disposed){status(config.debug?String(error?.stack||error):'This PDF could not be opened. Download the original to try another viewer.');root.dataset.error='open';}
}
await closed;
dioxus.send({closed:true});
