"""Pure workflow native receipt validation; no launches or Slack access."""
import copy
from pathlib import Path
import runpy
import tempfile
import unittest

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
DIRECTORY = Path('/private/workflow-captures')


def sample(scenario, narrow=False):
    fixture = 'workflow-' + scenario + ('-narrow' if narrow else '')
    viewport = [668, 380] if narrow else [1280, 780]
    rows = []
    for ordinal, stage in enumerate(R['STAGES'][scenario]):
        route = 'about' if stage in ('about', 'account-replaced') else 'managed' if stage in ('managed', 'empty-search') else 'editor'
        pending = stage in ('partial', 'unconfirmed', 'checked', 'checked-again')
        writes = 0 if stage in ('about', 'managed', 'empty-search', 'editor', 'draft') else 2
        title = 'Review requests' if stage == 'editor' else R['CHANGED_TITLE'] if stage == 'preflight-refused' else R['TITLE']
        description = 'Collect the details before a team review.' if stage == 'editor' else R['DESCRIPTION']
        outcomes = (['Workflow: Unconfirmed', 'Trigger: Unconfirmed'] if scenario == 'reply-loss'
                    else ['Workflow: Acknowledged', 'Trigger: Rejected']) if pending else [] if route != 'about' else None
        notice = {'saved': 'Workflow details saved.', 'partial': R['DIFFERENT'], 'unconfirmed': R['MATCHED'],
                  'checked': R['MATCHED'] if scenario == 'reply-loss' else R['DIFFERENT'],
                  'checked-again': R['MATCHED'] if scenario == 'reply-loss' else R['DIFFERENT'],
                  'adopted': 'Using the current workflow. Earlier update responses remain retained.',
                  'preflight-refused': R['REFUSED']}.get(stage)
        model = dict(route=route, visit=1 if route != 'about' else None, busy=False,
                     title=title if route == 'editor' else None, description=description if route == 'editor' else None,
                     dirty=pending or stage in ('draft', 'preflight-refused'), pending=pending, observed=pending,
                     notice=notice, outcomes=outcomes, writes=writes, reads=ordinal*8, networkCalls=0,
                     retainedDocuments=0 if stage in ('about', 'managed', 'empty-search') else 1,
                     retainedAttempts=0 if writes == 0 else 2 if stage == 'preflight-refused' else 1)
        box = dict(left=10, top=10, right=210, bottom=30, width=200, height=20)
        if route == 'about':
            ids = ['workflow-configuration']
        elif route == 'managed':
            ids = ['workflow-management-search'] + (['workflow-management-open-WF_REVIEW'] if stage == 'managed' else [])
        else:
            ids = ['workflow-management-check', 'workflow-management-managed'] + (['workflow-management-use-current'] if pending else [])
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, pngDimensions=[1280, 800],
                        save=dict(disabled=pending, opacity=.55 if pending else 1, rect=copy.deepcopy(box), visible=True) if route == 'editor' else None,
                        controls=[dict(id=i, rect=copy.deepcopy(box), visible=True) for i in ids],
                        feedback=[dict(text=t, rect=copy.deepcopy(box), visible=True) for t in ([notice] if notice else []) + (outcomes or [])],
                        fields=dict(title=title, description=description, titleReadonly=pending, descriptionReadonly=pending) if route == 'editor' else None)
        rows.append(dict(stage=stage, model=model, geometry=geometry))
    replacement = dict(before=copy.deepcopy(rows[-2]['model']), after=copy.deepcopy(rows[-1]['model']),
                       previousEpoch=3, currentEpoch=4, retainedPending=1, retainedDraftExact=True,
                       foreignDocumentHidden=True) if scenario == 'reply-loss' else None
    value = dict(ok=True, fixture=fixture, scenario=scenario, viewport=viewport, aboutConfiguration=True,
                 syntheticSession=True, noLiveNetwork=True, stages=rows, replacement=replacement,
                 captures={s: str(DIRECTORY / f'{fixture}-{s}.png') for s in R['STAGES'][scenario]},
                 final=copy.deepcopy(rows[-1]['model']))
    return fixture, value


class ReceiptTests(unittest.TestCase):
    def validate(self, fixture, value):
        R['validate'](fixture, value, DIRECTORY, platform='win32', png=False)

    def test_all_six_scene_receipts(self):
        for scenario in R['STAGES']:
            for narrow in (False, True):
                with self.subTest(scenario=scenario, narrow=narrow):
                    self.validate(*sample(scenario, narrow))

    def test_about_entry_cannot_be_skipped_or_reordered(self):
        for kind in ('missing', 'reordered', 'wrong-route', 'direct-open'):
            fixture, value = sample('editor')
            if kind == 'missing': value['stages'].pop(0)
            elif kind == 'reordered': value['stages'][0], value['stages'][1] = value['stages'][1], value['stages'][0]
            elif kind == 'wrong-route': value['stages'][0]['model']['route'] = 'editor'
            else: value['aboutConfiguration'] = False
            with self.assertRaises(RuntimeError): self.validate(fixture, value)

    def test_pending_matching_readback_does_not_authorize_success(self):
        for key in ('pending', 'dirty', 'observed'):
            fixture, value = sample('reply-loss')
            value['stages'][3]['model'][key] = False
            with self.assertRaises(RuntimeError): self.validate(fixture, value)
        fixture, value = sample('reply-loss')
        value['stages'][3]['model']['outcomes'] = ['Workflow: Acknowledged', 'Trigger: Acknowledged']
        with self.assertRaisesRegex(RuntimeError, 'outcomes'): self.validate(fixture, value)

    def test_partial_outcomes_are_independent(self):
        fixture, value = sample('editor')
        value['stages'][3]['model']['outcomes'] = ['Workflow: Rejected', 'Trigger: Acknowledged']
        with self.assertRaisesRegex(RuntimeError, 'outcomes'): self.validate(fixture, value)

    def test_checks_and_changed_preflight_cannot_dispatch(self):
        for stage in ('checked', 'checked-again', 'preflight-refused'):
            fixture, value = sample('editor')
            next(row for row in value['stages'] if row['stage'] == stage)['model']['writes'] = 3
            with self.assertRaisesRegex(RuntimeError, 'mutation count'): self.validate(fixture, value)
        fixture, value = sample('editor')
        value['stages'][5]['model']['reads'] = value['stages'][4]['model']['reads']
        with self.assertRaisesRegex(RuntimeError, 'fresh read'): self.validate(fixture, value)

    def test_pending_native_inputs_remain_locked(self):
        for key in ('titleReadonly', 'descriptionReadonly'):
            fixture, value = sample('reply-loss')
            value['stages'][3]['geometry']['fields'][key] = False
            with self.assertRaisesRegex(RuntimeError, 'editor lock'): self.validate(fixture, value)

    def test_pending_save_must_look_disabled_and_remain_visible(self):
        for key, wrong in (('disabled', False), ('opacity', 1), ('opacity', True), ('visible', False)):
            fixture, value = sample('reply-loss')
            value['stages'][3]['geometry']['save'][key] = wrong
            with self.assertRaises(RuntimeError): self.validate(fixture, value)
        fixture, value = sample('reply-loss', True)
        value['stages'][3]['geometry']['save']['rect'].update(top=375, bottom=395)
        with self.assertRaisesRegex(RuntimeError, 'clipped'): self.validate(fixture, value)

    def test_readiness_rejects_foreign_stale_and_ambiguous_identity(self):
        expected = R['ready_tuple']('workflow-editor', 'a'*32, 123)
        R['validate_ready'](copy.deepcopy(expected), expected)
        for key, wrong in (('fixture', 'workflow-editor-narrow'), ('pid', 124), ('pid', True),
                           ('nonce', 'b'*32), ('version', True), ('extra', 'unowned')):
            actual = {**expected, key: wrong}
            with self.assertRaises(RuntimeError): R['validate_ready'](actual, expected)
        for fixture, nonce, pid in (('wrong', 'a'*32, 123), ('workflow-editor', 'A'*32, 123),
                                    ('workflow-editor', 'a'*31, 123), ('workflow-editor', 'a'*32, False)):
            with self.assertRaises(RuntimeError): R['ready_tuple'](fixture, nonce, pid)

    def test_ready_publication_never_replaces_stale_marker(self):
        expected = R['ready_tuple']('workflow-editor', 'a'*32, 123)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            R['H']['publish_ready'](directory, expected)
            original = (directory / 'ready.json').read_bytes()
            with self.assertRaises(FileExistsError):
                R['H']['publish_ready'](directory, {**expected, 'pid': 124})
            self.assertEqual((directory / 'ready.json').read_bytes(), original)
            self.assertFalse((directory / 'ready.staged').exists())

    def test_terminal_must_retain_ready_identity_and_placement_dimensions(self):
        fixture, value = sample('managed')
        expected = R['ready_tuple'](fixture, 'a'*32, 123)
        value['readiness'] = expected.copy()
        R['validate_owned'](value, expected, [1280, 800])
        value['readiness']['pid'] = 456
        with self.assertRaises(RuntimeError): R['validate_owned'](value, expected, [1280, 800])
        value['readiness'] = expected.copy()
        value['stages'][0]['geometry']['pngDimensions'] = [1200, 800]
        with self.assertRaisesRegex(RuntimeError, 'dimensions'): R['validate_owned'](value, expected, [1280, 800])

    def test_closed_log_rejects_duplicate_readiness_or_partial_records(self):
        fixture, value = sample('managed')
        expected = R['ready_tuple'](fixture, 'a'*32, 123)
        value['readiness'] = expected.copy()
        edges = [dict(stage=stage, edge=edge) for stage in R['STAGES']['managed'] for edge in ('before', 'after')]
        import json
        with tempfile.TemporaryDirectory() as temporary:
            class Log:
                log = Path(temporary) / 'fixture.log'
                def receipts(self, prefix): return R['H']['closed_receipts'](self.log, prefix)
            native = Log()
            lines = [R['WAITING'] + json.dumps(expected)] + [R['EDGE'] + json.dumps(e) for e in edges] + [R['PREFIX'] + json.dumps(value)]
            valid = '\n'.join(lines) + '\n'
            native.log.write_text(valid, encoding='utf-8')
            R['validate_closed'](native, expected, DIRECTORY, value, [1280, 800], platform='win32', png=False)
            for invalid in (valid + lines[0] + '\n', valid.rstrip('\n'), valid.replace('"pid": 123', '"pid": 124', 1)):
                native.log.write_text(invalid, encoding='utf-8')
                with self.assertRaises(RuntimeError):
                    R['validate_closed'](native, expected, DIRECTORY, value, [1280, 800], platform='win32', png=False)

    def test_replaced_account_retains_exact_draft_and_attempt(self):
        for key, wrong in (('retainedPending', 0), ('retainedDraftExact', False), ('foreignDocumentHidden', False), ('currentEpoch', 3)):
            fixture, value = sample('reply-loss')
            value['replacement'][key] = wrong
            with self.assertRaises(RuntimeError): self.validate(fixture, value)
        fixture, value = sample('reply-loss')
        value['stages'][-1]['model']['retainedAttempts'] = 0
        with self.assertRaisesRegex(RuntimeError, 'custody'): self.validate(fixture, value)

    def test_retained_text_is_exact_in_model_and_dom(self):
        for location in ('model', 'dom'):
            fixture, value = sample('editor')
            target = value['stages'][3]['model'] if location == 'model' else value['stages'][3]['geometry']['fields']
            target['description'] = R['DESCRIPTION'].replace('\n', ' ')
            with self.assertRaisesRegex(RuntimeError, 'text'): self.validate(fixture, value)

    def test_feedback_must_be_visible_and_exact(self):
        for kind in ('missing', 'hidden', 'clipped', 'nonfinite'):
            fixture, value = sample('reply-loss', True)
            feedback = value['stages'][3]['geometry']['feedback']
            if kind == 'missing': feedback.pop(0)
            elif kind == 'hidden': feedback[0]['visible'] = False
            elif kind == 'clipped': feedback[0]['rect'].update(top=375, bottom=395)
            else: feedback[0]['rect']['top'] = float('nan')
            with self.assertRaises(RuntimeError): self.validate(fixture, value)

    def test_capture_paths_and_inventory_are_owned(self):
        fixture, value = sample('managed')
        value['captures']['saved'] = '/other.png'
        with self.assertRaisesRegex(RuntimeError, 'path'): self.validate(fixture, value)
        fixture, value = sample('managed')
        del value['captures']['draft']
        with self.assertRaisesRegex(RuntimeError, 'inventory'): self.validate(fixture, value)

    def test_network_and_boolean_counters_rejected(self):
        for key, wrong in (('networkCalls', 1), ('writes', True), ('reads', False)):
            fixture, value = sample('managed')
            value['stages'][0]['model'][key] = wrong
            with self.assertRaises(RuntimeError): self.validate(fixture, value)

    def test_terminal_receipt_and_capture_edges_must_be_exact(self):
        fixture, value = sample('managed')
        edges = [dict(stage=stage, edge=edge) for stage in R['STAGES']['managed'] for edge in ('before', 'after')]
        class Log:
            def __init__(self, terminals, captures): self.terminals, self.captures = terminals, captures
            def receipts(self, prefix): return self.terminals if prefix == R['PREFIX'] else self.captures
        R['validate_log'](Log([value], edges), fixture, DIRECTORY, platform='win32', png=False)
        for terminals, captures in (([], edges), ([value, value], edges), ([value], list(reversed(edges))), ([value], edges[:-1])):
            with self.assertRaises(RuntimeError):
                R['validate_log'](Log(terminals, captures), fixture, DIRECTORY, platform='win32', png=False)


if __name__ == '__main__':
    unittest.main()
