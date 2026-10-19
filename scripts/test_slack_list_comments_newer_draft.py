"""Adversarial newer-draft receipts. Pure tests; no app, disk journal or Slack IO."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_newer_draft.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_native.py')))
DIR = Path('/private/newer-draft')


def sample(narrow=False):
    fixture, base = BASE['sample'](narrow)
    initial = copy.deepcopy(base['stages'][2]['model'])
    initial['account'] = '8624eb74-23f4-4bee-9872-e37f392485c0'
    box = dict(left=10, top=10, right=210, bottom=35, width=200, height=25)
    scope = dict(workspace='T1', user='U1', list=V['READ']['LIST'], record=V['READ']['KNOWN'],
                 created=None, creator=None, root=V['READ']['ROOT_TS'])
    token = dict(type='user', user_id='U_FIXTURE_OPAQUE', opaque=dict(retain=[1,2]))
    link = dict(type='link', url='https://example.test/reference', text='Reference', opaque=dict(source='fixture'))
    seed_blocks = [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Existing context '), token, dict(type='text',text=' '), link, dict(type='text',text='\n')])])]
    a = copy.deepcopy(seed_blocks)
    a[0]['elements'][0]['elements'].insert(4,dict(type='text', text=V['A_TEXT'].split('\n')[0], style=dict(bold=True)))
    a[0]['elements'].append(dict(type='rich_text_section',elements=[dict(type='text',text=V['A_TEXT'].split('\n')[1])]))
    b = copy.deepcopy(a)
    b[0]['elements'].extend(dict(type='rich_text_section',elements=[dict(type='text',text=text)]) for text in V['B_TEXT'][1:].split('\n'))
    seed = dict(scope=scope, blocks=seed_blocks, revision=1, reopened=True)
    capture_a = '11111111-2222-4333-8444-555555555555'
    capture_b = '11111111-2222-4333-8444-666666666666'
    capture_empty = '11111111-2222-4333-8444-777777777777'
    posts = []
    for i, blocks in enumerate((a,b)):
        posts.append(dict(method='chat.postMessage', args=dict(channel='CCOMMENT01',thread_ts=scope['root'],
            ts=f'178152500{i}.000001',type='message',reply_broadcast='false',include_channel_perm_error='true',
            client_msg_id=f'00000000-0000-4000-8000-{i+1:012}',blocks=json.dumps(blocks,ensure_ascii=False),_x_reason='webapp_message_send')))
    record = dict(version=1, workspace='T1',user='U1',target=dict(document=initial['document'],record=initial['records'][scope['record']],channel='CCOMMENT01',root=scope['root']),
        root=dict(type='message',ts=scope['root']),text='Original body',blocks=a,client_ts=posts[0]['args']['ts'],client_msg_id=posts[0]['args']['client_msg_id'])
    attempt = dict(record=record,capture=capture_a,raw=None)
    messages = [dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],user='U1',blocks=blocks,
        client_msg_id=posts[i]['args']['client_msg_id'],ts=f'178152590{i}.000001') for i,blocks in enumerate((a,b))]
    policy = dict(request=dict(method='client.userBoot',args={'include_min_version_bump_check':'1'}),
        boot=dict(ok=True,team=dict(id='T1'),self=dict(id='U1',team_id='T1')),channel=dict(id='CCOMMENT01'),threadReadOnly=False)
    preflight = dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})
    rows=[]
    for i,name in enumerate(V['STAGES']):
        model=copy.deepcopy(initial)
        count=2 if i==4 else 1
        reads=[]
        for _ in range((2,2,3,4,5)[i]):
            for req in base['stages'][2]['model']['ledger']['requests'][:4]:
                req=copy.deepcopy(req);req['serial']=len(reads)+1;reads.append(req)
        model['ledger'].update(requests=reads,completed=len(reads),held=False)
        model['comments'].update(generation=11 if i<2 else 12,loading=False,error=None,active=True)
        if i>=3:
            model['comments']['page']['messages'].append(copy.deepcopy(messages[0]))
            model['comments']['visibleTs'].append(messages[0]['ts'])
        blocks=a if i==0 else b if i<4 else []
        capture=capture_a if i==0 else capture_b if i<4 else capture_empty
        pending=copy.deepcopy(attempt) if i<3 else None
        revision=(4,5,5,6,8)[i]
        generation=20 if i<2 else 30 if i<4 else 40
        draft=dict(id='original' if i<2 else 'reopened',scope=copy.deepcopy(scope),blocks=copy.deepcopy(blocks),capture=capture,
            sequence=i+1,generation=generation,busy=False,saving=False,loading=False,captureFailed=False,terminalPending=False,native=True,
            savedRevision=revision,attempt=copy.deepcopy(pending),error=None,status='Delivery unconfirmed' if i<3 else 'Confirmed')
        model['draftStore']=dict(active=f"{draft['id']}:{model['comments']['generation']}:{generation}",drafts=[draft])
        model['composeLedger']=dict(prepared=copy.deepcopy(posts[:count]),posts=copy.deepcopy(posts[:count]),messages=copy.deepcopy(messages[:count]),
            preflights=[copy.deepcopy(preflight)]*count,policies=[copy.deepcopy(policy)]*count,published=i>=3,held=False,heldPost=False)
        editor=dict(visible=True,editable=True,rect=copy.deepcopy(box),blocks=copy.deepcopy(blocks),text=V['visible_text'](blocks) if i<4 else '',bold=i<4,generation=str(generation))
        controls=[dict(id=k,disabled=(i!=3) if k=='slack-list-comment-send' else False,visible=True,hit=True,rect=copy.deepcopy(box)) for k in ('slack-list-comments-back','slack-list-comments-refresh','slack-list-comment-send')]
        g=dict(ok=True,noOverflow=True,viewport=[668,380] if narrow else [1280,780],editor=editor,controls=controls,dialog=True,comments=True,compose=True,
            notices=['Delivery unconfirmed. Refresh comments to check.'] if i<3 else ['Comment confirmed.'],rows=copy.deepcopy(messages[:1]) if i>=3 else [])
        journal=dict(scope=copy.deepcopy(scope),revision=revision,capture=capture,blocks=copy.deepcopy(blocks),attempt=copy.deepcopy(pending))
        proof=dict(originalA=copy.deepcopy(a),newerB=None if i==0 else copy.deepcopy(b),seed=copy.deepcopy(seed),journal=journal,runtimeForgotten=i==2)
        if i==0:proof['release']=dict(delivered=True,cancelledBeforeReply=False)
        rows.append(dict(stage=name,model=model,geometry=g,proof=proof,capture=str(DIR/f'{fixture}-{name}.png')))
    return fixture,dict(ok=True,case='newer-draft',fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],initialRecords=base['initialRecords'],stages=rows,final=copy.deepcopy(rows[-1]['model']))


class NewerDraftTests(unittest.TestCase):
    def check(self,f,r,**kwargs): return V['validate'](f,r,DIR,png=False,**kwargs)
    def reject(self,change,stage=1):
        f,r=sample();change(r['stages'][stage])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)): self.check(f,r)
    def test_both_widths(self):
        for narrow in (False,True): self.check(*sample(narrow))
    def test_unicode_oracle_codepoints_are_independent(self):
        self.assertEqual([ord(c) for c in V['A_TEXT'] if ord(c)>127],[0x4E2D,0x1F642])
        self.assertEqual([ord(c) for c in V['B_TEXT'] if ord(c)>127],[0x65E5,0x1F98A])
        for text in (V['A_TEXT'],V['B_TEXT']):self.assertNotIn('?',text)
        f,r=sample()
        original=''.join(c for c,_ in V['text_units'](r['stages'][0]['proof']['originalA']))
        newer=''.join(c for c,_ in V['text_units'](r['stages'][1]['proof']['newerB']))
        self.assertEqual([ord(c) for c in original if ord(c)>127],[0x4E2D,0x1F642])
        self.assertEqual([ord(c) for c in newer if ord(c)>127],[0x4E2D,0x1F642,0x65E5,0x1F98A])
    def test_wrong_case_or_incomplete_sequence(self):
        for mutation in (lambda r:r.update(case='compose'),lambda r:r['stages'].pop(),lambda r:r['stages'].reverse()):
            f,r=sample();mutation(r)
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_readiness_owner(self):
        for key,value in [('pid',False),('nonce','foreign'),('fixture','slack-list-narrow')]:
            f,r=sample();r['readiness'][key]=value
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_no_live_state(self):
        for key,value in [('noSession',False),('noTransport',False),('writes',1),('networkCalls',1),('account','foreign'),('epoch',9)]:
            self.reject(lambda s:s['model'].update({key:value}))
    def test_canonical_list_and_records_are_exact(self):
        for key in ('records','document','unrelated','initialRecords','initialDocument'):
            self.reject(lambda s:s['model'].update({key:{}}))
        self.reject(lambda s:s['model']['comments'].update(root='other'))
    def test_native_b_is_real_new_content(self):
        self.reject(lambda s:s['geometry']['editor'].update(blocks=s['proof']['originalA']))
        self.reject(lambda s:s['proof'].update(newerB=s['proof']['originalA']))
        self.reject(lambda s:s['geometry']['editor'].update(editable=False))
    def test_opaque_and_style_tampering_even_if_model_and_disk_agree(self):
        for style in (False,True):
            f,r=sample()
            for row in r['stages']:
                b=row['proof']['newerB']
                if b:
                    node=b[0]['elements'][0]['elements'][4 if style else 1]
                    if style:node['style']['bold']=False
                    else:node['opaque']['retain']=[2,1]
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_disk_disagreement(self):
        for key,value in [('blocks',[]),('capture','11111111-2222-4333-8444-999999999999'),('attempt',None),('revision',999)]:
            self.reject(lambda s:s['proof']['journal'].update({key:value}))
    def test_b_cannot_replace_attempt_a(self):
        self.reject(lambda s:s['model']['draftStore']['drafts'][0]['attempt']['record'].update(blocks=s['proof']['newerB']))
        self.reject(lambda s:s['proof']['journal']['attempt'].update(capture=s['proof']['journal']['capture']))
    def test_reopen_must_be_real_and_exact(self):
        self.reject(lambda s:s['proof'].update(runtimeForgotten=False),2)
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(id='original'),2)
        self.reject(lambda s:s['proof']['journal'].update(revision=6),2)
    def test_confirmed_a_cannot_clear_b_or_rotate_its_mount(self):
        self.reject(lambda s:s['geometry']['editor'].update(blocks=[]),3)
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(generation=999),3)
        self.reject(lambda s:s['proof']['journal'].update(capture='11111111-2222-4333-8444-999999999999'),3)
    def test_confirmation_needs_actual_page_and_visible_message(self):
        self.reject(lambda s:s['geometry'].update(rows=[]),3)
        self.reject(lambda s:s['model']['comments']['page'].update(messages=[]),3)
        self.reject(lambda s:s['model']['composeLedger'].update(published=False),3)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(user='foreign'),3)
    def test_no_second_post_until_explicit_send(self):
        for i in range(4):self.reject(lambda s:s['model']['composeLedger']['posts'].append(copy.deepcopy(s['model']['composeLedger']['posts'][0])),i)
    def test_b_send_has_new_uuid_and_exact_b(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'][1]['args'].update(client_msg_id=s['model']['composeLedger']['posts'][0]['args']['client_msg_id']),4)
        self.reject(lambda s:s['model']['composeLedger']['posts'][1]['args'].update(blocks=json.dumps(s['proof']['originalA'])),4)
        self.reject(lambda s:s['geometry']['editor'].update(blocks=s['proof']['newerB'],text='still here'),4)
    def test_send_admission_and_uncertainty_notice(self):
        for i in range(5):self.reject(lambda s:s['geometry']['controls'][2].update(disabled=not s['geometry']['controls'][2]['disabled']),i)
        for i in range(3):self.reject(lambda s:s['geometry'].update(notices=[]),i)
    def test_observed_policy_and_read_scope(self):
        self.reject(lambda s:s['model']['composeLedger'].update(policies=[]))
        self.reject(lambda s:s['model']['composeLedger']['policies'][0]['boot']['self'].update(id='other'))
        self.reject(lambda s:s['model']['ledger']['requests'][1]['args'].update(id='R_LIST_001'))
    def test_geometry_and_capture_path(self):
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry']['editor']['rect'].update(right=9999))
        self.reject(lambda s:s.update(capture='/foreign.png'))
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),800]))
    def test_exact_positive_a_release(self):
        self.reject(lambda s:s['proof']['release'].update(delivered=False),0)
    def test_png_check_receives_all_owned_paths_and_dimensions(self):
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda path,dimensions:seen.append((path,dimensions))}):
            V['validate'](f,r,DIR,png=True,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{s}.png',[1280,780]) for s in V['STAGES']])
    def test_consistent_b_capture_cannot_be_original_attempt_capture(self):
        f,r=sample();original=r['stages'][0]['proof']['journal']['capture']
        for row in r['stages'][1:4]:
            row['proof']['journal']['capture']=original
            row['model']['draftStore']['drafts'][0]['capture']=original
        with self.assertRaisesRegex(RuntimeError,'locked original capture'):self.check(f,r)
    def test_consistent_rebound_attempt_is_rejected(self):
        f,r=sample()
        for row in r['stages'][1:3]:
            for attempt in (row['proof']['journal']['attempt'],row['model']['draftStore']['drafts'][0]['attempt']):
                attempt['record']['blocks']=copy.deepcopy(row['proof']['newerB'])
        with self.assertRaisesRegex(RuntimeError,'rebound'):self.check(f,r)
    def test_consistent_b_uuid_replay_is_rejected(self):
        f,r=sample();row=r['stages'][4];ledger=row['model']['composeLedger'];first=ledger['posts'][0]['args']['client_msg_id']
        ledger['posts'][1]['args']['client_msg_id']=first
        ledger['prepared'][1]['args']['client_msg_id']=first
        ledger['messages'][1]['client_msg_id']=first
        r['final']=copy.deepcopy(row['model'])
        with self.assertRaisesRegex(RuntimeError,'reused UUID'):self.check(f,r)
    def test_readback_wrong_blocks_with_correct_uuid_is_rejected(self):
        f,r=sample();row=r['stages'][3]
        next(m for m in row['model']['comments']['page']['messages'] if 'client_msg_id' in m)['blocks']=copy.deepcopy(row['proof']['newerB'])
        with self.assertRaisesRegex(RuntimeError,'send identity'):self.check(f,r)
    def test_semantically_equal_split_styled_runs_remain_supported(self):
        f,r=sample();b=copy.deepcopy(r['stages'][1]['proof']['newerB']);nodes=b[0]['elements'][0]['elements'];styled=nodes[4]
        nodes[4:5]=[dict(type='text',text=styled['text'][:4],style=copy.deepcopy(styled['style'])),dict(type='text',text=styled['text'][4:],style=copy.deepcopy(styled['style']))]
        for i,row in enumerate(r['stages'][1:],1):
            row['proof']['newerB']=copy.deepcopy(b)
            if i<4:
                row['proof']['journal']['blocks']=copy.deepcopy(b);row['model']['draftStore']['drafts'][0]['blocks']=copy.deepcopy(b);row['geometry']['editor']['blocks']=copy.deepcopy(b)
            else:
                row['model']['composeLedger']['posts'][1]['args']['blocks']=json.dumps(b,ensure_ascii=False)
                row['model']['composeLedger']['prepared'][1]['args']['blocks']=json.dumps(b,ensure_ascii=False)
                row['model']['composeLedger']['messages'][1]['blocks']=copy.deepcopy(b)
        r['final']=copy.deepcopy(r['stages'][-1]['model']);self.check(f,r)
    def test_final_custody(self):
        f,r=sample();r['final']['epoch']=999
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_actual_native_a_receipt(self):
        path=Path(__file__).resolve().parent.parent/'tmp/list-comments-compose-recovery-20261018/native-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained native-a evidence is not installed')
        outer=json.loads(path.read_text(encoding='utf-8'))
        receipt=outer['terminalReceipts'][0]
        V['validate'](receipt['fixture'],receipt,outer['readyDirectory'],dimensions=outer['expectedPngDimensions'],png=False)
    def test_paragraph_structure_cannot_flatten_or_reorder(self):
        for change in ('flatten','swap','linebreak'):
            f,r=sample()
            for row in r['stages'][1:]:
                sections=row['proof']['newerB'][0]['elements']
                if change=='flatten':sections[2]['elements']+=sections.pop(3)['elements']
                elif change=='swap':sections[2],sections[3]=sections[3],sections[2]
                else:sections[2]['elements'][0]['text']+='\n'
            with self.assertRaisesRegex(RuntimeError,'paragraph|prefix'):self.check(f,r)
    def test_visible_paragraphs_cannot_disagree_with_exact_source(self):
        for text in ('wrong text','Existing context flattened',''):
            self.reject(lambda s:s['geometry']['editor'].update(text=text))
    def test_paragraph_boundary_cannot_move_with_same_flat_text(self):
        f,r=sample()
        for row in r['stages'][1:]:
            sections=row['proof']['newerB'][0]['elements']
            text=sections[2]['elements'][0]['text']
            sections[2]['elements'][0]['text']=text[:-1]
            sections[3]['elements'][0]['text']=text[-1]+sections[3]['elements'][0]['text']
        with self.assertRaisesRegex(RuntimeError,'paragraph'):self.check(f,r)

if __name__=='__main__':unittest.main()
