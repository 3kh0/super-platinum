import copy
import json
import runpy
import unittest
from pathlib import Path

import workflow_management_share_drafts as V

VIEW = [1280, 780]
BOX = dict(left=20, top=30, right=320, bottom=90, width=300, height=60)
CAPTURES = Path('tmp/share-draft-oracle-synthetic').resolve()
FIXTURE = 'workflow-editor'


def field(name, identity, text, selected=False):
    return dict(id=name+'-composer', nodeId=identity, connected=True, displayed=True,
                rect=copy.deepcopy(BOX), value=text, selectionStart=7 if selected else 0,
                selectionEnd=22 if selected else 0, selectionDirection='forward' if selected else 'none',
                draftScope='original-'+name, projectionVisit='11', readonly=False, disabled=False)


def node(identity):
    return dict(id='', nodeId=identity, connected=True, displayed=True, rect=copy.deepcopy(BOX))


def model(main='', thread='', active=False):
    return dict(enabled=True, channel=V.CHANNEL, root=V.ROOT, reply=V.REPLY,
                activeRoot=V.ROOT if active else None, threadGeneration=2 if active else 0,
                scope='original-scope', loading=False, composer=V.composer(main), threadComposer=V.composer(thread),
                mainDraftKey=dict(team='TWORKFLOW', channel=V.CHANNEL, thread=None),
                threadDraftKey=dict(team='TWORKFLOW', channel=V.CHANNEL, thread=V.ROOT),
                storedMainDraft=V.local(main) if main else None,
                storedThreadDraft=V.local(thread) if thread else None,
                mainAttachments=0, threadAttachments=0, historyLoaded=True, threadLoaded=True,
                history=copy.deepcopy(V.RAW[:1]), thread=copy.deepcopy(V.RAW), historyPending=[], threadPending=[])


def native(main='', thread='', active=False, about=False):
    pane = node(3) if active else None
    body = node(4) if active else None
    if body:
        body['rows'] = [dict(id=r['ts'], nodeId=10+i, text=r['text']) for i, r in enumerate(V.RAW)]
    return dict(ok=True, windowOriginal=True, channel=None if about else field('channel', 1, main, bool(main)),
                thread=field('thread', 5, thread, bool(thread)) if active else None,
                threadPane=pane, threadBody=body,
                original=dict(channelNodeId=1, threadPaneNodeId=3 if active else None,
                              threadBodyNodeId=4 if active else None, threadComposerNodeId=5 if active else None),
                channelOriginalConnected=not about, threadOriginalConnected=active, sameThreadNodes=active)


def observed(main='', thread='', active=False, about=False):
    return dict(model=model(main, thread, active), native=native(main, thread, active, about))


def action(kind, label, identity=20, text=None):
    return dict(ok=True, kind='dom-input' if kind=='input' else 'dom-click', label=label, nodeId=identity,
                event=dict(type='input' if kind=='input' else 'click', isTrusted=False, targetNodeId=identity),
                hitPoint=dict(x=50, y=50), rect=copy.deepcopy(BOX), value=text,
                selectionStart=7 if kind=='input' else None, selectionEnd=22 if kind=='input' else None,
                selectionDirection='forward' if kind=='input' else None, messageId=V.ROOT if kind=='reply' else None)


def shot(label, observation):
    return dict(path=str(CAPTURES/(FIXTURE+'-'+label+'.png')), pngDimensions=[1942, 1256],
                before=copy.deepcopy(observation), after=copy.deepcopy(observation))


def ledger():
    seed = dict(owner=copy.deepcopy(V.F.OWNER), channel=V.CHANNEL, root=V.ROOT, reply=V.REPLY,
                raw=copy.deepcopy(V.RAW), channelGeneration=0, composerStateSeeded=False)
    raw = {
        'conversations.info': dict(ok=True, channel=dict(id=V.CHANNEL, name='Review requests'),
             app_home=dict(app_id='A_HOME', app_installed_team_id='TWORKFLOW', home_tab_enabled=True,
                           messages_tab_enabled=True, home_view_id='V_HOME')),
        'apps.about.info': dict(ok=True, app_id='A_HOME', name='Review requests', owner_ids=['UWORKFLOW']),
        'apps.profile.get': dict(ok=True, app_profile=dict(id='A_HOME', team_id='TWORKFLOW', is_installed=True, is_workflow_app=True)),
        'experiments.getByUser': dict(ok=True, assignments=dict(agent_about_page=dict(group='on'), flexpane_content_parity=dict(group='on'))),
    }
    reads = [dict(serial=i, method=method, body=copy.deepcopy(body), raw=raw[method],
                  endpoint='https://example.test/api/'+method, retrySafe=method=='conversations.info',
                  owner=copy.deepcopy(V.F.OWNER), channel=V.CHANNEL, channelGeneration=2)
             for i, (method, body) in enumerate(V.FIELDS.items(), 1)]
    return dict(seed=seed, about=reads, networkCalls=0)


def sample(navigated=True):
    setup = dict(before=observed(), messages=observed(), channelDrafted=observed(V.MAIN),
                 opened=observed(V.MAIN, active=True), threadDrafted=observed(V.MAIN, V.THREAD, True),
                 after=observed(V.MAIN, V.THREAD, True, True),
                 messagesActivation=action('tab', 'Messages'), channelInput=action('input','channel-composer',1,V.MAIN),
                 replyActivation=action('reply','Reply'), threadInput=action('input','thread-composer',5,V.THREAD),
                 aboutActivation=action('tab','About'))
    for key, generation in [('before',0),('messages',1),('channelDrafted',1),
                            ('opened',1),('threadDrafted',1),('after',2)]:
        m=setup[key]['model']
        m['scope']=f"0-TWORKFLOW-{V.CHANNEL}-{generation}-{m['activeRoot'] or ''}-{m['threadGeneration']}"
    setup['capture'] = shot('draft-setup', setup['threadDrafted'])
    stages = list(V.A.PREFIX)+['share-open','share-edited']+(['share-settled','destination'] if navigated else ['share-unavailable','share-cancelled'])
    rows = []
    for name in stages:
        obs=copy.deepcopy(setup['after'])
        if name=='destination':
            obs['model'].update(activeRoot=None, composer=V.composer(''), threadComposer=V.composer(''), threadGeneration=3)
            obs['native'].update(channel=field('channel',6,''),thread=None,threadPane=None,threadBody=None,
                                 sameThreadNodes=False,threadOriginalConnected=False)
        rows.append(dict(stage=name,geometry=dict(pngDimensions=[1942,1256]),model=dict(shareDrafts=obs['model']),
                         proof=dict(native=dict(shareDrafts=obs['native']),server=dict(
                             seed=dict(boot=dict(channels=[dict(id=V.CHANNEL,name='review-requests')])),
                             confirmationAdd=dict(share=dict(drafts=ledger()))))))
    last=rows[-1]
    before=dict(model=copy.deepcopy(last['model']['shareDrafts']),native=copy.deepcopy(last['proof']['native']['shareDrafts']))
    restored=copy.deepcopy(setup['threadDrafted'])
    restored['native']['channel']=field('channel',7,V.MAIN)
    restored['native']['channelOriginalConnected']=False
    if navigated:
        restored['model']['threadGeneration']=4
        restored['native']['thread']=field('thread',8,V.THREAD)
        restored['native']['threadPane']['nodeId']=9
        restored['native']['threadBody']['nodeId']=12
        restored['native'].update(sameThreadNodes=False,threadOriginalConnected=False)
    returned=dict(before=before,sidebarActivation=action('sidebar','Review requests') if navigated else None,
                  messagesActivation=action('tab','Messages'),after=copy.deepcopy(restored),
                  replyActivation=action('reply','Reply') if navigated else None,restored=restored)
    returned['capture']=shot('draft-returned', restored)
    return dict(shareDraftCustody=True,shareDrafts=dict(setup=setup,returned=returned,
                captures={'draft-setup':setup['capture']['path'],'draft-returned':returned['capture']['path']}),stages=rows)


def validate(value, png=False, validate_png=lambda *args: None):
    V.validate_setup(value,FIXTURE,CAPTURES,VIEW,lambda *args:None,validate_png,png=png)
    checked=next(r for r in value['stages'] if r['stage']=='checked')
    for row in value['stages']:
        if row['stage'] not in V.A.PREFIX:
            V.stage(value,row,checked)
    V.validate_return(value,FIXTURE,CAPTURES,VIEW,lambda *args:None,validate_png,png=png)


class DraftCustody(unittest.TestCase):
    def rejected(self, change, navigated=True):
        value=sample(navigated); validate(value); change(value)
        with self.assertRaises((RuntimeError,ValueError,TypeError,KeyError)):
            validate(value)

    def test_complete_destination_and_failed_destination_paths(self):
        validate(sample());validate(sample(False))

    def test_captured_success_terminal_full_oracle(self):
        root=Path(__file__).resolve().parents[1]
        folder=root/'tmp/workflow-share-drafts-20261021/integration/success-normal-b'
        path=folder/'terminal-extracted.json'
        if not path.exists():
            self.skipTest('Optional retained offline terminal is absent')
        h=runpy.run_path(str(root/'scripts/check-workflow-management-native.py'))
        terminal=json.loads(path.read_text(encoding='utf-8'))
        if 'people' not in terminal['stages'][-1]['proof']['firstPublish']['attempts'][-1]['confirmationShare']:
            self.skipTest('Historical original terminal predates current person-option ABI; prior revalidation is retained')
        h['validate']('workflow-editor',terminal,folder,
                      platform='win32',png=False,first_publish_case='first-add-success',
                      share_case='success',share_drafts=True)

    def test_only_actual_tab_scope_transition_allowed(self):
        self.rejected(lambda v:v['shareDrafts']['setup']['after']['model'].update(scope='foreign'))

    def test_stage_scope_stays_at_after_about_visit(self):
        self.rejected(lambda v:v['stages'][2]['model']['shareDrafts'].update(
            scope=v['shareDrafts']['setup']['threadDrafted']['model']['scope']))

    def test_about_request_uses_post_tab_generation(self):
        self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0].update(channelGeneration=0))

    def test_exact_retry_safety_not_all_read_requests(self):
        self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][1].update(retrySafe=True))

    def test_canonical_app_dm_label_required(self):
        self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0]['raw']['channel'].update(name='ship'))

    def background(self, partial=False):
        def box(l,t,r,b):return dict(left=l,top=t,right=r,bottom=b,width=r-l,height=b-t)
        pane=box(473,0,668,780); r=box(400 if partial else 500,500,620,540)
        region=box(400,500,473,540)
        exposure=dict(rect=r,displayed=True,originalThread=True,viewport=r,occluder=pane,
                      regions=[region] if partial else [],centerHit=False,
                      hitPoint=dict(x=436.5,y=520) if partial else None,occluded=not partial,coveredByThread=True)
        item=dict(id='workflow-management-finish-up',nodeId=50,rect=r,hit=False,visible=False,exposure=exposure)
        native=dict(modal=None,originalNodes=True,backgroundThread=dict(original=True,controlsRetained=True,
                    measurement='individual-original-exposed-region-or-owned-thread-occlusion'),
                    shareDrafts=dict(threadPane=dict(rect=pane)),controls=[item],feedback=[])
        return dict(shareDraftCustody=True),dict(proof=dict(native=native)),item

    def test_passive_covered_or_partly_exposed_retains_false_center_values(self):
        for partial in (False,True):
            receipt,row,item=self.background(partial)
            V.control(receipt,row,item,[668,780],lambda *args:None)
            self.assertFalse(item['visible']);self.assertFalse(item['hit'])

    def test_passive_cover_requires_exact_original_thread_geometry(self):
        changes=[lambda r,w,i:r.update(shareDraftCustody=False),
                 lambda r,w,i:w['proof']['native'].update(modal={}),
                 lambda r,w,i:i['exposure'].update(originalThread=False),
                 lambda r,w,i:i['exposure']['occluder'].update(left=472),
                 lambda r,w,i:i['exposure'].update(coveredByThread=False),
                 lambda r,w,i:i['exposure'].update(centerHit=True),
                 lambda r,w,i:i['exposure'].update(regions=[i['rect']]),
                 lambda r,w,i:i['exposure'].update(hitPoint=dict(x=550,y=520),occluded=False)]
        for change in changes:
            r,w,i=self.background()
            # Keep independent pane observation independent of exposure mutation.
            i['exposure']['occluder']=copy.deepcopy(i['exposure']['occluder'])
            change(r,w,i)
            with self.assertRaises(RuntimeError):V.control(r,w,i,[668,780],lambda *args:None)

    def test_passive_mode_not_a_general_hidden_control_bypass(self):
        r,w,i=self.background();w['proof']['native'].pop('backgroundThread')
        with self.assertRaises(RuntimeError):V.control(r,w,i,[668,780],lambda *args:None)

    def test_actual_action_cannot_use_passive_cover_instead_of_hit(self):
        a=action('tab','Messages');a['exposure']=dict(hitPoint=None,occluded=True,regions=[])
        with self.assertRaises(RuntimeError):V.activation(a,'tab','Messages',VIEW)

    def test_trusted_keyboard_cannot_replace_generic_draft_input(self):
        a=dict(ok=True,kind='trusted-keyboard',label='channel-composer')
        with self.assertRaises(RuntimeError):V.activation(a,'input','channel-composer',VIEW,trusted=True)

    def test_physical_png_dimensions_are_not_css_viewport(self):
        seen=[];validate(sample(),True,lambda path,dims:seen.append((path,dims)))
        self.assertEqual([v[1] for v in seen],[[1942,1256],[1942,1256]])

    def test_extra_capture_dimensions_must_match_base_window(self):
        self.rejected(lambda v:v['shareDrafts']['setup']['capture'].update(pngDimensions=[1280,780]))

    def test_base_stage_dimension_drift_rejected(self):
        self.rejected(lambda v:v['stages'][2]['geometry'].update(pngDimensions=[1942,1257]))

    def test_capture_original_window_required(self):
        v=sample()
        for edge in ('before','after'):
            v['shareDrafts']['setup']['capture'][edge]['native']['windowOriginal']=False
        with self.assertRaisesRegex(RuntimeError, 'another window'):
            V.capture(v,v['shareDrafts']['setup']['capture'],'draft-setup',FIXTURE,CAPTURES,lambda *args:None,False)

    def test_missing_explicit_mode(self):self.rejected(lambda v:v.update(shareDraftCustody=False))
    def test_no_private_buffer_seed(self):self.rejected(lambda v:v['shareDrafts']['setup']['before']['model'].update(composer=V.composer(V.MAIN)))
    def test_unicode_input_must_be_exact(self):self.rejected(lambda v:v['shareDrafts']['setup']['channelInput'].update(value=V.MAIN.replace('\u4e2d','?')))
    def test_dom_event_required(self):self.rejected(lambda v:v['shareDrafts']['setup']['threadInput'].update(event=None))
    def test_no_os_trusted_input_claim(self):self.rejected(lambda v:v['shareDrafts']['setup']['channelInput']['event'].update(isTrusted=True))
    def test_input_on_wrong_node(self):self.rejected(lambda v:v['shareDrafts']['setup']['channelInput'].update(nodeId=999))
    def test_utf16_selection_required(self):self.rejected(lambda v:v['shareDrafts']['setup']['threadInput'].update(selectionEnd=23))
    def test_core_byte_end_is_independent_native_selection(self):
        self.assertNotEqual(V.composer(V.MAIN)['selection']['end'],22)
        self.rejected(lambda v:v['shareDrafts']['setup']['channelDrafted']['model']['composer']['selection'].update(end=22))
    def test_reply_must_target_original_root(self):self.rejected(lambda v:v['shareDrafts']['setup']['replyActivation'].update(messageId='foreign'))
    def test_occluded_action_refused(self):self.rejected(lambda v:v['shareDrafts']['setup']['aboutActivation'].update(hitPoint=dict(x=-1,y=50)))
    def test_main_unmount_is_not_consumption(self):
        v=sample();validate(v);self.assertIsNone(v['stages'][0]['proof']['native']['shareDrafts']['channel'])
        self.rejected(lambda v:v['stages'][0]['model']['shareDrafts'].update(storedMainDraft=None))
    def test_prefix_stage_checked_independently(self):self.rejected(lambda v:v['stages'][1]['model']['shareDrafts']['threadComposer'].update(text='lost'))
    def test_thread_node_replacement_during_modal(self):self.rejected(lambda v:v['stages'][7]['proof']['native']['shareDrafts']['thread'].update(nodeId=999))
    def test_thread_selection_drift(self):self.rejected(lambda v:v['stages'][8]['proof']['native']['shareDrafts']['thread'].update(selectionStart=0))
    def test_thread_lease_change(self):self.rejected(lambda v:v['stages'][5]['proof']['native']['shareDrafts']['thread'].update(projectionVisit='new'))
    def test_history_body_changed(self):self.rejected(lambda v:v['stages'][5]['model']['shareDrafts']['thread'][1].update(text='changed'))
    def test_pending_history_write(self):self.rejected(lambda v:v['stages'][5]['model']['shareDrafts']['historyPending'].append(V.ROOT))
    def test_source_owner_changed(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['seed']['owner'].update(epoch=1))
    def test_source_claims_seeded_composer(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['seed'].update(composerStateSeeded=True))
    def test_raw_history_changed(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['seed']['raw'][0].update(user='OTHER'))
    def test_fresh_about_read_required(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts'].update(about=[]))
    def test_about_bounded(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'].extend([{}]*9))
    def test_about_foreign_endpoint(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0].update(endpoint='https://slack.com/api/conversations.info'))
    def test_about_token_leak(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0]['body'].append(['token','secret']))
    def test_about_server_response_identity(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0]['raw']['app_home'].update(app_id='OTHER'))
    def test_about_retired_generation(self):self.rejected(lambda v:v['stages'][0]['proof']['server']['confirmationAdd']['share']['drafts']['about'][0].update(channelGeneration=1))
    def test_original_about_receipts_immutable(self):self.rejected(lambda v:v['stages'][1]['proof']['server']['confirmationAdd']['share']['drafts']['about'].pop())
    def test_destination_does_not_inherit_original_live_text(self):self.rejected(lambda v:v['stages'][-1]['model']['shareDrafts'].update(composer=V.composer(V.MAIN)))
    def test_destination_must_retire_thread(self):self.rejected(lambda v:v['stages'][-1]['proof']['native']['shareDrafts'].update(threadOriginalConnected=True))
    def test_return_actual_sidebar_required(self):self.rejected(lambda v:v['shareDrafts']['returned'].update(sidebarActivation=None))
    def test_return_actual_reply_required(self):self.rejected(lambda v:v['shareDrafts']['returned'].update(replyActivation=None))
    def test_no_navigation_when_already_original(self):self.rejected(lambda v:v['shareDrafts']['returned'].update(sidebarActivation=action('sidebar','Review requests')),False)
    def test_return_must_render_both_drafts(self):self.rejected(lambda v:v['shareDrafts']['returned']['restored']['native']['thread'].update(value=''))
    def test_return_cannot_rebind_original(self):self.rejected(lambda v:v['shareDrafts']['returned']['restored']['native']['original'].update(threadComposerNodeId=8))
    def test_capture_stability(self):self.rejected(lambda v:v['shareDrafts']['setup']['capture']['after']['model'].update(scope='changed'))
    def test_capture_owned_path(self):self.rejected(lambda v:v['shareDrafts']['returned']['capture'].update(path=str(CAPTURES/'wrong.png')))
    def test_no_extra_captures(self):self.rejected(lambda v:v['shareDrafts']['captures'].update(extra='any'))


if __name__=='__main__':
    unittest.main()
