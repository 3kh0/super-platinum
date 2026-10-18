"""Pure Activity receipt regressions; no native launches or Slack access."""
import copy
import json
from pathlib import Path
import runpy
import unittest

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-activity-native.py')))
DIRECTORY = Path('/private/activity-captures')


def sample(scenario='browse', narrow=False):
    if scenario in R['SURFACE_STAGES']:
        from test_workflow_activity_surfaces import sample as surface_sample
        return surface_sample(scenario, narrow)
    if scenario.startswith('execution'):
        return execution_sample(scenario,narrow)
    if scenario in ('export','export-retirement'):
        return export_sample(scenario,narrow)
    if scenario in ('user-refresh','user-deactivated'):
        return refresh_sample(narrow,scenario)
    if scenario in ('user-real','user-display'):
        return users_sample(scenario,narrow)
    if scenario == 'channel':
        fixture, receipt = channel_sample(narrow)
        return fixture, receipt, scenario
    if scenario == 'picker':
        return picker_sample(narrow)
    fixture = 'workflow-activity' + ('-narrow' if narrow else '')
    viewport = [668, 380] if narrow else [1280, 780]
    filters = dict(workflow_instance_status='SUCCESS', from_timestamp=1781496000, to_timestamp=1781668799)
    basic = dict(workflow_id='WF_REVIEW', limit=100, filters={})
    filtered = dict(workflow_id='WF_REVIEW', limit=20, filters=filters)
    queries = [basic, filtered, dict(filtered, cursor=R['CURSOR']), filtered,
               dict(workflow_id='WF_REVIEW', workflow_execution_id='E_REVIEW'),
               dict(filtered, parent_workflow_execution_id='E_REVIEW'), filtered, filtered, filtered, filtered]
    if scenario == 'retirement':
        queries = [basic, basic]
    requests = []
    for i, query in enumerate(queries, 1):
        detail = 'workflow_execution_id' in query
        args = {k: json.dumps(v) if isinstance(v, (dict, list)) else str(v) for k, v in query.items()}
        args['_x_reason'] = 'Load instance steps for activity tab' if detail else 'multi-click-activity-list-fetcher'
        requests.append(dict(serial=i, method='workflows.activity.get' if detail else 'workflows.activity.list', args=args))
    counts = dict(about=0, draft=0, **{'uncommitted-refused': 0, 'composition-refused': 0}, unconfirmed=0,
                  list=1, filtered=2, next=3, previous=4, detail=5, nested=6, error=8, retried=9, empty=10,
                  returned=10, held=2, **{'account-replaced': 2})
    box = dict(left=10, top=10, right=210, bottom=30, width=200, height=20)
    rows = []
    for index, stage in enumerate(R['STAGES'][scenario]):
        hidden = stage in ('about', 'account-replaced')
        active = not hidden and stage not in ('draft', 'uncommitted-refused', 'composition-refused', 'unconfirmed', 'returned')
        pending = scenario == 'retirement' and stage not in ('about', 'draft', 'account-replaced')
        notice = R['REFUSAL'] if stage in ('uncommitted-refused', 'composition-refused') else None
        metadata = dict(route='about' if hidden else 'editor', networkCalls=0, busy=stage == 'held',
                        writes=2 if scenario == 'retirement' and stage not in ('about', 'draft') else 0,
                        pending=pending, dirty=not hidden, title=None if hidden else R['TITLE'],
                        description=None if hidden else R['DESCRIPTION'], notice=None, observed=pending,
                        outcomes=['Workflow: Unconfirmed', 'Trigger: Unconfirmed'] if pending else [])
        q = copy.deepcopy(basic if stage in ('list', 'held') else filtered)
        if stage == 'next': q['cursor'] = R['CURSOR']
        if stage == 'nested': q['parent_workflow_execution_id'] = 'E_REVIEW'
        activity = None if hidden else dict(active=active, query=q, previous=[filtered] if stage == 'next' else [],
                    page=dict(ok=True, workflow_events=[] if stage == 'empty' else [dict(workflow_execution_id='E_REVIEW')]),
                    breadcrumbs=[dict(workflow_execution_id='E_REVIEW')] if stage == 'nested' else [],
                    notice=R['FAILURE'] if stage == 'error' else None,
                    detailRow=dict(workflow_instance_status='SUCCESS') if stage == 'detail' else None,
                    detail=dict(workflow_event=dict(workflow_execution_id='E_REVIEW', workflow_status='PENDING')) if stage == 'detail' else None)
        ledger = dict(requests=copy.deepcopy(requests[:counts[stage]]), finished=counts[stage] - (stage == 'held'),
                      held=2 if stage == 'held' else None, cancelled=1 if stage == 'account-replaced' else 0, networkCalls=0)
        ids = (['workflow-activity-'+i for i in ('back', 'status', 'from', 'to', 'limit', 'apply')]
               if active else ['workflow-configuration'] if hidden else ['workflow-management-activity'])
        notices = ([notice] if notice else []) + ([R['FAILURE']] if stage == 'error' else [])
        fields = None if active or hidden else dict(title='Uncommitted native title 未確定' if stage == 'uncommitted-refused' else R['TITLE'],
                    description=R['DESCRIPTION'], titleReadonly=pending, descriptionReadonly=pending)
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, activity=active, detail=stage == 'detail',
                        detailStatus='Processing' if stage == 'detail' else None,
                        detailStatusGeometry=dict(text='Processing', visible=True, scrollSample=True, rect=copy.deepcopy(box)) if stage == 'detail' else None,
                        lastStepStatuses=[dict(text='Completed', visible=True, scrollSample=True, rect=copy.deepcopy(box))] if stage == 'detail' else [],
                        controls=[dict(id=i, visible=True, rect=copy.deepcopy(box), disabled=False, scrollSample=True) for i in ids],
                        controlMeasurement='individual-scroll-samples',
                        notices=[dict(text=t, visible=True, rect=copy.deepcopy(box)) for t in notices],
                        fields=fields, pngDimensions=[1280, 800])
        proof = None
        if stage in ('uncommitted-refused', 'composition-refused'):
            buffer = 'Uncommitted native title 未確定' if stage == 'uncommitted-refused' else R['TITLE']
            proof = dict(draftRetained=True, noActivityRead=True, unchangedRustState=True,
                         sameRevision=True, beforeRevision=1, afterRevision=1, beforeRefusals=0, afterRefusals=1,
                         probe=dict(value=buffer), native=dict(value=buffer, notice=R['REFUSAL']),
                         composition=stage == 'composition-refused', uncommitted=stage == 'uncommitted-refused')
        if stage == 'filtered': proof = dict(ok=True, **{'from': filters['from_timestamp'], 'to': filters['to_timestamp']})
        if stage == 'detail':
            proof = dict(emoji=dict(ok=True, viewport=viewport, paragraphCount=2,
                paragraphs=[dict(id=identity, text=text, visible=True, rect=copy.deepcopy(box), scrollSample=True,
                    spans=[dict(display='inline', margins=['0px']*4)]) for identity, text in zip(
                    ('workflow-activity-trigger-message', 'workflow-activity-step-message-0'), (R['EMOJI_TRIGGER'], R['EMOJI_STEP']))],
                images=[dict(alt=':activity_ready:',src='http://super-platinum-media.emoji/fixture?v=1',
                    complete=True,naturalWidth=16,naturalHeight=16)], coldFallback=':activity_cold:',coldImages=0,
                standardGlyphs=['\U0001f44b']*2,explicitGlyphCount=1,trailingGlyphCount=1,explicitTitle=None,trailingTitle=':wave:',
                forbiddenNodes=0,inlineMargins=True,measurement='individual-scroll-samples'))
        rows.append(dict(stage=stage, model=dict(metadata=metadata, activity=activity, ledger=ledger), geometry=geometry, proof=proof))
    replacement = None if scenario == 'browse' else dict(foreignDocumentHidden=True, retainedDraftExact=True,
                    retainedPending=1, currentEpoch=4, previousEpoch=3, before=dict(retainedAttempts=1),
                    after=dict(retainedAttempts=1, writes=2), lateReply=dict(delivered=False, cancelledBeforeReply=True))
    receipt = dict(ok=True, fixture=fixture, case=scenario, viewport=viewport, aboutConfiguration=True,
                   actualActivityEntry=True, noLiveNetwork=True, stages=rows,
                   captures={s:str(DIRECTORY / f'{fixture}-{s}.png') for s in R['STAGES'][scenario]},
                   replacement=replacement, final=copy.deepcopy(rows[-1]['model']))
    return fixture, receipt, scenario


def picker_sample(narrow=False):
    _, baseline, _ = sample('browse', narrow)
    fixture = 'workflow-activity-members' + ('-narrow' if narrow else '')
    source = {row['stage']: row for row in baseline['stages']}
    counts = dict(zip(R['STAGES']['picker'], (0, 0, 0, 1, 1, 1, 1, 2, 3, 5, 6, 7, 8, 9, 9)))
    eligible = [['UACTADA01', 'Ada Lovelace'], ['UACTZOE01', 'Zo\u00eb Alvarez']]
    queries = ('a', 'a', 'ad', 'zz-no-match', 'a', 'a', 'a', 'a', 'zz-no-match')
    requests = [dict(method='users/search', args=dict(count=20, fuzz=1, enable_workspace_ranking=True,
                 filter='NOT deactivated', include_profile_only_users=True, query=q)) for q in queries]
    stages = []
    for index, stage in enumerate(R['STAGES']['picker']):
        active = stage not in ('about', 'draft', 'returned')
        row = copy.deepcopy(source[stage if stage in ('about', 'draft', 'returned') else 'list'])
        row['stage'] = stage
        model, geometry = row['model'], row['geometry']
        count = 0 if stage in ('about', 'draft') else 1 if index < 7 else 2
        reads = copy.deepcopy(source['list']['model']['ledger']['requests']) if count else []
        if count == 2:
            selected = copy.deepcopy(reads[0]); selected['serial'] = 2
            selected['args']['filters'] = json.dumps(dict(user_id='UACTADA01')); reads.append(selected)
        model['ledger'] = dict(requests=reads, finished=count, held=None, cancelled=0, networkCalls=0)
        picker = None if stage == 'about' else dict(generation=0, rows=[], pending=False, notice=None,
                  query='zz-no-match' if stage in ('local-empty', 'query-retired', 'empty', 'returned') else 'a',
                  selection=None, filterUser='', filterRevision=1)
        if stage in ('remote', 'raw-refused', 'option-refused', 'retried'):
            picker.update(generation=1, rows=copy.deepcopy([eligible[1], eligible[0]]))
        if stage == 'selected':
            picker.update(rows=copy.deepcopy(eligible[:1]), selection=eligible[0], filterUser='UACTADA01')
        if stage in ('malformed', 'failure'):
            picker['notice'] = ('People are unavailable while offline. Try again when connected.' if stage == 'failure'
                                else 'People could not be loaded. Try again.')
        cancelled = 0 if index < 8 else 1 if stage == 'query-retired' else 2
        members = dict(ledger=dict(requests=copy.deepcopy(requests[:counts[stage]]), searches=counts[stage],
                       completed=counts[stage], cancelled=cancelled, held=False, networkCalls=0), picker=picker)
        native = None if not active else dict(ok=True, query=picker['query'], selected=picker['filterUser'],
                                              options=[['', 'Anyone']] + copy.deepcopy(picker['rows']))
        exercise = None
        if stage.endswith('-refused'):
            actor = dict(filterUser='', filterRevision=1)
            exercise = dict(kind=stage, before=actor, after=actor, searchesBefore=counts[stage], searchesAfter=counts[stage],
                            nativeAttempt=dict(selected='UACTSTALE' if stage == 'option-refused' else 'UACTADA01',
                                query='Uncommitted native query' if stage == 'raw-refused' else 'a'))
        if stage in ('query-retired', 'filter-retired'):
            exercise = dict(release=dict(query='ad' if stage == 'query-retired' else 'a', delivered=False, cancelledBeforeReply=True))
        row['proof'] = dict(members=members, native=native, exercise=exercise)
        if active:
            box = copy.deepcopy(geometry['controls'][0]['rect'])
            for suffix in ('user', 'user-search', 'find-people'):
                geometry['controls'].append(dict(id='workflow-activity-'+suffix, visible=True, rect=box, disabled=False, scrollSample=True))
            if stage in ('malformed', 'failure'):
                geometry['notices'] = [dict(text=picker['notice'], visible=True, rect=box)]
        if stage in ('malformed', 'failure', 'retried', 'empty', 'returned'):
            box = copy.deepcopy(geometry['controls'][0]['rect'])
            failed = stage in ('malformed', 'failure')
            row['proof']['feedback'] = dict(ok=True, globalToastCount=0, notice=picker['notice'] or '',
                outsideScroll=failed, stable=failed,
                samples=[dict(edge=edge, scrollTop=0, visible=True, rect=copy.deepcopy(box))
                         for edge in ('start', 'end')] if failed else [],
                actions=[dict(id='workflow-management-'+name, visible=True, disabled=False,
                              rect=copy.deepcopy(box), scrollSample=True)
                         for name in ('activity', 'managed', 'check', 'save')] if stage == 'returned' else [],
                actionMeasurement='individual-scroll-samples')
        stages.append(row)
    receipt = dict(ok=True, fixture=fixture, case='picker', viewport=baseline['viewport'], aboutConfiguration=True,
                   actualActivityEntry=True, noLiveNetwork=True, stages=stages,
                   captures={s:str(DIRECTORY / f'{fixture}-{s}.png') for s in R['STAGES']['picker']},
                   final=copy.deepcopy(stages[-1]['model']))
    return fixture, receipt, 'picker'


class ReceiptTests(unittest.TestCase):
    def validate(self, fixture, value, scenario):
        return R['validate'](fixture, value, DIRECTORY, scenario, platform='win32', png=False)

    def mutate(self, stage, edit, scenario='browse'):
        fixture, value, scenario = sample(scenario)
        edit(next(r for r in value['stages'] if r['stage'] == stage))
        with self.assertRaises(RuntimeError): self.validate(fixture, value, scenario)

    def test_four_complete_scenarios(self):
        for scenario in R['STAGES']:
            for narrow in (False, True): self.validate(*sample(scenario, narrow))

    def test_picker_raw_buffer_refusal_is_observed(self):
        self.mutate('raw-refused', lambda r:r['proof']['exercise']['nativeAttempt'].update(query='a'), 'picker')

    def test_picker_refused_option_may_be_restored_after_dispatch(self):
        fixture, receipt, scenario = sample('picker')
        exercise = next(r for r in receipt['stages'] if r['stage'] == 'option-refused')['proof']['exercise']
        exercise['nativeAfter'] = dict(ok=True, selected='', query='a', options=[['', 'Anyone']])
        self.validate(fixture, receipt, scenario)
        exercise['nativeAttempt']['selected'] = ''
        with self.assertRaisesRegex(RuntimeError, 'never attempted'):
            self.validate(fixture, receipt, scenario)

    def test_picker_filters_require_actual_admitted_actor(self):
        self.mutate('selected', lambda r:r['model']['ledger']['requests'][-1]['args'].update(filters='{}'), 'picker')

    def test_picker_rejects_self_or_bot_option(self):
        self.mutate('remote', lambda r:r['proof']['members']['picker']['rows'].append(['UACTBOT01','Bot']), 'picker')

    def test_picker_cannot_claim_cancel_after_delivering_retired_reply(self):
        self.mutate('query-retired', lambda r:r['proof']['exercise']['release'].update(delivered=True), 'picker')

    def test_picker_preserves_actual_rank_order_in_native_options(self):
        self.mutate('remote', lambda r:r['proof']['native']['options'].reverse(), 'picker')

    def test_picker_malformed_is_not_successful_empty(self):
        self.mutate('malformed', lambda r:r['proof']['members']['picker'].update(notice=None), 'picker')

    def test_picker_failure_guidance_must_be_visible(self):
        self.mutate('failure', lambda r:r['geometry']['notices'][0].update(visible=False), 'picker')

    def test_picker_feedback_cannot_remain_in_scroll_content(self):
        self.mutate('malformed', lambda r:r['proof']['feedback'].update(outsideScroll=False), 'picker')

    def test_picker_feedback_must_remain_visible_at_both_scroll_edges(self):
        self.mutate('failure', lambda r:r['proof']['feedback']['samples'][1].update(visible=False), 'picker')

    def test_picker_feedback_cannot_move_with_scroll(self):
        self.mutate('malformed', lambda r:r['proof']['feedback']['samples'][1]['rect'].update(top=999), 'picker')

    def test_picker_scoped_feedback_cannot_duplicate_global_toast(self):
        for stage in ('malformed', 'failure', 'retried', 'empty', 'returned'):
            self.mutate(stage, lambda r:r['proof']['feedback'].update(globalToastCount=1), 'picker')

    def test_picker_retry_must_clear_native_failure(self):
        self.mutate('retried', lambda r:r['proof']['feedback'].update(notice='People could not be loaded. Try again.'), 'picker')

    def test_picker_offline_feedback_is_distinct(self):
        self.mutate('failure', lambda r:r['proof']['feedback'].update(notice='People could not be loaded. Try again.'), 'picker')

    def test_picker_returned_actions_must_be_hit_visible(self):
        self.mutate('returned', lambda r:r['proof']['feedback']['actions'][-1].update(visible=False), 'picker')

    def test_picker_returned_actions_require_all_four_measurements(self):
        self.mutate('returned', lambda r:r['proof']['feedback']['actions'].pop(), 'picker')

    def test_picker_local_path_sends_no_search(self):
        self.mutate('local-empty', lambda r:r['proof']['members']['ledger'].update(searches=1), 'picker')

    def test_picker_exact_request_fields(self):
        self.mutate('remote', lambda r:r['proof']['members']['ledger']['requests'][0]['args'].update(count=50), 'picker')

    def test_picker_retry_must_return_ranked_options(self):
        self.mutate('retried', lambda r:r['proof']['members']['picker'].update(rows=[]), 'picker')

    def test_picker_retains_dirty_metadata_after_return(self):
        self.mutate('returned', lambda r:r['model']['metadata'].update(dirty=False), 'picker')

    def test_detail_requires_actual_emoji_observations(self):
        self.mutate('detail', lambda r:r.update(proof={}))

    def test_activity_prefix_shortcode_must_remain_literal(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['paragraphs'][0].update(text=R['EMOJI_TRIGGER'].replace(':wave:', '\U0001f44b')))

    def test_activity_unknown_and_falsy_placeholders_preserve_exact_text(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['paragraphs'][1].update(text=R['EMOJI_STEP'].replace('{zero}', '')))

    def test_activity_literal_markup_cannot_create_html_or_actions(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(forbiddenNodes=1))

    def test_activity_ready_custom_emoji_must_decode(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['images'][0].update(naturalWidth=0))

    def test_activity_custom_emoji_uses_native_media(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['images'][0].update(src='https://fixture.invalid/activity-ready.png'))

    def test_activity_cold_custom_emoji_stays_readable_without_img(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(coldImages=1))

    def test_activity_explicit_and_trailing_standard_emoji_both_render(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(standardGlyphs=['\U0001f44b']))

    def test_activity_explicit_emoji_does_not_gain_tooltip(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(explicitTitle=':wave:'))

    def test_activity_trailing_emoji_retains_tooltip(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(trailingTitle=None))

    def test_activity_two_trailing_glyphs_do_not_replace_explicit_emoji(self):
        self.mutate('detail', lambda r:r['proof']['emoji'].update(explicitGlyphCount=0,trailingGlyphCount=2))

    def test_activity_nested_span_margins_are_measured_not_asserted(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['paragraphs'][1]['spans'][0].update(margins=['0px','8px','0px','0px']))

    def test_activity_emoji_paragraphs_are_visibly_sampled(self):
        self.mutate('detail', lambda r:r['proof']['emoji']['paragraphs'][0].update(visible=False))

    def test_exact_unicode_is_not_replaced_by_shell_encoding(self):
        self.assertEqual(R['TITLE'], 'R\u00e9vision retained for Activity')
        self.assertEqual(R['DESCRIPTION'], 'Keep these unsaved details.\nReview \u4e2d\U0001f642 e\u0301 together.')

    def test_scroll_samples_not_simultaneous_visibility_claim(self):
        self.mutate('list', lambda r:r['geometry'].update(controlMeasurement='simultaneous'))

    def test_requires_real_entry(self):
        fixture, value, scenario = sample()
        value['actualActivityEntry'] = False
        with self.assertRaisesRegex(RuntimeError, 'entry'): self.validate(fixture, value, scenario)

    def test_terminal_error_is_actionable(self):
        fixture, value, scenario = sample()
        value.update(ok=False, error='Click workflow-activity-next: last geometry')
        with self.assertRaisesRegex(RuntimeError, 'workflow-activity-next'): self.validate(fixture, value, scenario)

    def test_missing_stage(self):
        fixture, value, scenario = sample(); value['stages'].pop(2)
        with self.assertRaisesRegex(RuntimeError, 'stages'): self.validate(fixture, value, scenario)

    def test_uncommitted_text_cannot_disappear(self):
        self.mutate('uncommitted-refused', lambda r:r['geometry']['fields'].update(title=R['TITLE']))

    def test_composition_refusal_cannot_mutate_rust_revision(self):
        self.mutate('composition-refused', lambda r:r['proof'].update(afterRevision=2))
        self.mutate('composition-refused', lambda r:r['proof'].update(sameRevision=False))

    def test_refusal_requires_native_counter_completion(self):
        self.mutate('uncommitted-refused', lambda r:r['proof'].update(afterRefusals=0))
        self.mutate('composition-refused', lambda r:r['proof'].update(beforeRefusals=-1))

    def test_refusal_does_not_mutate_stored_notice_or_buffer(self):
        self.mutate('uncommitted-refused', lambda r:r['model']['metadata'].update(notice=R['REFUSAL']))
        self.mutate('uncommitted-refused', lambda r:r['proof']['native'].update(value=R['TITLE']))

    def test_refusal_notice_must_be_visible(self):
        self.mutate('uncommitted-refused', lambda r:r['geometry']['notices'][0].update(visible=False))
        self.mutate('composition-refused', lambda r:r['geometry'].update(notices=[]))

    def test_refusal_must_not_send_activity_read(self):
        self.mutate('uncommitted-refused', lambda r:r['proof'].update(noActivityRead=False))

    def test_exact_draft_is_preserved_on_return(self):
        self.mutate('returned', lambda r:r['model']['metadata'].update(description='lost'))

    def test_activity_never_acknowledges_uncertain_write(self):
        self.mutate('list', lambda r:r['model']['metadata'].update(outcomes=['Workflow: Acknowledged']), 'retirement')

    def test_pending_native_fields_stay_readonly(self):
        self.mutate('unconfirmed', lambda r:r['geometry']['fields'].update(titleReadonly=False), 'retirement')

    def test_no_metadata_replay(self):
        self.mutate('detail', lambda r:r['model']['metadata'].update(writes=1))

    def test_exact_cursor_retained(self):
        self.mutate('next', lambda r:r['model']['activity']['query'].update(cursor='normalized'))

    def test_previous_query_restored(self):
        self.mutate('previous', lambda r:r['model']['activity'].update(previous=[{}]))

    def test_renderer_local_dates_required(self):
        self.mutate('filtered', lambda r:r['proof'].update(to=0))

    def test_nested_original_parent_required(self):
        self.mutate('nested', lambda r:r['model']['activity']['query'].update(parent_workflow_execution_id='foreign'))

    def test_detail_requires_real_decoded_identity(self):
        self.mutate('detail', lambda r:r['model']['activity']['detail']['workflow_event'].update(workflow_execution_id='foreign'))

    def test_detail_and_original_list_status_must_differ(self):
        self.mutate('detail', lambda r:r['model']['activity']['detail']['workflow_event'].update(workflow_status='SUCCESS'))
        self.mutate('detail', lambda r:r['model']['activity']['detailRow'].update(workflow_instance_status='PENDING'))

    def test_detail_dom_must_use_current_detail_status(self):
        self.mutate('detail', lambda r:r['geometry'].update(detailStatus='Completed'))
        self.mutate('detail', lambda r:r['geometry']['detailStatusGeometry'].update(visible=False))

    def test_final_step_dom_uses_original_selected_list_completion(self):
        self.mutate('detail', lambda r:r['geometry']['lastStepStatuses'][0].update(text='Processing'))
        self.mutate('detail', lambda r:r['geometry'].update(lastStepStatuses=[]))
        self.mutate('detail', lambda r:r['geometry']['lastStepStatuses'][0]['rect'].update(bottom=900))

    def test_malformed_cannot_become_empty_success(self):
        self.mutate('error', lambda r:r['model']['activity'].update(notice=None, page=None))

    def test_feedback_missing_or_clipped_rejected(self):
        self.mutate('error', lambda r:r['geometry'].update(notices=[]))
        self.mutate('error', lambda r:r['geometry']['notices'][0]['rect'].update(bottom=900))

    def test_request_fields_no_started_by_or_secret(self):
        for key, value in [('token', 'synthetic'), ('unexpected', '1')]:
            self.mutate('list', lambda r:r['model']['ledger']['requests'][0]['args'].update({key:value}))
        self.mutate('list', lambda r:r['model']['ledger']['requests'][0]['args'].update(filters='{"user_id":"U1"}'))

    def test_wrong_method_or_reason(self):
        self.mutate('list', lambda r:r['model']['ledger']['requests'][0].update(method='workflows.write'))
        self.mutate('list', lambda r:r['model']['ledger']['requests'][0]['args'].update(_x_reason='guessed'))

    def test_account_refusal_is_not_late_decode_claim(self):
        fixture, value, scenario = sample('retirement')
        value['replacement']['lateReply']['delivered'] = True
        with self.assertRaisesRegex(RuntimeError, 'cancellation'): self.validate(fixture, value, scenario)

    def test_real_account_replacement_retains_attempt(self):
        fixture, value, scenario = sample('retirement')
        value['replacement']['after']['retainedAttempts'] = 0
        with self.assertRaisesRegex(RuntimeError, 'attempt custody'): self.validate(fixture, value, scenario)

    def test_held_read_must_be_actual_pending(self):
        self.mutate('held', lambda r:r['model']['ledger'].update(held=None), 'retirement')

    def test_capture_paths_owned(self):
        fixture, value, scenario = sample(); value['captures']['about'] = '/foreign.png'
        with self.assertRaisesRegex(RuntimeError, 'Foreign capture'): self.validate(fixture, value, scenario)

    def test_readiness_exact_identity(self):
        expected = R['ready_tuple']('workflow-activity', 'a'*32, 123)
        R['validate_ready'](expected, expected)
        for key, value in [('pid',124), ('nonce','b'*32), ('fixture','workflow-activity-narrow')]:
            actual = dict(expected); actual[key] = value
            with self.assertRaises(RuntimeError): R['validate_ready'](actual, expected)


def users_sample(scenario='user-real',narrow=False):
    fixture,base,_=sample('browse',narrow); viewport=base['viewport']; box=dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
    rows=[]; requests=[]
    for stage in R['STAGES'][scenario]:
        if stage in ('policy-failed','detail','held'):
            if not requests:requests.append(dict(method='client.userBoot',args={}))
            requests.extend([dict(method='client.gantryBoot',args={'include_channels':'false','_x_reason':'fetch-client-data'}),dict(method='users.prefs.get',args={})])
        replaced=stage=='account-replaced';held=stage=='held'
        metadata=dict(networkCalls=0,writes=0,title=R['TITLE'],description=R['DESCRIPTION'],dirty=stage!='about',busy=held)
        activity=None if replaced else dict(active=stage not in ('about','draft','returned'),detail={'workflow_event':{}} if stage in ('detail','returned') else None,notice=None)
        detail_count={'about':0,'draft':0,'list':0,'policy-failed':1,'detail':2,'returned':2,'held':3,'account-replaced':3}[stage]
        activity_requests=[] if stage in ('about','draft') else [copy.deepcopy(base['stages'][4]['model']['ledger']['requests'][0])]
        detail_request=copy.deepcopy(base['stages'][8]['model']['ledger']['requests'][-1])
        activity_requests.extend(copy.deepcopy(detail_request) for _ in range(detail_count))
        for serial,req in enumerate(activity_requests,1):req['serial']=serial
        model=dict(account='foreign' if replaced else 'synthetic-workflow-activity-user',epoch=int(replaced),metadata=metadata,activity=activity,
                   ledger=dict(requests=activity_requests,networkCalls=0,held=None,cancelled=0,finished=len(activity_requests)))
        policy=dict(override=1 if scenario=='user-real' else -1,namesRevision=int(stage in ('held','account-replaced')),users=[dict(id=id,raw=dict(name=first.lower()+'_username',real_name=first+' top real',profile=dict(real_name=first+' profile real',display_name=first+' display'))) for id,first in [('UACTADA01','Ada'),('UACTZOE01','Zoe')]],
                    ledger=dict(initialBoot=dict(method='client.userBoot',args={},applied=True),requests=copy.deepcopy(requests),completed=len(requests)-int(held),cancelled=int(replaced),held=held,searches=0,networkCalls=0))
        exercise=None
        g=dict(ok=True,noOverflow=True,viewport=viewport,pngDimensions=[1280,800],controls=[dict(visible=True,rect=copy.deepcopy(box))],notices=[])
        if stage=='policy-failed':
            activity['notice']='Run details could not be loaded. Close the details and try again.'
            exercise=dict(ok=True,users=0,notice=activity['notice']);g['notices']=[dict(text=activity['notice'],visible=True,rect=copy.deepcopy(box))]
        if stage=='detail':
            suffix='profile real' if scenario=='user-real' else 'display'
            exercise=dict(ok=True,measurement='individual-scroll-samples',rows=[dict(text=f'{name} {suffix}',user='UACTADA01' if i<2 else 'UACTZOE01',part='3' if i==1 else '1',message='workflow-activity-trigger-message' if i<2 else 'workflow-activity-step-message-0',tag='STRONG',bold=True,inert=True,visible=True,rect=copy.deepcopy(box)) for i,name in enumerate(('Ada','Ada','Zoe'))])
        if stage=='returned':g['fields']=dict(title=R['TITLE'],description=R['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False)
        if held:exercise=dict(invalidation=dict(before=0,after=1),native=dict(ok=True,users=0))
        if replaced:exercise=dict(foreignDocumentHidden=True,lateReply=dict(delivered=False,cancelledBeforeReply=True))
        rows.append(dict(stage=stage,model=model,geometry=g,proof=dict(policy=policy,exercise=exercise)))
    return fixture,dict(ok=True,fixture=fixture,case=scenario,viewport=viewport,aboutConfiguration=True,actualActivityEntry=True,noLiveNetwork=True,
        stages=rows,captures={stage:str(DIRECTORY/f'{fixture}-{stage}.png') for stage in R['STAGES'][scenario]},final=copy.deepcopy(rows[-1]['model'])),scenario


def refresh_sample(narrow=False,scenario='user-refresh'):
    fixture,base,_=users_sample('user-real',narrow)
    seed={r['stage']:r for r in base['stages']}
    stages=[];counts=[0,0,0,1,2,2,2,2,3,4,5,6,7,7,7,8,9,9,10,10];revisions=[0,0,0,0,1,1,1,2,2,3,3,4,4,5,6,7,8,8,9,0]
    failure='Names could not be refreshed. Close the run details and reopen them to try again.'
    box=seed['detail']['geometry']['controls'][0]['rect']
    raw=copy.deepcopy(seed['detail']['model']['activity']['detail'])
    list_request=seed['list']['model']['ledger']['requests'][0];get_request=seed['detail']['model']['ledger']['requests'][-1]
    for index,stage in enumerate(R['STAGES'][scenario]):
        source='account-replaced' if stage=='account-replaced' else 'about' if stage=='about' else 'returned' if stage in ('draft','warm-return','editor-invalidated','returned') else 'detail' if index>=3 else 'list'
        row=copy.deepcopy(seed[source]);row['stage']=stage;m=row['model'];a=m['activity'];g=row['geometry'];policy=row['proof']['policy']
        held=stage in ('held','account-held');m['metadata']['busy']=held
        if a is not None:a.update(detail=raw if index>=3 else None,notice=None,nameNotice=failure if stage in ('offline','malformed') else None,nameRefreshPending=held,filters=dict(revision=0,status='',**{'from':'','to':'','limit':'100'}))
        get_count=0 if index<3 else 1 if index<10 else 2 if index<12 else 3
        reqs=([copy.deepcopy(list_request)] if index>=2 else [])+[copy.deepcopy(get_request) for _ in range(get_count)]
        for serial,r in enumerate(reqs,1):r['serial']=serial
        m['ledger'].update(requests=reqs,finished=len(reqs))
        policy['override']=-1 if stage in ('event-display','warm-return','warm-reentry','latest-display','returned') else 1
        policy['namesRevision']=revisions[index]
        requests=[dict(method='client.gantryBoot',args={'include_channels':'false','_x_reason':'fetch-client-data'}),dict(method='users.prefs.get',args={})]*counts[index]
        policy['ledger'].update(requests=copy.deepcopy(requests),completed=len(requests)-held,held=held,cancelled=2 if stage=='account-replaced' else 1 if index>=16 else 0)
        ex={};g['notices']=[]
        if stage in ('detail-real','event-display','warm-reentry','reentry-real','offline-retried','malformed-retried','latest-display'):
            ex=copy.deepcopy(seed['detail']['proof']['exercise'])
            if policy['override']==-1:
                for n in ex['rows']:n['text']=n['text'].replace('profile real','display')
        if stage in ('event-display','editor-invalidated','offline','malformed','raw-refused','composition-refused','held','latest-display','account-held'):
            ex=dict(names=ex) if ex else {}
            ex['invalidation']=dict(before=revisions[index]-1,after=revisions[index])
        if stage in ('offline','malformed'):
            ex['footer']=dict(ok=True,notice=failure,visible=True,outsideScroll=True,globalToastCount=0,rect=copy.deepcopy(box));g['notices']=[dict(text=failure,visible=True,rect=copy.deepcopy(box))]
        if stage in ('raw-refused','composition-refused'):
            ex.update(requestsUnchanged=True,before=copy.deepcopy(m),after=copy.deepcopy(m),native=dict(ok=True,value='2026-06-01' if stage=='raw-refused' else '',composing=stage=='composition-refused'))
        if stage=='latest-display':ex['release']=dict(delivered=False,cancelledBeforeReply=True)
        if stage=='account-replaced':ex=copy.deepcopy(seed['account-replaced']['proof']['exercise'])
        if scenario=='user-deactivated':
            for u in policy['users']:u['raw']['deleted']=u['id']=='UACTADA01'
            named=ex.get('names',ex)
            for i,n in enumerate(named.get('rows',[])):
                deleted=i<2
                n.update(rawName=n['text'],nameTag='SPAN',nameCount=1,nameVisible=True,nameRect=copy.deepcopy(box),
                    indicator='(deactivated)' if deleted else None,indicatorTag='SPAN' if deleted else None,
                    indicatorCount=int(deleted),indicatorVisible=True if deleted else None,indicatorRect=copy.deepcopy(box) if deleted else None,
                    indicatorMargin=4 if deleted else None,indicatorWeight=400 if deleted else None,
                    indicatorColor=[161]*3 if deleted else None,deactivated=str(deleted).lower(),deactivatedClass=deleted,
                    shape=True,parentColor=[161]*3 if deleted else [240]*3,mutedColor=[161]*3,ordinaryColor=[240]*3,opacity=1,
                    fragments=[dict(hit=True,rect=copy.deepcopy(box))],nameFragments=[dict(hit=True,rect=copy.deepcopy(box))],
                    indicatorFragments=[dict(hit=True,rect=copy.deepcopy(box))] if deleted else [])
                if deleted:n['text']+='(deactivated)'
        row['proof']['exercise']=ex;stages.append(row)
    receipt=dict(ok=True,fixture=fixture,case=scenario,viewport=base['viewport'],aboutConfiguration=True,actualActivityEntry=True,noLiveNetwork=True,stages=stages,captures={r['stage']:str(DIRECTORY/f"{fixture}-{r['stage']}.png") for r in stages},final=copy.deepcopy(stages[-1]['model']))
    return fixture,receipt,scenario


class RefreshPolicyTests(unittest.TestCase):
    def validate(self,fixture,receipt,scenario):
        return R['validate'](fixture,receipt,DIRECTORY,scenario,platform='win32',png=False)
    def mutate(self,stage,edit):
        f,r,c=refresh_sample();edit(next(x for x in r['stages'] if x['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    def test_refresh_both_widths(self):
        for narrow in (False,True):self.validate(*refresh_sample(narrow))
    def test_refresh_cannot_refetch_activity(self):
        self.mutate('event-display',lambda r:r['model']['ledger']['requests'].append(copy.deepcopy(r['model']['ledger']['requests'][-1])))
    def test_refresh_warm_reentry_cannot_refetch_prefs(self):
        self.mutate('warm-reentry',lambda r:r['proof']['policy']['ledger']['requests'].append(dict(method='users.prefs.get',args={})))
    def test_refresh_cannot_depend_on_picker(self):
        self.mutate('event-display',lambda r:r['proof']['policy']['ledger'].update(searches=1))
    def test_refresh_requires_display_names_after_event(self):
        self.mutate('event-display',lambda r:r['proof']['exercise']['names']['rows'][0].update(text='Ada profile real'))
    def test_refresh_failure_must_retain_raw_detail(self):
        self.mutate('offline',lambda r:r['model']['activity'].update(detail=None))
    def test_refresh_failure_footer_must_be_fixed_visible(self):
        self.mutate('malformed',lambda r:r['proof']['exercise']['footer'].update(outsideScroll=False))
        self.mutate('offline',lambda r:r['proof']['exercise']['footer'].update(visible=False))
    def test_refresh_failure_cannot_duplicate_toast(self):
        self.mutate('offline',lambda r:r['proof']['exercise']['footer'].update(globalToastCount=1))
    def test_refresh_retry_clears_error(self):
        self.mutate('malformed-retried',lambda r:r['model']['activity'].update(nameNotice='stale'))
    def test_refresh_raw_refusal_preserves_attempted_buffer(self):
        self.mutate('raw-refused',lambda r:r['proof']['exercise']['native'].update(value=''))
    def test_refresh_composition_refusal_is_exercised(self):
        self.mutate('composition-refused',lambda r:r['proof']['exercise']['native'].update(composing=False))
    def test_refresh_superseded_reply_is_cancelled(self):
        self.mutate('latest-display',lambda r:r['proof']['exercise']['release'].update(delivered=True))
    def test_refresh_account_retirement_blocks_late_reply(self):
        self.mutate('account-replaced',lambda r:r['proof']['exercise']['lateReply'].update(delivered=True))
    def test_refresh_retains_dirty_return(self):
        self.mutate('returned',lambda r:r['model']['metadata'].update(description='lost'))


class DeactivatedPolicyTests(unittest.TestCase):
    def validate(self,fixture,receipt,scenario):
        return R['validate'](fixture,receipt,DIRECTORY,scenario,platform='win32',png=False)
    def mutate(self,stage,change):
        f,r,c=refresh_sample(False,'user-deactivated');change(next(x for x in r['stages'] if x['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    @staticmethod
    def names(row):
        ex=row['proof']['exercise'];return ex.get('names',ex)['rows']
    def test_deactivated_both_widths_reuses_full_refresh_flow(self):
        for narrow in (False,True):self.validate(*refresh_sample(narrow,'user-deactivated'))
    def test_deactivated_requires_actual_deleted_member(self):
        self.mutate('detail-real',lambda r:r['proof']['policy']['users'][0]['raw'].update(deleted=False))
    def test_deactivated_cannot_mark_ordinary_zoe(self):
        self.mutate('event-display',lambda r:self.names(r)[2].update(deactivated='true'))
    def test_deactivated_raw_label_must_not_include_suffix(self):
        self.mutate('detail-real',lambda r:self.names(r)[0].update(rawName='Ada profile real(deactivated)'))
    def test_deactivated_duplicate_occurrence_requires_indicator(self):
        self.mutate('detail-real',lambda r:self.names(r)[1].update(indicatorCount=0))
    def test_deactivated_rejects_literal_separator_text(self):
        self.mutate('event-display',lambda r:self.names(r)[0].update(shape=False))
    def test_deactivated_indicator_must_be_visible(self):
        self.mutate('latest-display',lambda r:self.names(r)[0].update(indicatorVisible=False))
    def test_deactivated_marker_requires_muted_class(self):
        self.mutate('reentry-real',lambda r:self.names(r)[0].update(deactivatedClass=False))
    def test_deactivated_color_must_differ_from_ordinary(self):
        self.mutate('detail-real',lambda r:self.names(r)[0].update(parentColor=[240]*3))
    def test_deactivated_cannot_dim_using_opacity(self):
        self.mutate('detail-real',lambda r:self.names(r)[0].update(opacity=.5))
    def test_deactivated_css_spacing_and_weight_required(self):
        self.mutate('detail-real',lambda r:self.names(r)[0].update(indicatorMargin=0))
        self.mutate('detail-real',lambda r:self.names(r)[0].update(indicatorWeight=700))
    def test_deactivated_wrapped_fragment_cannot_be_hidden(self):
        self.mutate('detail-real',lambda r:self.names(r)[0]['nameFragments'][0].update(hit=False))
    def test_deactivated_wrapped_fragments_must_be_measured(self):
        self.mutate('detail-real',lambda r:self.names(r)[0].update(indicatorFragments=[]))
    def test_deactivated_must_remain_inert(self):
        self.mutate('offline-retried',lambda r:self.names(r)[0].update(inert=False))
    def test_deactivated_cannot_masquerade_as_ordinary_scenario(self):
        f,r,c=refresh_sample(False,'user-deactivated')
        with self.assertRaises(RuntimeError):self.validate(f,r,'user-refresh')


class UserPolicyTests(unittest.TestCase):
    def validate(self,fixture,receipt,scenario):
        return R['validate'](fixture,receipt,DIRECTORY,scenario,platform='win32',png=False)
    def mutate(self,stage,change,scenario='user-real'):
        f,r,c=users_sample(scenario);change(next(row for row in r['stages'] if row['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    def test_both_policy_outcomes_both_widths(self):
        for case in ('user-real','user-display'):
            for narrow in (False,True):self.validate(*users_sample(case,narrow))
    def test_user_detail_cannot_depend_on_picker(self):
        self.mutate('detail',lambda r:r['proof']['policy']['ledger'].update(searches=1))
    def test_profile_real_precedes_top_and_username(self):
        for wrong in ('Ada top real','ada_username','Ada display'):
            self.mutate('detail',lambda r:r['proof']['exercise']['rows'][0].update(text=wrong))
    def test_display_override_is_observed_not_default(self):
        self.mutate('detail',lambda r:r['proof']['policy'].update(override=1),'user-display')
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'][0].update(text='Ada profile real'),'user-display')
    def test_duplicate_occurrence_is_preserved(self):
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'].pop(1))
    def test_user_occurrence_identity_not_only_matching_text(self):
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'][1].update(part='1'))
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'][2].update(user='UACTADA01'))
    def test_fixture_source_fields_must_distinguish_name_precedence(self):
        self.mutate('detail',lambda r:r['proof']['policy']['users'][0]['raw'].update(real_name='Ada profile real'))
    def test_user_is_bold_inert_and_visible(self):
        for key in ('bold','inert','visible'):
            self.mutate('detail',lambda r:r['proof']['exercise']['rows'][1].update({key:False}))
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'][0].update(tag='BUTTON'))
    def test_missing_policy_cannot_publish_detail(self):
        self.mutate('policy-failed',lambda r:r['model']['activity'].update(detail={'workflow_event':{}}))
        self.mutate('policy-failed',lambda r:r['geometry'].update(notices=[]))
    def test_actual_policy_retry_required(self):
        self.mutate('detail',lambda r:r['proof']['policy']['ledger']['requests'].pop())
    def test_preference_event_must_retire_snapshot(self):
        self.mutate('held',lambda r:r['proof']['exercise']['invalidation'].update(after=0))
        self.mutate('held',lambda r:r['proof']['exercise']['native'].update(users=3))
    def test_account_replacement_cancels_original_policy(self):
        self.mutate('account-replaced',lambda r:r['proof']['exercise']['lateReply'].update(delivered=True))
        self.mutate('account-replaced',lambda r:r['proof']['policy']['ledger'].update(cancelled=0))
        self.mutate('account-replaced',lambda r:r['model'].update(activity={'detail':{}}))
    def test_dirty_draft_returns_without_write(self):
        self.mutate('returned',lambda r:r['geometry']['fields'].update(description='lost draft'))
        self.mutate('detail',lambda r:r['model']['metadata'].update(writes=1))
    def test_policy_transport_is_closed(self):
        self.mutate('policy-failed',lambda r:r['proof']['policy']['ledger']['requests'][1].update(method='users/search'))
        self.mutate('policy-failed',lambda r:r['proof']['policy']['ledger']['requests'][0]['args'].update(token='synthetic'))


def channel_labels_sample(fetched=False):
    rows=[]
    for i,id in enumerate(['workflow-activity-channel-trigger-1','workflow-activity-channel-trigger-3','workflow-activity-channel-step-0-1']):
        known=i<2 or fetched
        name='activity-known' if i<2 else 'activity-fetched' if fetched else 'Channel unavailable'
        box=dict(left=30,top=10,right=180,bottom=30,width=150,height=20)
        icon=dict(tag='svg',ariaHidden='true',viewBox='0 -960 960 960',material=True,pathMatches=True,visible=True,
                  rect=dict(left=10,top=12,right=26,bottom=28,width=16,height=16)) if known else None
        rows.append(dict(id=id,channel='CACTKNOWN01' if i<2 else 'CACTMISS01',text=name,name=name,tag='BUTTON',labelTag='SPAN',nameTag='SPAN',
                         iconCount=int(known),svgCount=int(known),icon=icon,nameRect=box,buttonRect=dict(left=8,top=8,right=182,bottom=32,width=174,height=24),
                         visible=True,inline=True if known else None,scrollSample=True,ok=True))
    return dict(ok=True,fetched=fetched,measurement='individual-scroll-samples',rows=rows)


def channel_sample(narrow=False):
    fixture, base, _ = sample('browse', narrow)
    viewport=base['viewport']; box=dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
    activity=base['stages'][4]['model']['ledger']['requests']
    doc=dict(id=17,title=R['TITLE'],description=R['DESCRIPTION'],dirty=True,pending=None,revision=23)
    channel_read=dict(method='conversations.info',args={'channel':'CACTMISS01','include_num_members':'false'})
    about=[dict(method='conversations.info',args={'channel':'DAPP','return_app_home':'true','_x_reason':'app-home-open-api'}),
           dict(method='apps.about.info',args={'app_id':'A_HOME'}),dict(method='apps.profile.get',args={'app':'A_HOME'}),dict(method='experiments.getByUser',args={})]
    counts=dict.fromkeys(R['STAGES']['channel'],0);counts.update(malformed=1,offline=2,held=3,retired=3,retried=4,**{'final-return':4})
    rows=[]
    for i,stage in enumerate(R['STAGES']['channel']):
        returned=i>=7; final=stage=='final-return'; n=counts[stage]
        ch=dict(ledger=dict(original='DAPP',requests=copy.deepcopy([channel_read]*n),aboutRequests=copy.deepcopy(about*(2 if final else int(returned))),
                    completed=n-(stage=='held'),cancelled=int(i>=11),held=stage=='held',networkCalls=0),
                documents=[] if stage=='about' else [copy.deepcopy(doc)],selected='CACTKNOWN01' if stage=='known' else 'CACTMISS01' if stage=='retried' else 'DAPP',
                generation=1,aboutVisit=3 if final else 2 if returned else 1,managementVisit=6 if final else 5 if returned else 4,
                missingPresent=i>=12)
        ids=['DAPP','CACTKNOWN01']+(['CACTMISS01'] if i>=12 else [])
        names={'DAPP':'Review requests','CACTKNOWN01':'activity-known','CACTMISS01':'activity-fetched'}
        sidebar_rows=[dict(id=id,name=names[id],index=index,section='apps' if id=='DAPP' else 'channels',active=id==ch['selected']) for index,id in enumerate(ids)]
        sections=[dict(id='apps',title='Agents & apps',indices=[0]),dict(id='channels',title='Channels',indices=list(range(1,len(ids))))]
        active=ids.index(ch['selected'])
        ch['sidebar']=dict(rows=sidebar_rows,sections=sections,activeIndex=active,activeId=ch['selected'])
        ch['sourceRows']=[dict(id=id,name=names[id],isMember=None if id=='DAPP' else True,isOpen=None,unreadCount=None,unreadDisplay=None,mentionCount=None,hasUnreads=False) for id in ids]
        metadata=dict(networkCalls=0,writes=0,reads=36 if final else 24 if returned else 12)
        reads=copy.deepcopy(activity if i>=2 else [])
        model=dict(epoch=0,account='synthetic-workflow-activity-channel',metadata=metadata,ledger=dict(networkCalls=0,requests=reads,finished=len(reads),held=None,cancelled=0))
        g=dict(ok=True,noOverflow=True,viewport=viewport,pngDimensions=[1280,800],controls=[dict(id='control',visible=True,rect=copy.deepcopy(box))],notices=[])
        exercise=None
        if stage=='detail':
            exercise=dict(rows=[dict(id='workflow-activity-channel-trigger-1',channel='CACTKNOWN01',text='activity-known',tag='BUTTON'),dict(id='workflow-activity-channel-trigger-3',channel='CACTKNOWN01',text='activity-known',tag='BUTTON'),dict(id='workflow-activity-channel-step-0-1',channel='CACTMISS01',text='Channel unavailable',tag='BUTTON')])
        if stage.endswith('-refused'):
            value='ERROR' if stage=='filter-refused' else 'Uncommitted native search' if stage=='search-refused' else ''
            composing=stage=='composition-refused'
            exercise=dict(unchangedRustState=True,before=copy.deepcopy(ch),after=copy.deepcopy(ch),attempt=dict(value=value,composing=composing,refusals=0),native=dict(value=value,composing=composing,refusals=1,notice='Finish editing the current text before opening this conversation. Your draft is unchanged.'))
        if stage in ('returned','final-return'):
            before=copy.deepcopy(ch);before['aboutVisit']-=1;before['managementVisit']-=1
            exercise=dict(before=before,after=copy.deepcopy(ch),nativeAboutClick=dict(ok=True,selected='Review requests',scope='0-TWORKFLOW-DAPP-5'),reentryLabels=channel_labels_sample(final))
            g['fields']=dict(title=R['TITLE'],description=R['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False)
        if stage in ('malformed','offline'):
            message='Could not open this conversation. It may no longer be accessible.' if stage=='malformed' else 'Connect to Slack to open this conversation.'
            exercise=dict(native=dict(detail=True,notice=message,globalToastCount=0,visible=True,outsideScroll=True,rect=copy.deepcopy(box)),
                          before=copy.deepcopy(ch),after=copy.deepcopy(ch),modelBefore=copy.deepcopy(model),modelAfter=copy.deepcopy(model))
            g['notices']=[dict(text=message,visible=True,rect=copy.deepcopy(box))]
        if stage=='retired':exercise=dict(delivered=False,cancelledBeforeReply=True)
        proof=dict(channel=ch,exercise=exercise)
        if stage in ('detail','filter-refused','search-refused','composition-refused','malformed','offline','held'):
            proof['channelLabels']=channel_labels_sample()
        proof['sidebarNative']=dict(ok=True,rows=[dict(name=row['name'],active=row['active']) for row in sidebar_rows],headings=[section['title'] for section in sections],activeId=ch['selected'],activeIndex=active,activePosition=active,measured=stage in ('known','retried'),visible=True,hit=True,rect=copy.deepcopy(box))
        if stage in ('known','retried'):
            name='activity-known' if stage=='known' else 'activity-fetched'
            timeline=f"0-TWORKFLOW-{ch['selected']}-1"
            proof['conversationOwner']=dict(timeline=timeline,composer='synthetic-current-composer')
            g.update(conversation=True,timelineScope=timeline,composerScope='synthetic-current-composer',headerText='#'+name,placeholder='Message #'+name,
                     controls=[dict(id=id,visible=True,rect=copy.deepcopy(box)) for id in ('channel-details-trigger','message-timeline','channel-composer')])
        rows.append(dict(stage=stage,model=model,geometry=g,proof=proof))
    return fixture,dict(ok=True,fixture=fixture,case='channel',viewport=viewport,aboutConfiguration=True,actualActivityEntry=True,noLiveNetwork=True,
        stages=rows,captures={stage:str(DIRECTORY/f'{fixture}-{stage}.png') for stage in R['STAGES']['channel']},final=copy.deepcopy(rows[-1]['model']))


class ChannelTests(unittest.TestCase):
    def validate(self,fixture,receipt):
        return R['validate'](fixture,receipt,DIRECTORY,'channel',platform='win32',png=False)
    def mutate(self,stage,change):
        f,r=channel_sample();change(next(row for row in r['stages'] if row['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r)
    def test_channel_both_widths(self):
        for narrow in (False,True):self.validate(*channel_sample(narrow))
    def test_channel_raw_name_separate_from_icon(self):
        self.mutate('detail',lambda r:r['proof']['channelLabels']['rows'][0].update(name='#activity-known'))
        self.mutate('offline',lambda r:r['proof']['channelLabels']['rows'][1].update(nameTag='BUTTON'))
    def test_both_known_occurrences_require_material_tag(self):
        self.mutate('detail',lambda r:r['proof']['channelLabels']['rows'][1]['icon'].update(pathMatches=False))
        self.mutate('malformed',lambda r:r['proof']['channelLabels']['rows'][0]['icon'].update(ariaHidden='false'))
        self.mutate('held',lambda r:r['proof']['channelLabels']['rows'][0].update(iconCount=0))
    def test_unavailable_channel_has_no_fake_icon(self):
        self.mutate('detail',lambda r:r['proof']['channelLabels']['rows'][2].update(svgCount=1))
        self.mutate('search-refused',lambda r:r['proof']['channelLabels']['rows'][2].update(icon={}))
    def test_icon_name_geometry_is_visible_and_inline(self):
        self.mutate('detail',lambda r:r['proof']['channelLabels']['rows'][0]['icon'].update(visible=False))
        self.mutate('filter-refused',lambda r:r['proof']['channelLabels']['rows'][0]['icon']['rect'].update(right=70))
        self.mutate('composition-refused',lambda r:r['proof']['channelLabels']['rows'][1]['nameRect'].update(bottom=9999))
    def test_real_reentry_rechecks_known_and_fetched_label(self):
        self.mutate('returned',lambda r:r['proof']['exercise'].pop('reentryLabels'))
        self.mutate('final-return',lambda r:r['proof']['exercise'].update(reentryLabels=channel_labels_sample(False)))
        self.mutate('final-return',lambda r:r['proof']['exercise']['reentryLabels']['rows'][2].update(name='Channel unavailable'))
    def test_fetched_sidebar_row_required_after_acceptance(self):
        self.mutate('retried',lambda r:r['proof']['channel']['sidebar']['rows'].pop())
    def test_cancelled_lookup_cannot_publish_sidebar_row(self):
        self.mutate('retired',lambda r:r['proof']['channel']['sidebar']['rows'].append(dict(id='CACTMISS01',name='activity-fetched',index=2,section='channels',active=False)))
    def test_native_active_row_maps_exact_index(self):
        self.mutate('retried',lambda r:r['proof']['sidebarNative'].update(activeIndex=0))
        self.mutate('known',lambda r:r['proof']['sidebarNative']['rows'][0].update(active=True))
    def test_sidebar_target_must_be_visible_and_hit(self):
        self.mutate('retried',lambda r:r['proof']['sidebarNative'].update(visible=False))
        self.mutate('known',lambda r:r['proof']['sidebarNative'].update(hit=False))
    def test_sidebar_projection_cannot_invent_counts_or_membership(self):
        self.mutate('retried',lambda r:r['proof']['channel']['sourceRows'][-1].update(unreadCount=0))
        self.mutate('known',lambda r:r['proof']['channel']['sourceRows'][0].update(isMember=True))
    def test_unbound_account_refused(self):
        self.mutate('about',lambda r:r['model'].update(account=None))
    def test_stale_conversation_dom_refused(self):
        self.mutate('returned',lambda r:r['proof']['exercise']['nativeAboutClick'].update(scope='0-TWORKFLOW-CACTKNOWN01-4'))
    def test_conversation_scope_and_labels_required_without_sidebar_membership(self):
        self.mutate('retried',lambda r:r['geometry'].update(timelineScope='0-TWORKFLOW-C2-1'))
        self.mutate('known',lambda r:r['geometry'].update(composerScope='retired'))
        self.mutate('retried',lambda r:r['geometry'].update(headerText='#wrong-channel'))
        self.mutate('retried',lambda r:r['geometry'].update(placeholder='Message #wrong-channel'))
    def test_actual_conversation_geometry_required(self):
        self.mutate('retried',lambda r:r['geometry']['controls'][0].update(id='selected-sidebar-row'))
    def test_reinstalled_document_refused(self):
        self.mutate('returned',lambda r:r['proof']['channel']['documents'][0].update(id=99))
    def test_missing_fresh_about_reads_refused(self):
        self.mutate('returned',lambda r:r['proof']['channel']['ledger'].update(aboutRequests=[]))
    def test_missing_fresh_policy_reads_refused(self):
        self.mutate('returned',lambda r:r['model']['metadata'].update(reads=12))
    def test_refusal_must_preserve_actual_native_buffer(self):
        self.mutate('search-refused',lambda r:r['proof']['exercise']['native'].update(value=''))
    def test_duplicate_occurrences_have_distinct_ids(self):
        self.mutate('detail',lambda r:r['proof']['exercise']['rows'][1].update(id='workflow-activity-channel-trigger-1'))
    def test_retired_response_cannot_publish(self):
        self.mutate('retired',lambda r:r['proof']['channel'].update(missingPresent=True))
        self.mutate('retired',lambda r:r['proof']['exercise'].update(delivered=True))
    def test_known_navigation_never_fetches(self):
        self.mutate('known',lambda r:r['proof']['channel']['ledger']['requests'].append(dict(method='conversations.info',args={})))
    def test_failure_remains_distinct_from_navigation(self):
        self.mutate('offline',lambda r:r['proof']['exercise']['native'].update(notice=''))
    def test_failure_footer_cannot_be_hidden_or_duplicate_toast(self):
        self.mutate('malformed',lambda r:r['proof']['exercise']['native'].update(globalToastCount=1))
        self.mutate('offline',lambda r:r['proof']['exercise']['native'].update(visible=False))
        self.mutate('offline',lambda r:r['proof']['exercise']['native'].update(outsideScroll=False))
    def test_failure_preserves_source_and_model(self):
        self.mutate('malformed',lambda r:r['proof']['exercise']['before'].update(selected='foreign'))
        self.mutate('offline',lambda r:r['proof']['exercise']['modelBefore'].update(account='foreign'))
    def test_secret_or_foreign_endpoint_refused(self):
        self.mutate('malformed',lambda r:r['proof']['channel']['ledger']['requests'][0]['args'].update(token='synthetic'))
        self.mutate('returned',lambda r:r['proof']['channel']['ledger']['aboutRequests'][0].update(method='conversations.open'))



def export_sample(scenario='export',narrow=False):
    fixture='workflow-activity'+('-narrow' if narrow else '')
    viewport=[668,380] if narrow else [1280,780]
    box=dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
    selected=dict(user='UWORKFLOW',label='Owner display',initialFixtureSelection=True,pickerSelectionClaimed=False)
    dates=dict(from_=1781496000,to=1781668799);dates['from']=dates.pop('from_')
    counts=[0,0,0,0,0,0,0,0,1,1,2,3,4,5,6,6,6] if scenario=='export' else [0,0,0,1,1,2,2]
    phases=({'preflight-refused':'not-sent','held':'sent','initiated':'initiated','rejected':'rejected','retried':'initiated','offline':'not-sent','offline-retried':'initiated','malformed':'unknown','unknown-blocked':'unknown','returned':'unknown'} if scenario=='export' else {'route-held':'sent','route-retired':'unknown','account-held':'sent','account-replaced':'initiated'})
    history=[];requests=[];rows=[]
    for i,stage in enumerate(R['STAGES'][scenario]):
        hidden=stage in ('about','account-replaced');active=not hidden and stage not in ('draft','returned','route-retired')
        held=stage in ('held','route-held','account-held')
        count=counts[i]
        if stage=='preflight-refused':history.append(dict(id=1,visit=1,document=1,phase='not-sent',arguments=None,receipt=None,initiated=False))
        if len(requests)<count:
            ordinal=len(requests)+1;aid=ordinal+(scenario=='export')
            filters=dict(user_id='UWORKFLOW',from_timestamp=dates['from']-(86400 if scenario=='export-retirement' and ordinal==2 else 0),to_timestamp=dates['to'])
            arguments=dict(workflow_id='WF_REVIEW',filters=filters)
            history.append(dict(id=aid,visit=1,document=1,phase='sent',arguments=arguments,receipt=None,initiated=False))
            requests.append(dict(serial=ordinal,attempt=aid,method='workflows.activity.export',args=dict(workflow_id='WF_REVIEW',filters=json.dumps(filters),_x_reason='export_workflow_activity_csv'),dispatch='not-sent' if scenario=='export' and ordinal==4 else 'synthetic-entered'))
        if stage in phases:
            a=history[-1];a['phase']=phases[stage];a['initiated']=a['phase']=='initiated'
            if a['initiated']:a['receipt']=dict(ok=True,fixture_receipt='initiation-only',retained_unknown=17)
            if stage=='rejected':a['receipt']=dict(ok=False,error='not_allowed')
            if stage=='offline':a['receipt']=dict(failure='offline')
            if stage=='malformed':a['receipt']=dict(ok='malformed')
            if stage=='route-retired':a['receipt']=dict(failure='transport')
        book=dict(attempts=copy.deepcopy(history),current=dict(shown=active,disabled=held or stage in ('malformed','unknown-blocked'),notice=R['EXPORT_NOTICES'][phases[stage]] if active and stage in phases else None),retained=len(history))
        ledger=dict(requests=copy.deepcopy(requests),networkCalls=0,finished=count-int(held),held=history[-1]['id'] if held else None,cancelled=0,policy='missing' if stage in ('about','draft','gate-missing') and scenario=='export' else 'off' if stage in ('gate-off','preflight-refused') else 'on')
        meta=dict(route='about' if hidden else 'editor',networkCalls=0,writes=0,pending=False,busy=held,title=None if hidden else R['TITLE'],description=None if hidden else R['DESCRIPTION'],dirty=not hidden)
        model=dict(metadata=meta,activity=None if hidden else dict(active=active,query=dict(parent_workflow_execution_id='E_REVIEW') if stage=='held' else dict(cursor=R['CURSOR']) if stage=='rejected' else {}),ledger=dict(networkCalls=0,requests=[]),export=book,exportFixture=ledger)
        measure=lambda text:dict(text=text,visible=True,rect=copy.deepcopy(box))
        native=dict(button=None if not active or stage in ('gate-missing','gate-off','preflight-refused') else dict(disabled=held or stage in ('malformed','unknown-blocked'),text='Export CSV'),notice=measure(R['EXPORT_NOTICES'][phases[stage]]) if active and stage in phases else None,nativeNotice=None,user='UWORKFLOW' if active else None,userLabel=selected['label'] if active else None,from_='2026-06-15',to='2026-06-16',status='SUCCESS',limit='20')
        native['from']=native.pop('from_')
        exercise=None
        if stage=='draft':exercise=selected.copy()
        if stage in ('ready','list'):exercise=dict(dates=dates.copy(),selected=selected.copy())
        if stage in ('raw-refused','composition-refused'):
            v='2026-06-01' if stage=='raw-refused' else '2026-06-15';native['from']=v
            native['nativeNotice']=measure(R['EXPORT_REFUSAL'])
            exercise=dict(unchangedRustState=True,composition=stage=='composition-refused',rawValue=v,before=dict(refusals=0,value=v),after=dict(refusals=1,value=v,notice=R['EXPORT_REFUSAL']))
        if stage in ('initiated','route-retired'):exercise=dict(delivered=True,cancelledBeforeReply=False)
        if stage=='account-replaced':exercise=dict(previousEpoch=0,currentEpoch=1,foreignDocumentHidden=True,release=dict(delivered=True,cancelledBeforeReply=False))
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,activity=active,controls=[dict(id='workflow-activity-export' if active else 'workflow-configuration',visible=True,rect=box)],notices=[],pngDimensions=[1024,656] if narrow else [1600,975])
        rows.append(dict(stage=stage,model=model,proof=dict(export=copy.deepcopy(book),ledger=copy.deepcopy(ledger),native=native,exercise=exercise),geometry=geometry))
    receipt=dict(ok=True,fixture=fixture,case=scenario,viewport=viewport,aboutConfiguration=True,actualActivityEntry=True,noLiveNetwork=True,stages=rows,captures={s:str(DIRECTORY/f'{fixture}-{s}.png') for s in R['STAGES'][scenario]},final=copy.deepcopy(rows[-1]['model']))
    return fixture,receipt,scenario


class ExportTests(unittest.TestCase):
    def validate(self,f,r,c):return R['validate'](f,r,DIRECTORY,c,platform='win32',png=False)
    def mutate(self,stage,change,case='export'):
        f,r,c=export_sample(case);change(next(x for x in r['stages'] if x['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    def test_both_export_cases_both_widths(self):
        for c in ('export','export-retirement'):
            for narrow in (False,True):self.validate(*export_sample(c,narrow))
    def test_export_requires_actual_nested_source(self):self.mutate('held',lambda r:r['model']['activity']['query'].clear())
    def test_export_requires_actual_cursor_source(self):self.mutate('rejected',lambda r:r['model']['activity']['query'].clear())
    def test_gate_missing_cannot_show_export(self):self.mutate('gate-missing',lambda r:r['proof']['native'].update(button={'disabled':False}))
    def test_gate_off_cannot_show_export(self):self.mutate('gate-off',lambda r:r['proof']['native'].update(button={'disabled':False}))
    def test_raw_refusal_requires_original_buffer(self):self.mutate('raw-refused',lambda r:r['proof']['exercise']['after'].update(value='2026-06-15'))
    def test_refusal_requires_fresh_native_ack(self):self.mutate('composition-refused',lambda r:r['proof']['exercise']['after'].update(refusals=0))
    def test_refusal_requires_no_rust_write(self):self.mutate('raw-refused',lambda r:r['proof']['exercise'].update(unchangedRustState=False))
    def test_refusal_feedback_must_be_readable(self):self.mutate('raw-refused',lambda r:r['proof']['native']['nativeNotice'].update(visible=False))
    def test_feedback_clipping_rejected(self):self.mutate('malformed',lambda r:r['proof']['native']['notice']['rect'].update(bottom=9999,height=9989))
    def test_positive_ack_is_not_completion(self):self.mutate('initiated',lambda r:r['proof']['native']['notice'].update(text='CSV ready to download.'))
    def test_selected_user_requires_actual_native_value(self):self.mutate('ready',lambda r:r['proof']['native'].update(user=''))
    def test_initial_selection_does_not_claim_picker(self):self.mutate('draft',lambda r:r['proof']['exercise'].update(pickerSelectionClaimed=True))
    def test_unknown_equivalent_query_stays_disabled(self):self.mutate('unknown-blocked',lambda r:r['proof']['native']['button'].update(disabled=False))
    def test_held_duplicateclick_stays_disabled(self):self.mutate('held',lambda r:r['proof']['native']['button'].update(disabled=False))
    def test_account_late_reply_retained(self):self.mutate('account-replaced',lambda r:r['proof']['exercise']['release'].update(delivered=False),'export-retirement')
    def test_account_must_really_advance(self):self.mutate('account-replaced',lambda r:r['proof']['exercise'].update(currentEpoch=0),'export-retirement')
    def test_export_request_forbids_page_and_parent(self):
        f,r,c=export_sample();row=next(x for x in r['stages'] if x['stage']=='held');req=row['proof']['ledger']['requests'];args={a['id']:a['arguments'] for a in row['proof']['export']['attempts']}
        for field in ('cursor','limit','parent_workflow_execution_id','token'):
            bad=copy.deepcopy(req);bad[0]['args'][field]='unexpected'
            with self.assertRaises(RuntimeError):R['validate_export_requests'](bad,args)
    def test_export_request_does_not_lowercase_ui_status(self):
        f,r,c=export_sample();row=next(x for x in r['stages'] if x['stage']=='held');req=copy.deepcopy(row['proof']['ledger']['requests']);args={a['id']:a['arguments'] for a in row['proof']['export']['attempts']}
        filters=json.loads(req[0]['args']['filters']);filters['workflow_instance_status']='success';req[0]['args']['filters']=json.dumps(filters)
        with self.assertRaises(RuntimeError):R['validate_export_requests'](req,args)
    def test_export_request_never_replays_attempt(self):
        f,r,c=export_sample();row=next(x for x in r['stages'] if x['stage']=='rejected');req=copy.deepcopy(row['proof']['ledger']['requests']);args={a['id']:a['arguments'] for a in row['proof']['export']['attempts']};req[1]['attempt']=req[0]['attempt']
        with self.assertRaises(RuntimeError):R['validate_export_requests'](req,args)


def execution_sample(scenario='execution',narrow=False):
    fixture='workflow-activity'+('-narrow' if narrow else '');viewport=[668,380] if narrow else [1280,780]
    box=dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
    source=[dict(ts='1781524800.000001',blocks=[dict(type='actions',block_id='execution-'+str(i),elements=[dict(type='button',action_id='workflow_error_view_logs',accessibility_label=label,value=json.dumps(dict(type='wfb',wf_id='WF_REVIEW',execution_id=execution)))]) for i,(label,execution) in enumerate(zip(('View workflow execution','View missing execution','Unavailable execution action'),('E_REVIEW','E_ABSENT',7)))])]
    counts={'execution':[0,2,2,3,4,4,4,6,7],'execution-failure':[0,0,2,3,4,4],'execution-retirement':[0,1,1,2,2]}[scenario]
    method_suffixes={'execution':['list','get','list','list','list','get','list'],'execution-failure':['list','get','list','list'],'execution-retirement':['list','list']}[scenario]
    requests=[]
    for i,method in enumerate(method_suffixes,1):
        args=dict(workflow_id='WF_REVIEW',_x_reason='Load instance steps for activity tab' if method=='get' else 'multi-click-activity-list-fetcher')
        if method=='get':args['workflow_execution_id']='E_REVIEW'
        else:args.update(limit='100',filters='{}')
        requests.append(dict(serial=i,method='workflows.activity.'+method,args=args))
    rows=[]
    for i,stage in enumerate(R['STAGES'][scenario]):
        direct=stage not in ('transcript','returned','route-retired','account-replaced');active=direct and stage not in ('metadata-draft','policy-denied')
        detail=stage in ('opened','reopened','policy-retried');held=stage in ('route-held','account-held');dirty=scenario=='execution' and i>=5
        cancelled=2 if stage=='account-replaced' else 1 if stage in ('route-retired','account-held') else 0
        entry=dict(active=direct,busy=held,autoPending=False,workflow='WF_REVIEW' if direct else None,execution=('E_ABSENT' if stage=='missing-target' else 'E_REVIEW') if direct else None,visit=6 if direct else None,notice=R['ENTRY_FAILURE'] if stage=='policy-denied' else None)
        meta=dict(networkCalls=0,writes=0,pending=False,busy=held,title=R['TITLE'] if dirty and direct else None,description=R['DESCRIPTION'] if dirty and direct else None,dirty=dirty and direct)
        activity=None if not active and stage!='metadata-draft' else dict(active=active,detail=dict(workflow_event=dict(workflow_execution_id='E_REVIEW',workflow_status='PENDING')) if detail else None,query=dict(limit=20,filters=dict(workflow_instance_status='SUCCESS'),**({'cursor':R['CURSOR']} if stage=='paged' else {})),notice=R['FAILURE'] if stage=='read-failed' else None)
        model=dict(entry=entry,metadata=meta,activity=activity,ledger=dict(networkCalls=0,requests=copy.deepcopy(requests[:counts[i]]),finished=counts[i]-int(held),cancelled=cancelled,held=counts[i] if held else None))
        origin=dict(team='TWORKFLOW',channel='C2',ordinaryChannel=True,messagesTab=True,aboutPresent=False,homePresent=False,composerModel='',appModal=False,source=copy.deepcopy(source),documents=[dict(id=8,title=R['TITLE'],description=R['DESCRIPTION'],dirty=True)] if dirty else [])
        composer=None if stage=='account-replaced' else dict(sameNode=True,value=R['EXECUTION_COMPOSER'],start=3,end=10,visible=not direct,visit='1',scope='fixture-scope')
        ex=None
        if stage=='transcript':ex=dict(unchangedAfterMalformed=True,malformed=dict(disabled=True,label='Unavailable execution action'),initialComposer=copy.deepcopy(composer))
        if stage=='missing-target':ex=dict(readCountBefore=counts[i]-1,readCountAfter=counts[i])
        if stage=='route-retired':ex=dict(cancelledBeforeReply=True,delivered=False)
        if stage=='account-replaced':ex=dict(previousEpoch=0,currentEpoch=1,foreignDocumentHidden=True,release=dict(cancelledBeforeReply=True,delivered=False))
        if stage=='metadata-draft':ex=dict(backRefusal=dict(before=copy.deepcopy(model),after=copy.deepcopy(model),sourceBefore=copy.deepcopy(origin),sourceAfter=copy.deepcopy(origin),native=dict(ok=True,sameRoot=True,sameTitle=True,sameNotice=True,visible=True,value=R['ENTRY_RAW_TITLE'],notice=R['ENTRY_BACK_REFUSAL'],rect=copy.deepcopy(box)),restored=dict(ok=True,sameTitle=True,value=R['TITLE'])))
        notice=R['ENTRY_FAILURE'] if stage=='policy-denied' else R['FAILURE'] if stage=='read-failed' else None
        if stage=='metadata-draft':notice=R['ENTRY_BACK_REFUSAL']
        geometry=dict(ok=True,noOverflow=True,viewport=viewport,activity=active,detail=detail,detailStatus='Processing' if detail else None,controls=[dict(id='workflow-entry-back',visible=True,rect=copy.deepcopy(box))],notices=[dict(text=notice,visible=True,rect=copy.deepcopy(box))] if notice else [],pngDimensions=[1024,656] if narrow else [1600,975])
        header=dict(ok=True,sameHeader=True,sameName=True,sameConversation=True,visible=True,text='#ship',display='flex',scrollTop=0,originalScrollTop=0,rect=copy.deepcopy(box)) if stage in ('transcript','returned','route-retired') else None
        rows.append(dict(stage=stage,model=model,geometry=geometry,proof=dict(source=origin,composer=composer,header=header,exercise=ex)))
    receipt=dict(ok=True,fixture=fixture,case=scenario,viewport=viewport,actualMessageAction=True,aboutConfiguration=False,actualActivityEntry=True,noLiveNetwork=True,stages=rows,captures={s:str(DIRECTORY/f'{fixture}-{s}.png') for s in R['STAGES'][scenario]},final=copy.deepcopy(rows[-1]['model']))
    return fixture,receipt,scenario


class ExecutionTests(unittest.TestCase):
    def validate(self,f,r,c):return R['validate'](f,r,DIRECTORY,c,platform='win32',png=False)
    def mutate(self,stage,change,case='execution'):
        f,r,c=execution_sample(case);change(next(x for x in r['stages'] if x['stage']==stage))
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    def test_execution_three_cases_both_widths(self):
        for case in ('execution','execution-failure','execution-retirement'):
            for narrow in (False,True):self.validate(*execution_sample(case,narrow))
    def test_header_proof_required(self):self.mutate('returned',lambda r:r['proof'].update(header=None))
    def test_header_display_restored(self):self.mutate('returned',lambda r:r['proof']['header'].update(display='none'))
    def test_header_visible_after_retirement(self):self.mutate('route-retired',lambda r:r['proof']['header'].update(visible=False),'execution-retirement')
    def test_header_original_node_retained(self):self.mutate('returned',lambda r:r['proof']['header'].update(sameHeader=False))
    def test_header_original_label_node_retained(self):self.mutate('returned',lambda r:r['proof']['header'].update(sameName=False))
    def test_header_original_conversation_retained(self):self.mutate('returned',lambda r:r['proof']['header'].update(sameConversation=False))
    def test_header_exact_channel(self):self.mutate('returned',lambda r:r['proof']['header'].update(text='#other'))
    def test_header_not_scrolled_out(self):self.mutate('returned',lambda r:r['proof']['header'].update(scrollTop=40))
    def test_header_initial_scroll_not_fabricated(self):self.mutate('transcript',lambda r:r['proof']['header'].update(originalScrollTop=40))
    def test_header_not_clipped(self):self.mutate('returned',lambda r:r['proof']['header']['rect'].update(top=-5))
    def test_back_refusal_proof_required(self):self.mutate('metadata-draft',lambda r:r['proof'].update(exercise=None))
    def test_back_refusal_cannot_change_model(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['after']['metadata'].update(busy=True))
    def test_back_refusal_cannot_change_document(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['sourceAfter']['documents'][0].update(title='lost'))
    def test_back_refusal_original_title_node_required(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(sameTitle=False))
    def test_back_refusal_original_root_required(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(sameRoot=False))
    def test_back_refusal_original_notice_required(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(sameNotice=False))
    def test_back_refusal_cannot_erase_raw_title(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(value=R['TITLE']))
    def test_back_refusal_notice_exact(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(notice='Other'))
    def test_back_refusal_notice_visible(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native'].update(visible=False))
    def test_back_refusal_notice_unclipped(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['native']['rect'].update(bottom=9999,height=9989))
    def test_back_refusal_restores_native_title(self):self.mutate('metadata-draft',lambda r:r['proof']['exercise']['backRefusal']['restored'].update(value='Other'))
    def test_back_refusal_captured_guidance_required(self):self.mutate('metadata-draft',lambda r:r['geometry'].update(notices=[]))
    def test_no_fake_about_owner(self):self.mutate('opened',lambda r:r['proof']['source'].update(aboutPresent=True))
    def test_no_bot_dm_navigation(self):self.mutate('opened',lambda r:r['proof']['source'].update(ordinaryChannel=False))
    def test_actual_message_action_required(self):
        f,r,c=execution_sample();r['actualMessageAction']=False
        with self.assertRaises(RuntimeError):self.validate(f,r,c)
    def test_workflow_target_exact(self):self.mutate('opened',lambda r:r['model']['entry'].update(workflow='WF_OTHER'))
    def test_execution_target_exact(self):self.mutate('opened',lambda r:r['model']['activity']['detail']['workflow_event'].update(workflow_execution_id='E_OTHER'))
    def test_close_does_not_reopen(self):self.mutate('closed',lambda r:r['model']['activity'].update(detail={'workflow_event':{}}))
    def test_filter_does_not_reopen(self):self.mutate('filtered',lambda r:r['model']['activity'].update(detail={'workflow_event':{}}))
    def test_page_does_not_reopen(self):self.mutate('paged',lambda r:r['model']['activity'].update(detail={'workflow_event':{}}))
    def test_missing_target_no_extra_get(self):self.mutate('missing-target',lambda r:r['proof']['exercise'].update(readCountBefore=5))
    def test_composer_original_node_required(self):self.mutate('opened',lambda r:r['proof']['composer'].update(sameNode=False))
    def test_composer_exact_native_buffer_required(self):self.mutate('returned',lambda r:r['proof']['composer'].update(value=''))
    def test_composer_selection_preserved(self):self.mutate('reopened',lambda r:r['proof']['composer'].update(start=0))
    def test_composer_cannot_remount(self):self.mutate('returned',lambda r:r['proof']['composer'].update(visit='2'))
    def test_composer_hidden_while_panel_active(self):self.mutate('opened',lambda r:r['proof']['composer'].update(visible=True))
    def test_native_buffer_does_not_fake_model_acceptance(self):self.mutate('opened',lambda r:r['proof']['source'].update(composerModel=R['EXECUTION_COMPOSER']))
    def test_malformed_client_action_disabled(self):self.mutate('transcript',lambda r:r['proof']['exercise']['malformed'].update(disabled=False))
    def test_malformed_does_not_fall_into_generic_modal(self):self.mutate('transcript',lambda r:r['proof']['source'].update(appModal=True))
    def test_unsaved_document_retained(self):self.mutate('returned',lambda r:r['proof']['source'].update(documents=[]))
    def test_unsaved_document_identity_retained(self):self.mutate('reopened',lambda r:r['proof']['source']['documents'][0].update(id=9))
    def test_policy_refusal_copy_required(self):self.mutate('policy-denied',lambda r:r['model']['entry'].update(notice='Try later'),'execution-failure')
    def test_read_failure_feedback_must_be_visible(self):self.mutate('read-failed',lambda r:r['geometry']['notices'][0].update(visible=False),'execution-failure')
    def test_policy_failure_feedback_cannot_clip(self):self.mutate('policy-denied',lambda r:r['geometry']['notices'][0]['rect'].update(bottom=9999,height=9989),'execution-failure')
    def test_route_retirement_cancels_actual_read(self):self.mutate('route-retired',lambda r:r['proof']['exercise'].update(delivered=True),'execution-retirement')
    def test_account_retirement_advances_real_epoch(self):self.mutate('account-replaced',lambda r:r['proof']['exercise'].update(currentEpoch=0),'execution-retirement')
    def test_canonical_source_retains_raw_action(self):self.mutate('opened',lambda r:r['proof']['source']['source'][0]['blocks'][0]['elements'][0].update(action_id='generic_action'))

if __name__ == '__main__':
    unittest.main()
