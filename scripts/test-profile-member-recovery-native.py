"""Pure negative receipt contracts; never starts a native process."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

H = runpy.run_path(str(Path(__file__).with_name('check-profile-member-recovery-native.py')))


def rect():
    return dict(left=10, top=10, right=110, bottom=30, width=100, height=20)


def request(index, released=True):
    member = index % 2 == 0
    return dict(serial=index, kind='member' if member else 'discovery',
                method='users/info' if member else 'api.getFlannelHttpUrl', account=H['ACCOUNT'], team='TINIT0001', epoch=0,
                body=dict(updated_ids={H['MEMBER']: 0}, check_interaction=True, include_profile_only_users=True) if member else {
                    'include_external_workspaces': 'true', '_x_reason': 'client-needs-valid-flannel-url'}, released=released)


def receipt(case='recovery', narrow=False):
    fixture = H['FIXTURES'][int(narrow)]
    viewport = H['expected_viewport'](fixture, 'win32')
    counts = {'initial': (0, 0), 'held': (2, 0), 'malformed': (2, 1), 'offline': (4, 2),
              'cancelled': (6, 3), 'source-refused': (8, 4), 'success': (10, 5), 'reopened': (10, 5), 'account-replaced': (2, 1)}
    proofs = {'initial': ['targetTrulyAbsent', 'originalDraft'], 'held': ['actualOpeningRead', 'duplicateIdsCoalesced'],
              'malformed': ['noPlaceholder', 'draftRetained'], 'offline': ['noPlaceholder', 'draftRetained'],
              'cancelled': ['lateResultDidNotOpen', 'draftRetained'], 'source-refused': ['oldSourceResultRefused', 'draftRetained'],
              'success': ['actualReadPageAndPrivateAdoption', 'originalDraft'], 'reopened': ['noRedundantRead', 'originalDraft'],
              'account-replaced': ['actualInstallLoaded']}
    out = dict(ok=True, fixture=fixture, case=case, viewport=viewport, captureChecksPassed=True,
               readiness=H['ready_tuple'](fixture, 'a' * 32, 123), stages=[], captures={},
               requests=[request(i + 1) for i in range(10 if case == 'recovery' else 2)])
    for stage in H['STAGES'][case]:
        count, finished = counts[stage]
        replaced = stage == 'account-replaced'
        known = stage in ('success', 'reopened')
        waiting = stage in ('held', 'malformed', 'offline', 'source-refused')
        profile = dict(enabled=True, blocks=H['blocks'](), dirty=True, attempts=[], reads=0)
        rows = [request(i + 1, not (stage == 'held' and i == 1)) for i in range(count)]
        model = dict(profile=profile if not replaced else dict(enabled=False), account=H['FOREIGN'] if replaced else H['ACCOUNT'],
                     epoch=int(replaced), sourceObserved=True, sourcePresent=known,
                     native=dict(id=H['MEMBER'], profile=dict(display_name=H['LABEL'])) if known else None,
                     requests=rows, pending=[rows[-1]] if stage == 'held' else [],
                     driver=dict(started=finished + (stage == 'held'), finished=finished, active=int(stage == 'held')))
        wanted = [] if replaced else ['Back', 'Save section'] + (
            ['Use description', 'Cancel description edit'] if known else
            ['Retry description', 'Cancel description edit'] if waiting else ['Edit description'])
        captured = dict(snapshot=dict(blocks=H['blocks'](), composing=False, revision=0, generation='1'),
                        tokens=[dict(value=b, label='@' + H['LABEL']) for b in H['blocks']()[0]['elements'][0]['elements'][1:]]) if known else None
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, dialog=not replaced, dialogRect=rect(),
                        controls=[dict(text=t, disabled=(t == 'Save section' and (known or waiting)), visible=True, rect=rect()) for t in wanted],
                        notices=[dict(text=H['GUIDANCE'], visible=True, rect=rect())] if waiting else [],
                        tokens=[dict(text='@' + H['LABEL'], editable=False, visible=True, rect=rect(), fragments=[rect()]) for _ in range(2)] if known else [],
                        feedbackRequired=waiting, feedbackVisible=waiting, richEditable=True if known else None,
                        captured=captured, globalToast=None, pngDimensions=[1942, 1256])
        proof = {key: True for key in proofs[stage]}
        if replaced: proof.update(before=dict(profile=profile), after=dict(account=H['FOREIGN']))
        out['stages'].append(dict(stage=stage, model=model, geometry=geometry, proof=proof))
        out['captures'][stage] = str(Path('/captures') / f'{fixture}-{stage}.png')
    return out


class ReceiptTests(unittest.TestCase):
    def validate(self, value):
        H['validate'](value['fixture'], value, Path('/captures'), platform='win32', png=False)

    def reject(self, mutation, case='recovery'):
        value = receipt(case)
        mutation(value)
        with self.assertRaises(RuntimeError): self.validate(value)

    def test_both_cases_and_viewports(self):
        for case in H['STAGES']:
            for narrow in (False, True): self.validate(receipt(case, narrow))

    def test_missing_stage(self):
        self.reject(lambda r: r['stages'].pop())

    def test_wrong_viewport(self):
        self.reject(lambda r: r.update(viewport=[668, 380]))

    def test_native_terminal_error_is_reported(self):
        value = receipt(); value.update(ok=False, error='specific original cause')
        with self.assertRaisesRegex(RuntimeError, 'specific original cause'): self.validate(value)

    def test_missing_feedback(self):
        self.reject(lambda r: r['stages'][2]['geometry'].update(notices=[], feedbackVisible=False))

    def test_clipped_feedback(self):
        self.reject(lambda r: r['stages'][2]['geometry']['notices'][0]['rect'].update(top=779, bottom=799))

    def test_wrong_feedback_copy(self):
        self.reject(lambda r: r['stages'][3]['geometry']['notices'][0].update(text='Try again'))

    def test_wrong_member_label(self):
        self.reject(lambda r: r['stages'][6]['geometry']['tokens'][0].update(text='@username-must-not-win'))

    def test_duplicate_token_loss(self):
        self.reject(lambda r: r['stages'][6]['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'].pop())

    def test_unknown_field_loss(self):
        self.reject(lambda r: r['stages'][6]['geometry']['captured']['tokens'][0]['value'].pop('opaque'))

    def test_draft_changed_while_held(self):
        self.reject(lambda r: r['stages'][1]['model']['profile']['blocks'][0]['elements'][0]['elements'][0].update(text='changed'))

    def test_placeholder_after_offline(self):
        self.reject(lambda r: r['stages'][3]['model'].update(sourcePresent=True, native={'id': H['MEMBER'], 'isNonExistent': True}))

    def test_cancelled_editor_mounted(self):
        self.reject(lambda r: r['stages'][4]['geometry'].update(richEditable=True))

    def test_stale_source_publication(self):
        self.reject(lambda r: r['stages'][5]['model'].update(sourcePresent=True))

    def test_driver_not_finished(self):
        self.reject(lambda r: r['stages'][2]['model']['driver'].update(active=1, finished=0))

    def test_wrong_request_ids(self):
        self.reject(lambda r: r['requests'][1]['body'].update(updated_ids={'UOTHER001': 0}))

    def test_credential_field_rejected(self):
        self.reject(lambda r: r['requests'][1]['body'].update(token='synthetic'))

    def test_duplicate_request_replay(self):
        self.reject(lambda r: r['requests'].append(copy.deepcopy(r['requests'][-1])))

    def test_write_not_permitted(self):
        self.reject(lambda r: r['stages'][6]['model']['profile']['attempts'].append({'write': True}))

    def test_account_not_replaced(self):
        self.reject(lambda r: r['stages'][-1]['model'].update(account=H['ACCOUNT']), 'account')

    def test_account_late_adoption(self):
        self.reject(lambda r: r['stages'][-1]['model'].update(native={'id': H['MEMBER']}), 'account')

    def test_capture_path_escape(self):
        self.reject(lambda r: r['captures'].update(initial='/elsewhere/initial.png'))

    def test_inline_fragment_clipping(self):
        self.reject(lambda r: r['stages'][6]['geometry']['tokens'][0]['fragments'][0].update(top=779, bottom=799))

    def test_readiness_identity(self):
        wanted = H['ready_tuple'](H['FIXTURES'][0], 'a' * 32, 123)
        H['validate_ready'](wanted, wanted)
        for key, value in [('pid', 124), ('nonce', 'b' * 32), ('fixture', H['FIXTURES'][1])]:
            wrong = dict(wanted); wrong[key] = value
            with self.assertRaises(RuntimeError): H['validate_ready'](wrong, wanted)

    def test_binary_png_dimensions(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / 'bad.png'; p.write_bytes(b'invalid')
            with self.assertRaises(RuntimeError): H['validate_png'](p, [1280, 780])


if __name__ == '__main__':
    unittest.main()
