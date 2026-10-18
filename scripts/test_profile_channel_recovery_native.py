"""Pure validation of the separate conditional recovery native acceptance."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
R=runpy.run_path(str(Path(__file__).with_name('check-profile-channel-recovery-native.py')))

def sample(directory,narrow=False):
    fixture='profile-channel-recovery'+('-narrow' if narrow else '')
    viewport=[668,380] if narrow else [1280,780]
    box=dict(left=120,top=180,right=360,bottom=204,width=240,height=24)
    rows=[]
    for index,stage in enumerate(R['STAGES']):
        replaced=stage=='account-replaced'
        pending=stage in ('conditional-pending','socket-pending')
        name=R['OLD'] if index<2 else R['LATEST'] if stage=='pending-reopened' else R['NEW']
        label='#'+(R['OLD'] if index<3 else R['LATEST'] if stage=='pending-reopened' else R['NEW'])
        profile=dict(enabled=True,blocks=R['blocks'](),dirty=True,busy=False,uncertain=False,attempts=[],reads=0,
                     notice=R['REFRESH_FAILURE'] if index in (5,6) else None,
                     choiceValue='Morning',countryValue='US',managedValue='Preserved organization value')
        source=dict(id=R['CHANNEL'],name=name,updated=101 if index<2 else 103 if stage=='pending-reopened' else 102,fixture_metadata=dict(retained=True))
        model=dict(profile=profile,source=source,nativeName=name,account=R['ACCOUNT'],epoch=20,
                   socketGeneration=2 if index>=7 else 1,idle=index!=1 and not pending)
        if pending:model.update(source=None,nativeName=R['LATEST'] if stage=='socket-pending' else R['NEW'])
        if pending or stage=='pending-reopened':
            model['conditional']=dict(drainHeld=pending,held=0,requestCount=10,
                queues=[dict(pending=pending,incoming=[dict(id=R['CHANNEL'],name=R['QUEUED'],updated=103)] if pending else [])],
                managers=[dict(worker=False,active=False,pendingIds=[],freshIds=[R['CHANNEL']])])
        topic=dict(topicRetainedPredecessor=True,topicDidNotFetch=True)
        proof=[dict(initialConditionalRead=True),topic,dict(mountedLabelRetained=True),dict(newCanonicalLabel=True),
               dict(predecessorRetained=True,fallbackAttempted=False,topic=topic),dict(predecessorRetained=True,fallbackAttempted=True,topic=topic),
               dict(predecessorRetained=True,fallbackAttempted=True,topic=topic),dict(latePublicationRefused=True,successorRead=True,socket=dict(before=1,after=2)),
               dict(conditionalQueued=True,canonicalUnavailable=True),
               dict(socketAccepted=True,canonicalUnavailable=True,topic=dict(source=None,nativeName=R['NEW'],socketGeneration=2,
                    conditional=dict(managers=[dict(freshIds=[])]))),
               dict(latestSocketLabel=True,noRedundantRead=True,requestCount=10),
               dict(oldOpeningRefused=True,oldEditorHidden=True,previousAccount=R['ACCOUNT'],account=R['FOREIGN'],
                    previousBlocks=R['blocks'](),previousAttempts=[])][index]
        if replaced:model.update(profile=dict(enabled=False),source=None,nativeName=None,account=R['FOREIGN'],epoch=21,socketGeneration=0)
        captured=None if replaced else dict(snapshot=dict(blocks=R['blocks'](),composing=False,revision=1,generation='22'),
            tokens=[dict(value=v,label=label) for v in R['blocks']()[0]['elements'][0]['elements'][1:]])
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,pngDimensions=[1002,600] if narrow else [1920,1200],
            dialog=not replaced,dialogRect=None if replaced else dict(left=100,top=12,right=540,bottom=368,width=440,height=356),
            controls=[] if replaced else [dict(text=t,disabled=t=='Save section',visible=True,rect=copy.deepcopy(box))
                for t in ('Back','Save section','Use description','Cancel description edit')],
            notices=[dict(text=R['REFRESH_FAILURE'],visible=True,rect=copy.deepcopy(box))] if index in (5,6) else [],
            feedbackRequired=index in (5,6),feedbackVisible=index in (5,6),globalToast=None,
            tokens=[] if replaced else [dict(text=label,editable=False,visible=True,rect=copy.deepcopy(box),fragments=[copy.deepcopy(box)]) for _ in range(2)],
            captured=captured,richEditable=None if replaced else True)
        rows.append(dict(stage=stage,model=model,proof=proof,geometry=geometry))
    return dict(ok=True,fixture=fixture,viewport=viewport,stages=rows,captureChecksPassed=True,
        transport='closed entered requests; no HTTP polling',visualReview='PNG inspection required separately',
        requests=[dict(serial=i+1,method='conversations.genericInfo' if i in (4,6) else 'channels.info',ids=[R['CHANNEL']],versions={R['CHANNEL']:101 if i<2 else 103 if i==10 else 102},
            predecessor=dict(source=dict(id=R['CHANNEL'],updated=101 if i<2 else 103 if i==10 else 102,name=R['OLD'] if i<2 else R['LATEST'] if i==10 else R['NEW'],fixture_metadata=dict(retained=True)),nativeName=R['OLD'] if i<2 else R['LATEST'] if i==10 else R['NEW'],account=R['ACCOUNT'],socketGeneration=2 if i>=8 else 1)) for i in range(11)],
        captures={s:str(directory/f'{fixture}-{s}.png') for s in R['STAGES']})

class Tests(unittest.TestCase):
    def validate(self,value,directory):R['validate'](value['fixture'],value,directory,platform='win32',png=False)
    def test_all_twelve_stages_both_widths(self):
        with tempfile.TemporaryDirectory() as d:
            for narrow in (False,True):self.validate(sample(Path(d).resolve(),narrow),Path(d).resolve())
    def rejected(self,mutate,pattern=None):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve();value=sample(directory);mutate(value)
            with self.assertRaisesRegex(RuntimeError,pattern or '.'):
                self.validate(value,directory)
    def test_exact_conditional_ids_versions_and_count(self):
        self.rejected(lambda v:v['requests'].pop(),'count')
        self.rejected(lambda v:v['requests'][1].update(ids=['COTHER001']),'ID/version')
        self.rejected(lambda v:v['requests'][2].update(versions={R['CHANNEL']:101}),'ID/version')
        self.rejected(lambda v:v['requests'][1].update(serial=1),'duplicate')
    def test_raw_native_publication_and_unchanged_predecessor(self):
        self.rejected(lambda v:v['stages'][2]['model'].update(nativeName=R['OLD']),'raw/native')
        self.rejected(lambda v:v['stages'][4]['model']['source'].update(unknown='invented'),'predecessor')
        self.rejected(lambda v:v['stages'][5]['model']['source'].pop('fixture_metadata'),'raw/native')
    def test_failure_and_malformed_require_the_actual_generic_fallback(self):
        self.rejected(lambda v:v['requests'][4].update(method='channels.info'),'fallback method')
        self.rejected(lambda v:v['requests'][6].update(method='channels.info'),'fallback method')
        self.rejected(lambda v:v['stages'][6]['proof'].update(fallbackAttempted=False),'retention proof')
    def test_prepared_response_custody_includes_raw_and_socket_predecessor(self):
        self.rejected(lambda v:v['requests'][1]['predecessor']['source'].update(updated=102),'predecessor before')
        self.rejected(lambda v:v['requests'][8]['predecessor'].update(socketGeneration=1),'predecessor before')
    def test_mounted_labels_wait_for_reopen(self):
        self.rejected(lambda v:v['stages'][2]['geometry']['tokens'][0].update(text='#'+R['NEW']),'label custody')
        self.rejected(lambda v:v['stages'][3]['geometry']['tokens'][0].update(text='#'+R['OLD']),'label custody')
    def test_duplicate_opaque_token_order_metadata_and_unsaved_text(self):
        self.rejected(lambda v:v['stages'][5]['model']['profile']['blocks'][0]['elements'][0]['elements'].pop(),'draft')
        self.rejected(lambda v:v['stages'][2]['geometry']['captured']['tokens'].reverse(),'witness')
        self.rejected(lambda v:v['stages'][6]['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'][0].update(text='lost'),'unsaved')
    def test_no_profile_writes_or_reads(self):
        self.rejected(lambda v:v['stages'][5]['model']['profile'].update(attempts=[{}]),'Profile data')
        self.rejected(lambda v:v['stages'][6]['model']['profile'].update(reads=1),'Profile data')
    def test_actual_topic_and_terminal_settlement_required(self):
        self.rejected(lambda v:v['stages'][1]['proof'].update(topicDidNotFetch=False),'Topic')
        self.rejected(lambda v:v['stages'][1]['model'].update(idle=True),'held/settled')
        self.rejected(lambda v:v['stages'][6]['model'].update(idle=False),'held/settled')
    def test_malformed_cannot_fabricate_missing_row(self):
        self.rejected(lambda v:v['stages'][6]['model'].update(source=dict(id=R['CHANNEL'],isNonExistent=True)),'raw/native')
    def test_stale_socket_and_account_are_real_replacements(self):
        self.rejected(lambda v:v['stages'][7]['proof']['socket'].update(after=1),'socket')
        self.rejected(lambda v:v['stages'][7]['proof'].update(successorRead=False),'socket')
        self.rejected(lambda v:v['stages'][-1]['model'].update(epoch=20),'Account')
        self.rejected(lambda v:v['stages'][-1]['model'].update(nativeName='retired-account-name'),'Retired account')
        self.rejected(lambda v:v['stages'][-1]['proof'].update(oldOpeningRefused=False),'replacement')
    def test_pending_source_is_not_canonical_or_already_drained(self):
        self.rejected(lambda v:v['stages'][8]['model'].update(source=v['stages'][7]['model']['source']),'Pending source')
        self.rejected(lambda v:v['stages'][8]['model']['conditional'].update(drainHeld=False),'held drain')
        self.rejected(lambda v:v['stages'][9]['model']['conditional']['queues'][0].update(incoming=[]),'queued predecessor')
    def test_pending_topic_then_socket_restores_real_freshness(self):
        self.rejected(lambda v:v['stages'][9]['proof']['topic']['conditional']['managers'][0].update(freshIds=[R['CHANNEL']]),'Topic did not stale')
        self.rejected(lambda v:v['stages'][9]['model']['conditional']['managers'][0].update(freshIds=[]),'restore freshness')
        self.rejected(lambda v:v['stages'][9]['model'].update(nativeName=R['QUEUED']),'native order')
    def test_pending_drain_reopen_never_reads_again_or_regresses_to_conditional(self):
        self.rejected(lambda v:v['stages'][10]['model']['conditional'].update(requestCount=11),'redundant')
        self.rejected(lambda v:v['stages'][10]['model']['conditional']['managers'][0].update(worker=True),'settled source')
        self.rejected(lambda v:v['stages'][10]['model']['source'].update(name=R['QUEUED']),'raw/native')
        self.rejected(lambda v:v['requests'][10].update(versions={R['CHANNEL']:102}),'ID/version')
    def test_pending_mounted_label_stays_old_until_real_reopen(self):
        self.rejected(lambda v:v['stages'][9]['geometry']['tokens'][0].update(text='#'+R['LATEST']),'label custody')
        self.rejected(lambda v:v['stages'][10]['geometry']['tokens'][0].update(text='#'+R['NEW']),'label custody')
    def test_visible_fragments_and_clipping(self):
        self.rejected(lambda v:v['stages'][2]['geometry']['tokens'][0].update(visible=False),'Hidden')
        self.rejected(lambda v:v['stages'][2]['geometry']['tokens'][0].update(fragments=[]),'fragments')
        self.rejected(lambda v:v['stages'][2]['geometry']['tokens'][0]['fragments'][0].update(top=-5,bottom=19),'clipped')
    def test_scoped_failure_notice_must_be_exact_visible_and_unclipped(self):
        for index in (5,6):
            self.rejected(lambda v:v['stages'][index]['model']['profile'].update(notice=None),'owned failure')
            self.rejected(lambda v:v['stages'][index]['geometry'].update(notices=[],feedbackVisible=False),'readable Profile')
            self.rejected(lambda v:v['stages'][index]['geometry']['notices'][0].update(text='Something failed'),'feedback measurement')
            self.rejected(lambda v:v['stages'][index]['geometry']['notices'][0].update(visible=False),'Hidden')
            self.rejected(lambda v:v['stages'][index]['geometry']['notices'][0]['rect'].update(top=-5,bottom=19),'clipped')
    def test_hidden_global_toast_does_not_replace_readable_profile_notice(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve();value=sample(directory,True)
            value['stages'][5]['geometry']['globalToast']=dict(text=R['REFRESH_FAILURE'],positioned=False,visibility='hidden',visible=False,rect={},dismiss=None)
            self.validate(value,directory)
            value['stages'][5]['geometry'].update(notices=[],feedbackVisible=False)
            with self.assertRaisesRegex(RuntimeError,'readable Profile'):self.validate(value,directory)
    def test_actual_embedded_layout_requires_readable_scoped_feedback(self):
        source=(Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/recovery_check.rs').read_text()
        script=source.split('const LAYOUT: &str = r#"',1)[1].split('"#;',1)[0]
        setup=r'''
const mode=process.argv[1];globalThis.innerWidth=668;globalThis.innerHeight=380;
const dialogBox={left:100,top:20,right:540,bottom:360,width:440,height:340};
const noticeBox={left:120,top:300,right:400,bottom:324,width:280,height:24};
if(mode==='clipped'){noticeBox.top=355;noticeBox.bottom=379;}
const dialog={parentElement:null,scrollWidth:440,clientWidth:440,contains:e=>e===notice,
 getBoundingClientRect:()=>dialogBox,getClientRects:()=>[dialogBox],querySelector:()=>null,
 querySelectorAll:selector=>selector.includes('.profile-edit-notice')&&mode!=='absent'?[notice]:[]};
const notice={parentElement:dialog,textContent:mode==='wrong'?'Other status':'Channel metadata could not refresh.',contains:()=>false,
 getBoundingClientRect:()=>noticeBox,getClientRects:()=>[noticeBox]};
const getComputedStyle=e=>({visibility:mode==='hidden'&&e===notice?'hidden':'visible',display:'block',overflowX:'hidden',overflowY:'hidden'});
const document={querySelector:()=>dialog,getElementById:()=>null,documentElement:{scrollWidth:668},
 elementFromPoint:(x,y)=>x>=noticeBox.left&&x<=noticeBox.right&&y>=noticeBox.top&&y<=noticeBox.bottom?notice:dialog};
'''
        for mode,required,expected in [('readable',True,True),('absent',True,False),('wrong',True,False),('clipped',True,False),('hidden',True,False),('absent',False,True)]:
            result=subprocess.run(['node','-e',setup+'\nconst requireFeedback='+json.dumps(required)+';console.log(JSON.stringify('+script+'));',mode],capture_output=True,text=True,check=True)
            self.assertIs(json.loads(result.stdout)['ok'],expected,mode)
    def test_visible_global_toast_requires_reachable_dismissal_and_bounds(self):
        toast=dict(text=R['REFRESH_FAILURE'],positioned=True,visibility='visible',visible=True,
                   rect=dict(left=120,top=180,right=360,bottom=204,width=240,height=24),dismiss=dict(visible=False,rect={}))
        self.rejected(lambda v:v['stages'][5]['geometry'].update(globalToast=toast),'dismissal')
    def test_persisted_socket_notice_is_also_measured_and_readable(self):
        def mutate(value):
            g=value['stages'][7]['geometry'];g['notices']=[dict(text=R['REFRESH_FAILURE'],visible=False,rect={})];g['feedbackVisible']=False
        self.rejected(mutate,'Hidden')
    def test_transport_does_not_claim_actual_http(self):
        self.rejected(lambda v:v.update(transport='actual HTTP success'),'transport')
    def test_exact_capture_inventory_and_paths(self):
        self.rejected(lambda v:v['captures'].pop('unchanged'),'inventory')
        self.rejected(lambda v:v['captures'].update(initial='wrong.png'),'owned capture')
    def test_readiness_owned_identity(self):
        ready=R['ready_tuple']('profile-channel-recovery','a'*32,12)
        R['validate_ready'](ready.copy(),ready)
        for key,value in [('fixture','profile-channel-recovery-narrow'),('nonce','b'*32),('pid',True)]:
            with self.assertRaises(RuntimeError):R['validate_ready']({**ready,key:value},ready)
    def test_copied_native_fragment_check_remains_identical_to_accepted_checker(self):
        root=Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture'
        def visible(name):return (root/name).read_text().split('const visible=e=>',1)[1].split(';\n const controls=',1)[0]
        self.assertEqual(visible('mentions_check.rs'),visible('recovery_check.rs'))

if __name__=='__main__':unittest.main()
