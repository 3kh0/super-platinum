// Actual checker execution: parent-return must observe restored focus, not scroll Add.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../src/desktop/slack_lists/check/subitem.js'),'utf8');
function setup({clipped=false,labelClipped=false}={}){
 const nodes=new Map();let addScrolls=0;
 const make=(id,bounds)=>({id,dataset:{},disabled:false,clientLeft:0,clientTop:0,clientWidth:bounds[2]-bounds[0],clientHeight:bounds[3]-bounds[1],scrollWidth:bounds[2]-bounds[0],
  getBoundingClientRect(){return {left:bounds[0],top:bounds[1],right:bounds[2],bottom:bounds[3],width:bounds[2]-bounds[0],height:bounds[3]-bounds[1]}},closest(){return null},contains(e){return e===this},querySelector(){return null},querySelectorAll(){return []},scrollIntoView(){},classList:{contains(){return false}}});
 const root=make('root',[0,0,500,400]),detail=make('slack-list-record-detail',[10,60,490,390]),back=make('slack-list-record-back',[20,70,120,102]),add=make('slack-list-subitem-add',[350,130,470,162]),child=make('child',[20,300,470,332]),label=make('label',labelClipped?[50,305,400,350]:[50,305,400,325]);
 const scroll=make('scroll',[10,120,490,clipped?320:370]),rows=make('rows',[20,180,470,350]),section=make('slack-list-subitems',[20,130,470,350]);
 const picker=make('slack-list-view-select',[20,10,200,42]);picker.value='V_CUSTOM';picker.selectedOptions=[{textContent:'Project delivery'}];
 child.dataset.subitemId='R_SERVER_SUBITEM';detail.dataset.recordId='R_HYDRATE_PARENT';detail.dataset.detailOpenGeneration='5';
 child.querySelector=()=>label;child.contains=e=>e===child||e===label;child.closest=s=>s==='.slack-list-detail-scroll'?scroll:s==='.slack-list-subitems-rows'?rows:null;
 detail.contains=e=>[child,back,add,label].includes(e);section.querySelectorAll=()=>[add];add.scrollIntoView=()=>addScrolls++;
 for(const e of [detail,back,add,picker,section])nodes.set(e.id,e);
 const document={activeElement:child,getElementById:id=>nodes.get(id)||null,querySelector:s=>s==='.slack-list-dialog'?root:null,
  querySelectorAll:s=>s==='.slack-list-warning'?[{textContent:'Only part is loaded'}]:s==='[data-subitem-id]'?[child]:[],
  elementFromPoint(x,y){return [back,add,child].find(e=>{const r=e.getBoundingClientRect();return x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom})||root;}};
 const window={};vm.runInNewContext(source,{document,window,innerWidth:500,innerHeight:400});
 return {run:()=>window.__listSubitem.geometry('parent-return'),scrolls:()=>addScrolls,document,child};
}
test('parent-return observes exact restored child without scrolling Add or changing focus',()=>{const h=setup();const result=h.run();assert.equal(result.ok,true);assert.equal(h.scrolls(),0);assert.equal(h.document.activeElement,h.child);assert.equal(result.focusedRow.id,'R_SERVER_SUBITEM');assert.equal(result.focusedRow.fullyVisible,true);assert.equal(result.focusedRow.labelVisible,true)});
test('a focused row partly below its scrollport fails even when viewport hit testing passes',()=>{const h=setup({clipped:true});const r=h.run();assert.equal(r.focusedRow.hit,true);assert.equal(r.focusedRow.fullyVisible,false);assert.equal(r.ok,false);assert.equal(h.scrolls(),0)});
test('a label overflowing the focused row fails independently of row geometry',()=>{const h=setup({labelClipped:true});const r=h.run();assert.equal(r.focusedRow.fullyVisible,true);assert.equal(r.focusedRow.labelVisible,false);assert.equal(r.ok,false)});
