"""Pure negative receipt contracts; never starts a native process."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

H = runpy.run_path(str(Path(__file__).with_name('check-profile-member-known-native.py')))


def rect():
    return dict(left=10, top=10, right=110, bottom=30, width=100, height=20)


def request(index, released=True):
    member = index % 2 == 0
    return dict(serial=index, kind='member' if member else 'discovery',
                method='users/info' if member else 'api.getFlannelHttpUrl', account=H['ACCOUNT'], team='TINIT0001', epoch=0,
                body=dict(ids=[H['MEMBER']], check_interaction=True, include_profile_only_users=True) if member else {
                    'include_external_workspaces': 'true', '_x_reason': 'client-needs-valid-flannel-url'}, released=released)


def receipt(case='stale', narrow=False):
    fixture = H['FIXTURES'][int(narrow)]
    viewport = H['expected_viewport'](fixture, 'win32')
    final_count = 6 if case == 'stale' else 2
    counts = {'initial': (0, 0), 'ordinary-open': (0, 0), 'held': (2, 0), 'malformed': (2, 1),
              'offline': (4, 2), 'repair-held': (6, 2), 'success': (final_count, final_count // 2),
              'reopened': (final_count, final_count // 2), 'account-replaced': (2, 1)}
    proofs = {'initial': ['initialRawKnown', 'originalDraft'], 'ordinary-open': ['noForcedRead', 'originalDraft'],
              'held': ['explicitRetryRead', 'duplicateIdsCoalesced'], 'malformed': ['sourceRetained', 'draftRetained'],
              'offline': ['sourceRetained', 'draftRetained'], 'repair-held': ['sourceUnchanged', 'nativeDriftBefore'],
              'success': ['actualReadPageAndPrivateAdoption', 'sameVersionPositive', 'originalDraft'],
              'reopened': ['noRedundantRead', 'originalDraft'], 'account-replaced': ['actualInstallLoaded']}
    out = dict(ok=True, fixture=fixture, case=case, viewport=viewport, captureChecksPassed=True,
               readiness=H['ready_tuple'](fixture, 'a' * 32, 123), stages=[], captures={},
               requests=[request(i + 1) for i in range(final_count)])
    original = dict(id=H['MEMBER'], updated=0 if case == 'terminal' else 101, profile=dict(display_name=H['LABEL']))
    if case == 'terminal': original['isNonExistent'] = True
    for stage in H['STAGES'][case]:
        count, finished = counts[stage]
        replaced = stage == 'account-replaced'
        editor = stage in ('success', 'reopened')
        waiting = stage in ('ordinary-open', 'held', 'malformed', 'offline', 'repair-held')
        held = stage in ('held', 'repair-held')
        profile = dict(enabled=True, blocks=H['blocks'](), dirty=True, attempts=[], reads=0)
        rows = [request(i + 1, not (held and i == count - 1)) for i in range(count)]
        source = copy.deepcopy(original)
        if editor: source.pop('isNonExistent', None)
        native = copy.deepcopy(source)
        if stage == 'repair-held': native['profile']['display_name'] = 'Native drift must be repaired'
        model = dict(profile=profile if not replaced else dict(enabled=False), account=H['FOREIGN'] if replaced else H['ACCOUNT'],
                     epoch=int(replaced), sourceObserved=True, sourcePresent=not replaced,
                     native=native if not replaced else None, source=source if not replaced else None,
                     interaction=None, disconnected={}, invalidated=case != 'terminal' and not editor,
                     absentDriver=dict(started=0, finished=0, active=0), requests=rows, pending=[rows[-1]] if held else [],
                     driver=dict(started=finished + held, finished=finished, active=int(held)))
        wanted = [] if replaced else ['Back', 'Save section'] + (
            ['Use description', 'Cancel description edit'] if editor else
            ['Retry description', 'Cancel description edit'] if waiting else ['Edit description'])
        captured = dict(snapshot=dict(blocks=H['blocks'](), composing=False, revision=0, generation='1'),
                        tokens=[dict(value=b, label='@' + H['LABEL']) for b in H['blocks']()[0]['elements'][0]['elements'][1:]]) if editor else None
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, dialog=not replaced, dialogRect=rect(),
                        controls=[dict(text=t, disabled=((t == 'Save section' and (editor or waiting)) or (t == 'Retry description' and held)), visible=True, rect=rect()) for t in wanted],
                        notices=[dict(text=H['LOADING'] if held else H['GUIDANCE'], visible=True, rect=rect())] if waiting else [],
                        tokens=[dict(text='@' + H['LABEL'], editable=False, visible=True, rect=rect(), fragments=[rect()]) for _ in range(2)] if editor else [],
                        feedbackRequired=waiting, feedbackVisible=waiting, richEditable=True if editor else None,
                        captured=captured, globalToast=None, pngDimensions=[1942, 1256])
        proof = {key: True for key in proofs[stage]}
        if replaced: proof.update(before=dict(profile=profile, source=copy.deepcopy(original)), after=dict(account=H['FOREIGN']))
        out['stages'].append(dict(stage=stage, model=model, geometry=geometry, proof=proof))
        out['captures'][stage] = str(Path('/captures') / f'{fixture}-{stage}.png')
    return out


def stage(value, name):
    return next(s for s in value['stages'] if s['stage'] == name)


class ReceiptTests(unittest.TestCase):
    def validate(self, value):
        H['validate'](value['fixture'], value, Path('/captures'), platform='win32', png=False)

    def reject(self, mutation, case='stale'):
        value = receipt(case)
        mutation(value)
        with self.assertRaises(RuntimeError): self.validate(value)

    def test_all_cases_and_viewports(self):
        for case in H['STAGES']:
            for narrow in (False, True): self.validate(receipt(case, narrow))

    def test_terminal_error_keeps_original_diagnostic(self):
        value=receipt();value.update(ok=False,error='specific original cause')
        with self.assertRaisesRegex(RuntimeError,'specific original cause'):self.validate(value)

    def test_missing_stage(self): self.reject(lambda r:r['stages'].pop())
    def test_wrong_viewport(self): self.reject(lambda r:r.update(viewport=[668,380]))
    def test_no_guidance(self): self.reject(lambda r:stage(r,'malformed')['geometry'].update(notices=[],feedbackVisible=False))
    def test_clipped_guidance(self): self.reject(lambda r:stage(r,'malformed')['geometry']['notices'][0]['rect'].update(top=779,bottom=799))
    def test_wrong_loading_copy(self): self.reject(lambda r:stage(r,'held')['geometry']['notices'][0].update(text=H['GUIDANCE']))
    def test_retry_must_be_disabled_while_loading(self):
        self.reject(lambda r:next(c for c in stage(r,'held')['geometry']['controls'] if c['text']=='Retry description').update(disabled=False))
    def test_retry_must_enable_after_failure(self):
        self.reject(lambda r:next(c for c in stage(r,'offline')['geometry']['controls'] if c['text']=='Retry description').update(disabled=True))
    def test_ordinary_open_must_not_force_read(self): self.reject(lambda r:stage(r,'ordinary-open')['model']['requests'].append(request(1)))
    def test_not_absent_read(self): self.reject(lambda r:r['requests'][1]['body'].update(updated_ids={H['MEMBER']:0}))
    def test_wrong_requested_id(self): self.reject(lambda r:r['requests'][1]['body'].update(ids=['UOTHER001']))
    def test_no_credentials(self): self.reject(lambda r:r['requests'][1]['body'].update(token='synthetic'))
    def test_source_retained_on_malformed(self): self.reject(lambda r:stage(r,'malformed')['model']['source'].update(updated=102))
    def test_native_retained_on_offline(self): self.reject(lambda r:stage(r,'offline')['model']['native']['profile'].update(display_name='Changed'))
    def test_disconnection_retained_on_offline(self): self.reject(lambda r:stage(r,'offline')['model'].update(disconnected={H['MEMBER']:True}))
    def test_interaction_retained_on_failure(self): self.reject(lambda r:stage(r,'malformed')['model'].update(interaction=True))
    def test_terminal_zero_positive_required(self): self.reject(lambda r:stage(r,'success')['model']['source'].update(isNonExistent=True),'terminal')
    def test_terminal_zero_version_not_upgraded(self): self.reject(lambda r:stage(r,'success')['model']['source'].update(updated=101),'terminal')
    def test_same_version_native_repair_required(self): self.reject(lambda r:stage(r,'success')['model']['native']['profile'].update(display_name='Native drift must be repaired'))
    def test_actual_native_drift_required(self): self.reject(lambda r:stage(r,'repair-held')['model']['native']['profile'].update(display_name=H['LABEL']))
    def test_source_not_changed_for_drift(self): self.reject(lambda r:stage(r,'repair-held')['model']['source']['profile'].update(display_name='Native drift must be repaired'))
    def test_wrong_label(self): self.reject(lambda r:stage(r,'success')['geometry']['tokens'][0].update(text='@fallback'))
    def test_duplicate_token_loss(self): self.reject(lambda r:stage(r,'success')['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'].pop())
    def test_opaque_token_loss(self): self.reject(lambda r:stage(r,'success')['geometry']['captured']['tokens'][0]['value'].pop('opaque'))
    def test_draft_changed_while_held(self): self.reject(lambda r:stage(r,'held')['model']['profile']['blocks'][0]['elements'][0]['elements'][0].update(text='Changed'))
    def test_driver_not_settled(self): self.reject(lambda r:stage(r,'malformed')['model']['driver'].update(active=1))
    def test_missing_driver_never_started(self): self.reject(lambda r:stage(r,'ordinary-open')['model']['absentDriver'].update(started=1,active=1))
    def test_read_replayed_on_reopen(self): self.reject(lambda r:stage(r,'reopened')['model']['requests'].append(request(7)))
    def test_profile_write_forbidden(self): self.reject(lambda r:stage(r,'success')['model']['profile']['attempts'].append({'write':True}))
    def test_late_account_publication(self): self.reject(lambda r:stage(r,'account-replaced')['model'].update(native={'id':H['MEMBER']}),'account')
    def test_real_account_replacement(self): self.reject(lambda r:stage(r,'account-replaced')['model'].update(account=H['ACCOUNT']),'account')
    def test_capture_path_escape(self): self.reject(lambda r:r['captures'].update(initial='/elsewhere/initial.png'))
    def test_wrapped_fragment_clipping(self): self.reject(lambda r:stage(r,'success')['geometry']['tokens'][0]['fragments'][0].update(top=779,bottom=799))
    def test_readiness_identity(self):
        wanted=H['ready_tuple'](H['FIXTURES'][0],'a'*32,123)
        H['validate_ready'](wanted,wanted)
        for key,value in [('pid',124),('nonce','b'*32),('fixture',H['FIXTURES'][1])]:
            wrong=dict(wanted);wrong[key]=value
            with self.assertRaises(RuntimeError):H['validate_ready'](wrong,wanted)
    def test_invalid_png(self):
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory)/'bad.png';p.write_bytes(b'invalid')
            with self.assertRaises(RuntimeError):H['validate_png'](p,[1280,780])

if __name__=='__main__':unittest.main()
