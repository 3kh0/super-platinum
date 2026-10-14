import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source=fs.readFileSync(new URL('../src/desktop/read_preferences/observe.js',import.meta.url),'utf8');
function harness() {
  let now=0,next=0;
  const timers=new Map(),events=new Map(),messages=[];
  const rect=(top,bottom,left=0,right=600)=>({top,bottom,left,right,height:bottom-top,width:right-left});
  const element=(bounds,parent=null)=>({
    bounds,parentElement:parent,isConnected:true,dataset:{},hidden:false,
    style:{display:'block',visibility:'visible',opacity:'1',overflowX:'visible',overflowY:'visible'},
    getBoundingClientRect(){return this.bounds;},getClientRects(){return this.hidden?[]:[this.bounds];},
    getAttribute(name){return name==='aria-hidden'&&this.ariaHidden?'true':null;},
  });
  const ancestor=element(rect(50,450));
  ancestor.style.overflowY='hidden';
  const root=element(rect(0,500),ancestor),divider=element(rect(100,124),root),row=element(rect(80,150),root);
  root.dataset={timelineScope:'scope-1',readKey:'key-1',readGhost:'false',scrollPending:'false'};
  root.scrollHeight=1000;root.scrollTop=500;root.clientHeight=500;
  row.dataset.rowId='100.000001';
  root.querySelector=()=>divider;
  root.querySelectorAll=()=>[row];
  function bind(target){target.addEventListener=(type,fn)=>events.set(`${target.name}:${type}`,fn);target.removeEventListener=(type)=>events.delete(`${target.name}:${type}`);}
  root.name='root';bind(root);
  const document={name:'document',visibilityState:'visible',focused:true,hasFocus(){return this.focused;},getElementById:()=>root};bind(document);
  const window={name:'window'};bind(window);
  const timeout=(fn,delay)=>{const id=++next;timers.set(id,{fn,at:now+delay});return id;};
  class MutationObserver {constructor(fn){this.fn=fn;h.mutation=this;}observe(){}disconnect(){this.disconnected=true;}}
  class ResizeObserver {constructor(fn){this.fn=fn;h.resize=this;}observe(){}unobserve(){}disconnect(){this.disconnected=true;}}
  const h={root,divider,row,ancestor,document,window,messages,timers,rect,
    event(target,type){events.get(`${target}:${type}`)?.();},
    advance(ms){const end=now+ms;for(;;){const due=[...timers].filter(([,x])=>x.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!due)break;timers.delete(due[0]);now=due[1].at;due[1].fn();}now=end;},
    last(){return messages.at(-1);},
  };
  const context={window,document,innerWidth:800,innerHeight:600,getComputedStyle:e=>e.style,
    setTimeout:timeout,clearTimeout:id=>timers.delete(id),requestAnimationFrame:fn=>timeout(fn,16),cancelAnimationFrame:id=>timers.delete(id),
    MutationObserver,ResizeObserver,dioxus:{send:v=>messages.push(v)},console};
  vm.runInNewContext(source,context);
  return h;
}

test('350 ms of unchanged real visibility is required, with the exact scope/key',()=>{
  const h=harness();assert.equal(h.last().stable,false);assert.equal(h.last().dividerVisible,true);
  h.advance(349);assert.equal(h.last().stable,false);
  h.advance(1);assert.equal(h.last().stable,true);assert.equal(h.last().scope,'scope-1');
  assert.equal(h.last().key,'key-1');assert.equal(h.last().firstId,'100.000001');assert.equal(h.last().offset,80);
});

test('queued completion rechecks navigation and restarts the 350 ms interval',()=>{
  const h=harness();h.advance(300);h.root.dataset.timelineScope='scope-2';h.root.dataset.readKey='key-2';
  h.advance(50);assert.equal(h.last().scope,'scope-2');assert.equal(h.last().stable,false);
  h.advance(349);assert.equal(h.last().stable,false);h.advance(1);assert.equal(h.last().stable,true);
});

test('focus loss revokes stable attention synchronously and focus regain waits again',()=>{
  const h=harness();h.advance(350);h.document.focused=false;h.event('window','blur');
  assert.equal(h.last().visible,false);assert.equal(h.last().stable,false);
  h.document.focused=true;h.event('window','focus');h.advance(349);assert.equal(h.last().stable,false);
  h.advance(1);assert.equal(h.last().stable,true);
});

test('a divider clipped by an ancestor or outside the native viewport is not visible',()=>{
  const h=harness();h.divider.bounds=h.rect(30,54);h.mutation.fn();h.advance(16);
  assert.equal(h.last().visible,true);assert.equal(h.last().dividerVisible,false);
  h.divider.bounds=h.rect(420,460);h.resize.fn();h.advance(16);assert.equal(h.last().dividerVisible,false);
  h.divider.bounds=h.rect(100,124);h.resize.fn();h.advance(16);assert.equal(h.last().dividerVisible,true);
});

test('ghost identity and pending navigation survive the bridge instead of becoming visibility authority',()=>{
  const h=harness();h.root.dataset.readGhost='true';h.root.dataset.scrollPending='true';h.mutation.fn();h.advance(366);
  assert.equal(h.last().ghost,true);assert.equal(h.last().pending,true);
  assert.equal(h.last().dividerVisible,true,'geometry and read permission remain separate');
});

test('hidden divider and zero-size viewport cannot prove it was seen',()=>{
  const h=harness();h.divider.style.visibility='hidden';h.resize.fn();h.advance(16);assert.equal(h.last().dividerVisible,false);
  h.ancestor.bounds=h.rect(50,50);h.resize.fn();h.advance(16);assert.equal(h.last().visible,false);assert.equal(h.last().firstId,null);
});

test('scrollmark position updates without extending an unchanged visibility debounce',()=>{
  const h=harness();h.advance(300);h.row.bounds=h.rect(60,130);h.event('root','scroll');h.advance(16);
  assert.equal(h.last().offset,60);assert.equal(h.last().stable,false);h.advance(34);assert.equal(h.last().stable,true);
});

test('disposing a timeline ends its native receiver and cancels delayed permission',()=>{
  const h=harness();h.window.superPlatinumReadVisibility.dispose();assert.equal(h.last(),null);
  h.advance(1000);assert.equal(h.last(),null);assert.equal(h.timers.size,0);
  assert.equal(h.mutation.disconnected,true);assert.equal(h.resize.disconnected,true);
});
