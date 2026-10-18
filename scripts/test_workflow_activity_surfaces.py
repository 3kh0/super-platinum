"""Pure Thread/Pins execution-entry receipt tests. No native processes."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from workflow_activity_surfaces import ROOT, TS, THREAD_COMPOSER

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-activity-native.py')))
DIRECTORY = Path('/private/activity-captures')


def sample(scenario='execution-thread', narrow=False):
    if scenario.endswith('-retirement'):
        from test_workflow_activity_surface_retirement import sample as retirement_sample
        return retirement_sample(scenario, narrow, base_sample=sample)
    fixture = 'workflow-activity' + ('-narrow' if narrow else '')
    viewport = [668, 380] if narrow else [1280, 780]
    box = dict(left=10, top=10, right=210, bottom=30, width=200, height=20)
    focused = scenario == 'execution-focused'
    thread = scenario in ('execution-thread', 'execution-focused')
    message = dict(ts=TS, channel='C2', thread_ts=ROOT, blocks=[dict(type='actions', elements=[dict(
        type='button', action_id='workflow_error_view_logs', accessibility_label=label,
        value=json.dumps(dict(type='wfb', wf_id='WF_REVIEW', execution_id=execution)))])
        for label, execution in zip(('View workflow execution', 'View missing execution', 'Unavailable execution action'),
                                    ('E_REVIEW', 'E_ABSENT', 7))])
    origin = dict(team='TWORKFLOW', channel='C2', epoch=0, channelGeneration=3, mainView='Activity' if focused else 'Home',
                  detailOpen=True, conversationVisible=not focused,
                  selectedSurface=dict(channel='C2', threadRoot=ROOT) if focused else None,
                  ordinaryChannel=True, aboutPresent=False, homePresent=False, messagesTab=thread,
                  surface='thread' if thread else 'pins', threadRoot=ROOT if thread else None,
                  threadGeneration=2, threadScope='original-thread-scope',
                  threadComposerModel=THREAD_COMPOSER + '.' if thread else '',
                  threadSource=[dict(ts=ROOT, blocks=[]), message] if thread else None,
                  pins=dict(scopeCurrent=not thread, channel='C2' if not thread else '', generation=1,
                            loaded=not thread, items=[] if thread else [dict(key='message:' + TS, kind='message',
                                                                           channel='C2', message=message)]),
                  composerModel='', appModal=False, documents=[], source=[dict(ts=ROOT, blocks=[])])
    requests = []
    for serial, suffix in enumerate(('list', 'get', 'list', 'get'), 1):
        args = dict(workflow_id='WF_REVIEW', _x_reason='Load instance steps for activity tab' if suffix == 'get'
                    else 'multi-click-activity-list-fetcher')
        if suffix == 'get':
            args['workflow_execution_id'] = 'E_REVIEW'
        else:
            args.update(filters='{}', limit='100')
        requests.append(dict(serial=serial, method='workflows.activity.' + suffix, args=args))
    stages = []
    for index, stage in enumerate(R['STAGES'][scenario]):
        direct, detail, dirty = stage not in ('source', 'returned'), stage in ('opened', 'reopened'), index >= 2
        source = copy.deepcopy(origin)
        if dirty:
            source['documents'] = [dict(id=8, title=R['TITLE'], description=R['DESCRIPTION'], dirty=True)]
        model = dict(entry=dict(active=direct, busy=False, autoPending=False, workflow='WF_REVIEW' if direct else None,
                                execution='E_REVIEW' if direct else None, visit=6 if direct else None),
                     metadata=dict(writes=0, networkCalls=0, pending=False, busy=False, dirty=dirty and direct,
                                   title=R['TITLE'] if dirty and direct else None,
                                   description=R['DESCRIPTION'] if dirty and direct else None),
                     activity=dict(active=detail, detail=dict(workflow_event=dict(workflow_execution_id='E_REVIEW',
                                   workflow_status='PENDING')) if detail else None) if direct else None,
                     ledger=dict(networkCalls=0, requests=copy.deepcopy(requests[:(0, 2, 2, 2, 4)[index]]),
                                 finished=(0, 2, 2, 2, 4)[index], cancelled=0, held=None))
        editors = [dict(id=key, sameNode=True, visit='1', scope=key + '-scope', value=value,
                        start=3, end=10, visible=not direct,
                        clientHeight=60 if not direct else 0, scrollHeight=60 if not direct else 0, fits=not direct)
                   for key, value in [('channel-composer', R['EXECUTION_COMPOSER']), ('thread-composer', THREAD_COMPOSER)] if not focused or key == 'thread-composer'] if thread else []
        native = dict(ok=True, samePanel=True, sameBody=True, sameAction=True, sameCard=None if thread else True,
                      actionCount=1, actionKey='canonical-' + ('thread' if thread else 'pins'), visible=not direct,
                      display='none' if thread and direct else 'flex',
                      header=dict(sameNode=True, text='Thread' if thread else '#ship', channelLabel='#ship' if thread else None,
                                  display='none' if not thread and direct else 'flex', visible=not direct, rect=copy.deepcopy(box)), editors=editors,
                      channelComposerCount=int(thread and not focused), threadComposerCount=int(thread))
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, activity=detail, detail=detail,
                        detailStatus='Processing' if detail else None,
                        controls=[dict(id='source-action' if not direct else 'workflow-entry-back', visible=True, rect=copy.deepcopy(box))],
                        notices=[], pngDimensions=[1024, 656] if narrow else [1600, 975])
        stages.append(dict(stage=stage, model=model, proof=dict(source=source, nativeSurface=native), geometry=geometry))
    return fixture, dict(ok=True, fixture=fixture, case=scenario, viewport=viewport, actualMessageAction=True,
                         actualActivityEntry=True, aboutConfiguration=False, noLiveNetwork=True, stages=stages,
                         captures={s: str(DIRECTORY / f'{fixture}-{s}.png') for s in R['STAGES'][scenario]},
                         final=copy.deepcopy(stages[-1]['model'])), scenario


class SurfaceTests(unittest.TestCase):
    def validate(self, fixture, receipt, scenario):
        return R['validate'](fixture, receipt, DIRECTORY, scenario, platform='win32', png=False)

    def reject(self, change, stage='returned', scenario='execution-thread'):
        f, r, c = sample(scenario)
        change(next(row for row in r['stages'] if row['stage'] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(f, r, c)

    def test_thread_draft_matches_independent_rust_literal_and_codepoints(self):
        import re
        rust = (Path(__file__).resolve().parents[1] /
                'src/desktop/app_home/about/management/entry/fixture.rs').read_text()
        raw = re.search(r'pub\(crate\) const THREAD_COMPOSER: &str\s*=\s*"([^";]+)";', rust).group(1)
        actual = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda m: chr(int(m.group(1), 16)), raw)
        self.assertEqual(THREAD_COMPOSER, actual)
        self.assertEqual([ord(c) for c in THREAD_COMPOSER[-2:]], [0x4E2D, 0x1F642])

    def test_both_surfaces_at_both_widths(self):
        for case in R['SURFACE_STAGES']:
            for narrow in (False, True):
                self.validate(*sample(case, narrow))

    def test_focused_thread_requires_selected_surface(self):
        for fields in [dict(mainView='Home'), dict(detailOpen=False), dict(conversationVisible=True),
                       dict(selectedSurface=None), dict(selectedSurface=dict(channel='C2', threadRoot='other'))]:
            with self.subTest(fields=fields):
                self.reject(lambda row: row['proof']['source'].update(fields), scenario='execution-focused')

    def test_focused_thread_cannot_create_channel_editor(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(channelComposerCount=1),
                    scenario='execution-focused')

    def test_focused_thread_preserves_its_single_native_editor(self):
        for fields in [dict(value='lost'), dict(sameNode=False), dict(scope='successor'), dict(start=0),
                       dict(clientHeight=20, scrollHeight=60)]:
            with self.subTest(fields=fields):
                self.reject(lambda row: row['proof']['nativeSurface']['editors'][0].update(fields),
                            scenario='execution-focused')

    def test_original_nodes_required(self):
        for key in ('samePanel', 'sameBody', 'sameAction'):
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['nativeSurface'].update({key: False}))

    def test_original_pins_card_required(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(sameCard=False), scenario='execution-pins')

    def test_restored_thread_display_required(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(display='none'))

    def test_restored_pins_visible_required(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(visible=False), scenario='execution-pins')

    def test_hidden_source_not_claimed_visible(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(visible=True), stage='opened')

    def test_action_key_cannot_change(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(actionKey='timeline-key'))

    def test_unique_scoped_action_required(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(actionCount=2))

    def test_header_restoration(self):
        for key, value in [('sameNode', False), ('visible', False), ('display', 'none'), ('text', '#other'), ('channelLabel', '#other')]:
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['nativeSurface']['header'].update({key: value}))

    def test_direct_pins_header_hidden_by_its_own_display(self):
        self.reject(lambda row: row['proof']['nativeSurface']['header'].update(display='flex'),
                    stage='opened', scenario='execution-pins')

    def test_header_cannot_clip(self):
        self.reject(lambda row: row['proof']['nativeSurface']['header']['rect'].update(top=-10))

    def test_both_native_draft_buffers_required(self):
        for index in (0, 1):
            with self.subTest(index=index):
                self.reject(lambda row: row['proof']['nativeSurface']['editors'][index].update(value='lost'))

    def test_native_selection_preserved(self):
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(start=0))

    def test_composer_mount_preserved(self):
        for key, value in [('sameNode', False), ('visit', 'new'), ('scope', 'new')]:
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['nativeSurface']['editors'][0].update({key: value}))

    def test_composer_hidden_during_detail(self):
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(visible=True), stage='opened')

    def test_thread_autosize_model_remains_distinct_from_raw_buffer(self):
        for stage in ('source', 'returned', 'reopened'):
            with self.subTest(stage=stage):
                self.reject(lambda row: row['proof']['source'].update(threadComposerModel=THREAD_COMPOSER), stage=stage)

    def test_visible_thread_draft_requires_measured_fit(self):
        for fields in [dict(fits=False), dict(clientHeight=0), dict(scrollHeight=0),
                       dict(clientHeight=float('inf')), dict(scrollHeight=float('nan')),
                       dict(clientHeight=True), dict(clientHeight=30, scrollHeight=60),
                       dict(clientHeight=60, scrollHeight=62)]:
            with self.subTest(fields=fields):
                self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(fields))

    def test_source_thread_draft_must_fit_too(self):
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(clientHeight=30), stage='source')

    def test_hidden_editor_cannot_claim_fit(self):
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(fits=True), stage='opened')

    def test_one_pixel_scroll_rounding_is_allowed(self):
        f, r, c = sample()
        for row in r['stages']:
            if row['stage'] in ('source', 'returned'):
                row['proof']['nativeSurface']['editors'][1]['scrollHeight'] = 61
        self.validate(f, r, c)

    def test_pins_does_not_fabricate_textarea(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(channelComposerCount=1), scenario='execution-pins')

    def test_no_timeline_fallback(self):
        self.reject(lambda row: row['proof']['source']['source'][0].update(blocks=[dict(type='actions')]))

    def test_exact_thread_root_and_generation(self):
        for key, value in [('threadRoot', 'other'), ('threadGeneration', 3), ('threadScope', 'new'), ('channelGeneration', 4)]:
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['source'].update({key: value}))

    def test_actual_thread_reply_required(self):
        self.reject(lambda row: row['proof']['source']['threadSource'][1].update(thread_ts='other'))

    def test_no_model_draft_adoption_from_native_buffer(self):
        for key in ('composerModel', 'threadComposerModel'):
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['source'].update({key: 'native buffer'}))

    def test_pins_scope_and_item_identity_required(self):
        for key, value in [('scopeCurrent', False), ('channel', 'other'), ('generation', 5), ('loaded', False)]:
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['source']['pins'].update({key: value}), scenario='execution-pins')
        self.reject(lambda row: row['proof']['source']['pins']['items'][0].update(key='other'), scenario='execution-pins')

    def test_pinned_reply_does_not_fabricate_open_thread(self):
        self.reject(lambda row: row['proof']['source'].update(threadRoot=ROOT), scenario='execution-pins')

    def test_raw_client_action_required(self):
        self.reject(lambda row: row['proof']['source']['threadSource'][1]['blocks'][0]['elements'][0].update(action_id='generic'))

    def test_actual_list_get_counts_no_autopage(self):
        self.reject(lambda row: row['model']['ledger']['requests'].append(copy.deepcopy(row['model']['ledger']['requests'][-1])))

    def test_detail_exact_execution_and_detail_status(self):
        self.reject(lambda row: row['model']['activity']['detail']['workflow_event'].update(workflow_execution_id='other'), stage='opened')
        self.reject(lambda row: row['geometry'].update(detailStatus='Completed'), stage='opened')

    def test_dirty_document_preserved(self):
        self.reject(lambda row: row['proof']['source'].update(documents=[]))
        self.reject(lambda row: row['proof']['source']['documents'][0].update(id=9), stage='reopened')

    def test_exact_dirty_editor_on_reopen(self):
        self.reject(lambda row: row['model']['metadata'].update(description='lost'), stage='reopened')

    def test_no_generic_modal_or_fake_about(self):
        for key in ('aboutPresent', 'homePresent', 'appModal'):
            with self.subTest(key=key):
                self.reject(lambda row: row['proof']['source'].update({key: True}))

    def test_no_network_or_metadata_write(self):
        self.reject(lambda row: row['model']['metadata'].update(writes=1))
        self.reject(lambda row: row['model']['ledger'].update(networkCalls=1))

    def test_control_geometry_must_fit(self):
        self.reject(lambda row: row['geometry']['controls'][0]['rect'].update(right=9999))

    def test_capture_order_paths_and_final_model_exact(self):
        for change in (lambda r: r['stages'].reverse(), lambda r: r['captures'].update(returned='/foreign.png'),
                       lambda r: r['final'].update(entry=None)):
            f, r, c = sample()
            change(r)
            with self.assertRaises(RuntimeError):
                self.validate(f, r, c)


if __name__ == '__main__':
    unittest.main()
