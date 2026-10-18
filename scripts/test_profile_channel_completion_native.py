"""Pure completion-receipt regressions; never starts a native process."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

R=runpy.run_path(str(Path(__file__).with_name('check-profile-channel-completion-native.py')))

def sample(captures,narrow=False):
    fixture=R['FIXTURES'][int(narrow)];viewport=[668,380] if narrow else [1280,780]
    rect=dict(left=160,top=170,right=400,bottom=194,width=240,height=24)
    initial=R['initial']();saved=copy.deepcopy(initial)
    saved[0]['elements'][0]['elements'] += [dict(type='text',text=' '),dict(type='channel',channel_id='CCOMPLETE1'),dict(type='text',text='  '),dict(type='channel',channel_id='CCOMPLETE2'),dict(type='text',text=' ')]
    write=[dict(element_id='long',long_text=dict(blocks=json.dumps(saved))),dict(element_id='locked',text=dict(text='Preserved organization value'))]
    queries=['project','project-co','project-held','project-co','no-malformed','no-offline','zz-no-matching-channel','project-cancel','project']
    ledger=[dict(serial=i+1,method='channels/search',args=dict(query=q,count=25,fuzz=1,filter='xws',include_record_channels=True,check_membership=True,top_channels=[],default_workspace='TINIT0001')) for i,q in enumerate(queries)]
    rows=[]
    for i,stage in enumerate(R['STAGES']):
        count=2 if i<3 else 3 if stage=='keyboard' else 4
        current=copy.deepcopy(initial if count==2 else saved)
        if count==3:current[0]['elements'][0]['elements']=current[0]['elements'][0]['elements'][:5]+[dict(type='text',text=' ')]
        tokenvalues=[v for v in current[0]['elements'][0]['elements'] if v['type']=='channel']
        labels=[R['LABELS'][0],R['LABELS'][0]]+R['LABELS'][1:count-1]
        native=dict(snapshot=dict(blocks=current,revision=4,generation='1',composing=False),tokens=[dict(value=v,label=l) for v,l in zip(tokenvalues,labels)])
        names=['Back','Save section']+([] if stage=='cancelled' else ['Use description','Cancel description edit'])
        item=lambda text,disabled=False:dict(text=text,disabled=disabled,visible=True,rect=copy.deepcopy(rect))
        choices=R['LABELS'] if stage in ('refreshed','query-retired') else [R['LABELS'][0]] if stage=='local' else []
        notice=R['FAILURE'] if stage in ('malformed','failure','stale-refused') else 'No matching channels' if stage=='empty' else None
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,dialog=dict(left=100,top=12,right=540,bottom=368,width=440,height=356),
            controls=[item(n,n=='Save section') for n in names],notices=[item(notice)] if notice else [],rows=[item(n) for n in choices],
            tokens=[item(n) for n in labels] if stage!='cancelled' else [],native=native if stage!='cancelled' else None,pngDimensions=viewport,
            editor=dict(visible=True,rect=rect,text='synthetic'),bodyScroll=400,serial='1')
        searches=[0,1,1,1,2,2,4,5,6,7,8,9][i]
        cancelled=0 if i<6 else 1 if i<10 else 2
        source=dict(requests=copy.deepcopy(ledger[:searches]),searches=searches,active=1 if stage=='local' else 0,
                    completed=searches-(1 if stage=='local' else 0),cancelled=cancelled,networkCalls=0,
                    held=stage=='local',heldQueries=[dict(serial=1,query='project')] if stage=='local' else [])
        model=dict(enabled=True,busy=False,uncertain=False,managedValue='Preserved organization value',choiceValue='Morning',countryValue='US',
            blocks=copy.deepcopy(initial if i<5 else saved),dirty=False,attempts=[] if i<5 else [copy.deepcopy(write)],reads=0 if i<5 else 1)
        proof={}
        if stage=='local':proof=dict(sequence=1,held=True)
        if stage=='refreshed':proof=dict(sequence=1,release=dict(serial=1,query='project',delivered=True,cancelledBeforeReply=False))
        if stage in ('keyboard','mouse'):proof=dict(keyboard=stage=='keyboard',channel='CCOMPLETE1' if stage=='keyboard' else 'CCOMPLETE2')
        if stage=='saved':proof=dict(exactReadback=True,expectedBlocks=copy.deepcopy(saved))
        if stage in ('query-retired','cancelled'):proof=dict(release=dict(serial=3 if stage=='query-retired' else 8,query='project-held' if stage=='query-retired' else 'project-cancel',delivered=False,cancelledBeforeReply=True))
        if stage in ('malformed','failure'):proof=dict(noWrite=True,retainedBlocks=copy.deepcopy(saved))
        if stage=='stale-refused':proof=dict(refused=True,previousSequence=20,nextSequence=21,exactNativeBlocks=copy.deepcopy(current))
        rows.append(dict(stage=stage,model=model,source=source,geometry=geometry,proof=proof))
    result=dict(ok=True,fixture=fixture,initialBlocks=initial,savedBlocks=saved,stages=rows,
                captures={s:str(captures.resolve()/f'{fixture}-{s}.png') for s in R['STAGES']},
                model=copy.deepcopy(rows[-1]['model']),source=copy.deepcopy(rows[-1]['source']))
    return fixture,result

class Validator(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.directory=Path(self.tmp.name);self.fixture,self.value=sample(self.directory)
    def validate(self):R['validate'](self.fixture,self.value,self.directory,platform='win32',png=False)
    def reject(self,mutate):
        mutate(self.value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.validate()
    def test_normal(self):self.validate()
    def test_narrow(self):
        self.fixture,self.value=sample(self.directory,True);self.validate()
    def test_readiness_owner(self):
        expected=R['ready_tuple'](self.fixture,'a'*32,42);R['validate_ready'](expected,expected)
        with self.assertRaises(RuntimeError):R['validate_ready']({**expected,'pid':43},expected)
    def test_readiness_invalid_nonce(self):
        with self.assertRaises(RuntimeError):R['ready_tuple'](self.fixture,'../stale',42)
    def test_terminal_error_preserved(self):
        self.value.update(ok=False,error='Exact last native diagnostic')
        with self.assertRaisesRegex(RuntimeError,'Exact last native diagnostic'):self.validate()
    def test_missing_stage(self):self.reject(lambda r:r['stages'].pop())
    def test_missing_capture(self):self.reject(lambda r:r['captures'].pop('failure'))
    def test_foreign_capture(self):self.reject(lambda r:r['captures'].__setitem__('failure',str(self.directory.parent/'foreign.png')))
    def test_bad_viewport(self):self.reject(lambda r:r['stages'][0]['geometry'].__setitem__('viewport',[1280,800]))
    def test_hidden_popup(self):self.reject(lambda r:r['stages'][1]['geometry']['rows'][0].__setitem__('visible',False))
    def test_clipped_popup(self):self.reject(lambda r:r['stages'][1]['geometry']['rows'][0]['rect'].update(top=-5,bottom=19))
    def test_hidden_failure(self):self.reject(lambda r:r['stages'][8]['geometry']['notices'][0].__setitem__('visible',False))
    def test_absent_failure(self):self.reject(lambda r:r['stages'][8]['geometry'].__setitem__('notices',[]))
    def test_wrong_failure(self):self.reject(lambda r:r['stages'][7]['geometry']['notices'][0].__setitem__('text','Unknown network error'))
    def test_missing_empty(self):self.reject(lambda r:r['stages'][9]['geometry'].__setitem__('notices',[]))
    def test_disabled_cancel(self):self.reject(lambda r:r['stages'][1]['geometry']['controls'][3].__setitem__('disabled',True))
    def test_original_style(self):self.reject(lambda r:r['stages'][0]['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][0].pop('style'))
    def test_duplicate_raw_metadata(self):self.reject(lambda r:r['stages'][0]['geometry']['native']['tokens'][1]['value'].__setitem__('fixture_occurrence',1))
    def test_inserted_label_as_id(self):self.reject(lambda r:r['stages'][3]['geometry']['native']['tokens'][2]['value'].__setitem__('channel_id','#project-completion'))
    def test_wrong_token_label(self):self.reject(lambda r:r['stages'][3]['geometry']['native']['tokens'][2].__setitem__('label','#project-cove'))
    def test_native_composition(self):self.reject(lambda r:r['stages'][3]['geometry']['native']['snapshot'].__setitem__('composing',True))
    def test_local_was_not_held(self):self.reject(lambda r:r['stages'][1]['proof'].__setitem__('held',False))
    def test_refresh_sequence(self):self.reject(lambda r:r['stages'][2]['proof'].__setitem__('sequence',2))
    def test_fake_refresh_callback(self):self.reject(lambda r:r['stages'][2]['proof']['release'].__setitem__('delivered',False))
    def test_keyboard_route(self):self.reject(lambda r:r['stages'][3]['proof'].__setitem__('keyboard',False))
    def test_saved_wire(self):self.reject(lambda r:r['stages'][5]['model']['attempts'][0][0]['long_text'].__setitem__('blocks','[]'))
    def test_automatic_write(self):self.reject(lambda r:r['stages'][8]['model']['attempts'].append([]))
    def test_query_release_accepted(self):self.reject(lambda r:r['stages'][6]['proof']['release'].__setitem__('delivered',True))
    def test_wrong_cancelled_query(self):self.reject(lambda r:r['stages'][10]['proof']['release'].__setitem__('query','new-successor'))
    def test_missing_cancellation(self):self.reject(lambda r:r['stages'][10]['source'].__setitem__('cancelled',0))
    def test_stale_not_restarted(self):self.reject(lambda r:r['stages'][11]['proof'].__setitem__('nextSequence',20))
    def test_live_network(self):self.reject(lambda r:r['stages'][2]['source'].__setitem__('networkCalls',1))
    def test_fabricated_method(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0].__setitem__('method','fake.search'))
    def test_wrong_search_count(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['args'].__setitem__('count',100))
    def test_trigger_sent_to_source(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['args'].__setitem__('query','#project'))
    def test_secret_leak(self):self.reject(lambda r:r['stages'][1]['source']['requests'][0]['args'].__setitem__('token','test-secret'))
    def test_closed_capture_order(self):
        class Log:
            def receipts(s,prefix):
                if prefix==R['PREFIX']:return [self.value]
                return [dict(stage=stage,edge=edge) for stage in reversed(R['STAGES']) for edge in ('before','after')]
        with self.assertRaisesRegex(RuntimeError,'capture order'):R['validate_log'](Log(),self.fixture,self.directory,'win32',False)
    def test_duplicate_terminal(self):
        class Log:
            def receipts(s,prefix):return [self.value,self.value]
        with self.assertRaisesRegex(RuntimeError,'duplicate terminal'):R['validate_log'](Log(),self.fixture,self.directory,'win32',False)
    def test_actual_embedded_query_wait_requires_new_sequence(self):
        script=r'''
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const root={dataset:{sectionSerial:'7'}},popup={dataset:{querySequence:'4'}},tail={textContent:' #old',firstChild:{length:7}};
const section={querySelector:()=>tail};
const editor={isContentEditable:true,__profileRichCompletion:{},querySelector:s=>s==='.rich-editor-section'?section:tail,focus(){},dispatchEvent(){}};
const document={getElementById:id=>id==='shell-overlay-dialog'?root:editor,querySelector:()=>popup,
createRange:()=>({setStart(){},collapse(){}})};
const context=vm.createContext({document,window:{},getSelection:()=>({removeAllRanges(){},addRange(){}}),InputEvent:class{}});
const source=fs.readFileSync('src/desktop/profile_edit/sections/fixture/completion_check.js','utf8');
const helper=vm.runInContext(source,context);
const input=helper('query','#new');assert(input.ok);assert.equal(input.previousSequence,4);assert.equal(input.serial,'7');
assert.equal(tail.textContent,' #new');assert.equal(helper('query-ready',input).ok,false,'old failure cannot satisfy newer query');
popup.dataset.querySequence='5';assert.equal(helper('query-ready',input).ok,true);
popup.dataset.querySequence='3';assert.equal(helper('query-ready',input).ok,false);
popup.dataset.querySequence='5';root.dataset.sectionSerial='8';assert.equal(helper('query-ready',input).ok,false);
root.dataset.sectionSerial='7';tail.textContent=' #other';assert.equal(helper('query-ready',input).ok,false);
tail.textContent=' #new';editor.isContentEditable=false;assert.equal(helper('query-ready',input).ok,false);
console.log('actual query admission: prior sequence, fresh sequence, older sequence, serial, tail, editable guards passed');
'''
        result=subprocess.run(['node','-e',script],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

if __name__=='__main__':unittest.main()
