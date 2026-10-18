"""Pure receipt rejection tests; imports never launch a process."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

R=runpy.run_path(str(Path(__file__).with_name('check-profile-group-completion-native.py')))
M=runpy.run_path(str(Path(__file__).with_name('test_profile_member_completion_native.py')))

def convert(value):
    if isinstance(value,list):return [convert(v) for v in value]
    if not isinstance(value,dict):return value
    result={k:convert(v) for k,v in value.items()}
    if result.get('type')=='user' and result.get('user_id') in ('UCOMPLETE1','UCOMPLETE2'):
        result['type']='usergroup';result['usergroup_id']={'UCOMPLETE1':R['FIRST'],'UCOMPLETE2':R['SECOND']}[result.pop('user_id')]
    if isinstance(result.get('blocks'),str):result['blocks']=json.dumps(convert(json.loads(result['blocks'])))
    return result

def augment_source(source,restricted=False):
    requests=[]
    for row in source['requests']:
        row=copy.deepcopy(row);requests.append(row)
        if row['method']=='users/search':
            q={'harbor':'cafe','zz-no-matching-person':'zz-no-matching-group'}.get(row['args']['query'],row['args']['query']);row['args']['query']=q
            if not restricted:requests.append(dict(method='usergroups/search',args=dict(query=q,count=25,org_wide=True)))
    for i,row in enumerate(requests,1):row['serial']=i
    source['requests']=requests;members=sum(r['method']=='users/search' for r in requests);groups=sum(r['method']=='usergroups/search' for r in requests)
    source.update(memberSearches=members,groupSearches=groups,searches=members+groups,completed=members+groups-source['active'])

def snapshot_rows():
    rows=[]
    for gid,handle,deleted in [(R['FIRST'],'Alex Harbor',0),(R['SECOND'],'@Alex Cafe\u0301',0),(R['DELETED'],'Alex Deleted Group',123)]:
        source=dict(id=gid,handle=handle,date_delete=deleted,fixture_opaque=dict(preserve=gid,ordered=['raw','effective']))
        rows.append(dict(source=source,effective={**source,'is_usergroup':True}))
    return rows

def catalog(labels):
    ids=['UMENTION01','UCOMPLETE1','UCOMPLETE2','UCOMPLETE3','UINIT0001',R['FIRST'],R['SECOND']]
    return dict(sequence=1,revision=0,values=[dict(value=dict(type='user' if i<5 else 'usergroup',**({'user_id':ids[i]} if i<5 else {'usergroup_id':ids[i]})),label=label) for i,label in enumerate(labels)])

def sample(directory,narrow=False,restricted=False):
    fixture,value=M['sample'](directory,narrow);value=convert(value);value['restricted']=restricted
    base=value['stages'];first=copy.deepcopy(base[2]);extra=copy.deepcopy(first);extra['stage']='refreshed-lower'
    base.insert(4,extra)
    labels=[R['LOCAL'],R['SAME'],R['SAME'],'@Alex Retired','@Alex Self',R['SAME'],R['DOUBLE']]
    original=copy.deepcopy(first['geometry']['resultRows'][0]);choices=catalog(labels)
    common=dict(sequence=1,release=dict(delivered=True,cancelledBeforeReply=False),choices=choices,sourceGroups=dict(rows=snapshot_rows(),selfRestricted=False))
    for page in base:
        stage=page['stage'];g=page['geometry'];p=page['proof']
        augment_source(page['source'])
        if stage=='opened':p['initialSource']=dict(rows=[],membership=None,selfRestricted=False)
        if stage=='local':
            p['choices']=catalog(labels[:5]);g['resultRows']=[{**copy.deepcopy(original),'text':label} for label in labels[:5]]
        if stage.startswith('refreshed'):
            page['proof']=copy.deepcopy(common)
            g['resultRows']=[{**copy.deepcopy(original),'text':label} for label in labels]
        if stage in ('keyboard','mouse'):
            gid=R['FIRST'] if stage=='keyboard' else R['SECOND'];index=5 if stage=='keyboard' else 6
            p.pop('user',None);p['group']=gid;p['choice']=dict(id=gid,type='usergroup',index=index,label=labels[index],values=choices['values'],keyboard=stage=='keyboard',sequence=1)
        if stage=='saved':p['reopenedLabel']=R['REOPENED']
        if g.get('native'):
            for i,token in enumerate(g['native']['tokens']):
                if i==3:token['label']=R['DOUBLE'] if stage=='mouse' else R['REOPENED']
        if stage=='empty':p['empty']=True
    augment_source(value['source'])
    if restricted:
        value['stages']=[base[0]]+[copy.deepcopy(base[i]) for i in (2,3,4)]
        value.pop('savedBlocks',None)
        for i,page in enumerate(value['stages']):
            page['stage']=R['SHORT'][i]
            # Start from the original member sample's positive closed ledger.
            member_source=copy.deepcopy(M['sample'](directory,narrow)[1]['stages'][0 if i==0 else 2]['source'])
            augment_source(member_source,True);page['source']=member_source
            if i==0:page['proof']['initialSource']['selfRestricted']=True
            else:
                rows=page['geometry']['resultRows'][:5];page['geometry']['resultRows']=rows
                page['proof']=dict(choices=catalog(labels[:5]),sourceGroups=dict(rows=[],selfRestricted=True),noGroupRequest=True)
        value['source']=copy.deepcopy(value['stages'][-1]['source'])
    for page in value['stages']:
        stage=page['stage'];g=page['geometry'];rows=g.get('resultRows')
        if not rows:continue
        targets={'refreshed':0,'refreshed-middle':2,'refreshed-lower':4,'refreshed-bottom':6,'restricted':0,'restricted-middle':2,'restricted-bottom':4}
        target=targets.get(stage);total=len(rows)*32+8;height=min(total,72 if narrow else 128);top=0 if target is None else min(target*32,total-height)
        for i,row in enumerate(rows):
            y=104+i*32-top;row.update(id=f'option-1-{i}',index=i,rect=dict(left=154,right=414,top=y,bottom=y+32,width=260,height=32),visible=y>=100 and y+32<=100+height)
        g['rows']=[r for r in rows if r['visible']];g['popup']=dict(id='popup-1',sequence=1,scrollTop=top,clientHeight=height,scrollHeight=total,targetIndex=target,rect=dict(left=150,right=420,top=100,bottom=100+height,width=270,height=height))
    value['captures']={p['stage']:str(directory.resolve()/f"{fixture}-{p['stage']}.png") for p in value['stages']}
    value['model']=copy.deepcopy(value['stages'][-1]['model'])
    return fixture,value

class Validator(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.path=Path(self.tmp.name);self.fixture,self.value=sample(self.path)
    def validate(self):R['validate'](self.fixture,self.value,self.path,platform='win32',png=False)
    def reject(self,fn):
        fn(self.value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError,IndexError)):self.validate()
    def test_normal(self):self.validate()
    def test_narrow(self):self.fixture,self.value=sample(self.path,True);self.validate()
    def test_restricted(self):self.fixture,self.value=sample(self.path,restricted=True);self.validate()
    def test_restricted_narrow(self):self.fixture,self.value=sample(self.path,True,True);self.validate()
    def test_wrong_category(self):self.reject(lambda v:v['stages'][6]['proof']['choice']['values'][5]['value'].update(type='user'))
    def test_equal_label_wrong_id(self):self.reject(lambda v:v['stages'][6]['proof']['choice'].__setitem__('index',1))
    def test_nfc_lost(self):self.reject(lambda v:v['stages'][7]['geometry']['native']['tokens'][3].__setitem__('label','@@Alex Cafe\u0301'))
    def test_prefix_removed(self):self.reject(lambda v:v['stages'][7]['geometry']['native']['tokens'][3].__setitem__('label','@Alex Café'))
    def test_existing_label_wrongly_normalized(self):self.reject(lambda v:v['stages'][8]['geometry']['native']['tokens'][3].__setitem__('label','@Alex Café'))
    def test_original_opaque(self):self.reject(lambda v:v['stages'][0]['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_wrong_org_policy(self):self.reject(lambda v:next(r for r in v['stages'][1]['source']['requests'] if r['method']=='usergroups/search')['args'].__setitem__('org_wide',False))
    def test_wrong_count(self):self.reject(lambda v:next(r for r in v['stages'][1]['source']['requests'] if r['method']=='usergroups/search')['args'].__setitem__('count',20))
    def test_missing_source_opaque(self):self.reject(lambda v:v['stages'][2]['proof']['sourceGroups']['rows'][0]['source'].pop('fixture_opaque'))
    def test_changed_effective_handle(self):self.reject(lambda v:v['stages'][2]['proof']['sourceGroups']['rows'][1]['effective'].__setitem__('handle','normalized'))
    def test_source_boot_fabricated(self):self.reject(lambda v:v['stages'][0]['proof']['initialSource']['rows'].append({}))
    def test_scroll_missing(self):self.reject(lambda v:v['stages'].pop(4))
    def test_scroll_identity(self):self.reject(lambda v:v['stages'][3]['geometry']['resultRows'][0].__setitem__('id','other'))
    def test_scroll_target(self):self.reject(lambda v:v['stages'][5]['geometry']['popup'].__setitem__('targetIndex',0))
    def test_late_reply_delivered(self):self.reject(lambda v:v['stages'][9]['proof']['release'].__setitem__('delivered',True))
    def test_failure_hidden(self):self.reject(lambda v:v['stages'][10]['geometry']['notices'][0].__setitem__('visible',False))
    def test_stale_selected(self):self.reject(lambda v:v['stages'][14]['proof'].__setitem__('nextSequence',v['stages'][14]['proof']['previousSequence']))
    def test_readback_changed(self):self.reject(lambda v:v['stages'][8]['model']['attempts'][0][0]['long_text'].__setitem__('blocks','[]'))
    def test_restricted_request(self):
        self.fixture,self.value=sample(self.path,restricted=True)
        self.reject(lambda v:v['stages'][1]['source'].__setitem__('groupSearches',1))
    def test_import_does_not_launch(self):self.assertEqual(R['B']['PREFIX'],'profile-group-completion-check: ')
    def test_actual_js_requires_current_catalog_before_delegating_native_input(self):
        script=r'''
const assert=require('node:assert/strict'),fs=require('node:fs');
const helper=eval(fs.readFileSync('src/desktop/profile_edit/sections/fixture/group_completion_check.js','utf8'));
const editor={isContentEditable:true},popup={dataset:{querySequence:'7'}};
const values=[{value:{type:'user',user_id:'UCOMPLETE1'},label:'@Alex Harbor'},
 {value:{type:'user',user_id:'UCOMPLETE2'},label:'@Alex Harbor'},
 {value:{type:'usergroup',usergroup_id:'SCOMPLETE1'},label:'@Alex Harbor'}];
const rows=values.map(v=>({textContent:v.label}));
global.document={activeElement:editor,getElementById:()=>editor,querySelector:()=>popup,querySelectorAll:()=>rows};
let calls=[];const shared=arg=>{calls.push(arg);return {ok:true,index:2,...arg}};
const arg={id:'SCOMPLETE1',keyboard:true,choices:{sequence:7,revision:3,values}};
let result=helper(arg,shared);assert.equal(result.id,'SCOMPLETE1');assert.equal(result.type,'usergroup');assert.equal(calls[0].occurrence,2);assert.equal(calls[0].keyboard,true);
arg.keyboard=false;helper(arg,shared);assert.equal(calls[1].keyboard,false);
const before=calls.length;
assert.equal(helper({...arg,choices:{...arg.choices,sequence:8}},shared).ok,false);
assert.equal(helper({...arg,id:'UCOMPLETE1'},shared).ok,false);
rows[0].textContent='Changed';assert.equal(helper(arg,shared).ok,false);rows[0].textContent='@Alex Harbor';
document.activeElement={};assert.equal(helper(arg,shared).ok,false);document.activeElement=editor;
editor.isContentEditable=false;assert.equal(helper(arg,shared).ok,false);editor.isContentEditable=true;
assert.equal(calls.length,before);
assert.equal(helper({action:'scroll',index:4},shared).action,'scroll');
assert.equal(helper({action:'layout',index:0},shared).action,'layout');
console.log('PASS 8 group wrapper guard/delegation checks');
'''
        result=subprocess.run(['node','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True,timeout=15)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('PASS 8',result.stdout)

if __name__=='__main__':unittest.main()
