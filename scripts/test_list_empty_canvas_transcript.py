"""Portable receipt mutations; no native or visual acceptance is implied."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import slack_list_empty_canvas_transcript as O

RUN = runpy.run_path(str(Path(__file__).with_name('check-list-empty-canvas-transcript-native.py')))
DIR = Path('/private/cold-files')


def sample(case='plain-empty', narrow=False):
    test = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_cold_files.py')))
    scope = test['sample'].__globals__; base = O.adapted(case)
    scope['V'] = base
    fixture, receipt = scope['sample'](O.CASES[case], narrow)
    tokens = [v for v in base['N']['received']()['blocks'][0]['elements'][0]['elements'] if v['type'] in ('canvas', 'file')]
    for row in receipt['stages']:
        row['proof']['emptyCanvas'] = dict(case=case, expectedLabel='Untitled')
        for click in row['proof']['clicks']:
            if click['id'] == base['CANVAS']:
                click['before'] = scope['B']['measured']('Untitled')
        sources = {r['id']: r for r in row['model']['coldFiles']['source'].get('rows', [])}
        for attempt in row['model']['coldAcquisition']['rows']:
            attempt['titleUsers'] = None
            if attempt['state'] != 'ready':
                continue
            fid = attempt['id']; token = next(t for t in tokens if t['file_id'] == fid)
            canvas = fid == base['CANVAS']; fallback = {}
            if canvas:
                fallback[fid] = dict(current=True, descriptor=copy.deepcopy(O.DESCRIPTOR),
                    quipTitle=None if case == 'plain-empty' else dict(current=True, fileId=fid, value='nullish'))
            attempt['titleUsers'] = dict(current=True, policy=None, titles=dict(current=True,
                tokens=[copy.deepcopy(token)], required=[], users=None, files=[dict(id=fid,
                    source=copy.deepcopy(sources[fid]['source']),receipt=copy.deepcopy(sources[fid]['receipt']),
                    effective={**{k:copy.deepcopy(v) for k,v in sources[fid]['source'].items()
                        if k in ('id','mode','filetype','name','title','title_blocks','created','updated','timestamp','is_deleted','is_external','permalink')},
                        'is_deleted':False,'is_tombstoned':False,'isUnknown':False,'isNonExistent':False})],
                labels={json.dumps(token, separators=(',', ':'), sort_keys=True): 'Untitled' if canvas else 'Alex List'},
                canvasFallback=fallback))
    receipt['final'] = copy.deepcopy(receipt['stages'][-1]['model'])
    return fixture, receipt


class TranscriptEmptyTests(unittest.TestCase):
    def setUp(self):
        self.case = 'plain-empty'; self.fixture, self.receipt = sample()
    def row(self, stage='cold-loaded'):
        return next(r for r in self.receipt['stages'] if r['stage'] == stage)
    def title(self, stage='cold-loaded'):
        return self.row(stage)['model']['coldAcquisition']['rows'][0]['titleUsers']['titles']
    def validate(self):
        return O.validate(self.fixture, self.receipt, DIR, case=self.case, png=False)
    def reject(self, mutation):
        mutation()
        with self.assertRaises((RuntimeError, ValueError, KeyError, TypeError, IndexError)):
            self.validate()
    def recovery(self):
        self.case = 'blocks-empty'; self.fixture, self.receipt = sample(self.case)
    def test_both_cases_widths(self):
        for case in O.CASES:
            for narrow in (False, True):
                self.case = case; self.fixture, self.receipt = sample(case, narrow); self.validate()
    def test_baseline_harness_contract_preserved(self):
        for case, actual in O.CASES.items():
            h = RUN['harness'](case)
            self.assertEqual(tuple(h['case_contract'](actual)[0]), O.STAGES[case])
            self.assertEqual(h['PREFIX'], 'slack-list-comments-compose-check: ')
            self.assertEqual(h['EDGE'], 'slack-list-comments-compose-stage: ')
            self.assertIs(h['validate_log'].__globals__, h)
    def test_exact_gate_without_launch(self):
        for case, actual in O.CASES.items():
            cls = RUN['harness'](case)['NativeFixture']; parent = cls.__bases__[0]; seen = []
            with patch.object(parent, '__init__', lambda self,*a,**kw: seen.append((a,kw))):
                cls('slack-list', Path('unused'), Path('unused'), {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': actual})
            self.assertEqual(seen[0][0][-1], {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': actual,
                RUN['FLAG']: '1', RUN['CASE_FLAG']: case})
    def test_mismatched_gate_refused_before_launch(self):
        for case in O.CASES:
            cls = RUN['harness'](case)['NativeFixture']; parent = cls.__bases__[0]
            with patch.object(parent, '__init__', side_effect=AssertionError('must not launch')):
                for checks in ({}, {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': 'compose'},
                               {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': O.CASES[case], RUN['FLAG']: '1'}):
                    with self.assertRaises(RuntimeError):cls('slack-list', Path('unused'), Path('unused'), checks)
    def test_unknown_case_refused(self):
        with self.assertRaises(RuntimeError):RUN['harness']('other')
    def test_descriptor_required(self):
        self.reject(lambda: self.title().pop('canvasFallback'))
    def test_actual_english_not_slack_locale(self):
        self.reject(lambda: next(iter(self.title()['canvasFallback'].values()))['descriptor'].update(origin='slack-locale'))
    def test_wrong_locale_refused(self):
        self.reject(lambda: next(iter(self.title()['canvasFallback'].values()))['descriptor'].update(locale='fr-FR'))
    def test_currentness_original(self):
        self.reject(lambda: next(iter(self.title()['canvasFallback'].values())).update(current=False))
    def test_numeric_currentness_refused(self):
        self.reject(lambda: next(iter(self.title()['canvasFallback'].values())).update(current=1))
    def test_plain_empty_no_quip_consumption(self):
        self.reject(lambda: next(iter(self.title()['canvasFallback'].values())).update(quipTitle=dict(current=True,value='nullish',fileId='FLISTFILE1')))
    def test_blocks_actual_quip_required(self):
        self.recovery()
        self.reject(lambda: next(iter(self.title('cold-recovered')['canvasFallback'].values())).update(quipTitle=None))
    def test_blocks_quip_wrong_file(self):
        self.recovery()
        self.reject(lambda: next(iter(self.title('cold-recovered')['canvasFallback'].values()))['quipTitle'].update(fileId='FLISTFILE2'))
    def test_blocks_quip_not_override_empty(self):
        self.recovery()
        self.reject(lambda: next(iter(self.title('cold-recovered')['canvasFallback'].values()))['quipTitle'].update(value='text',text=''))
    def test_blocks_quip_not_stale(self):
        self.recovery()
        self.reject(lambda: next(iter(self.title('cold-recovered')['canvasFallback'].values()))['quipTitle'].update(current=False))
    def test_no_query_policy_borrow(self):
        self.reject(lambda: self.row()['model']['coldAcquisition']['rows'][0]['titleUsers'].update(policy={}))
    def test_no_member_acquisition(self):
        self.reject(lambda: self.title().update(required=['UOTHER'], users={}))
    def test_original_source_envelope(self):
        self.reject(lambda: self.title()['files'][0].update(receipt={'ok':True,'files':[]}))
    def test_original_effective_channel_order(self):
        self.reject(lambda: self.title()['files'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
    def test_title_projection_is_not_candidate_projection(self):
        self.reject(lambda: self.title()['files'][0].update(effective=copy.deepcopy(self.row()['model']['coldFiles']['source']['rows'][0]['effective'])))
    def test_title_status_flags_required(self):
        self.reject(lambda: self.title()['files'][0]['effective'].pop('isUnknown'))
    def test_title_status_cannot_be_tombstoned(self):
        self.reject(lambda: self.title()['files'][0]['effective'].update(is_tombstoned=True))
    def test_title_raw_source_keeps_channel_map(self):
        self.reject(lambda: self.title()['files'][0]['source'].pop('channel_map'))
    def test_title_raw_source_keeps_opaque(self):
        self.reject(lambda: self.title()['files'][0]['source'].pop('fixture_opaque'))
    def test_title_effective_permalink_exact(self):
        self.reject(lambda: self.title()['files'][0]['effective'].update(permalink='https://foreign.invalid/'))
    def test_original_canvas_occurrence_style(self):
        self.reject(lambda: self.title()['tokens'][0].update(style={'italic':True}))
    def test_display_never_enters_token(self):
        self.reject(lambda: self.title()['tokens'][0].update(text='Untitled'))
    def test_original_ordinal(self):
        self.reject(lambda: self.row()['model']['coldAcquisition']['rows'][0].update(ordinal=1))
    def test_no_list_fallback(self):
        self.reject(lambda: self.row()['model']['coldAcquisition']['rows'][1]['titleUsers']['titles'].update(canvasFallback={'FLISTFILE2':{}}))
    def test_no_missing_ready_receipt(self):
        self.reject(lambda: self.row()['model']['coldAcquisition']['rows'][0].update(titleUsers=None))
    def test_exact_label_map(self):
        self.reject(lambda: self.title().update(labels={'{}':'Untitled'}))
    def test_raw_token_not_replaced_by_label(self):
        self.reject(lambda: self.row()['model']['navigation']['received']['blocks'][0]['elements'][0]['elements'][0].update(text='Untitled'))
    def test_unavailable_cannot_navigate(self):
        self.reject(lambda: self.row('cold-held')['proof']['coldGeometry']['tokens'][0]['actual'].update(disabled=False))
    def test_source_bytes_not_reserialized(self):
        def change():
            c=self.row()['model']['coldFiles'];text=json.dumps(c['requests'][0]['plannedResponse'],sort_keys=True)
            c['requests'][0]['responseText']=c['responses'][0]['responseText']=text
        self.reject(change)
    def test_no_extra_policy_request(self):
        self.reject(lambda: self.row()['model']['completionSource']['requests'].append({'method':'users.prefs.get'}))
    def test_retry_original_control(self):
        self.recovery()
        self.reject(lambda: self.row('retry-held')['proof']['retries'][0]['control'].update(disabled=True))
    def test_record_departure_late_refusal(self):
        self.recovery()
        self.reject(lambda: self.row('visit-retired')['model']['coldFiles']['releases'][-1].update(delivered=True,cancelledBeforeReply=False))
    def test_no_late_successor_acquisition(self):
        self.recovery()
        self.reject(lambda: self.row('visit-retired')['model']['coldAcquisition']['rows'].append({'current':True}))
    def test_retirement_requires_original_receiver(self):
        self.recovery()
        self.reject(lambda: self.row('visit-retired')['proof']['cancelledVisitRead'].update(held=[]))
    def test_durable_original_draft(self):
        self.reject(lambda: self.row()['proof']['journal'].update(blocks=[]))
    def test_target_body_stays_independent(self):
        self.reject(lambda: self.row('canvas-open')['geometry']['title'].update(text='Untitled'))
    def test_original_hit_test(self):
        self.reject(lambda: self.row('canvas-open')['proof']['clicks'][0]['before'].update(hit=False))
    def test_original_png_path(self):
        self.reject(lambda: self.row().update(capture=str(DIR/'foreign.png')))
    def test_baseline_bytes_preserved(self):
        root=Path(__file__).resolve().parents[1]
        pins=json.loads((root/'tmp/list-empty-canvas-transcript-20261022/oracle/baseline-pins.json').read_text())
        for entry in pins['files']:
            self.assertEqual(hashlib.sha256((root/entry['path']).read_bytes()).hexdigest(),entry['sha256'])


if __name__ == '__main__':
    unittest.main()
