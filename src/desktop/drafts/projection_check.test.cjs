const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');
function fixture(){
 let wire='Original <@U1> \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}',count=0;
 class Textarea {}
 Object.defineProperty(Textarea.prototype,'value',{get(){return wire.replace('<@U1>','Maya Chen');}});
 const e=new Textarea();
 Object.assign(e,{isConnected:true,dataset:{draftScope:'scope',projectionVisit:'visit'},selectionStart:0,selectionEnd:0,selectionDirection:'none',focus(){},scrollIntoView(){},setSelectionRange(s,n,d){this.selectionStart=s;this.selectionEnd=n;this.selectionDirection=d},getBoundingClientRect(){return {left:1,top:1,right:301,bottom:33,width:300,height:32}},dispatchEvent(event){if(event.type==='input')count++;if(event.type==='compositionstart')this.dataset.composing='true';if(event.type==='compositionend')this.dataset.composing='false';}});
 Object.defineProperty(e,'value',{get(){return wire;},set(v){wire=v;}});
 const Event=class{constructor(type,options){this.type=type;Object.assign(this,options)}};
 const context=vm.createContext({document:{getElementById(){return e}},HTMLTextAreaElement:Textarea,InputEvent:Event,CompositionEvent:Event,innerWidth:668,innerHeight:380,queueMicrotask});
 vm.runInContext(fs.readFileSync(__dirname+'/projection_check.js','utf8'),context);
 return {e,count:()=>count,run:(op,args={})=>context.draftProjectionCheck(op,{id:'channel-composer',...args})};
}
test('native-ahead setup uses instance wire setter and never fabricates input',async()=>{const f=fixture();const r=await f.run('set',{text:'New <@U1> \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}',input:false});assert.equal(r.text,'New <@U1> \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}');assert.notEqual(r.display,r.text);assert.equal(r.direction,'backward');assert.equal(r.end,r.text.length);assert.equal(r.text.codePointAt(r.start),0x1f680);assert.ok(r.codePoints.includes(0x65e5));assert.ok(r.codePoints.includes(0x301));assert.equal(r.utf16Length,r.codePoints.length+1);assert.equal(f.count(),0)});
test('queued input resolves only after synchronous event delivery',async()=>{const f=fixture();const r=await f.run('queued-input',{text:'New <@U1> \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}',input:true});assert.equal(f.count(),1);assert.equal(r.text,'New <@U1> \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}')});
test('synthetic composition ordering is explicit and does not invent local events',async()=>{const f=fixture();assert.equal((await f.run('composition-start')).composing,true);assert.equal((await f.run('composition-end')).composing,false);assert.equal(f.count(),0)});
test('missing mounted reference accessor or disconnected node is unavailable',async()=>{const f=fixture();f.e.isConnected=false;assert.equal((await f.run('ready')).ok,false)});
