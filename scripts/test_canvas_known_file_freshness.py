"""Portable complete receipts plus custody, original-wire and stale-result mutations."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

import canvas_known_file_freshness as O
from test_canvas_canonical_file_roots import rect, read, retained, visible


def ledger(case, i):
    count = O.COUNTS[case][i]
    rows = [(n, mode, 1 if n == 1 else 2)
            for n, mode in enumerate(O.MODES[case][:count], 1)]
    completed = (0 if i <= 2 else count) if case == 'success' else (0 if i <= 2 else 1 if i <= 4 else 2)
    releases = ([O.release(1, True)] if i >= 3 else []) if case == 'success' else (
        ([O.release(1, False)] if i >= 4 else []) + ([O.release(2, True)] if i == 5 else []))
    held = [dict(serial=1, id=O.ID, receiverClosed=False)] if i == 2 else []
    if case == 'retirement' and i in (3, 4):
        held = ([dict(serial=1, id=O.ID, receiverClosed=True)] if i == 3 else []) + [
            dict(serial=2, id=O.ID, receiverClosed=False)]
    return dict(requests=[dict(serial=n, id=O.ID, method='files.info', path='/api/files.info',
        args=copy.deepcopy(O.ARGS), mode=m, version=v, reason=O.REASON) for n, m, v in rows],
        responses=[dict(serial=n, id=O.ID, disposition=m, version=v, raw=O.response(m, v))
                   for n, m, v in rows],
        settlements=[dict(serial=n, cancelled=case == 'retirement' and n == 1)
                     for n in range(1, completed + 1)], releases=releases, held=held,
        completed=completed, cancelled=int(case == 'retirement' and i >= 3),
        networkCalls=0, httpCapability=False)


def sample(case='success', viewport=(668, 380)):
    stages = []
    for i, name in enumerate(O.STAGES[case]):
        current = name in ('known', 'draft', 'renamed', 'recovered')
        version = 2 if name == 'recovered' else 1 if case == 'success' and i >= 3 else 0
        raw = O.positive(version)['file'] if version else O.D.file(O.ID)
        phase = None if i < 2 else 'ready' if current else 'unavailable' if name in (
            'offline', 'malformed', 'rejected') else 'loading'
        notice = O.OFFLINE if name == 'offline' else O.FAILED if name in ('malformed', 'rejected') else None
        response = O.positive(version) if phase == 'ready' else O.response('malformed', 2) if name == 'malformed' else O.response('error', 2) if name == 'rejected' else None
        event = 1 if i in (2, 3) and case == 'success' or i == 2 else 2 if i >= 3 else None
        rows = [O.D.roots()[0], O.D.REPLY]
        v = retained(rows, True, 2)
        v.update(pendingFile=False, navigationError=None, catalog=dict(serial=2,
            origins=[dict(ts=O.D.ROOT, firstId=O.ID, blockId='original-' + O.D.ROOT)],
            rows=[dict(id=O.ID, source=raw, effective=raw, current=current,
                       link=dict(name=raw['name'], url=raw['permalink']))]),
            knownAcquisition=dict(running=phase == 'loading', cooling=False, rows=[] if i < 2 else [
                dict(id=O.ID, rootTs=O.D.ROOT, phase=phase, current=True,
                     retryable=phase == 'unavailable', notice=notice, receipt=response,
                     demand='known-event', eventRevision=event)]))
        acq = ledger(case, i)
        proof = dict(requests=[read(), read('replies', O.D.ROOT)],
            responses=[O.D.envelope(), O.D.envelope(thread=True)], retained=v, originals=rows,
            resolver=dict(requests=[], responses=[], settlements=[], releases=[], held=None),
            media=[dict(url=url, registered=0) for url in O.D.URLS], known=copy.deepcopy(acq))
        blocks = O.C.blocks() if i == 0 else O.C.edited()
        text = 'Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.' + (
            ' Canonical tail \u4e2d\U0001f98a' if i else '')
        reply = dict(runtime=dict(blocks=blocks, text=text, loading=False, saving=False,
            sending=False, error=None, capture='capture-b' if i else 'capture-a', serial=1),
            journal=dict(drafts=[dict(id='draft-a', blocks=blocks, text=text,
                revision=2 if i else 1, capture='capture-b' if i else 'capture-a')], attempt=None, history=[]))
        layout = dict(ok=True, viewport=list(viewport), inScroll=True, scroll=rect(0, 0, 600, 300),
            target=visible('actual original target'), targetKind='editor-line' if name == 'draft' else 'link' if current else 'metadata-status',
            native=dict(blocks=blocks, composing=False, running=True, mount='mount-1', scope='original-scope',
                        generation=1, sameNode=i > 0, sameMount=i > 0, sameBlocks=i > 0),
            sourceIdentity=dict(sameRow=True, sameEditor=True, sameControls=True, root=O.D.ROOT),
            body=raw['name'] if current else rows[0]['text'], images=[], viewer=None,
            link=dict(id=O.ID, name=raw['name'], url=raw['permalink'], root=O.D.ROOT) if current else None,
            status=None if current else dict(text=O.LOADING if phase == 'loading' else notice, phase=phase),
            retry=dict(visible('Retry file details'), disabled=False, style=dict(opacity=1, cursor='pointer'))
                  if phase == 'unavailable' else None)
        stage = dict(ok=True, noSession=True, noTransport=True, case=case, stage=name, ordinal=i,
                     layout=layout, proof=proof, acquisition=acq, reply=reply, exercise={})
        obs = copy.deepcopy(O.observation(stage))
        ex = stage['exercise']
        if name != 'draft':
            ex['quiet'] = dict(before=copy.deepcopy(obs), after=copy.deepcopy(obs), observedMs=501)
        if name in ('loading', 'offline', 'superseded'):
            ex['event'] = dict(id=O.ID, team='T1', before=event - 1, after=event)
        if name in ('malformed', 'rejected') or name == 'recovered' and case == 'success':
            ex['retry'] = dict(ok=True, before=dict(root=O.D.ROOT, text='Retry file details', disabled=False),
                               sameNode=True, connected=True)
        if name in ('renamed', 'late-refused') or name == 'recovered' and case == 'retirement':
            ex['release'] = O.release(2 if name == 'recovered' else 1, name != 'late-refused')
        if name == 'late-refused':
            ex['before'] = copy.deepcopy(O.observation(stages[-1]));ex['after'] = copy.deepcopy(obs)
        stages.append(stage)
    return copy.deepcopy(dict(ok=True, case=case, noSession=True, noTransport=True,
        draftPreserved=True, originalReadRetained=True, noNavigation=True, noAutomaticLoop=True,
        requestCount=2, acquisitionCount=5 if case == 'success' else 2, resolverCount=0,
        stages=stages, **O.observation(stages[-1])))


def refresh_quiet(stage):
    """Keep incidental equality honest so mutations exercise deeper policy."""
    obs = copy.deepcopy(O.observation(stage))
    if 'quiet' in stage['exercise']:
        stage['exercise']['quiet'].update(before=copy.deepcopy(obs), after=obs)


class KnownOracle(unittest.TestCase):
    def refuse(self, mutate, case='success'):
        r = sample(case);mutate(r)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            O.validate(r)

    def test_complete_cases_both_widths(self):
        for case in O.STAGES:
            for viewport in ((668, 380), (1088, 728)):
                O.validate(sample(case, viewport))

    def test_case_order_isolation_terminal(self):
        for key, value in [('case', 'cold'), ('requestCount', 3), ('acquisitionCount', True),
                           ('noNavigation', False), ('noSession', False)]:
            self.refuse(lambda r: r.update({key: value}))
        self.refuse(lambda r: r['stages'][2].update(case='retirement'))
        self.refuse(lambda r: r['stages'].reverse())

    def test_known_wire_not_absent_or_navigation_factory(self):
        for args in ({**O.ARGS, 'skip_shares': 'true'}, {**O.ARGS, '_x_reason': 'files-store-unknown-fetch'},
                     {'file': O.ID}, {**O.ARGS, 'file': 'FCANON002'}):
            self.refuse(lambda r: r['stages'][2]['acquisition']['requests'][0].update(args=args))

    def test_raw_opaque_source_and_exact_version(self):
        self.refuse(lambda r: r['stages'][3]['acquisition']['responses'][0]['raw']['file'].pop('fixture_opaque'))
        self.refuse(lambda r: r['stages'][5]['acquisition']['responses'][2].update(raw=O.positive(2)))
        self.refuse(lambda r: r['stages'][6]['acquisition']['responses'][3]['raw'].update(ok=True))
        self.refuse(lambda r: r['stages'][3]['acquisition']['requests'][0].update(version=2))

    def test_no_current_known_fetch_or_retry_loop(self):
        for i in (0, 1, 3, 4):
            self.refuse(lambda r: r['stages'][i]['acquisition']['requests'].append({}))
        self.refuse(lambda r: r['stages'][4]['exercise']['quiet'].update(observedMs=499))
        self.refuse(lambda r: r['stages'][0]['exercise']['quiet']['after']['proof']['requests'].append({}))

    def test_actual_event_before_dispatch_and_successor_owner(self):
        for patch in ({'team': 'OTHER'}, {'id': 'FCANON002'}, {'after': 0}, {'after': True}):
            self.refuse(lambda r: r['stages'][2]['exercise']['event'].update(patch))
        self.refuse(lambda r: r['stages'][4]['exercise']['event'].update(before=99))
        self.refuse(lambda r: r['stages'][3]['proof']['retained']['knownAcquisition']['rows'][0].update(eventRevision=1), 'retirement')

    def test_attempt_stored_notice_separate_from_loading_copy(self):
        self.refuse(lambda r: r['stages'][2]['proof']['retained']['knownAcquisition']['rows'][0].update(notice=O.LOADING))
        self.refuse(lambda r: r['stages'][2]['layout']['status'].update(text=None))
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['knownAcquisition']['rows'][0].update(demand='absent'))

    def test_stale_link_and_title_substitution_refused(self):
        self.refuse(lambda r: r['stages'][2]['layout'].update(link=r['stages'][0]['layout']['link']))
        self.refuse(lambda r: r['stages'][3]['layout'].update(body=O.positive(1)['file']['title']))
        self.refuse(lambda r: r['stages'][4]['proof']['retained']['catalog']['rows'][0].update(current=True))

    def test_same_node_mount_controls_and_private_draft(self):
        for key in ('sameNode', 'sameMount', 'sameBlocks'):
            self.refuse(lambda r: r['stages'][2]['layout']['native'].update({key: False}))
        self.refuse(lambda r: r['stages'][2]['layout']['sourceIdentity'].update(sameControls=False))
        self.refuse(lambda r: r['stages'][3]['reply']['journal']['drafts'][0].update(capture='new-capture'))
        self.refuse(lambda r: r['stages'][3]['reply']['runtime'].update(serial=2))
        self.refuse(lambda r: r['stages'][3]['reply']['journal']['history'].append({}))

    def test_coherent_mount_replacement_still_rejected(self):
        def change(r):
            s = r['stages'][3];s['layout']['native']['mount'] = 'replacement';refresh_quiet(s)
        self.refuse(change)

    def test_comment_source_never_replaced_or_navigated(self):
        self.refuse(lambda r: r['stages'][3]['proof']['originals'][0]['blocks'].clear())
        self.refuse(lambda r: r['stages'][3]['proof']['resolver']['requests'].append({}))
        self.refuse(lambda r: r['stages'][3]['proof']['retained'].update(serial=3))
        self.refuse(lambda r: r['stages'][3]['layout'].update(viewer={'ready': True}))

    def test_actual_original_late_release_not_rebound(self):
        self.refuse(lambda r: r['stages'][4]['exercise']['release'].update(delivered=True), 'retirement')
        self.refuse(lambda r: r['stages'][3]['acquisition']['held'][0].update(receiverClosed=False), 'retirement')
        self.refuse(lambda r: r['stages'][4]['exercise']['before']['proof']['retained']['catalog']['rows'][0].update(source=O.positive(1)['file']), 'retirement')
        self.refuse(lambda r: r['stages'][5]['proof']['retained']['catalog']['rows'][0].update(source=O.positive(1)['file']), 'retirement')

    def test_retry_original_control_and_geometry(self):
        self.refuse(lambda r: r['stages'][5]['exercise']['retry'].update(sameNode=False))
        self.refuse(lambda r: r['stages'][5]['layout']['retry'].update(disabled=True))
        self.refuse(lambda r: r['stages'][5]['layout']['retry']['style'].update(opacity=.45))
        self.refuse(lambda r: r['stages'][2]['layout']['target'].update(hit=False))
        self.refuse(lambda r: r['stages'][2]['layout']['target']['rect'].update(y=-1))

    def test_runner_owned_flags_and_wrong_case(self):
        path = Path(__file__).with_name('check-canvas-known-file-freshness.py')
        spec = importlib.util.spec_from_file_location('known_runner', path)
        runner = importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory() as folder:
            for case in O.STAGES:
                checks = runner.private_checks(runner.SPECS[case], Path(folder))
                self.assertEqual(checks['SUPER_PLATINUM_CANVAS_KNOWN_FILE_CASE'], case)
                self.assertEqual(checks['SUPER_PLATINUM_CANVAS_COLD_FILE_CHECK'], '0')
                self.assertEqual(checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            with self.assertRaises(RuntimeError):
                runner.private_checks(copy.deepcopy(runner.SPECS['success']), Path(folder))
        with self.assertRaises(RuntimeError):
            runner.validate(runner.SPECS['success'], O.FIXTURES[0], sample('retirement'))


if __name__ == '__main__':
    unittest.main()
