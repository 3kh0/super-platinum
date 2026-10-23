"""Focused synthetic receipt mutations; no native execution/visual claim."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch
import profile_empty_canvas_title as O
import profile_empty_canvas_completion as C
import test_profile_file_titles_native as T
import test_profile_user_title_completion as Q

RUN=runpy.run_path(str(Path(__file__).with_name('check-profile-empty-canvas-title-native.py')))
H=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
PATH=Q.PATH


def existing(narrow=False):
    T.R['MODE'].update(list=False,blocks=True)
    fixture,r=T.sample(PATH,narrow)
    r['emptyCanvas']=O.snapshot('existing')
    for i,row in enumerate(r['stages']):
        stage=row['stage'];g=row['geometry'];proof=row['proof'];m=row['model']
        prefix='Working with ' if i<3 else 'x'*(int(stage[5:])-38) if stage.startswith('limit') else 'Saved with '
        m['blocks']=T.R['blocks'](prefix)
        rich=stage in T.R['RICH'];changed=stage in ('renamed-mounted','confirmed','retried','held','cancelled')
        label='Untitled' if stage in ('confirmed','retried') else 'Canvas before empty'
        for token in g['tokens']:token['text']=label
        if rich:
            g['captured']['snapshot']['blocks']=copy.deepcopy(m['blocks'])
            for token in g['captured']['tokens']:token['label']=label
        if i:
            raw=O.raw_file(changed)['file'];effective=copy.deepcopy(raw);effective.pop('fixture_opaque')
            cache=proof['cache'];cache.update(source=raw,effective=effective,
                canvasFallback=dict(current=cache['current'],descriptor=copy.deepcopy(O.DESCRIPTOR)))
            if changed:cache.pop('quipTitle',None)
        proof['emptyCanvas']=O.snapshot('existing')
        source=proof['source']
        for request in source['requests']:
            serial=request['serial']
            if serial==1 and i>=1:request.update(response=dict(raw=O.raw_file()),releasedResponse=O.raw_file())
            if serial==2:request['response']=dict(error='offline')
            if serial==3:request['response']=dict(raw=O.raw_file(True))
            if serial==4 and i>=7:request['releasedResponse']=O.raw_file()
            if serial==5:request['response']=dict(raw=O.raw_file())
        if stage=='renamed-mounted':proof['exercise']['frozenLabel']='Canvas before empty'
        if stage=='confirmed':proof['exercise']['reopenedLabel']='Untitled'
        if stage.startswith('limit'):proof['exercise']['labelUnits']=19
    return fixture,r


def completion(narrow=False):
    fixture,r=Q.sample('known',narrow)
    r.pop('titleCompletion');r.pop('savedBlocks');r['case']='completion';r['emptyCanvas']=O.snapshot('completion')
    raw=O.completion_bytes();raw_files=json.loads(raw)['files']
    def relabel(value):
        if isinstance(value,dict):
            for key,item in value.items():
                if key in ('label','text') and item==Q.U.label('known'):value[key]='Untitled'
                else:relabel(item)
        elif isinstance(value,list):
            for item in value:relabel(item)
    relabel(r)
    for row in r['stages']:
        p=row['proof'];p.pop('titleCompletion');p['emptyCanvas']=O.snapshot('completion')
        for req in row['source']['requests']:
            if req['method']=='search.autocomplete.files':req['raw']=raw
            if req['method']=='files.info':req['raw']=dict(ok=True,file=copy.deepcopy(raw_files[0]))
        choices=p['choices']
        if choices:
            choices.pop('titleUsers',None)
            choices['canvasFallback']={'FCOMPLETE1':dict(current=True,descriptor=copy.deepcopy(O.DESCRIPTOR),
                quipTitle=dict(fileId='FCOMPLETE1',current=True,value='nullish'))}
            p['exercise']['choices']=copy.deepcopy(choices)
        if p['sourceFiles']:
            for new,old in zip(raw_files,p['sourceFiles']['rows']):
                channels=old['effective']['channels'];old.update(source=copy.deepcopy(new),effective=dict(new,channels=channels))
        if row['stage']=='saved':p['exercise']['reopened']=dict(ok=True,native=copy.deepcopy(row['geometry']['native']))
    r['source']=copy.deepcopy(r['stages'][-1]['source'])
    return fixture,r


class EmptyTitleTests(unittest.TestCase):
    def setUp(self):self.case='existing';self.fixture,self.value=existing()
    def row(self,stage):return next(r for r in self.value['stages'] if r['stage']==stage)
    def validate(self):return O.validate(self.fixture,self.value,PATH,H,case=self.case,platform='win32',png=False)
    def reject(self,fn):
        fn()
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError,IndexError)):self.validate()
    def choose_completion(self):self.case='completion';self.fixture,self.value=completion()
    def test_positive_both_cases_widths(self):
        for case,factory in [('existing',existing),('completion',completion)]:
            for narrow in (False,True):
                self.case=case;self.fixture,self.value=factory(narrow);self.validate()
    def test_exact_two_cases(self):
        self.assertEqual(RUN['CASES'],('existing','completion'))
        with self.assertRaises(RuntimeError):RUN['harness']('failure')
    def test_baseline_prefix_and_capture_protocol_unchanged(self):
        a=RUN['harness']('existing');b=RUN['harness']('completion')
        self.assertEqual(a['PREFIX'],'profile-file-titles-check: ')
        self.assertEqual(b['PREFIX'],'profile-file-completion-check: ')
        self.assertEqual(a['EDGE'],'profile-sections-mentions-capture: ')
        self.assertEqual(b['EDGE'],'profile-channel-completion-capture: ')
        self.assertEqual(tuple(b['STAGES']),('opened','query-ready','inserted','saved'))
        self.assertIs(a['validate_closed'].__globals__,a)
        self.assertIs(b['validate_closed'].__globals__,b)
    def test_exact_named_flags_without_launch(self):
        for case in ('existing','completion'):
            cls=RUN['harness'](case)['NativeFixture'];parent=cls.__bases__[0];seen=[]
            with patch.object(parent,'__init__',lambda self,*a,**k:seen.append((a,k))):
                cls('profile-sections-mentions',Path('unused'),Path('unused'),{})
            self.assertEqual(seen[0][0][-1],{RUN['FLAG']:'1',RUN['CASE_FLAG']:case})
    def test_conflicting_flag_refused_before_launch(self):
        for case in ('existing','completion'):
            cls=RUN['harness'](case)['NativeFixture'];parent=cls.__bases__[0]
            with patch.object(parent,'__init__',side_effect=AssertionError('must not launch')):
                for field in (RUN['FLAG'],RUN['CASE_FLAG']):
                    with self.assertRaises(RuntimeError):cls('profile-sections-mentions',Path('unused'),Path('unused'),{field:'other'})
    def test_no_default_fallback(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache'].pop('canvasFallback'))
    def test_locale_not_guessed(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['canvasFallback']['descriptor'].update(locale='fr-FR'))
    def test_source_origin_not_slack_catalog(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['canvasFallback']['descriptor'].update(origin='slack-locale'))
    def test_stale_fallback_refused(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['canvasFallback'].update(current=False))
    def test_cancelled_fallback_not_current(self):
        self.reject(lambda:self.row('cancelled')['proof']['cache']['canvasFallback'].update(current=True))
    def test_original_nonempty_label_frozen(self):
        self.reject(lambda:self.row('renamed-mounted')['geometry']['tokens'][0].update(text='Untitled'))
    def test_fresh_empty_label_not_old(self):
        self.reject(lambda:self.row('confirmed')['geometry']['tokens'][0].update(text='Canvas before empty'))
    def test_retry_empty_label(self):
        self.reject(lambda:self.row('retried')['geometry']['tokens'][0].update(text='Canvas before empty'))
    def test_plain_whitespace_not_nonempty(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['effective'].update(title='new title'))
    def test_blocks_empty_not_missing(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['source'].pop('title_blocks'))
    def test_no_quip_observation_on_empty_array(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache'].update(quipTitle=dict(current=True,value='nullish',fileId=O.ID)))
    def test_nonempty_quip_current_required(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['quipTitle'].update(current=False))
    def test_actual_retry_response_empty(self):
        self.reject(lambda:self.row('retried')['proof']['source']['requests'][2].update(response=dict(raw=O.raw_file())))
    def test_late_original_release_preserved(self):
        self.reject(lambda:self.row('cancelled')['proof']['source']['requests'][3].pop('releasedResponse'))
    def test_no_cancelled_adoption(self):
        self.reject(lambda:self.row('cancelled')['proof']['cache'].update(current=True))
    def test_no_extra_write(self):
        self.reject(lambda:self.row('confirmed')['model']['attempts'].append([]))
    def test_label_never_enters_wire(self):
        self.reject(lambda:self.row('confirmed')['model']['blocks'][0]['elements'][0]['elements'][1].update(text='Untitled'))
    def test_original_style_preserved(self):
        self.reject(lambda:self.row('opened')['geometry']['tokens'][0].update(fontWeight='400'))
    def test_source_utf16_limit_not_url(self):
        self.reject(lambda:self.row('limit5000')['proof']['exercise'].update(labelUnits=len(O.URL)))
    def test_original_owner_preserved(self):
        self.reject(lambda:self.row('retried')['model'].update(epoch=1))
    def test_png_owned_path(self):
        self.reject(lambda:self.value['captures'].update(confirmed=str(PATH/'foreign.png')))
    def test_source_bytes_not_reserialized(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['source']['requests'][-1].update(raw=json.dumps(json.loads(O.completion_bytes()))))
    def test_completion_fallback_original_query(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback']['FCOMPLETE1'].update(current=False))
    def test_completion_quip_observation_required(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback']['FCOMPLETE1'].pop('quipTitle'))
    def test_completion_quip_original_file(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback']['FCOMPLETE1']['quipTitle'].update(fileId='FCOMPLETE2'))
    def test_completion_quip_current(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback']['FCOMPLETE1']['quipTitle'].update(current=False))
    def test_completion_quip_empty_text_not_nullish(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback']['FCOMPLETE1']['quipTitle'].update(value='text',text=''))
    def test_no_numeric_currentness(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['canvasFallback'].update(current=1))
    def test_original_source_opaque_not_normalized(self):
        self.reject(lambda:self.row('confirmed')['proof']['cache']['source'].pop('fixture_opaque'))
    def test_list_has_no_canvas_fallback(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['canvasFallback'].update(FCOMPLETE2=dict(current=True,descriptor=O.DESCRIPTOR)))
    def test_whitespace_source_preserved(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].update(title_blocks=[]))
    def test_plain_title_does_not_replace_extraction(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['values'][5].update(label='Misleading plain title'))
    def test_private_choice_wire(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices']['values'][5]['value'].update(text='Untitled'))
    def test_choice_original_index(self):
        self.choose_completion()
        self.reject(lambda:self.row('inserted')['proof']['exercise']['choice'].update(index=6))
    def test_choice_original_sequence(self):
        self.choose_completion()
        self.reject(lambda:self.row('inserted')['proof']['exercise']['choice'].update(sequence=99))
    def test_source_order_preserved(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
    def test_fresh_info_original_canvas(self):
        self.choose_completion()
        self.reject(lambda:self.row('saved')['source']['requests'][-1]['args'].update(file='FCOMPLETE2'))
    def test_query_no_member_title_borrow(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['proof']['choices'].update(titleUsers={'FCOMPLETE1':{}}))
    def test_reopened_label_exact(self):
        self.choose_completion()
        self.reject(lambda:self.row('saved')['proof']['exercise']['reopened']['native']['tokens'][2].update(label='Misleading plain title'))
    def test_no_extra_search(self):
        self.choose_completion()
        self.reject(lambda:self.row('query-ready')['source']['requests'].append(dict(serial=7,method='users/search',args=dict(query='other'))))
    def test_baselines_unchanged(self):
        import hashlib
        root=Path(__file__).resolve().parents[1]
        pins=json.loads((root/'tmp/profile-empty-canvas-title-20261022/oracle/baseline-pins.json').read_text())
        for item in pins['files']:
            self.assertEqual(hashlib.sha256((root/item['path']).read_bytes()).hexdigest(),item['sha256'],item['path'])

if __name__=='__main__':unittest.main()
