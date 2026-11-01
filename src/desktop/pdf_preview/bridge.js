// Shared local PDF.js canvas/text renderer. A model loader is explicit and never native proof.
async function runPdfPreview(config, environment = globalThis, bridge, loadModule = path => import(path)) {
const {document,window,MutationObserver,ResizeObserver,AbortController,URL,location,fetch,Worker,devicePixelRatio} = environment;
// Only bundled PDF.js and current-viewer bytes enter this renderer. No document
// actions, links, annotations, HTML, attachments or scripting are activated.
const root = document.getElementById(config.id);
const isolated = config.isolated === true;
if (!root) { bridge.send(isolated ? {kind:'failed',id:config.id,nonce:config.nonce} : {closed:true}); return; }
if (isolated && (!/^[A-Za-z0-9_-]{1,128}$/.test(config.nonce) ||
    !/^[A-Za-z0-9_-]+$/.test(config.opaque) ||
    config.source !== `/pdf/search-data/${config.nonce}/${config.opaque}`)) {
  bridge.send({kind:'failed',id:config.id,nonce:config.nonce}); return;
}
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
  if (isolated) {
    for (const name of ['previous','next','page','zoom','rotate','find','find-previous','find-next','find-clear']) controls(name).disabled = true;
    controls('password-form').hidden = true;
    root.dataset.ready = 'false';
  }
  events.abort(); resize?.disconnect(); observer.disconnect();
  rendering?.cancel(); textLayer?.cancel();
  documentTask?.destroy().catch(() => {});
  canvas.width = canvas.height = 0;
  layer.replaceChildren(); passwordCallback = undefined; rendered = undefined;
  if (window.superPlatinumPdf.get(config.id) === session) window.superPlatinumPdf.delete(config.id);
  resolveClosed();
}};
window.superPlatinumPdf.set(config.id, session);
const current = () => !disposed && root.isConnected && document.getElementById(config.id) === root && window.superPlatinumPdf.get(config.id) === session;
const observer = new MutationObserver(() => { if (!current()) session.destroy(); });
observer.observe(document.body, {childList:true,subtree:true});
const status = text => { controls('status').textContent = text; };
const active = g => current() && g === generation;
function buttons() {
  controls('previous').disabled = !pdf || pageNumber === 1;
  controls('next').disabled = !pdf || pageNumber === pdf.numPages;
  for (const name of ['page','zoom','rotate']) controls(name).disabled = !pdf;
  controls('page').value = pageNumber;
  if (pdf) { controls('page').max = pdf.numPages; controls('count').textContent = `of ${pdf.numPages}`; }
}
let library;
let failureSent = false;
function failed() {
  if (isolated && current() && !failureSent) {
    failureSent = true;
    try { bridge.send({kind:'failed',id:config.id,nonce:config.nonce}); }
    finally { session.destroy(); }
  }
}
function render() {
  if (!pdf || !current()) return;
  const g = ++generation, number = pageNumber;
  rendering?.cancel(); textLayer?.cancel(); buttons(); status(`Loading page ${number}…`);
  root.dataset.ready = 'false'; delete root.dataset.error;
  renderQueue = renderQueue.catch(() => {}).then(async () => {
    if (!active(g)) return;
    const page = await pdf.getPage(number);
    if (!active(g)) return;
    pageObject?.cleanup(); pageObject = page;
    const base = page.getViewport({ scale:1, rotation:(page.rotate + rotation) % 360 });
    const positive = value => Number.isFinite(value) && value > 0;
    if (isolated && (![base.width,base.height,scroll.clientWidth,zoom].every(positive))) throw Error('unsupported page geometry');
    const scale = Math.max(0.1, (scroll.clientWidth - 24) / base.width) * zoom;
    if (isolated && !positive(scale)) throw Error('unsupported page scale');
    const viewport = page.getViewport({scale,rotation:(page.rotate + rotation) % 360});
    let ratio, width, height;
    if (isolated) {
      // Defensive native presentation limits, not a Source PDF geometry grant.
      // Check CSS geometry before assigning it or asking canvas to allocate.
      const maxDimension = 16_384, maxPixels = 16_000_000;
      const area = viewport.width * viewport.height;
      if (![viewport.width,viewport.height,area,devicePixelRatio].every(positive) ||
          viewport.width > maxDimension || viewport.height > maxDimension) throw Error('unsupported page geometry');
      ratio = Math.min(devicePixelRatio,2,maxDimension / viewport.width,maxDimension / viewport.height,Math.sqrt(maxPixels / area));
      width = Math.ceil(viewport.width * ratio);
      height = Math.ceil(viewport.height * ratio);
      // Ceil can exceed the area budget even when the continuous ratio fits.
      // Reduce by at least one integer pixel on both axes, then validate again.
      if (width * height > maxPixels || width > maxDimension || height > maxDimension) {
        ratio = Math.min(ratio,(width - 1) / viewport.width,(height - 1) / viewport.height) * (1 - Number.EPSILON);
        width = Math.ceil(viewport.width * ratio);
        height = Math.ceil(viewport.height * ratio);
      }
      if (!positive(ratio) || !Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
          width < 1 || height < 1 || width > maxDimension || height > maxDimension || width * height > maxPixels) throw Error('unsupported canvas geometry');
    } else {
      // Preserve the legacy viewer's original geometry and ratio behavior.
      ratio = Math.max(0.1,Math.min(devicePixelRatio || 1,2,Math.sqrt(16_000_000 / (viewport.width * viewport.height))));
      width = Math.max(1,Math.ceil(viewport.width * ratio));
      height = Math.max(1,Math.ceil(viewport.height * ratio));
    }
    // Reset first so changing aspect ratio cannot temporarily allocate the new
    // width against a previous page's tall canvas height.
    if (isolated) { canvas.width = 0; canvas.height = 0; }
    canvas.width = width;
    canvas.height = height;
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
      status(isolated ? 'This page could not be rendered.' : 'This page could not be rendered. Try another page or download the original.');
      root.dataset.error = 'render'; failed();
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
  const searchModule = await loadModule('/pdf/search.mjs');
  if (current()) search = searchModule.createPdfSearch({root,signal,getDocument:()=>pdf,getRendered:()=>root.dataset.ready === 'true' ? rendered : undefined,showPage:number=>{pageNumber=number;scroll.scrollTo(0,0);render();}});
  library = await loadModule('/pdf/pdf.mjs');
  if (current()) {
    library.GlobalWorkerOptions.workerSrc = new URL('/pdf/pdf.worker.mjs',location.href).href;
    const response = await fetch(config.source,{cache:'no-store',signal});
    if (!response.ok) throw Error('unavailable');
    const data = new Uint8Array(await response.arrayBuffer());
    if (current()) {
      documentTask = library.getDocument({data,isEvalSupported:false,enableScripting:false,enableXfa:false,useWorkerFetch:false,cMapUrl:new URL('/pdf/cmaps/',location.href).href,cMapPacked:true,standardFontDataUrl:new URL('/pdf/standard_fonts/',location.href).href,wasmUrl:new URL('/pdf/wasm/',location.href).href});
      documentTask.onPassword = (update,reason)=>{
        if(!current())return;
        passwordCallback=update;
        controls('password-form').hidden=false;
        status(reason===library.PasswordResponses.INCORRECT_PASSWORD?'Incorrect password. Try again.':'This PDF requires a password.');
      };
      const decoded = await documentTask.promise;
      if (!current()) { session.destroy(); return; }
      if (isolated) {
        if (!Number.isSafeInteger(decoded.numPages) || decoded.numPages < 1 || decoded.numPages > 200) throw Error('page limit');
        bridge.send({kind:'decoded',id:config.id,nonce:config.nonce,pages:decoded.numPages});
        const reply = await Promise.race([bridge.recv(),closed.then(()=>null)]);
        if (!current()) { session.destroy(); return; }
        if (!reply || Object.keys(reply).sort().join(',') !== 'id,kind,nonce,pages' ||
            reply.kind !== 'admit' || reply.id !== config.id || reply.nonce !== config.nonce || reply.pages !== decoded.numPages) {
          failed(); return;
        }
      }
      pdf = decoded;
      root.dataset.worker = documentTask._worker?.port instanceof Worker ? 'thread' : 'fallback';
      if(current()) {
        let width=scroll.clientWidth;
        resize=new ResizeObserver(()=>{const next=scroll.clientWidth;if(Math.abs(width-next)>1){width=next;render();}});
        search.loaded();resize.observe(scroll); render();
      }
    }
  }
} catch(error) {
  if(current()){status(isolated ? 'This PDF could not be opened.' : config.debug?String(error?.stack||error):'This PDF could not be opened. Download the original to try another viewer.');root.dataset.error='open';failed();}
}
await closed;
bridge.send(isolated ? {kind:'closed',id:config.id,nonce:config.nonce} : {closed:true});

}
if (typeof module !== 'undefined' && module.exports) module.exports = {runPdfPreview};
