"""Pure receipt validation for disconnected Profile channel-mention acceptance."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

R = runpy.run_path(str(Path(__file__).with_name('check-profile-mentions-native.py')))


def sample(captures, narrow=False):
    fixture = 'profile-sections-mentions' + ('-narrow' if narrow else '')
    viewport = [668, 380] if narrow else [1280, 780]
    rect = dict(left=120, top=180, right=360, bottom=204, width=240, height=24)
    dialog = dict(left=100, top=12, right=540, bottom=368 if narrow else 700, width=440, height=356 if narrow else 688)
    prefixes = ['Working with ', 'Saved with ', 'Rejected with ', 'Uncertain with ', 'Uncertain with ',
                'Saved with ', 'Saved with ', 'Saved with ', 'x'*4990, 'x'*4991]
    attempts = [[dict(element_id='long', long_text=dict(blocks=json.dumps(R['blocks'](text)))),
                 dict(element_id='locked', text=dict(text='Preserved organization value'))]
                for text in ('Saved with ', 'Rejected with ', 'Uncertain with ')]
    rows = []
    for index, stage in enumerate(R['stages']()):
        replaced = stage == 'account-replaced'
        rich = stage in ('opened', 'confirmed', 'rejected', 'retried', 'limit5000') or (R['MODE']['groups'] and stage in R['GROUP_STAGES'])
        if replaced:
            model = dict(enabled=False)
            previous = rows[-1]['model']
            proof = dict(oldOpeningRefused=True, oldEditorHidden=True, previousAccount=previous['account'],
                         account='00000000-0000-4000-8000-000000000082', previousEpoch=20, epoch=21,
                         previousAttempts=copy.deepcopy(previous['attempts']), previousBlocks=copy.deepcopy(previous['blocks']))
        else:
            model = dict(enabled=True, account='00000000-0000-4000-8000-000000000081', epoch=20,
                         blocks=R['blocks'](R['prefix'](stage)), managedValue='Preserved organization value',
                         choiceValue='Morning', countryValue='US',
                         busy=False, uncertain=stage in ('uncertain','checked'), dirty=stage in ('rejected','uncertain','checked','group-empty','limit5000','limit5001'), canSave=stage in ('rejected','group-empty','limit5000'),
                         canCheck=index in (3,4), attempts=copy.deepcopy(attempts[:min(index,3)]),
                         reads=0 if index==0 else 1 if index<4 else 3)
            proof=copy.deepcopy(R['proofs']()[stage])
        actions=[] if replaced else ['Back']
        if stage in ('uncertain','checked'):
            actions+=['Check profile']
            if stage=='checked': actions+=['Keep current section']
        elif not replaced: actions+=['Save section']
        if rich: actions+=['Use description','Cancel description edit']
        if stage in ('stale-metadata','unavailable'): actions+=['Retry description','Cancel description edit']
        notice={'confirmed':R['SAVED'],'rejected':R['REJECTED'],'uncertain':R['UNCERTAIN'],'checked':R['MISMATCH'],
                'stale-metadata':R['UNAVAILABLE'],'unavailable':R['UNAVAILABLE'],
                'limit5001':"Check this field's value or length."}.get(stage)
        texts=[notice] if notice else []
        if stage in ('uncertain','checked'):texts+=['Checking reads your saved section. It does not send the change again.']
        captured=None
        if rich:
            captured=dict(snapshot=dict(blocks=R['blocks'](R['prefix'](stage)),generation='22',revision=1,composing=False),
                          tokens=[dict(value=v,label=R['label'](stage)) for v in R['blocks']('')[0]['elements'][0]['elements'][1:]])
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,pngDimensions=[1002,600] if narrow else [1920,1200],
                      dialog=not replaced,dialogRect=None if replaced else copy.deepcopy(dialog),
                      controls=[dict(text=a,disabled=a=='Save section',visible=True,rect=copy.deepcopy(rect)) for a in actions],
                      notices=[dict(text=t,visible=True,rect=copy.deepcopy(rect)) for t in texts],
                      tokens=[dict(text=R['label'](stage),editable=False,visible=True,rect=copy.deepcopy(rect)) for _ in range(2)] if rich else [],
                      captured=captured,richEditable=True if rich else None,
                      longDisabled=True if stage in ('uncertain','checked') else False if stage=='limit5001' else None,
                      managedFieldPreserved=True if stage in ('uncertain','checked','limit5001') else None)
        geometry.update(choiceValue='Morning' if stage in ('uncertain','checked','limit5001') else None,
                        countryValue='US' if stage in ('uncertain','checked','limit5001') else None)
        if stage=='group-empty':
            for token in geometry['tokens']:
                token['visible']=False;token['rect'].update(width=0,right=token['rect']['left'])
        rows.append(dict(stage=stage,model=model,proof=proof,geometry=geometry))
    return dict(ok=True,fixture=fixture,viewport=viewport,stages=rows,captureChecksPassed=True,
                mentionKind=R['kind'](),
                visualReview='PNG inspection required separately',
                captures={stage:str(captures/f'{fixture}-{stage}.png') for stage in R['stages']()})


class Tests(unittest.TestCase):
    def validate(self,value,directory):
        R['validate'](value['fixture'],value,directory,platform='win32',png=False)

    def test_normal_and_narrow_all_eleven_stages(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for narrow in (False,True):self.validate(sample(directory,narrow),directory)

    def test_user_mode_preserves_exact_labels_raw_ids_and_mode(self):
        R['MODE']['users'] = True
        try:
            with tempfile.TemporaryDirectory() as d:
                directory = Path(d).resolve()
                for narrow in (False, True):
                    value = sample(directory, narrow)
                    self.validate(value, directory)
                    self.assertEqual(value['stages'][0]['geometry']['tokens'][0]['text'], '@  Cafe\u0301 Harbor  ')
                    value['stages'][0]['geometry']['tokens'][0]['text'] = '@Cafe\u0301 Harbor'
                    with self.assertRaisesRegex(RuntimeError, 'labels missing'):
                        self.validate(value, directory)
                value = sample(directory)
                value['mentionKind'] = 'channel'
                with self.assertRaisesRegex(RuntimeError, 'Wrong mention mode'):
                    self.validate(value, directory)
                value = sample(directory)
                value['stages'][0]['model']['blocks'][0]['elements'][0]['elements'][1]['user_id'] = 'UOTHER01'
                with self.assertRaisesRegex(RuntimeError, 'exact duplicate tokens'):
                    self.validate(value, directory)
        finally:
            R['MODE']['users'] = False

    def test_native_terminal_error_survives_outer_failure_summary(self):
        with tempfile.TemporaryDirectory() as d:
            value=dict(fixture='profile-sections-mentions',ok=False,
                       error='Slack API returned error: channel_initialization_unavailable')
            with self.assertRaisesRegex(RuntimeError,'native assertion failed: Slack API returned error: channel_initialization_unavailable'):
                self.validate(value,Path(d).resolve())

    def test_untouched_model_and_native_select_values_are_required(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for index,owner in ((0,'model'),(1,'model'),(3,'geometry'),(4,'geometry'),(9,'geometry')):
                for key in ('choiceValue','countryValue'):
                    v=sample(directory);v['stages'][index][owner][key]=''
                    with self.assertRaisesRegex(RuntimeError,'Untouched .* selects'):
                        self.validate(v,directory)

    def test_actual_post_save_render_barrier_refuses_stale_serial(self):
        source=(Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/mentions_check.rs').read_text()
        code=source.split('const expected={expected},d=',1)[1].split('"#',1)[0]
        code=('(()=>{const expected={visit:"7",serial:"8",locked:"false",notice:"Profile fields saved."},d='+code).replace('{{','{').replace('}}','}')
        setup='const d={dataset:{dialogGeneration:"7",sectionSerial:process.argv[1],sectionLocked:"false"}};const document={getElementById:()=>d,querySelector:()=>({textContent:"Profile fields saved."})};'
        for serial,expected in [('7',False),('8',True)]:
            result=subprocess.run(['node','-e',setup+'console.log(JSON.stringify('+code+'));',serial],capture_output=True,text=True,check=True)
            self.assertIs(json.loads(result.stdout)['ok'],expected)

    def test_duplicate_wire_tokens_order_and_unknown_fields_are_required(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for mode in ('missing','reversed','metadata','label_as_id'):
                v=sample(directory);wire=v['stages'][1]['model']['attempts'][0][0]['long_text']
                value=json.loads(wire['blocks']);tokens=value[0]['elements'][0]['elements']
                if mode=='missing':tokens.pop()
                elif mode=='reversed':tokens[1:]=reversed(tokens[1:])
                elif mode=='metadata':del tokens[1]['fixture_occurrence']
                else:tokens[1]['channel_id']=R['label']()
                wire['blocks']=json.dumps(value)
                with self.assertRaisesRegex(RuntimeError,'Wire lost'):self.validate(v,directory)

    def test_visible_label_cannot_be_an_id_or_editable_text(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for key,value in [('text','#CMENTION01'),('editable',True),('visible',False)]:
                v=sample(directory);v['stages'][0]['geometry']['tokens'][0][key]=value
                with self.assertRaises(RuntimeError):self.validate(v,directory)

    def test_native_token_witness_custody_is_independent_of_display_text(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve();v=sample(directory)
            v['stages'][0]['geometry']['captured']['tokens'].reverse()
            with self.assertRaisesRegex(RuntimeError,'witnesses'):self.validate(v,directory)

    def test_mounted_label_stability_requires_actual_source_rename(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for key,value in [('sourceName','project-harbor'),('nativeName','project-harbor'),('mountedLabelStable',False)]:
                v=sample(directory);v['stages'][0]['proof'][key]=value
                with self.assertRaisesRegex(RuntimeError,'transition proof'):self.validate(v,directory)
            v=sample(directory);v['stages'][0]['geometry']['captured']['tokens'][0]['label']='#renamed-harbor'
            with self.assertRaisesRegex(RuntimeError,'witnesses'):self.validate(v,directory)

    def test_uncertain_save_cannot_unlock_or_replay(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for key,value in [('canSave',True),('uncertain',False),('reads',4)]:
                v=sample(directory);v['stages'][4]['model'][key]=value
                with self.assertRaises(RuntimeError):self.validate(v,directory)
            v=sample(directory);v['stages'][4]['model']['attempts'].append(copy.deepcopy(v['stages'][4]['model']['attempts'][0]))
            with self.assertRaisesRegex(RuntimeError,'replay'):self.validate(v,directory)
            v=sample(directory);v['stages'][4]['geometry']['longDisabled']=False
            with self.assertRaisesRegex(RuntimeError,'unlocked'):self.validate(v,directory)

    def test_failure_retry_and_stale_opening_require_retained_draft(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for index in (2,3,5,6,7):
                v=sample(directory);v['stages'][index]['model']['blocks']=[]
                with self.assertRaisesRegex(RuntimeError,'Section lost'):self.validate(v,directory)
            v=sample(directory);v['stages'][5]['proof']['staleOpeningRefused']=False
            with self.assertRaisesRegex(RuntimeError,'transition proof'):self.validate(v,directory)

    def test_id_based_5000_5001_boundary_preserves_invalid_form(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for index,text in [(8,'x'*4980),(9,'x'*4990)]:
                v=sample(directory);v['stages'][index]['model']['blocks']=R['blocks'](text)
                with self.assertRaisesRegex(RuntimeError,'Section lost'):self.validate(v,directory)
            v=sample(directory);v['stages'][9]['model']['canSave']=True
            with self.assertRaisesRegex(RuntimeError,'Unsafe write'):self.validate(v,directory)

    def test_feedback_absent_hidden_or_clipped_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for mode in ('absent','hidden','clipped'):
                v=sample(directory,True);g=v['stages'][9]['geometry']
                if mode=='absent':g['notices']=[]
                elif mode=='hidden':g['notices'][0]['visible']=False
                else:g['notices'][0]['rect'].update(top=375,bottom=399)
                with self.assertRaises(RuntimeError):self.validate(v,directory)

    def test_actual_visibility_checks_scroll_ancestor_clipping(self):
        source=(Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/mentions_check.rs').read_text()
        fn='e=>'+source.split('const visible=e=>',1)[1].split(';\n const controls=',1)[0]
        setup=r'''
const mode=process.argv[1];globalThis.innerWidth=668;globalThis.innerHeight=380;
const body={parentElement:null,box:{left:100,top:200,right:500,bottom:300,width:400,height:100}};
const node={parentElement:body,box:{left:120,top:210,right:360,bottom:234,width:240,height:24},contains:()=>false};
node.getClientRects=()=>[node.box];
if(mode==='ancestor')node.box={left:120,top:190,right:360,bottom:214,width:240,height:24};
const rect=e=>e.box;
const getComputedStyle=e=>({overflowY:e===body?'auto':'visible',overflowX:'visible',visibility:'visible',display:'block'});
const document={elementFromPoint:()=>mode==='covered'?body:node};
'''
        for mode,expected in [('visible',True),('ancestor',False),('covered',False)]:
            result=subprocess.run(['node','-e',setup+'\nconst visible='+fn+';console.log(JSON.stringify(visible(node)));',mode],
                                  capture_output=True,text=True,check=True)
            self.assertIs(json.loads(result.stdout),expected)

    def test_actual_wrapped_token_checks_each_fragment_not_union_center(self):
        source=(Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/mentions_check.rs').read_text()
        fn='e=>'+source.split('const visible=e=>',1)[1].split(';\n const controls=',1)[0]
        setup=r'''
const mode=process.argv[1];globalThis.innerWidth=668;globalThis.innerHeight=380;
const body={parentElement:null,box:{left:100,top:200,right:500,bottom:300,width:400,height:100}};
const fragments=[{left:440,top:210,right:480,bottom:230,width:40,height:20},{left:120,top:230,right:220,bottom:250,width:100,height:20}];
if(mode==='ancestorY')fragments[1]={left:120,top:290,right:220,bottom:310,width:100,height:20};
if(mode==='ancestorX')fragments[1]={left:90,top:230,right:190,bottom:250,width:100,height:20};
if(mode==='viewport')fragments[1]={left:120,top:370,right:220,bottom:390,width:100,height:20};
if(mode==='empty')fragments.length=0;
const node={parentElement:body,contains:()=>false,getClientRects:()=>fragments};
const rect=e=>e.box;
const getComputedStyle=e=>({overflowY:e===body?'auto':'visible',overflowX:e===body?'auto':'visible',visibility:'visible',display:'inline'});
const document={elementFromPoint:(x,y)=>fragments.some((r,i)=>!(mode==='covered'&&i===1)&&x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom)?node:body};
// The union center falls in the empty area between these two line fragments.
if(mode==='wrapped'&&document.elementFromPoint(300,230)===node)throw Error('Invalid wrapped fixture');
'''
        for mode,expected in [('wrapped',True),('ancestorY',False),('ancestorX',False),('viewport',False),('covered',False),('empty',False)]:
            result=subprocess.run(['node','-e',setup+'\nconst visible='+fn+';console.log(JSON.stringify(visible(node)));',mode],
                                  capture_output=True,text=True,check=True)
            self.assertIs(json.loads(result.stdout),expected,mode)

    def test_actual_account_replacement_and_refusal_are_required(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for key,value in [('oldOpeningRefused',False),('epoch',20),('account','00000000-0000-4000-8000-000000000081')]:
                v=sample(directory);v['stages'][-1]['proof'][key]=value
                with self.assertRaises(RuntimeError):self.validate(v,directory)

    def test_capture_inventory_and_owner_path_are_exact(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve();v=sample(directory);del v['captures']['rejected']
            with self.assertRaisesRegex(RuntimeError,'inventory'):self.validate(v,directory)
            v=sample(directory);v['captures']['opened']=str(directory/'other.png')
            with self.assertRaisesRegex(RuntimeError,'owned capture'):self.validate(v,directory)

    def test_readiness_nonce_pid_and_final_dimensions_are_owned(self):
        ready=R['ready_tuple']('profile-sections-mentions','a'*32,12)
        R['validate_ready'](ready.copy(),ready)
        for key,value in [('fixture','profile-sections-mentions-narrow'),('nonce','b'*32),('pid',True)]:
            with self.assertRaises(RuntimeError):R['validate_ready']({**ready,key:value},ready)
        with self.assertRaises(RuntimeError):R['validate_owned'](dict(readiness=ready,stages=[dict(geometry=dict(pngDimensions=[1,2]))]),ready,[3,4])


class GroupTests(unittest.TestCase):
    def setUp(self): R['MODE'].update(users=False,groups=True)
    def tearDown(self): R['MODE'].update(users=False,groups=False)
    def validate(self,value,directory): R['validate'](value['fixture'],value,directory,platform='win32',png=False)
    def reject(self,change):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve();v=sample(directory)
            change(v)
            with self.assertRaises(RuntimeError):self.validate(v,directory)
    def row(self,v,stage):return next(r for r in v['stages'] if r['stage']==stage)
    def test_group_fourteen_stages_both_widths(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d).resolve()
            for narrow in (False,True):
                v=sample(directory,narrow);self.validate(v,directory)
                self.assertEqual(len(v['stages']),14)
                self.assertEqual(v['stages'][0]['geometry']['tokens'][0]['text'],'@@ Cafe\u0301 crew  ')
    def test_mode_cannot_be_user_or_channel(self):self.reject(lambda v:v.update(mentionKind='channel'))
    def test_group_metadata_and_tab_custody(self):
        for key in ('opaque','tab_id','fixture_occurrence'):
            self.reject(lambda v:self.row(v,'confirmed')['geometry']['captured']['tokens'][0]['value'].pop(key))
    def test_group_id_not_label_on_wire(self):
        def change(v):
            w=v['stages'][1]['model']['attempts'][0][0]['long_text'];raw=json.loads(w['blocks'])
            raw[0]['elements'][0]['elements'][1]['usergroup_id']=R['GROUP_LABEL'];w['blocks']=json.dumps(raw)
        self.reject(change)
    def test_one_prefix_and_whitespace_not_normalized_away(self):
        for bad in ('@ Cafe\u0301 crew  ','@@ Cafe\u0301 crew','@@ Cafe\u00e9 crew  '):
            self.reject(lambda v:self.row(v,'opened')['geometry']['tokens'][0].update(text=bad))
    def test_mounted_old_group_label_required_after_real_rename(self):
        self.reject(lambda v:self.row(v,'opened')['geometry']['captured']['tokens'][0].update(label='@renamed-group'))
        self.reject(lambda v:self.row(v,'opened')['proof'].update(sourceName=R['GROUP_LABEL']))
    def test_empty_is_not_id_or_fallback_label(self):
        for bad in ('@','@SMENTION01','@unknown-group'):
            self.reject(lambda v:self.row(v,'group-empty')['geometry']['tokens'][0].update(text=bad))
    def test_empty_keeps_two_exact_raw_tokens(self):
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['captured']['tokens'].pop())
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'].pop())
    def test_empty_cannot_have_visible_token_width(self):
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['tokens'][0].update(visible=True))
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['tokens'][0]['rect'].update(width=10,right=130))
    def test_empty_anchor_cannot_be_outside_viewport(self):
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['tokens'][0]['rect'].update(top=780,bottom=804))
    def test_empty_5000_uses_zero_mention_count(self):
        self.reject(lambda v:self.row(v,'group-empty')['model'].update(blocks=R['blocks']('x'*4990)))
        self.reject(lambda v:self.row(v,'group-empty')['proof'].update(text5001Refused=False))
    def test_unknown_requires_observed_flag_not_missing(self):
        self.reject(lambda v:self.row(v,'group-unknown')['proof']['observedRow'].pop('isUnknown'))
        self.reject(lambda v:self.row(v,'group-unknown')['geometry']['tokens'][0].update(text='@ignored-handle'))
    def test_deactivated_and_hidden_keep_label(self):
        self.reject(lambda v:self.row(v,'group-deactivated')['proof']['observedRow'].update(date_delete=0))
        self.reject(lambda v:self.row(v,'group-deactivated')['proof']['observedRow'].update(is_visible=True))
        self.reject(lambda v:self.row(v,'group-deactivated')['geometry']['tokens'][0].update(text='@unknown-group'))
    def test_standard_limit_still_counts_merged_group_id(self):
        self.reject(lambda v:self.row(v,'limit5000')['model'].update(blocks=R['blocks']('x'*5000)))
        self.reject(lambda v:self.row(v,'limit5001')['model'].update(canSave=True))
    def test_extra_group_stage_cannot_disappear(self):self.reject(lambda v:v['stages'].pop(8))
    def test_extra_group_stage_cannot_replay_write(self):
        self.reject(lambda v:self.row(v,'group-unknown')['model']['attempts'].append([]))
    def test_actual_embedded_empty_group_geometry_gate(self):
        source=Path('src/desktop/profile_edit/sections/fixture/mentions_check.rs').read_text()
        expression=source.split('&&tokens.every((t,i)=>',1)[1].split(',\n viewport',1)[0]
        expression='tokens.every((t,i)=>'+expression
        setup="""
const mode=process.argv[1];
const allowEmptyGroup=mode!=='other-stage';
const tokens=[{text:'',visible:false,rect:{width:0}}];
const native={tokens:[{label:'',value:{type:'usergroup'}}]};
if(mode==='text')tokens[0].text='@';
if(mode==='width')tokens[0].rect.width=2;
if(mode==='user')native.tokens[0].value.type='user';
if(mode==='wrong-label')native.tokens[0].label='@crew';
if(mode==='missing-witness')native.tokens=[];
"""
        for mode,wanted in [('empty',True),('other-stage',False),('text',False),('width',False),('user',False),('wrong-label',False),('missing-witness',False)]:
            result=subprocess.run(['node','-e',setup+'console.log(JSON.stringify('+expression+'));',mode],capture_output=True,text=True,check=True)
            self.assertIs(json.loads(result.stdout),wanted,mode)

    def test_empty_visible_controls_still_require_no_clipping(self):
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['controls'][0].update(visible=False))
        self.reject(lambda v:self.row(v,'group-empty')['geometry']['controls'][0]['rect'].update(top=780,bottom=804))


if __name__=='__main__':unittest.main()
