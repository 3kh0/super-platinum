const assert=require("node:assert/strict"),fs=require("node:fs"),vm=require("node:vm");
const source=fs.readFileSync(__dirname+"/bridge.js","utf8");
async function fixture() {
  const handlers=new Map(),nodes=new Map(),sent=[];let observer,finish;
  class Node {
    constructor(id,parent=null,dataset={}) {this.id=id;this.parent=parent;this.dataset=dataset;this.disabled=false;this.isConnected=true;this.style={};this.attrs={};nodes.set(id,this);}
    contains(node) {for(let n=node;n;n=n.parent)if(n===this)return true;return false;}
    closest(selector) {for(let n=this;n;n=n.parent) {if(selector==="form"&&n.id==="workflow-activity-filters")return n;if(selector==="[data-date]"&&n.dataset.date)return n;if(selector==="[data-calendar-action]"&&n.dataset.calendarAction)return n;}return null;}
    matches() {return false;}
    getAttribute(key) {return this.attrs[key]??null;}
    getClientRects() {return [];}
    querySelector(selector) {if(selector.startsWith("#"))return nodes.get(selector.slice(1));return [...nodes.values()].find(n=>selector===`[data-date="${n.dataset.date}"]`);}
    focus() {dispatch("focusin",this);}
  }
  function dispatch(type,target,more={}) {const event={type,target,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...more};for(const f of handlers.get(type)||[])f(event);return event;}
  const form=new Node("workflow-activity-filters"),panel=new Node("workflow-activity-filter-panel");
  form.parent=panel;
  const filter=new Node("workflow-activity-filter");filter.attrs["aria-expanded"]="true";
  const button=new Node("workflow-activity-date-range",form,{visit:"1",document:"2",revision:"3",calendarEpoch:"4",calendarAck:"0",calendarPending:"false",calendarOpen:"true",calendarReadBusy:"false"});
  const calendar=new Node("workflow-activity-calendar",form,{focus:"0099-01-02"});
  const day=new Node("day",calendar,{calendarAction:"day",date:"0099-01-01"});
  const save=new Node("workflow-activity-calendar-save",calendar,{calendarAction:"save"});
  const cancel=new Node("workflow-activity-calendar-cancel",calendar,{calendarAction:"cancel"});
  for(const [name,value]of Object.entries({status:"ERROR",limit:"50",user:"U1","user-search":"Uncommitted search"}))new Node("workflow-activity-"+name,form).value=value;
  const add=(t,f)=>handlers.set(t,[...(handlers.get(t)||[]),f]);
  const remove=(t,f)=>handlers.set(t,(handlers.get(t)||[]).filter(x=>x!==f));
  const context=vm.createContext({Element:Node,document:{documentElement:{lang:"en-US"},getElementById:id=>nodes.get(id),addEventListener:add,removeEventListener:remove},
    window:{addEventListener:add,removeEventListener:remove},navigator:{language:"en-US"},Date,Intl,
    MutationObserver:class{constructor(f){observer=f;}observe(){}disconnect(){}},innerWidth:800,innerHeight:600});
  vm.runInContext(source,context);
  const work=context.workflowCalendarBridge({visit:"1",document:"2"},{send:v=>{sent.push(JSON.parse(JSON.stringify(v)));return Promise.resolve();},recv:()=>new Promise(r=>finish=r)});
  await Promise.resolve();await Promise.resolve();
  return {sent,button,filter,day,save,cancel,calendar,dispatch,ack(seq,epoch="5"){button.dataset.calendarAck=String(seq);button.dataset.calendarEpoch=epoch;observer();},async close(){finish();await work;},nodes};
}
(async()=>{
  let count=0;
  {const f=await fixture();f.dispatch("pointerover",f.day);f.dispatch("click",f.day);
    assert.deepEqual(f.sent.filter(x=>x.action).map(x=>x.action),["hover","day"]);
    assert.equal(f.sent[1].seq,0);assert.equal(f.sent[2].seq,1);assert.equal(f.button.dataset.calendarPending,"true");await f.close();count++;}
  {const f=await fixture();f.button.dataset.calendarReadBusy="true";f.dispatch("click",f.day);
    assert.equal(f.sent.length,1);assert.equal(f.button.dataset.calendarPending,"false");
    f.button.dataset.calendarReadBusy="false";f.button.disabled=true;f.dispatch("click",f.day);assert.equal(f.sent.length,1);
    f.button.disabled=false;f.dispatch("click",f.day);assert.equal(f.sent[1].action,"day");await f.close();count++;}
  {const f=await fixture();f.dispatch("click",f.day);f.dispatch("click",f.day);assert.equal(f.sent.length,2);
    f.ack(0);assert.equal(f.button.dataset.calendarPending,"true");
    f.ack(1);assert.equal(f.button.dataset.calendarPending,"false");await f.close();count++;}
  {const f=await fixture();f.dispatch("keydown",f.day,{key:"ArrowRight"});
    assert.equal(f.sent[1].value,"0099-01-02");assert.equal(f.sent[1].action,"move");await f.close();count++;}
  {const f=await fixture();f.dispatch("click",f.save);
    assert.deepEqual(f.sent[1].buffers,{status:"ERROR",limit:"50",user:"U1",search:"Uncommitted search"});await f.close();count++;}
  {const f=await fixture();f.dispatch("click",f.cancel);f.button.dataset.calendarOpen="false";f.ack(1);
    assert.equal(f.sent.filter(x=>x.action).length,1,"Cancel focus restoration must not reopen");
    f.dispatch("focusin",f.button);assert.equal(f.sent.at(-1).action,"open");await f.close();count++;}
  {const f=await fixture();f.nodes.set(f.button.id,{...f.button});f.dispatch("click",f.day);assert.equal(f.sent.length,1);await f.close();count++;}
  {const f=await fixture();f.dispatch("click",f.day);
    assert.deepEqual({epoch:f.sent[1].epoch,revision:f.sent[1].revision,visit:f.sent[1].visit,document:f.sent[1].document},{epoch:"4",revision:"3",visit:"1",document:"2"});
    f.ack(1,"8");f.dispatch("click",f.day);assert.equal(f.sent[2].epoch,"8");assert.equal(f.sent[2].seq,2);
    await f.close();const countBefore=f.sent.length;f.dispatch("click",f.day);assert.equal(f.sent.length,countBefore);count++;}
  console.log(JSON.stringify({ok:true,tests:count,source:"date_range/bridge.js"}));
})().catch(e=>{console.error(e);process.exitCode=1;});
