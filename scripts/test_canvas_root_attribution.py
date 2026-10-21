"""Receipt adversaries; fixtures are examples, not native execution evidence."""
import copy
import unittest
import canvas_root_attribution as oracle


def request(index):
    f = {'channel': 'Cfixture', 'inclusive': 'true', 'limit': '28' if index else '20', 'cached_latest_updates': '{}'}
    if index: f['ts'] = oracle.ROOT
    else: f.update(document_stream='latest_reply', ignore_replies='true', include_pin_count='true', no_user_profile='true', include_stories='true', include_free_team_extra_messages='true', include_date_joined='false')
    return {'method': 'POST', 'url': 'https://fixture.slack.com/api/conversations.' + ('replies' if index else 'history'), 'fields': list(map(list, f.items()))}


def sample():
    c = copy.deepcopy
    def member(identifier):
        display, real = {oracle.EDITOR: ('Editor Display', 'Editor Real'), oracle.OTHER: ('Other Display', 'Other Real'), oracle.EMPTY: ('', '')}[identifier]
        return {'id': identifier, 'name': 'fixture_' + identifier, 'profile': {'display_name': display, 'real_name': real}}
    sources = [oracle.EDITOR, oracle.EDITOR, oracle.EDITOR, oracle.EDITOR, oracle.EMPTY,
               oracle.MISSING, oracle.EDITOR, None, oracle.OTHER, oracle.EDITOR, oracle.OTHER]
    responses = []
    for index, editor in enumerate(sources):
        comment = {'is_visible': index != 6, 'thread_id': 'fixture-attribution-root'}
        if editor is not None: comment['most_recent_editor'] = editor
        root = {'ts': oracle.ROOT, 'user': oracle.AUTHOR, 'text': oracle.BODY,
                'subtype': 'document_comment_root', 'document_comment': comment}
        rows = [root]
        if index: rows.append({'ts': '1719800001.000100', 'thread_ts': oracle.ROOT, 'user': 'UATTR0005', 'text': 'Ordinary reply sender remains unchanged.'})
        responses.append({'ok': True, 'has_more': False, 'messages': rows})
    prefs = [('u', 0, ''), ('t', None, None), ('u', 1, ''), ('u', 0, oracle.EDITOR), ('t', None, None),
             ('u', 0, ''), ('t', None, None), ('u', 0, ''), ('t', None, None), ('u', 0, ''), ('u', 0, ''), ('t', None, None)]
    prequests, presponses = [], []
    for kind, real, hidden in prefs:
        prequests.append({'method': 'POST', 'url': 'https://canvas-attribution-fixture.slack.com/api/' + ('users.prefs.get' if kind == 'u' else 'team.prefs.get'), 'fields': [] if kind == 'u' else [['target_team', 'T1']]})
        presponses.append({'ok': True, 'prefs': {'display_real_names_override': real, 'hidden_users': hidden} if kind == 'u' else {'display_real_names': False}})
    known = [('hidden', None, None)] + [('known', oracle.EDITOR, 'Editor Display')] * 5 + [
        ('known', oracle.EDITOR, 'Editor Real'), ('known', oracle.EDITOR, 'Name hidden'), ('known', oracle.EMPTY, ''),
        ('unavailable', None, None), ('hidden', None, None), ('hidden', None, None),
        ('known', oracle.OTHER, 'Other Display'), ('pending', None, None), ('known', oracle.OTHER, 'Other Display')]
    known[4] = ('unavailable', None, None)
    before = {'id': oracle.EDITOR, 'profile': {'title': ''}, 'im_mpim_ids': []}
    after = {'id': oracle.EDITOR, 'profile': {'title': 'Synthetic loaded profile'}, 'im_mpim_ids': ['DATTR0001']}
    hydration = [{'restore': False, 'before': before, 'after': after}, {'restore': True, 'before': after, 'after': before}]
    base = [{'type': 'rich_text', 'elements': [{'type': 'rich_text_section', 'elements': [
        {'type': 'text', 'text': 'Review the ', 'style': {'bold': True}},
        {'type': 'link', 'url': 'https://example.invalid/plan', 'text': 'launch plan', 'style': {'italic': True}},
        {'type': 'text', 'text': ' with '}, {'type': 'user', 'user_id': 'U1'},
        {'type': 'text', 'text': '\nKeep the exact formatting and \U0001f600.'}]}]}]
    typed = c(base)
    typed[0]['elements'][0]['elements'][-1]['text'] += ' Attribution draft \u4e2d\U0001f98a'
    stages = []
    for i, name in enumerate(oracle.STAGES):
        n = oracle.READS[i]
        selected = responses[n-1]['messages']
        kind, editor, label = known[i]
        geo = {'rect': {'x': 40, 'y': 50, 'width': 200, 'height': 35}, 'inView': True, 'hit': True}
        native = None if i == 0 else {'blocks': c(base if i == 1 else typed), 'mount': 'original-mount' if i <= 5 else f'mount-{i}', 'scope': 'original-scope' if i <= 5 else f'scope-{i}', 'generation': 2 if i <= 5 else i, 'composing': False, 'sameOriginal': 2 <= i <= 5}
        rows = [{'ts': oracle.ROOT, 'body': 'This content was deleted.' if i == 10 else oracle.BODY, 'header': label or '', 'senders': []}]
        if i: rows.append({'ts': '1719800001.000100', 'body': 'Ordinary reply sender remains unchanged.', 'header': 'Reply Author 12:00', 'senders': []})
        proof = {'isolation': c(oracle.ISOLATION), 'requests': [request(j) for j in range(n)], 'responses': c(responses[:n]),
                 'policyRequests': c(prequests[:oracle.POLICY_READS[i]]), 'policyResponses': c(presponses[:oracle.POLICY_READS[i]]),
                 'retained': {'ownerCurrent': True, 'loading': False, 'serial': n, 'rows': c(selected), 'root': c(selected[0]) if i else None},
                 'boot': {'accepted': True, 'response': {'self': {'id': 'UATTRSELF'}, 'users': [member(x) for x in (oracle.EDITOR, oracle.OTHER, oracle.EMPTY)]}, 'canonicalIntake': {'operation': 'ordinary-toLowerCase'}},
                 'held': i == 13, 'releases': [{'delivered': False}] if i == 14 else [], 'hydration': c(hydration[:0 if i < 4 else 1 if i == 4 else 2])}
        observed = None
        if kind == 'known' or i == 4:
            observed = {'editor': selected[0]['document_comment']['most_recent_editor'], 'team': 'T1', 'serial': n,
                        'root': c(selected[0]), 'member': oracle.canonical_member(member(selected[0]['document_comment']['most_recent_editor'])),
                        'userPreferences': {'ok': True, 'prefs': {'display_real_names_override': 1 if i == 6 else 0, 'hidden_users': oracle.EDITOR if i == 7 else ''}},
                        'enterprisePreferences': None, 'teamPreferences': {} if i == 6 else {'T1': {'ok': True, 'prefs': {'display_real_names': False}}}}
        proof['retained']['attribution'] = {'current': kind == 'known', 'loading': i == 13, 'observed': observed}
        reply = None if not i else {'runtime': {'blocks': c(native['blocks']), 'capture': 'capture-original' if i == 1 else 'capture-typed', 'saving': False, 'loading': False, 'sending': False, 'error': None},
                    'journal': {'drafts': [{'capture': 'capture-original' if i == 1 else 'capture-typed', 'blocks': c(native['blocks'])}], 'attempt': None, 'history': []}}
        layout = {'ok': True, 'viewport': [668, 380], 'target': c(geo), 'native': native, 'rows': rows,
                  'attribution': {'state': kind, 'id': editor, 'name': label, 'interactive': kind == 'known', 'avatars': []},
                  'hover': {'name': 'Editor Display', 'actions': 0} if i == 3 else None,
                  'profile': {'name': 'Editor Display', 'visible': True, 'close': c(geo)} if i == 4 else None}
        a = layout['attribution']
        a.update(label=None, avatar=None, retry=None)
        if kind == 'known':
            a['label'] = dict(c(geo), text=label, lineRects=[c(geo['rect'])] if label else [],
                              clientWidth=200, scrollWidth=200, fontSize=14)
            a['avatar'] = dict(c(geo), rect={'x': 10, 'y': 50, 'width': 24, 'height': 24})
        if kind == 'unavailable':
            a['retry'] = dict(c(geo), rect={'x': 40, 'y': 50, 'width': 100, 'height': 32},
                              text='Retry editor', clientWidth=100, scrollWidth=100, disabled=False)
        if i == 4:
            layout['profile']['identity'] = {
                'name': dict(c(geo), text='Editor Display'),
                'title': dict(c(geo), text='Synthetic loaded profile')}
        stages.append({'ok': True, 'ordinal': i, 'stage': name, 'noSession': False, 'noTransport': False,
                       'isolation': c(oracle.ISOLATION), 'proof': proof, 'reply': reply, 'layout': layout})
    return {'ok': True, 'requestCount': 11, 'isolation': c(oracle.ISOLATION), 'stages': stages,
            'proof': c(stages[-1]['proof']), 'layout': c(stages[-1]['layout'])}


class AttributionTests(unittest.TestCase):
    def test_complete_sample(self): oracle.validate(sample())

    def reject(self, mutate):
        value = sample()
        mutate(value)
        with self.assertRaises((RuntimeError, ValueError)): oracle.validate(value)

    def test_live_capability(self): self.reject(lambda v: v['stages'][1]['isolation'].update(noHttp=False))
    def test_wrong_original_author(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['rows'][0].update(user=oracle.EDITOR))
    def test_wrong_raw_body(self): self.reject(lambda v: v['stages'][1]['proof']['responses'][-1]['messages'][0].update(text='replacement'))
    def test_editor_fallback(self): self.reject(lambda v: v['stages'][9]['layout']['attribution'].update(name='UATTR0009'))
    def test_empty_name_fallback(self): self.reject(lambda v: v['stages'][8]['layout']['attribution'].update(name='Unknown member'))
    def test_hidden_author_leak(self): self.reject(lambda v: v['stages'][10]['layout']['attribution'].update(name='Original Author'))
    def test_stream_avatar_leak(self): self.reject(lambda v: v['stages'][0]['layout']['attribution'].update(avatars=[{'ready': True, 'src': 'opaque'}]))
    def test_broken_avatar(self): self.reject(lambda v: v['stages'][1]['layout']['attribution'].update(avatars=[{'ready': False, 'src': 'opaque'}]))
    def test_direct_avatar_url(self): self.reject(lambda v: v['stages'][1]['layout']['attribution'].update(avatars=[{'ready': True, 'src': 'https://example.test/face'}]))
    def test_changed_reply_sender(self): self.reject(lambda v: v['stages'][1]['layout']['rows'][1].update(header='Editor Display'))
    def test_scrolled_target(self): self.reject(lambda v: v['stages'][6]['layout']['target']['rect'].update(y=400))
    def test_false_hit(self): self.reject(lambda v: v['stages'][4]['layout']['profile']['close'].update(hit=False))
    def test_replaced_native_node(self): self.reject(lambda v: v['stages'][4]['layout']['native'].update(sameOriginal=False))
    def test_replaced_same_blocks_mount(self): self.reject(lambda v: v['stages'][4]['layout']['native'].update(mount='replacement'))
    def test_journal_block_loss(self): self.reject(lambda v: v['stages'][5]['reply']['journal']['drafts'][0].update(blocks=[]))
    def test_unexpected_send(self): self.reject(lambda v: v['stages'][5]['reply']['journal'].update(attempt={'id': 'write'}))
    def test_foreign_read(self): self.reject(lambda v: v['stages'][2]['proof']['requests'][1]['fields'].append(['text', 'write']))
    def test_wrong_policy_team(self): self.reject(lambda v: v['stages'][2]['proof']['policyRequests'][1].update(fields=[['target_team', 'T2']]))
    def test_old_response_delivered(self): self.reject(lambda v: v['stages'][-1]['proof']['releases'][0].update(delivered=True))
    def test_original_prefix_mutation(self): self.reject(lambda v: v['stages'][3]['proof']['policyResponses'][0]['prefs'].update(hidden_users='other'))
    def test_hydration_other_user(self): self.reject(lambda v: v['stages'][4]['proof']['hydration'][0]['after'].update(id=oracle.OTHER))
    def test_hydration_closes_profile(self): self.reject(lambda v: v['stages'][4]['layout']['profile'].update(visible=False))
    def test_hydration_does_not_keep_stale_attribution_action(self):
        self.reject(lambda v: v['stages'][4]['layout']['attribution'].update(
            state='known', id=oracle.EDITOR, name='Editor Display', interactive=True))
    def test_exact_profile_restore_returns_known_attribution(self):
        self.reject(lambda v: v['stages'][5]['layout']['attribution'].update(
            state='unavailable', id=None, name=None, interactive=False))
    def test_changed_terminal_proof(self): self.reject(lambda v: v['proof'].update(held=True))
    def test_bool_is_not_numeric_override(self):
        def mutate(v):
            for row in v['stages'][6:]: row['proof']['policyResponses'][2]['prefs']['display_real_names_override'] = True
            v['proof'] = copy.deepcopy(v['stages'][-1]['proof'])
        self.reject(mutate)
    def test_rich_mark_loss_in_all_copies(self):
        def mutate(v):
            for row in v['stages'][1:]:
                for blocks in (row['reply']['runtime']['blocks'], row['reply']['journal']['drafts'][0]['blocks'], row['layout']['native']['blocks']):
                    blocks[0]['elements'][0]['elements'][0]['style']['bold'] = False
        self.reject(mutate)
    def test_acquired_wrong_member(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['attribution']['observed'].update(member={'id': oracle.OTHER}))
    def test_acquired_wrong_policy(self): self.reject(lambda v: v['stages'][6]['proof']['retained']['attribution']['observed']['userPreferences']['prefs'].update(display_real_names_override=0))
    def test_pending_borrows_previous(self): self.reject(lambda v: v['stages'][13]['proof']['retained'].update(attribution=copy.deepcopy(v['stages'][12]['proof']['retained']['attribution'])))
    def test_canonical_missing_derived_field(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['attribution']['observed']['member'].pop('_name_lc'))
    def test_canonical_wrong_derived_field(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['attribution']['observed']['member'].update(_display_name_lc='replacement'))
    def test_canonical_original_field_mutation(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['attribution']['observed']['member']['profile'].update(real_name='replacement'))
    def test_canonical_arbitrary_enrichment(self): self.reject(lambda v: v['stages'][1]['proof']['retained']['attribution']['observed']['member'].update(unobserved=True))
    def test_crushed_avatar(self): self.reject(lambda v: v['stages'][1]['layout']['attribution']['avatar']['rect'].update(width=4))
    def test_letter_per_line_name(self): self.reject(lambda v: v['stages'][1]['layout']['attribution']['label']['lineRects'].append({'x': 40, 'y': 75, 'width': 8, 'height': 20}))
    def test_name_clipped(self): self.reject(lambda v: v['stages'][1]['layout']['attribution']['label'].update(scrollWidth=250))
    def test_name_offscreen_despite_header(self): self.reject(lambda v: v['stages'][1]['layout']['attribution']['label'].update(inView=False))
    def test_retry_vertical_wrap(self): self.reject(lambda v: v['stages'][9]['layout']['attribution']['retry']['rect'].update(height=160))
    def test_retry_hidden_by_other_control(self): self.reject(lambda v: v['stages'][9]['layout']['attribution']['retry'].update(hit=False))
    def test_profile_identity_below_scroll(self): self.reject(lambda v: v['stages'][4]['layout']['profile']['identity']['name'].update(inView=False))
    def test_profile_title_occluded(self): self.reject(lambda v: v['stages'][4]['layout']['profile']['identity']['title'].update(hit=False))
    def test_profile_title_substitution(self): self.reject(lambda v: v['stages'][4]['layout']['profile']['identity']['title'].update(text='wrong'))


if __name__ == '__main__': unittest.main()
