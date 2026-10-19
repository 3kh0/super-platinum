"""Adversarial original edit/read/ACK receipts, with no native or live IO."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_edits.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_ack_socket.py')))
DIR=Path('/private/list-comments-edits')
FIRST,ROOT,SENT,CH='1781524801.000001','1781524800.000001','1781525001.000001','CCOMMENT01'


def blocks(text,style):
    return [dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[dict(type='text',text=text,style=style)])])]


def frame(ts,text,**kwargs):
    return dict(type='message',subtype='message_changed',channel=CH,
                message=dict(dict(ts=ts,thread_ts=ROOT,user='U1',text=text),**kwargs))


def output(prior,text,rich=None,edited=None,metadata=None,reactions=None):
    value=copy.deepcopy(prior)
    value.update(type='message',channel=CH,text=text,blocks=[] if rich is None else rich,edited=edited,
        files=[],attachments=[],subtype=None,latest_reply=None,reply_users=[],reply_users_count=None,
        reply_count=0,no_display=False,_hidden_reply=True,slackbot_feels=None)
    if metadata is not None:value['metadata']=metadata
    if reactions is not None:value['reactions']=reactions
    return value


def sample(case='edits',narrow=False):
    fixture,base=BASE['sample']('ack-socket-remove',narrow)
    original=base['stages'][0];page=original['model']['comments']['page']
    # Older receipt factories only needed text/reactions. This case observes the
    # actual typed body, so retain the full independently specified fixture row.
    page['messages'][1].update(user='U1',blocks=[dict(type='rich_text',elements=[
        dict(type='rich_text_section',elements=[dict(type='text',text='First page comment.',style=dict(bold=True))]),
        dict(type='rich_text_section',elements=[dict(type='text',text='Review ready \u4e2d\U0001f642.')])])])
    typed=V['I']['typed'];first=typed(page['messages'][1]);root=typed(page['messages'][0])
    rich=output(first,'Edited rich reply.',blocks('Edited rich reply.',{'bold':True}),
        dict(user='U1',ts='1781525100.000001'),dict(event_type='list-edit',event_payload=dict(keep=1,old=True)))
    plain=output(rich,'Edited plain reply.',edited=dict(ts='1781525102.000001'),metadata=dict(event_type='plain',event_payload=dict(only=2)))
    cleared=output(plain,'Cleared rich reply.',metadata=dict(event_type='cleared',event_payload=dict(only=3)),reactions=[])
    updated=output(cleared,'Held edit reply.',metadata=copy.deepcopy(cleared['metadata']))
    raw_ack=base['stages'][1]['proof']['originalMessage'];socket=typed(raw_ack)
    edited=output(socket,'Socket edited reply.',blocks('Socket edited reply.',{'bold':True}),dict(ts='1781525200.000001'))
    reacted=copy.deepcopy(edited);reacted['reactions']=[]
    tail=output(reacted,'Accepted tail edited.',blocks('Accepted tail edited.',{'italic':True}),dict(ts='1781525200.000001'))
    canonical=dict(type='message',channel=CH,ts=FIRST if case=='edits' else SENT,thread_ts=ROOT,user='U1',
        text='Canonical readback reply.' if case=='edits' else 'Canonical sent reply.')
    canonical['blocks']=blocks(canonical['text'],{'italic':True})
    canonical['reactions']=[dict(name='wave',count=1,users=['U1'])] if case=='edits' else []
    if case=='edits':canonical['edited']=dict(ts='1781525109.000001')
    else:canonical.update(client_msg_id=raw_ack['client_msg_id'],user=raw_ack['user'])
    readback=copy.deepcopy(page)
    if case=='edits':readback['messages'][1]=copy.deepcopy(canonical)
    else:readback['messages'].append(copy.deepcopy(canonical))
    readback.update(has_more=False,response_metadata=dict(next_cursor=''))
    frames=[frame(FIRST,'Edited rich reply.',blocks=blocks('Edited rich reply.',{'bold':True}),edited=dict(user='U1',ts='1781525100.000001'),
                  metadata=dict(event_type='list-edit',event_payload=dict(keep=1,old=True)),opaque=dict(ignored=True)),
            frame(FIRST,'Edited plain reply.',edited=dict(ts='1781525102.000001'),metadata=dict(event_type='plain',event_payload=dict(only=2))),
            frame(FIRST,'Cleared rich reply.',blocks=[],reactions=[],edited=None,metadata=dict(event_type='cleared',event_payload=dict(only=3)))]
    rows=[];serial=1
    for i,name in enumerate(V['STAGES'][case]):
        source=original if case=='edits' else base['stages'][(0,1,2,2,3,4,5,6,6,6)[i]]
        row=copy.deepcopy(source);row.pop('ackProbe',None);row.update(stage=name,capture=str(DIR/f'{fixture}-{name}.png'))
        model,g,p=row['model'],row['geometry'],row['proof'];c=model['comments']
        final=i==len(V['STAGES'][case])-1
        if i in ((5,7,10) if case=='edits' else (1,9)):serial+=3
        c.update(read=serial,hasMore=not final,page=copy.deepcopy(readback if final else page),
            loading=case=='edits' and i in (5,6),error=V['NOTICE'] if case=='edits' and i in (7,8,9) else None,editCustody=[])
        p.update(heldResponse=copy.deepcopy(page) if case=='edits' and i>=5 else None,serverResponse=copy.deepcopy(readback) if final else None)
        if case=='edits':
            p['release']=dict(delivered=False,cancelledBeforeReply=True) if i>=8 else None
            current=(first,rich,plain,cleared,cleared,cleared,cleared,updated,updated,updated,typed(canonical))[i]
            messages=[root,current];live=[];accepted=[];visible=[FIRST];mark=[]
            reads=4 if i<5 else 8 if i<10 else 12
            completed=reads-int(i in (5,6));cancelled=int(i>=7);held=i in (5,6,7)
            body_text=current['text'];bold=i in (0,1);italic=final
        else:
            current=socket if i<3 else edited if i==3 else reacted if i<8 else tail if i==8 else typed(canonical)
            messages=[root,first]+([typed(canonical)] if final else [])
            live=[current] if 2<=i<=4 else [];accepted=[current] if 5<=i<=8 else [];visible=[FIRST]+([SENT] if i>=2 else [])
            mark=[SENT] if i==4 else [];c['editCustody']=[SENT] if i in (3,4) else []
            reads=4 if i==0 else 12 if final else 8;completed=reads;cancelled=0;held=False
            body_text=first['text'] if i<2 else current['text'];bold=i<=7;italic=i>=8
        c.update(pageMessages=copy.deepcopy(messages),live=copy.deepcopy(live),liveTs=[m['ts'] for m in live],
            accepted=copy.deepcopy(accepted),acceptedTs=[m['ts'] for m in accepted],liveReactions=mark,visibleTs=visible)
        lookup={m['ts']:m for m in messages+live+accepted}
        projection=[dict(ts=ts,reactions=[dict(name=p['name'],count=p['count']) for p in lookup[ts]['reactions']]) for ts in visible]
        c['reactions']=copy.deepcopy(projection)
        model['ledger'].update(requests=[dict(copy.deepcopy(r),serial=4*batch+n+1) for batch in range(reads//4)
            for n,r in enumerate(original['model']['ledger']['requests'])],completed=completed,cancelled=cancelled,held=held,replies=reads//4)
        g['rows']=[dict(copy.deepcopy(pr),text=lookup[pr['ts']]['text']) for pr in projection]
        g['notices']=[V['NOTICE']] if c['error'] else ['Loading comments\u2026'] if c['loading'] else ['Comment sent.'] if case=='edit-ack' and i==5 else []
        g['active']='' if case=='edits' and i>=5 or case=='edit-ack' and (1<=i<=4 or final) else 'slack-list-comment-draft'
        if final:g['controls']=[c for c in g['controls'] if c['id']!='slack-list-comments-next']
        for control in g['controls']:
            control['disabled']=(case=='edit-ack' and 1<=i<=5) if control['id']=='slack-list-comment-send' else c['loading'] if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else False
        target=FIRST if case=='edits' or i<2 else SENT
        box=copy.deepcopy(g['editor']['rect']);count=lookup[target]['reactions'][0]['count'] if lookup[target]['reactions'] else 0
        g['editBody']=dict(ts=target,text=body_text,bold=bold,italic=italic,visible=True,rect=box,
            pills=[dict(name='wave',visible=True,rect=copy.deepcopy(box),tag='SPAN')] if count else [],tools=0)
        rows.append(row)
    if case=='edits':
        specs=[(name,'T1',7,fr,ch,before,after) for name,fr,ch,before,after in
            [('rich',frames[0],True,0,1),('plain',frames[1],True,1,2),('cleared',frames[2],True,2,3),
             ('duplicate',frames[2],False,3,3),('held-duplicate',frames[2],False,5,5)]]
        for name,team,gen,outer,ts,rt,nested,kind in (
            ('wrong-team','T_OTHER',7,CH,FIRST,ROOT,None,'message_changed'),('stale-generation','T1',6,CH,FIRST,ROOT,None,'message_changed'),
            ('foreign-channel','T1',7,'COTHER',FIRST,ROOT,None,'message_changed'),('conflicting-channel','T1',7,CH,FIRST,ROOT,'COTHER','message_changed'),
            ('wrong-root','T1',7,CH,FIRST,'1781524998.000001',None,'message_changed'),('unknown-reply','T1',7,CH,'1999999999.000001',ROOT,None,'message_changed'),
            ('root','T1',7,CH,ROOT,ROOT,None,'message_changed'),('replied','T1',7,CH,FIRST,ROOT,None,'message_replied')):
            fr=frame(ts,'Ignored edit.',thread_ts=rt);fr.update(channel=outer,subtype=kind)
            if nested:fr['message']['channel']=nested
            specs.append((name,team,gen,fr,False,5,5))
        specs.extend([('updated-held','T1',7,frame(FIRST,'Held edit reply.'),True,6,7),
                      ('unsupported','T1',7,frame(FIRST,'Unsupported file body.',files=[dict(id='FUNSUPPORTED1',mode='hosted')]),False,8,8)])
    else:
        specs=[('socket','T1',7,raw_ack,True,1,2),
            ('edited','T1',7,frame(SENT,'Socket edited reply.',user=raw_ack['user'],blocks=blocks('Socket edited reply.',{'bold':True}),edited=dict(ts='1781525200.000001')),True,2,3),
            ('reacted','T1',7,dict(type='reaction_removed',user='U_RT_REACTOR',reaction='wave',item=dict(type='message',channel=CH,ts=SENT)),True,3,4),
            ('echo','T1',7,raw_ack,False,6,6),('tail-edited','T1',7,frame(SENT,'Accepted tail edited.',user=raw_ack['user'],blocks=blocks('Accepted tail edited.',{'italic':True})),True,7,8)]
    events=[]
    keys=('read','loading','error','page','pageMessages','live','accepted','editCustody','visibleTs','liveTs','acceptedTs','liveReactions','deleted','reactions','reveal')
    for name,team,gen,fr,change,before,after in specs:
        original_update=None
        if fr.get('subtype') in ('message_changed','message_replied'):
            original_update=dict(kind='Changed' if fr['subtype']=='message_changed' else 'Replied',outerChannel=fr['channel'],
                typedChannel=fr['message'].get('channel',fr['channel']),typedTs=fr['message']['ts'],raw=copy.deepcopy(fr['message']))
        events.append(dict(stage=name,team=team,generation=gen,frame=copy.deepcopy(fr),parsed=True,witness=original_update,changed=change,
            before={k:copy.deepcopy(rows[before]['model']['comments'][k]) for k in keys},
            after={k:copy.deepcopy(rows[after]['model']['comments'][k]) for k in keys}))
    return fixture,dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],
                       initialRecords=base['initialRecords'],stages=rows,editEvents=events,final=copy.deepcopy(rows[-1]['model']))


class EditsTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,index=2,case='edits'):
        f,r=sample(case);change(r['stages'][index])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)

    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))

    def test_preserved_native_edits_a_projected_channel(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-edits-20261018/edits-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Preserved offline native receipt is local evidence')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['terminalReceipts'][0]
        self.assertFalse(outer['ok']);self.assertEqual(len(receipt['stages']),11)
        self.assertIn('Typed edited message fields/body/scalars',outer['error'])
        rows=receipt['stages']
        self.assertIsNone(rows[0]['model']['comments']['pageMessages'][1]['channel'])
        for row in rows[1:10]:
            self.assertEqual(row['model']['comments']['pageMessages'][1]['channel'],CH)
            self.assertNotIn('channel',row['model']['comments']['page']['messages'][1])
        self.assertNotIn('channel',receipt['editEvents'][0]['witness']['raw'])
        V['validate']('slack-list',receipt,Path(outer['readyDirectory']),png=False)
        changed=copy.deepcopy(receipt);changed['stages'][1]['model']['comments']['pageMessages'][1]['channel']=None
        with self.assertRaises(RuntimeError):V['validate']('slack-list',changed,Path(outer['readyDirectory']),png=False)

    def test_dispatch_binds_case_stage_ready_final(self):
        for case in V['STAGES']:
            stages,validate=V['C']['case_contract'](case);self.assertEqual(stages,V['STAGES'][case])
            f,r=sample(case);validate(f,r,DIR,png=False)
            f,r=sample('edit-ack' if case=='edits' else 'edits')
            with self.assertRaises(RuntimeError):validate(f,r,DIR,png=False)
        for change in (lambda r:r.update(case='incoming'),lambda r:r['stages'].pop(),lambda r:r['readiness'].update(pid=False),lambda r:r.update(final={})):
            f,r=sample();change(r)
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_original_owner_source_pagination(self):
        for key,value in [('account','other'),('epoch',999),('noSession',False),('noTransport',False),('writes',1),('networkCalls',1)]:
            self.reject(lambda s:s['model'].update({key:value}))
        self.reject(lambda s:s['model'].update(records={}))
        self.reject(lambda s:s['model']['comments'].update(hasMore=False))
        self.reject(lambda s:s['model']['comments'].update(root='other'))
        self.reject(lambda s:s['model']['comments']['page']['messages'][1].update(text='Edited plain reply.'))
        self.reject(lambda s:s['model']['comments']['pageMessages'][0].update(text='Changed root'))

    def test_absent_blocks_remove_rich_body_without_losing_reactions(self):
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(blocks=blocks('Edited rich reply.',{'bold':True})))
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(reactions=[]))
        self.reject(lambda s:s['geometry']['editBody'].update(bold=True))
        self.reject(lambda s:s['geometry']['rows'][0].update(text='Edited rich reply.'))

    def test_explicit_empty_and_scalar_null_clear(self):
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(reactions=[dict(name='wave',count=2,users=['U1','U2'])]),3)
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(edited=dict(ts='1781525102.000001')),3)
        self.reject(lambda s:s['geometry']['rows'][0]['reactions'].append(dict(name='wave',count=2)),3)
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(blocks=blocks('wrong',{'bold':True})),3)

    def test_scalar_objects_replace_whole_and_unknown_not_adopted(self):
        self.reject(lambda s:s['model']['comments']['pageMessages'][1]['edited'].update(user='U1'))
        self.reject(lambda s:s['model']['comments']['pageMessages'][1]['metadata']['event_payload'].update(old=True))
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(opaque=dict(ignored=True)),1)
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].pop('metadata'),7)

    def test_generated_body_fields_are_not_generic_absent_preserve(self):
        for key,value in [('files',[dict(id='stale')]),('attachments',[dict(text='stale')]),('reply_count',None),
                          ('no_display',True),('_hidden_reply',False),('slackbot_feels','stale'),('language','old')]:
            self.reject(lambda s:s['model']['comments']['pageMessages'][1].update({key:value}))

    def test_held_duplicate_foreign_and_replied_do_not_cancel(self):
        for i in (5,6):
            self.reject(lambda s:s['model']['comments'].update(loading=False),i)
            self.reject(lambda s:s['model']['ledger'].update(cancelled=1),i)
        self.reject(lambda s:s['model']['comments'].update(read=999),6)
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(text='Ignored edit.'),6)

    def test_real_edit_cancels_only_held_replace_read(self):
        self.reject(lambda s:s['model']['comments'].update(read=4),7)
        self.reject(lambda s:s['model']['comments'].update(loading=True),7)
        self.reject(lambda s:s['model']['ledger'].update(cancelled=0),7)
        self.reject(lambda s:s['model']['comments'].update(error=None),7)
        self.reject(lambda s:s['proof']['release'].update(delivered=True,cancelledBeforeReply=False),8)
        self.reject(lambda s:s['proof']['release'].update(delivered=0),8)
        self.reject(lambda s:s['proof']['heldResponse']['messages'][1].update(text='Held edit reply.'),8)

    def test_unsupported_retains_body_without_new_requests(self):
        self.reject(lambda s:s['model']['comments']['pageMessages'][1].update(text='Unsupported file body.'),9)
        self.reject(lambda s:s['model']['comments'].update(error=None),9)
        self.reject(lambda s:s['model']['ledger']['requests'].append({}),9)
        self.reject(lambda s:s['model']['comments'].update(read=999),9)

    def test_canonical_readback_dedups_and_replaces_projection(self):
        for case,index in [('edits',10),('edit-ack',9)]:
            self.reject(lambda s:s['proof']['serverResponse'].update(has_more=True),index,case)
            self.reject(lambda s:s['model']['comments']['pageMessages'][-1].update(metadata=dict(old=True)),index,case)
            self.reject(lambda s:s['model']['comments']['visibleTs'].append(s['model']['comments']['visibleTs'][-1]),index,case)
        self.reject(lambda s:s['model']['comments']['accepted'].append(copy.deepcopy(s['model']['comments']['pageMessages'][-1])),9,'edit-ack')

    def test_exact_frame_parser_witness_and_replied_kind(self):
        for case in V['STAGES']:
            f,r=sample(case)
            for i,event in enumerate(r['editEvents']):
                for key,value in [('parsed',False),('changed',not event['changed']),('team','other'),('generation',99)]:
                    bad=copy.deepcopy(r);bad['editEvents'][i][key]=value
                    with self.assertRaises(RuntimeError):self.check(f,bad)
                if event['witness']:
                    for key,value in [('kind','Replied' if event['witness']['kind']=='Changed' else 'Changed'),('raw',{}),('typedTs','other'),('typedChannel','other')]:
                        bad=copy.deepcopy(r);bad['editEvents'][i]['witness'][key]=value
                        with self.assertRaises(RuntimeError):self.check(f,bad)
            bad=copy.deepcopy(r);bad['editEvents'][0]['after']['read']+=1
            with self.assertRaises(RuntimeError):self.check(f,bad)

    def test_field_presence_in_original_frame_is_not_reconstructed(self):
        for key,value in [('blocks',[]),('reactions',[]),('channel',CH),('type','message')]:
            f,r=sample();r['editEvents'][1]['frame']['message'][key]=value;r['editEvents'][1]['witness']['raw'][key]=value
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_one_post_raw_ack_and_pending_draft_not_settled_by_edit(self):
        for i in (1,2,3,4):
            self.reject(lambda s:s['proof']['journal'].update(attempt=None),i,'edit-ack')
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(busy=False),i,'edit-ack')
            self.reject(lambda s:s['model']['composeLedger'].update(heldPost=False),i,'edit-ack')
            self.reject(lambda s:s['geometry']['editor'].update(editable=True),i,'edit-ack')
        self.reject(lambda s:s['proof']['originalMessage'].update(text='Socket edited reply.'),3,'edit-ack')
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(reactions=[]),4,'edit-ack')
        self.reject(lambda s:s['proof']['journal']['attempt']['record'].update(user='U1'),3,'edit-ack')
        self.reject(lambda s:s['model']['composeLedger']['posts'].append({}),8,'edit-ack')

    def test_ack_keeps_new_body_and_empty_reactions_then_retires_custody(self):
        for i in (5,6,7):
            self.reject(lambda s:s['model']['comments']['accepted'][0].update(text='Original body'),i,'edit-ack')
            self.reject(lambda s:s['model']['comments']['accepted'][0].update(reactions=[dict(name='wave',count=1,users=['U_RT_REACTOR'])]),i,'edit-ack')
            self.reject(lambda s:s['model']['comments'].update(editCustody=[SENT]),i,'edit-ack')
            self.reject(lambda s:s['model']['comments'].update(liveReactions=[SENT]),i,'edit-ack')
        self.reject(lambda s:s['model']['comments'].update(editCustody=[]),3,'edit-ack')
        self.reject(lambda s:s['model']['comments'].update(liveReactions=[]),4,'edit-ack')
        self.reject(lambda s:s['proof']['release'].update(delivered=False),5,'edit-ack')

    def test_accepted_tail_edit_no_extra_reveal_or_send(self):
        self.reject(lambda s:s['model']['comments']['accepted'][0].update(edited=None),8,'edit-ack')
        self.reject(lambda s:s['model']['comments']['reveal'].__setitem__(0,999),8,'edit-ack')
        self.reject(lambda s:s['model']['comments'].update(read=999),8,'edit-ack')
        self.reject(lambda s:s['model']['composeLedger']['preflights'].append({}),8,'edit-ack')

    def test_exact_native_model_journal_a_b_and_seed(self):
        for case in V['STAGES']:
            for i in range(len(V['STAGES'][case])):
                if case=='edit-ack' and i==5:continue
                for field in ('blocks','capture','revision'):
                    self.reject(lambda s:s['proof']['journal'].update({field:[] if field=='blocks' else 'wrong'}),i,case)
                self.reject(lambda s:s['geometry']['editor'].update(blocks=[]),i,case)
        self.reject(lambda s:s['proof']['seed']['blocks'][0]['elements'][0]['elements'][1]['opaque'].update(retain=[]))
        self.reject(lambda s:s['geometry']['editor'].update(text='bad whitespace'))
        self.reject(lambda s:s['geometry']['editor'].update(text='  '),5,'edit-ack')
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(sequence=999),7,'edit-ack')

    def test_node_focus_generation_and_readonly_geometry(self):
        self.reject(lambda s:s['geometry']['editor'].update(sameNode=False))
        self.reject(lambda s:s['geometry'].update(active='other'),7)
        self.reject(lambda s:s['geometry'].update(active='other'),8,'edit-ack')
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'))
        self.reject(lambda s:s['geometry']['editBody'].update(visible=False))
        self.reject(lambda s:s['geometry']['editBody'].update(tools=1))
        self.reject(lambda s:s['geometry']['editBody']['pills'][0].update(tag='BUTTON'),1)
        self.reject(lambda s:s['geometry']['editBody']['pills'][0].update(visible=False),1)
        self.reject(lambda s:s['geometry']['editBody'].update(italic=False),10)

    def test_counts_controls_notices_and_captures(self):
        self.reject(lambda s:s['model']['ledger'].update(cancelled=True),7)
        self.reject(lambda s:s['geometry'].update(notices=[]),7)
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True),7)
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),380]))
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda p,d:seen.append((p,d))}):V['validate'](f,r,DIR,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{stage}.png',[1280,780]) for stage in V['STAGES']['edits']])


if __name__=='__main__':unittest.main()
