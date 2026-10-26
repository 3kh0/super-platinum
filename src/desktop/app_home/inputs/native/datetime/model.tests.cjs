// Actual production model against frozen original rBW6/SOLS vectors. No DOM,
// server, OS input or browser timezone acquisition claims.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),cp=require('node:child_process');
const read=p=>fs.readFileSync(path.join(__dirname,p),'utf8');
if(process.argv.includes('--zone')){
 const file=path.join(process.cwd(),'tmp/app-home-datetime-dispatch-20261025/reference',`composite-vectors-${process.env.TZ.replaceAll('/','_')}.json`);
 const vectors=JSON.parse(fs.readFileSync(file,'utf8')),stamp=Date.parse(vectors.fixedNow),RealDate=Date;
 class FixedDate extends RealDate{constructor(...args){super(...(args.length?args:[stamp]));}static now(){return stamp;}}
 const ctx=vm.createContext({Date:FixedDate});
 vm.runInContext(`const timeDayjs=${read('../time/vendor/dayjs.js')};timeDayjs.extend(${read('../time/vendor/custom-parse-format.js')});${read('model.js')};globalThis.model=homeDatetimeModel();`,ctx);
 for(const row of vectors.results){
  const out=ctx.model.combine(row.props.value,row.method==='handleDateChange'?'date':'time',row.arg,row.props.isRequired===false);
  const expected=row.out[0]===''?null:(Number(row.out[0])||null);
  assert.equal(out,expected,row.name);
 }
 assert.deepEqual(JSON.parse(JSON.stringify(ctx.model.parts(-1))),{date:null,time:null});
 assert.deepEqual(JSON.parse(JSON.stringify(ctx.model.parts(0))),{date:null,time:null});
 const fallback=ctx.model.change(-1,'time','17:45',true);
 assert.equal(fallback,Number(vectors.results.find(x=>x.name==='time-change-').out[0]));
 process.stdout.write(JSON.stringify({ok:true,count:vectors.results.length,zone:process.env.TZ}));
}else{
 for(const zone of ['UTC','America/New_York'])test(`original composite arithmetic and wrapper display: ${zone}`,()=>{
  const r=cp.spawnSync(process.execPath,[__filename,'--zone'],{encoding:'utf8',env:{...process.env,TZ:zone}});
  assert.equal(r.status,0,r.stderr||r.stdout);const out=JSON.parse(r.stdout);assert.equal(out.count,24);assert.equal(out.ok,true);
 });
}
