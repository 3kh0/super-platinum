"""Portable receipt adversaries, not claims of executed native acceptance."""
import copy
import unittest
import canvas_editor_recovery as oracle
import canvas_root_attribution as prior
import test_canvas_root_attribution as previous


def sample():
    c = copy.deepcopy
    old = previous.sample()
    boot = c(old['stages'][0]['proof']['boot'])
    for member in boot['response']['users']:
        member.update(team_id='T1', real_name=member['profile']['real_name'],
                      deleted=False, is_bot=False, is_app_user=False, updated=101)
    fresh = c(boot['response']['users'][0])
    fresh.update(updated=102, real_name='Recovered Real')
    fresh['profile'].update(display_name=oracle.LABEL, real_name='Recovered Real', title='Synthetic loaded profile')
    raw = {'ok': True, 'results': [fresh], 'can_interact': {prior.EDITOR: True}}
    projected = c(fresh)
    projected['is_stranger'] = False
    canonical = prior.canonical_member(projected)
    source0 = old['stages'][0]['proof']['responses'][0]['messages'][0]
    reply = old['stages'][1]['proof']['responses'][1]['messages'][1]
    responses = []
    for i in range(4):
        root = c(source0)
        root['document_comment']['most_recent_editor'] = prior.OTHER if i == 2 else prior.EDITOR
        responses.append({'ok': True, 'has_more': False, 'messages': [root] + ([c(reply)] if i else [])})
    requests = [previous.request(i) for i in range(4)]
    native0 = c(boot['response']['users'][0])
    native0.update(im_mpim_ids=[], has_more_mpims=True)
    native1 = c(native0)
    native1.update(im_mpim_ids=['DATTR0001'], has_more_mpims=False)
    native1['profile']['title'] = 'Synthetic loaded profile'
    native2 = c(canonical)
    native2.update(im_mpim_ids=['DATTR0001'], has_more_mpims=False)
    native2['profile']['title'] = 'Synthetic reloaded profile'
    hydration = [{'restore': False, 'before': native0, 'after': native1},
                 {'restore': False, 'before': canonical, 'after': native2}]
    # Second synthetic hydration changes only the exact discovery fields.
    hydration[1]['before'] = c(canonical)
    hydration[1]['after'] = c(canonical)
    hydration[1]['after'].update(im_mpim_ids=['DATTR0001'], has_more_mpims=False)
    hydration[1]['after']['profile']['title'] = 'Synthetic reloaded profile'
    native2 = hydration[1]['after']
    acquisition = []
    for i, mode in enumerate(('held', 'offline', 'rate-limited', 'held', 'success')):
        acquisition.extend([
            {'kind': 'discovery', 'request': {'method': 'POST', 'url': 'https://canvas-attribution-fixture.slack.com/api/api.getFlannelHttpUrl',
                                            'body': [['include_external_workspaces', 'true'], ['_x_reason', 'client-needs-valid-flannel-url']]},
             'response': {'ok': True, 'url': 'https://edgeapi.slack.com/cache/T1', 'ttl_seconds': 300}},
            {'kind': 'member', 'serial': i+1, 'mode': mode, 'request': {'method': 'POST', 'url': 'https://edgeapi.slack.com/cache/T1/users/info',
             'body': {'ids': [prior.EDITOR], 'check_interaction': True, 'include_profile_only_users': True}},
             'response': c(raw) if i in (0, 3, 4) else None}])
    policy_requests, policy_responses = [], []
    for i in range(8):
        policy_requests.append({'method': 'POST', 'url': 'https://canvas-attribution-fixture.slack.com/api/' + ('team.prefs.get' if i % 2 else 'users.prefs.get'),
                                'fields': [['target_team', 'T1']] if i % 2 else []})
        policy_responses.append({'ok': True, 'prefs': {'display_real_names': False} if i % 2 else {'display_real_names_override': 0, 'hidden_users': ''}})
    stages = []
    for i, name in enumerate(oracle.STAGES):
        template = 0 if i == 0 else 4 if i in (2, 6) else 13 if i in (4, 11) else 9 if oracle.KINDS[i] == 'unavailable' else 2
        row = c(old['stages'][template])
        row.update(stage=name, ordinal=i, isolation=c(oracle.ISOLATION))
        p = row['proof']
        n, policies = oracle.READS[i], oracle.POLICIES[i]
        p.update(isolation=c(oracle.ISOLATION), boot=c(boot), requests=c(requests[:n]), responses=c(responses[:n]),
                 policyRequests=c(policy_requests[:policies]), policyResponses=c(policy_responses[:policies]), held=False, releases=[], hydration=[])
        retained = p['retained']
        retained.update(serial=n, rows=c(responses[n-1]['messages']), root=c(responses[n-1]['messages'][0]) if i else None)
        o = retained['attribution']
        o.update(current=i in (1, 5, 12, 14), loading=i in (4, 11), observed=None)
        if i in (1, 2, 3, 5, 6, 7, 12, 14):
            editor = prior.OTHER if i == 12 else prior.EDITOR
            member = canonical if i in (5, 6, 7, 14) else prior.canonical_member(next(u for u in boot['response']['users'] if u['id'] == editor))
            o['observed'] = {'editor': editor, 'team': 'T1', 'serial': n, 'root': c(retained['root']), 'member': c(member),
                             'userPreferences': c(policy_responses[0]), 'enterprisePreferences': None, 'teamPreferences': {'T1': c(policy_responses[1])}}
        outcomes = [{'serial': 1, 'outcome': 'response'}] if i >= 5 else []
        if i >= 8: outcomes.append({'serial': 2, 'outcome': 'offline'})
        if i >= 9: outcomes.append({'serial': 3, 'outcome': 'rate-limited'})
        if i >= 12: outcomes.append({'serial': 4, 'outcome': 'cancelled'})
        if i >= 14: outcomes.append({'serial': 5, 'outcome': 'response'})
        releases = [{'serial': 1, 'delivered': True}] if i >= 5 else []
        if i >= 12: releases.append({'serial': 4, 'delivered': False})
        p['recovery'] = {
            'setup': {'path': 'canonical::fixture_seed', 'memberFlagsCurrent': True, 'assignments': {'ok': True, 'assignments': {}}, 'boot': c(boot['response'])},
            'requests': c(acquisition[:oracle.MEMBERS[i]*2]), 'outcomes': outcomes, 'releases': releases,
            'held': 1 if i == 4 else 4 if i == 11 else None, 'hydration': c(hydration[:0 if i < 2 else 1 if i < 6 else 2]),
            'stale': [{'before': {'profile': None, 'navigation': 1}, 'after': {'profile': None, 'navigation': 1}, 'current': False, 'opened': False}] if i >= 5 else [],
            'expiry': {'elapsedMillis': 15001, 'memberRequests': 3, 'automaticRequests': 0} if i >= 10 else None,
            'native': c(native0 if i < 2 else native1 if i <= 4 else canonical if i == 5 or i == 14 else native2)}
        layout = row['layout']
        layout['profile'] = c(old['stages'][4]['layout']['profile']) if i in (2, 6) else None
        if i == 6:
            layout['profile']['name'] = oracle.LABEL
            layout['profile']['identity']['name']['text'] = oracle.LABEL
            layout['profile']['identity']['title']['text'] = 'Synthetic reloaded profile'
        if i:
            row['reply'] = c(old['stages'][2]['reply'])
            layout['native'] = c(old['stages'][2]['layout']['native'])
            layout['rows'] = c(old['stages'][2]['layout']['rows'])
        a = layout['attribution']
        a.update(state=oracle.KINDS[i], id=None, name=None, interactive=False, label=None, avatar=None, retry=None, avatars=[])
        if i in (1, 5, 12, 14):
            a.update(c(old['stages'][2]['layout']['attribution']))
            label = 'Editor Display' if i == 1 else 'Other Display' if i == 12 else oracle.LABEL
            a.update(name=label, id=prior.OTHER if i == 12 else prior.EDITOR)
            a['label']['text'] = label
        elif i in (2, 3, 6, 7, 8, 10, 13): a['retry'] = c(old['stages'][9]['layout']['attribution']['retry'])
        stages.append(row)
    return {'ok': True, 'requestCount': 4, 'memberRequestCount': 5, 'isolation': c(oracle.ISOLATION),
            'stages': stages, 'proof': c(stages[-1]['proof']), 'layout': c(stages[-1]['layout'])}


class RecoveryTests(unittest.TestCase):
    def test_complete_sample(self): oracle.validate(sample())

    def reject(self, mutate):
        value = sample()
        mutate(value)
        with self.assertRaises((RuntimeError, ValueError)): oracle.validate(value)

    def test_no_http_false(self): self.reject(lambda v: v['stages'][1]['isolation'].update(noHttp=False))
    def test_passive_forced_request(self): self.reject(lambda v: v['stages'][3]['proof']['recovery']['requests'].extend(copy.deepcopy(v['stages'][4]['proof']['recovery']['requests'])))
    def test_duplicate_retry_dispatch(self): self.reject(lambda v: v['stages'][4]['proof']['recovery']['requests'].extend(copy.deepcopy(v['stages'][4]['proof']['recovery']['requests'])))
    def test_wrong_member(self): self.reject(lambda v: v['stages'][4]['proof']['recovery']['requests'][1]['request']['body'].update(ids=[prior.OTHER]))
    def test_foreign_endpoint(self): self.reject(lambda v: v['stages'][4]['proof']['recovery']['requests'][1]['request'].update(url='https://edgeapi.slack.com/cache/T2/users/info'))
    def test_wrong_original_response(self): self.reject(lambda v: v['stages'][5]['proof']['recovery']['requests'][1]['response']['results'][0].update(id=prior.OTHER))
    def test_no_observed_flags(self): self.reject(lambda v: v['stages'][1]['proof']['recovery']['setup'].update(memberFlagsCurrent=False))
    def test_hydration_silently_restored(self): self.reject(lambda v: v['stages'][3]['proof']['recovery'].update(native=copy.deepcopy(v['stages'][3]['proof']['recovery']['hydration'][0]['before'])))
    def test_second_hydration_restored(self): self.reject(lambda v: v['stages'][8]['proof']['recovery'].update(native=copy.deepcopy(v['stages'][8]['proof']['recovery']['hydration'][1]['before'])))
    def test_hydration_changes_identity(self): self.reject(lambda v: v['stages'][2]['proof']['recovery']['hydration'][0]['after'].update(id=prior.OTHER))
    def test_old_callback_reauthorized(self): self.reject(lambda v: v['stages'][5]['proof']['recovery']['stale'][0].update(current=True))
    def test_old_callback_opens(self): self.reject(lambda v: v['stages'][5]['proof']['recovery']['stale'][0]['after'].update(profile=prior.EDITOR))
    def test_late_delivery(self): self.reject(lambda v: v['stages'][12]['proof']['recovery']['releases'][1].update(delivered=True))
    def test_cancelled_became_success(self): self.reject(lambda v: v['stages'][12]['proof']['recovery']['outcomes'][3].update(outcome='response'))
    def test_offline_invented_ack(self): self.reject(lambda v: v['stages'][8]['proof']['recovery']['requests'][3].update(response={'ok': True}))
    def test_cooldown_expired_early(self): self.reject(lambda v: v['stages'][10]['proof']['recovery']['expiry'].update(elapsedMillis=1000))
    def test_cooldown_auto_dispatch(self): self.reject(lambda v: v['stages'][10]['proof']['recovery']['expiry'].update(memberRequests=4))
    def test_cooldown_retry_available(self): self.reject(lambda v: v['stages'][9]['layout']['attribution'].update(retry=copy.deepcopy(v['stages'][8]['layout']['attribution']['retry'])))
    def test_new_reply_node(self): self.reject(lambda v: v['stages'][5]['layout']['native'].update(mount='new-holder'))
    def test_changed_journal(self): self.reject(lambda v: v['stages'][7]['reply']['journal']['drafts'][0].update(blocks=[]))
    def test_wrong_canonical_name(self): self.reject(lambda v: v['stages'][5]['proof']['retained']['attribution']['observed']['member']['profile'].update(display_name='guessed'))
    def test_stale_policy(self): self.reject(lambda v: v['stages'][5]['proof']['retained']['attribution']['observed']['userPreferences']['prefs'].update(hidden_users=prior.EDITOR))
    def test_profile_wrapper_collapsed(self): self.reject(lambda v: v['stages'][6]['layout']['profile'].update(visible=False))
    def test_profile_title_offscreen(self): self.reject(lambda v: v['stages'][6]['layout']['profile']['identity']['title'].update(inView=False))
    def test_retry_wraps(self): self.reject(lambda v: v['stages'][8]['layout']['attribution']['retry']['rect'].update(height=160))
    def test_terminal_substitution(self): self.reject(lambda v: v['proof']['recovery'].update(stale=[]))


if __name__ == '__main__': unittest.main()
