"""Pure acceptance/rejection tests. No binary, network, or screenshots launched."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

R=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
G=runpy.run_path(str(Path(__file__).with_name('test_profile_group_completion_native.py')))
GR=G['R']

def convert(value):
    if isinstance(value,str):
        return {GR['SAME']:R['CANVAS_LABEL'],GR['DOUBLE']:R['LIST_LABEL'],GR['REOPENED']:R['LIST_LABEL']}.get(value,value)
    if isinstance(value,list):return [convert(v) for v in value]
    if not isinstance(value,dict):return value
    result={k:convert(v) for k,v in value.items()}
    if result.get('type')=='usergroup' and result.get('usergroup_id') in (GR['FIRST'],GR['SECOND']):
        return R['token'](R['CANVAS'] if result['usergroup_id']==GR['FIRST'] else R['LIST'])
    if isinstance(result.get('blocks'),str):result['blocks']=json.dumps(convert(json.loads(result['blocks'])))
    return result

def catalog():
    labels=['@Alex Local','@Alex Harbor','@Alex Harbor','@Alex Retired','@Alex Self',R['CANVAS_LABEL'],R['LIST_LABEL']]
    values=[dict(value=dict(type='user',user_id=uid),label=label) for uid,label in zip(['UMENTION01','UCOMPLETE1','UCOMPLETE2','UCOMPLETE3','UINIT0001'],labels)]
    values.extend(dict(value=R['token'](fid),label=label) for fid,label in [(R['CANVAS'],R['CANVAS_LABEL']),(R['LIST'],R['LIST_LABEL'])])
    return dict(sequence=1,revision=1,values=values)

def ordered_proof():
    # Read the exact checked-in original response fixture, not dumped parsed JSON.
    path=Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/completion/files-response.json'
    original=path.read_text(encoding='utf-8');raw=json.loads(original)
    expected={R['CANVAS']:['CZMAP001','CAMAP001','CTAIL001'],R['LIST']:['CYMAP002','CBMAP002']}
    return dict(fileResponseBytes=original,sourceFiles=dict(rows=[dict(id=row['id'],source=row,effective=dict(channels=expected[row['id']])) for row in raw['files'][:2]],unavailableIds=[],excludedIds=[R['HOSTED']]))

def ledger(original,saved,disabled):
    source=copy.deepcopy(original);requests=[]
    rename={'cafe':'list','alex-held':'pending-file','no-malformed':'broken-file','no-offline':'offline-file','alex-cancel':'cancel-file'}
    for row in source['requests']:
        if row['method'] in ('users/search','usergroups/search'):row['args']['query']=rename.get(row['args']['query'],row['args']['query'])
        requests.append(row)
        if row['method']=='usergroups/search' and not disabled:
            requests.append(dict(method='search.autocomplete.files',args=dict(query=row['args']['query'],include_shares='true',_x_reason='texty-autocomplete')))
            if row['args']['query']=='list' and saved:
                for fid in (R['CANVAS'],R['LIST']):requests.append(dict(method='files.info',args=dict(file=fid,page='1',count='500',truncate='true')))
    for i,row in enumerate(requests,1):row['serial']=i
    members=sum(r['method']=='users/search' for r in requests);groups=sum(r['method']=='usergroups/search' for r in requests);files=sum(r['method']=='search.autocomplete.files' for r in requests)
    source.update(requests=requests,memberSearches=members,groupSearches=groups,fileSearches=files,searches=members+groups+files,completed=members+groups+files-source['active'])
    return source

def sample(directory,narrow=False,disabled=False):
    fixture,old=G['sample'](directory,narrow,disabled)
    value=convert(old);value.pop('restricted',None);value['disabled']=disabled
    if disabled:
        for page,name in zip(value['stages'],R['SHORT']):page['stage']=name
    else:
        value['stages']=[p for p in value['stages'] if p['stage'] not in ('empty','stale-refused')]
        for page,name in zip(value['stages'],R['STAGES']):page['stage']=name
    cat=catalog()
    for i,page in enumerate(value['stages']):
        stage=page['stage'];geometry=page['geometry'];proof=page['proof']
        page['source']=ledger(page['source'],not disabled and i>=8,disabled)
        # Only inserted file labels changed; people remain exact labels.
        rows=geometry.get('resultRows')
        if rows:
            for index,row in enumerate(rows):row['text']=cat['values'][index]['label'] if len(rows)>1 else R['LIST_LABEL']
            geometry['rows']=[r for r in rows if r['visible']]
        if stage=='local-held':page['proof']=dict(choices={**cat,'values':cat['values'][:5]},beforeFiles=copy.deepcopy(page['source']))
        if stage.startswith('disabled'):page['proof']=dict(choices={**cat,'values':cat['values'][:5]},noFileRequest=True)
        if stage.startswith('mixed'):
            page['proof']=dict(choices=copy.deepcopy(cat),release=dict(delivered=True,cancelledBeforeReply=False),beforeFiles=copy.deepcopy(value['stages'][1]['source']),sameQuery=True,**ordered_proof())
        if stage in ('keyboard','mouse'):
            fid=R['CANVAS'] if stage=='keyboard' else R['LIST'];index=5 if stage=='keyboard' else 6
            page['proof']=dict(file=fid,keyboard=stage=='keyboard',choice=dict(id=fid,type=R['token'](fid)['type'],index=index,label=cat['values'][index]['label'],values=copy.deepcopy(cat['values']),keyboard=stage=='keyboard',sequence=1))
        if stage in ('query-retired','cancelled'):
            page['proof']['release']['query']='pending-file' if stage=='query-retired' else 'cancel-file'
        if geometry.get('native'):
            for index,t in enumerate(geometry['native']['tokens']):
                t['label']=['@Alex Local','@Alex Local',R['CANVAS_LABEL'],R['LIST_LABEL']][index]
    value['source']=copy.deepcopy(value['stages'][-1]['source'])
    value['model']=copy.deepcopy(value['stages'][-1]['model'])
    value['captures']={p['stage']:str(directory.resolve()/f"{fixture}-{p['stage']}.png") for p in value['stages']}
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
    def test_disabled(self):self.fixture,self.value=sample(self.path,disabled=True);self.validate()
    def test_disabled_narrow(self):self.fixture,self.value=sample(self.path,True,True);self.validate()
    def test_late_files_refetch_people(self):self.reject(lambda v:v['stages'][2]['proof']['beforeFiles'].__setitem__('memberSearches',99))
    def test_late_files_replace_query(self):self.reject(lambda v:v['stages'][2]['proof']['choices'].__setitem__('sequence',2))
    def test_response_value_reserialization_is_not_original_order(self):self.reject(lambda v:v['stages'][2]['proof'].__setitem__('fileResponseBytes',json.dumps(json.loads(v['stages'][2]['proof']['fileResponseBytes']),sort_keys=True)))
    def test_effective_channels_alphabetized(self):self.reject(lambda v:v['stages'][2]['proof']['sourceFiles']['rows'][0]['effective'].__setitem__('channels',['CAMAP001','CZMAP001','CTAIL001']))
    def test_foreign_channel_admitted(self):self.reject(lambda v:v['stages'][2]['proof']['sourceFiles']['rows'][0]['effective']['channels'].append('CFOREIGN1'))
    def test_incoming_channel_duplicate_not_deduplicated(self):self.reject(lambda v:v['stages'][2]['proof']['sourceFiles']['rows'][0]['effective']['channels'].append('CZMAP001'))
    def test_original_receipt_missing(self):self.reject(lambda v:v['stages'][2]['proof'].pop('fileResponseBytes'))
    def test_accepted_source_changed(self):self.reject(lambda v:v['stages'][2]['proof']['sourceFiles']['rows'][1]['source']['channel_map']['y-last'].__setitem__('team','TINIT0001'))
    def test_candidate_unavailable(self):self.reject(lambda v:v['stages'][2]['proof']['sourceFiles']['unavailableIds'].append(R['CANVAS']))
    def test_hosted_admitted(self):self.reject(lambda v:v['stages'][2]['proof']['choices']['values'][5]['value'].__setitem__('file_id',R['HOSTED']))
    def test_wrong_canvas_category(self):self.reject(lambda v:v['stages'][6]['proof']['choice']['values'][5]['value'].__setitem__('type','file'))
    def test_label_used_as_wire_text(self):self.reject(lambda v:v['stages'][7]['geometry']['native']['tokens'][3]['value'].__setitem__('text',R['LIST_LABEL']))
    def test_wrong_url(self):self.reject(lambda v:v['stages'][7]['geometry']['native']['tokens'][3]['value'].__setitem__('url','https://foreign.invalid'))
    def test_original_opaque(self):self.reject(lambda v:v['stages'][0]['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_wrong_file_request_options(self):self.reject(lambda v:next(r for r in v['stages'][1]['source']['requests'] if r['method']=='search.autocomplete.files')['args'].__setitem__('count',25))
    def test_wrong_file_reason(self):self.reject(lambda v:next(r for r in v['stages'][1]['source']['requests'] if r['method']=='search.autocomplete.files')['args'].__setitem__('_x_reason','other'))
    def test_missing_file_counter(self):self.reject(lambda v:v['stages'][1]['source'].pop('fileSearches'))
    def test_reopen_requires_info_factory(self):self.reject(lambda v:next(r for r in v['stages'][8]['source']['requests'] if r['method']=='files.info')['args'].__setitem__('truncate','false'))
    def test_scroll_missing(self):self.reject(lambda v:v['stages'].pop(4))
    def test_scroll_wrong_target(self):self.reject(lambda v:v['stages'][5]['geometry']['popup'].__setitem__('targetIndex',0))
    def test_late_reply_delivered(self):self.reject(lambda v:v['stages'][9]['proof']['release'].__setitem__('delivered',True))
    def test_failure_hidden(self):self.reject(lambda v:v['stages'][10]['geometry']['notices'][0].__setitem__('visible',False))
    def test_readback_changed(self):self.reject(lambda v:v['stages'][8]['model']['attempts'][0][0]['long_text'].__setitem__('blocks','[]'))
    def test_disabled_dispatch(self):
        self.fixture,self.value=sample(self.path,disabled=True)
        self.reject(lambda v:v['stages'][1]['source'].__setitem__('fileSearches',1))
    def test_import_does_not_launch(self):self.assertEqual(R['B']['PREFIX'],'profile-file-completion-check: ')
    def test_actual_js_private_identity_and_category_before_input(self):
        script=r'''
const assert=require('node:assert/strict'),fs=require('node:fs');
const helper=eval(fs.readFileSync('src/desktop/profile_edit/sections/fixture/file_completion_check.js','utf8'));
const editor={isContentEditable:true},popup={dataset:{querySequence:'7'}};
const values=[{value:{type:'user',user_id:'U1'},label:'Shared'},
{value:{type:'canvas',file_id:'F1'},label:'Shared'}, {value:{type:'file',file_id:'F2'},label:'List'}];
const rows=values.map(v=>({textContent:v.label}));
global.document={activeElement:editor,getElementById:()=>editor,querySelector:()=>popup,querySelectorAll:()=>rows};
let calls=[];const shared=arg=>{calls.push(arg);return {ok:true,index:1,...arg}};
const arg={id:'F1',keyboard:true,choices:{sequence:7,revision:3,values}};
let result=helper(arg,shared);assert.equal(result.id,'F1');assert.equal(result.type,'canvas');assert.equal(calls[0].occurrence,1);
result=helper({...arg,id:'F2',keyboard:false},shared);assert.equal(result.type,'file');assert.equal(calls[1].keyboard,false);
const before=calls.length;
assert.equal(helper({...arg,choices:{...arg.choices,sequence:8}},shared).ok,false);
assert.equal(helper({...arg,id:'U1'},shared).ok,false);
rows[0].textContent='Changed';assert.equal(helper(arg,shared).ok,false);rows[0].textContent='Shared';
document.activeElement={};assert.equal(helper(arg,shared).ok,false);document.activeElement=editor;
editor.isContentEditable=false;assert.equal(helper(arg,shared).ok,false);
assert.equal(calls.length,before);
console.log('PASS 7 file identity/owner checks');
'''
        result=subprocess.run(['node','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True,timeout=15)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('PASS 7',result.stdout)

if __name__=='__main__':unittest.main()
