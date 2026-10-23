"""Synthetic portable positives and meaningful mutations; not native evidence."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import slack_list_link_title as O

HERE = Path(__file__).parent
RUN = runpy.run_path(str(HERE / 'check-list-link-title-native.py'))
# Reuse the complete old synthetic lifecycle, changing only exact metadata and label.
_path = HERE / 'list_empty_canvas_reply_test_data.py'
_code = _path.read_text(encoding='utf-8')
assert _code.count('import slack_list_empty_canvas_reply as O') == 1
_code = _code.replace('import slack_list_empty_canvas_reply as O', 'import slack_list_link_title as O')
_code = _code.replace("'Untitled'", 'S.LABEL')
D = {'__file__': str(_path)}
exec(compile(_code, str(_path), 'exec'), D)
_sample = D['sample']


def sample(narrow=False):
    fixture, receipt = _sample(narrow)
    for row in receipt['stages']:
        row['model']['titleAcquisition'] = dict(generation=0, rows=[], running=False)
        if 'beforeReopen' in row['proof']:
            # The upstream user-title corpus had a different Boot seed; this
            # no-user lifecycle retains the same actual seed across reopen.
            row['proof']['beforeReopen']['completionSource']['seed'] = copy.deepcopy(
                row['model']['completionSource']['seed'])
        if row['stage'] in ('sent', 'readback'):
            job = row['model']['titleUsers']
            job['current'] = job['observed']['current'] = job['observed']['titles']['current'] = False
            job['observed']['titles']['canvasFallback'][O.S.CANVAS]['current'] = False
            message = row['proof']['renderedFiles']['message']
            tokens = [n for n in O.R['nodes'](message['blocks']) if n.get('type') in ('canvas','file')]
            envelope = json.loads(O.S.search_text())
            sources = {v['id']:v for v in envelope['files']}
            direct = dict(current=True, sourceCurrent=True, generation=row['model']['comments']['generation'],
                          socket=8, team='T1', ts=message['ts'], rows=[])
            native = dict(ok=True, sameRow=True, ts=message['ts'], tokens=[])
            for ordinal, raw in enumerate(tokens):
                fid = raw['file_id']; label = O.S.LABEL if fid == O.S.CANVAS else 'Alex List'
                direct['rows'].append(dict(ordinal=ordinal, raw=copy.deepcopy(raw), current=True, label=label,
                    file=dict(current=True, id=fid, source=copy.deepcopy(sources[fid]),
                              effective=O.S.U.title_projection(sources[fid]), receipt=copy.deepcopy(envelope)),
                    quipTitle=dict(fileId=fid,current=True,value='nullish') if fid==O.S.CANVAS else None))
                native['tokens'].append(dict(ordinal=ordinal,id=fid,kind=raw['type'],label=label,tag='BUTTON',
                    connected=True,sameNode=True,disabled=False,state='ready',ariaDisabled='false',ariaBusy='false',title='Open file'))
            row['proof']['directTranscript'] = dict(before=direct,after=copy.deepcopy(direct),
                nativeBefore=native,nativeAfter=copy.deepcopy(native))
    receipt['final'] = copy.deepcopy(receipt['stages'][-1]['model'])
    return fixture, receipt


class LinkTitleTests(unittest.TestCase):
    def setUp(self):
        self.fixture, self.receipt = sample()

    def row(self, stage='reopened'):
        return next(r for r in self.receipt['stages'] if r['stage'] == stage)

    def validate(self):
        O.validate(self.fixture, self.receipt, D['DIR'], png=False)

    def reject(self, mutation):
        mutation()
        with self.assertRaises((RuntimeError, KeyError, ValueError, TypeError, IndexError)):
            self.validate()

    def test_complete_both_widths(self):
        for narrow in (False, True):
            self.fixture, self.receipt = sample(narrow)
            self.validate()

    def test_unchanged_closed_runner(self):
        h = RUN['harness']()
        self.assertIs(h['run_fixture'].__globals__, h)
        self.assertIs(h['validate_log'].__globals__, h)
        self.assertEqual(h['PREFIX'], 'slack-list-comments-compose-check: ')
        self.assertEqual(h['case_contract'](O.LIFECYCLE)[0], O.STAGES[O.LIFECYCLE])

    def test_exact_gate(self):
        cls = RUN['harness']()['NativeFixture']
        seen = []
        checks = {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': O.LIFECYCLE}
        with patch.object(cls.__bases__[0], '__init__', lambda self, *a, **kw: seen.append(a[-1])):
            cls('slack-list', Path('log'), Path('binary'), checks)
        self.assertEqual(seen, [dict(checks, **{RUN['FLAG']: '1'})])
        self.assertNotIn(RUN['FLAG'], checks)

    def test_foreign_mode_refused(self):
        cls = RUN['harness']()['NativeFixture']
        with patch.object(cls.__bases__[0], '__init__') as init:
            for checks in ({}, {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': 'empty-canvas-lifecycle'},
                           {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE': O.LIFECYCLE, RUN['FLAG']: '0'}):
                with self.assertRaises(RuntimeError):
                    cls('slack-list', Path('log'), Path('binary'), checks)
            init.assert_not_called()

    def test_raw_link_input_and_literal_output(self):
        raw = json.loads(O.S.search_text())['files'][0]
        self.assertEqual(raw['title'], 'Alex Canvas')
        self.assertEqual(raw['title_blocks'], O.S.blocks())
        self.assertIn('&amp; %2F', O.S.LABEL)
        self.assertIn('a%2Fb?x=1&amp;y=2', O.S.LABEL)
        with self.assertRaises(RuntimeError): O.S.singleton(O.S.CANVAS, True)

    def test_wrong_raw_response(self):
        self.reject(lambda: self.row('query-ready')['model']['completionSource']['responses'][-1].__setitem__('raw', '{}'))

    def test_sorted_original_channel_order(self):
        self.reject(lambda: self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].__setitem__('channels', ['CAMAP001','CZMAP001','CTAIL001']))

    def test_decoded_label_refused(self):
        canvas = next(v for v in self.row('query-ready')['model']['completion']['values']
                      if v['value'].get('file_id') == O.S.CANVAS)
        self.reject(lambda: canvas.__setitem__('label', O.S.LABEL.replace('&amp;', '&')))

    def test_explicit_link_cannot_use_url(self):
        self.reject(lambda: self.row()['model']['titleUsers']['observed']['titles']['files'][0]['source']['title_blocks'][0]['elements'][0]['elements'][0].__setitem__('text', 'https://example.test/not-the-label'))

    def test_missing_quip(self):
        self.reject(lambda: self.row('query-ready')['model']['completion']['canvasFallback'][O.S.CANVAS].__setitem__('quipTitle', None))

    def test_stale_choice(self):
        self.reject(lambda: self.row('canvas-inserted')['proof']['inputs'][1]['publication'].__setitem__('current', False))

    def test_draft_query_receipt_borrow(self):
        self.reject(lambda: self.row()['model']['titleUsers'].__setitem__('observed', copy.deepcopy(self.row('query-ready')['model']['completion'])))

    def test_draft_capture(self):
        self.reject(lambda: self.row()['model']['titleUsers'].__setitem__('capture', 'foreign'))

    def test_policy_not_acquired(self):
        self.reject(lambda: self.row()['model']['titleUsers']['observed'].__setitem__('policy', {}))

    def test_members_not_acquired(self):
        self.reject(lambda: self.row()['model']['titleUsers']['observed']['titles'].__setitem__('users', {}))

    def test_wire_style_preserved(self):
        self.reject(lambda: self.row()['proof']['expectedBlocks'][0]['elements'][0]['elements'][1].pop('opaque'))

    def test_ordinary_list_cannot_borrow_canvas(self):
        self.reject(lambda: self.row('query-ready')['model']['completion']['canvasFallback'].__setitem__(O.S.LIST, D['fallback']()))

    def test_native_remount_is_new(self):
        self.reject(lambda: self.row()['geometry']['editor'].__setitem__('sameNode', True))

    def test_held_send_inert(self):
        self.reject(lambda: self.row('sending')['geometry']['editor'].__setitem__('editable', True))

    def test_transcript_must_not_invent_async_job(self):
        self.reject(lambda: self.row('sent')['model']['titleAcquisition'].__setitem__('rows', [{'current': True}]))

    def test_transcript_must_not_reuse_reply(self):
        self.reject(lambda: self.row('sent')['model']['titleUsers'].__setitem__('current', True))

    def test_transcript_source_snapshot_disagreement(self):
        self.reject(lambda: self.row('sent')['model']['coldFiles']['source'].__setitem__('unavailableIds', [O.S.CANVAS]))

    def test_transcript_unknown_generation(self):
        self.reject(lambda: self.row('sent')['model']['comments'].__setitem__('generation', 0))

    def test_transcript_stale_quip(self):
        self.reject(lambda: self.row('sent')['model']['titleUsers']['observed']['titles']['canvasFallback'][O.S.CANVAS]['quipTitle'].__setitem__('current', False))

    def test_transcript_changed_received_occurrence(self):
        message = self.row('sent')['proof']['renderedFiles']['message']
        token = next(n for n in O.R['nodes'](message['blocks']) if n.get('type') == 'canvas')
        self.reject(lambda: token.__setitem__('file_id', O.S.LIST))

    def direct_change(self, key, mutation):
        proof = self.row('sent')['proof']['directTranscript']
        mutation(proof[key])
        proof[{'before':'after','nativeBefore':'nativeAfter'}[key]] = copy.deepcopy(proof[key])

    def test_direct_source_stale(self):
        self.reject(lambda:self.direct_change('before',lambda p:p.__setitem__('sourceCurrent',False)))

    def test_direct_token_stale(self):
        self.reject(lambda:self.direct_change('before',lambda p:p['rows'][0].__setitem__('current',False)))

    def test_direct_file_source_stale(self):
        self.reject(lambda:self.direct_change('before',lambda p:p['rows'][0]['file'].__setitem__('current',False)))

    def test_direct_quip_stale(self):
        self.reject(lambda:self.direct_change('before',lambda p:p['rows'][0]['quipTitle'].__setitem__('current',False)))

    def test_direct_foreign_generation(self):
        self.reject(lambda:self.direct_change('before',lambda p:p.__setitem__('generation',999)))

    def test_direct_foreign_socket(self):
        self.reject(lambda:self.direct_change('before',lambda p:p.__setitem__('socket',9)))

    def test_direct_preboot_socket_refused(self):
        self.reject(lambda:self.direct_change('before',lambda p:p.__setitem__('socket',7)))

    def test_direct_boot_not_accepted(self):
        self.reject(lambda:self.row('sent')['model']['completionSource']['seed'].__setitem__('accepted',False))

    def test_direct_reopen_not_performed(self):
        self.reject(lambda:self.row('sent')['proof'].__setitem__('runtimeForgotten',False))

    def test_direct_foreign_raw(self):
        self.reject(lambda:self.direct_change('before',lambda p:p['rows'][0]['raw'].__setitem__('url','https://foreign.invalid')))

    def test_direct_foreign_order(self):
        self.reject(lambda:self.direct_change('before',lambda p:p['rows'].reverse()))

    def test_direct_node_disabled(self):
        self.reject(lambda:self.direct_change('nativeBefore',lambda p:p['tokens'][0].__setitem__('disabled',True)))

    def test_direct_node_replaced(self):
        self.reject(lambda:self.direct_change('nativeBefore',lambda p:p['tokens'][0].__setitem__('sameNode',False)))

    def test_direct_node_loading(self):
        self.reject(lambda:self.direct_change('nativeBefore',lambda p:p['tokens'][0].__setitem__('state','loading')))

    def test_direct_across_capture_change(self):
        self.reject(lambda:self.row('sent')['proof']['directTranscript']['after'].__setitem__('current',False))

    def test_extra_send_refused(self):
        self.reject(lambda: self.row('readback')['model']['composeLedger']['posts'].append(copy.deepcopy(self.row('readback')['model']['composeLedger']['posts'][0])))

    def test_transcript_occlusion(self):
        self.reject(lambda: self.row('sent')['proof']['renderedFiles']['before']['tokens'][0].__setitem__('hit', False))

    def test_foreign_owner(self):
        self.reject(lambda: self.row()['model'].__setitem__('epoch', 99))

    def test_final_model_unchanged(self):
        self.reject(lambda: self.receipt['final'].__setitem__('epoch', 99))


if __name__ == '__main__': unittest.main()
