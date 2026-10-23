"""Finite synthetic receipt mutations; original key/default queues remain native-only."""
import copy
import runpy
import unittest
from pathlib import Path
import slack_list_own_edit_smart_symbols as V
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_plain.py')))
D=B['D']

def sample(mode=0,narrow=False):
    fixture,base,closed=B['sample'](False,narrow)
    case=V.CASES[mode];enabled=mode==0;rows=[];first=base['stages'][0];initial=first['model']['comments'];scope=first['journal']['scope'];seed=V.seed_value(initial)
    rect=copy.deepcopy(base['stages'][1]['geometry']['edit']['rect'])
    def ticket(ops,gen=78,composing=False,pending=False):return dict(ops=copy.deepcopy(ops),mode='PlainText',revision=1,generation=str(gen),composing=composing,pending=pending,attachmentEffects='none-v1')
    def inv(ops=None):
        v=copy.deepcopy(first['editInventory'])
        if ops is not None:v.update(seed=copy.deepcopy(seed),revision=1,capture='00000000-0000-4000-8000-000000000003',plain_delta=dict(version=1,ops=copy.deepcopy(ops)))
        return v
    def button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def selection(collapsed=True,text=''):return dict(contained=True,collapsed=collapsed,anchorOffset=8,focusOffset=8 if collapsed else 6,text=text)
    exposed=dict(visible=True,hit=True,rect=rect)
    def event(action,before,after,gen=78,composing=False,after_composing=False,pending=False):return dict(ok=True,action=action,scope=f'list-edit:100:{gen}',generation=str(gen),sameNode=True,before=ticket(V.literal(before),gen,composing),after=ticket(V.literal(after),gen,after_composing,pending),exposed=exposed)
    def settled(text,gen=78):return dict(native=dict(event('state',text,text,gen),selection=selection(),controls=dict(saveDisabled=False,cancelDisabled=False)),inventory=inv(V.literal(text)))
    def key_event(name,before,after,consumed=True,default=False,gen=78,composing=False,collapsed=True):
        return dict(event('key',before,after,gen,composing,composing),key=name,keyCode={'Space':32,'Period':190,'Enter':13,'Undo':90,'Redo':90}[name],isTrusted=False,defaultPrevented=consumed,defaultApplied=default,ctrlKey=name in ('Undo','Redo'),shiftKey=name=='Redo',altKey=False,metaKey=False,selectionBefore=selection(collapsed),selectionAfter=selection())
    def key(name,before,after,consumed=True,default=False,gen=78,collapsed=True):return dict(event=key_event(name,before,after,consumed,default,gen,collapsed=collapsed),settled=settled(after,gen))
    def input(text,before,gen=78):return dict(native=dict(ok=True,action='native-select-all-insertText',text=text,sameNode=True,before=ticket(before,gen),after=ticket(V.literal(text),gen),exposed=exposed),inventory=inv(V.literal(text)))
    def inserted(before,text,after,gen=78):return dict(event=dict(event('insert',before,before+text,gen,pending=enabled),text=text,composing=False,inputCommand='insertText',selectionBefore=selection(),selectionAfter=selection()),settled=settled(after,gen))
    p=copy.deepcopy(first['proof']);p.update(smartSymbolsEnabled=enabled,smartSymbolsDisabled=mode==2,opens=[],inputs=[],actions=[])
    requests=[];pages=[]
    for i,name in enumerate(V.STAGES[case]):
        ops=V.ops_for(name);editing=ops is not None;saving=name=='saving';accepted=name in ('saved','readback');sent=saving or accepted
        index=0 if name=='draft' else 3 if saving else 4 if name=='saved' else 5 if name=='readback' else 2 if name=='cancelled' else 1
        row=copy.deepcopy(base['stages'][index]);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'));m=row['model'];c=m['comments'];n=V.READS[case][i]
        c['comments'].update(loading=saving,page=copy.deepcopy(V.FUTURE if name=='readback' else V.INITIAL),pageMessages=copy.deepcopy((V.FUTURE if accepted else V.INITIAL)['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(initial['ledger']['requests']*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];f.update(policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle(case)],prepared=[V.O['wire'](V.BODY)] if sent else [],updates=[V.O['wire'](V.BODY)] if sent else [],responses=[dict(disposition='accepted',raw=V.ACK)] if sent else [],futureRows=[V.FUTURE['messages'][1]] if sent else [])
        if sent:m['edit']['attempts'][0].update(blocks=V.FUTURE['messages'][1]['blocks'],receipt=None if saving else V.ACK)
        gen=79 if sent else 78
        if editing:m['edit']['active']['plainDelta']=dict(version=1,ops=copy.deepcopy(ops))
        while len(p['opens'])<(0 if i==0 else 2 if sent else 1):p['opens'].append(copy.deepcopy(base['stages'][1 if len(p['opens'])==0 else 3]['proof']['opens'][-1]))
        if name=='editing':p['inputs'].append(input('Marks --',V.INITIAL_OPS))
        if name=='space-symbols' or name=='literal':
            symbols=[];before='Marks --'
            for j,(text,converted) in enumerate([('Marks --','Marks — '),('Right ->','Right → '),('Left <-','Left ← '),('Both <->','Both ↔ ')]):
                if j:p['inputs'].append(input(text,V.literal(before)))
                after=converted if enabled else text+' ';v=dict(literal=text,space=key('Space',text,after,enabled,not enabled))
                if enabled:v.update(undo=key('Undo',converted,text+' '),redo=key('Redo',text+' ',converted))
                symbols.append(v);before=after
            p['symbols']=symbols
        if name=='period' or name=='literal':
            p['inputs'].append(input('Wait ..',V.literal('Both ↔ ' if enabled else 'Both <-> ')))
            p['period']=key('Period','Wait ..','Wait …' if enabled else 'Wait ...',enabled,not enabled)
            if enabled:p.update(periodUndo=key('Undo','Wait …','Wait ...'),periodRedo=key('Redo','Wait ...','Wait …'))
        if name=='guards':
            guards=[];before='Wait …'
            for kind,text,k,after in [('code','`Code --','Space','`Code -- '),('repeated-dash','Repeated ---','Space','Repeated --- '),('repeated-period','Wait ...','Period','Wait ....')]:
                p['inputs'].append(input(text,V.literal(before)));guards.append(dict(kind=kind,key=key(k,text,after,False,True)));before=after
            p['inputs'].append(input('Pick --',V.literal(before)))
            guards.append(dict(kind='selection',selection=dict(event('select','Pick --','Pick --'),start=5,end=7,selection=selection(False,'--')),key=key('Space','Pick --','Pick  ',False,True,collapsed=False)))
            p['inputs'].append(input('Compose --',V.literal('Pick  ')))
            guards.append(dict(kind='composition',start=dict(event('composition','Compose --','Compose --',after_composing=True),active=True,isTrusted=False,selection=selection()),key=key_event('Space','Compose --','Compose --',False,False,composing=True),end=dict(event('composition','Compose --','Compose --',composing=True),active=False,isTrusted=False,selection=selection()),settled=settled('Compose --')))
            p['guards']=guards
        if name=='quotes' or name=='literal':
            p['inputs'].append(input('He said ',V.literal('Compose --' if enabled else 'Wait ...')))
            p['quotes']=inserted('He said ',V.QUOTE_INPUT,V.QUOTE_RESULT if enabled else V.QUOTE_LITERAL)
            if enabled:
                p['inputs'].append(input('Compose ',V.literal(V.QUOTE_RESULT)))
                p['compositionQuotes']=dict(start=dict(event('composition','Compose ','Compose ',after_composing=True),active=True,isTrusted=False,selection=selection()),input=dict(event('insert','Compose ','Compose "yes"',composing=True,after_composing=True),text='"yes"',composing=True,inputCommand='insertText',selectionBefore=selection(),selectionAfter=selection()),end=dict(event('composition','Compose "yes"','Compose "yes"',composing=True,pending=True),active=False,isTrusted=False,selection=selection()),settled=settled(V.COMPOSE_RESULT))
        if name=='cancelled':p['cancel']=button('slack-list-comment-edit-cancel')
        if saving:
            p['inputs'].append(input('Both <->',V.INITIAL_OPS,79));p['saveArrow']=key('Space','Both <->','Both ↔ ',gen=79)
            p['saveInput']=inserted('Both ↔ ',V.SAVE_INPUT,V.SAVE_QUOTES,79);p['savePeriod']=key('Period',V.SAVE_QUOTES,V.LIVE,gen=79)
            p['enter']=key_event('Enter',V.LIVE,V.LIVE,gen=79);p['doubleSave']=dict(ok=True,disabled=True)
        if name=='saved':p['release']=V.O['RELEASE']
        if name=='readback':p['actions'].append(button('slack-list-comments-refresh'))
        row['editInventory']=inv(ops)
        if saving:
            a=copy.deepcopy(base['stages'][3]['editInventory']['attempt']);a['record'].update(blocks=V.FUTURE['messages'][1]['blocks'],text=V.TEXT,plain_delta=dict(version=1,ops=V.ENTER));row['editInventory']['attempt']=a
        while len(requests)<n//4:
            j=len(requests);future=name=='readback';text=V.FUTURE_BYTES if future else V.INITIAL_BYTES;raw=V.W['decoded'](text)
            requests.append(dict(serial=4*(j+1),readSerial=j+1,method='conversations.replies',args=initial['ledger']['requests'][3]['args'],variant='future' if future else 'initial',bytes=text,raw=raw,owner=dict(serial=initial['comments']['generation'],account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None if future else ['italic','bold']),current=True,requestSerial=4*(j+1),readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=None if name=='readback' else ['italic','bold']))
        row['ordered'].update(futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(requests),pages=copy.deepcopy(pages),entryPage=entry,completed=n//4)
        row['plain']['preferences']=V.prefs(case);row['plain']['completion']['seed']['boot']['prefs']=V.prefs(case)['prefs']
        g=row['geometry'];g['row']['text']=(V.TEXT if accepted else V.INITIAL['messages'][1]['text'])+' Preserved original context'
        if editing:g['edit'].update(delta=ticket(ops,gen),text=''.join(o['insert'] for o in ops).rstrip('\n'))
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(p['originalDisk']),edit=copy.deepcopy(rows[-1]['editInventory']))
    receipt=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    closed.update(editInventories=[rows[-1]['editInventory']],editHistory=[dict(copy.deepcopy(rows[7]['editInventory']['attempt']),phase='Confirmed',raw=V.ACK)] if enabled else [])
    return copy.deepcopy((fixture,receipt,closed))

class SmartTests(unittest.TestCase):
    def reject(self,change,mode=0):
        f,r,_=sample(mode);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,expected_case=r['case'],png=False)
    def test_complete_cases_widths_and_closed_database(self):
        for mode in range(3):
            for narrow in (False,True):
                f,r,c=sample(mode,narrow);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_exact_authored_future_bytes(self):self.assertEqual(Path('src/desktop/slack_lists/comments/fixture/plain/smart_symbols-future.json').read_bytes(),V.FUTURE_BYTES.encode())
    def test_undo_restores_typed_space_not_prekey_input(self):self.reject(lambda r:r['stages'][2]['proof']['symbols'][0]['undo']['event']['after'].update(ops=V.literal('Marks --')))
    def test_period_is_original_key190(self):self.reject(lambda r:r['stages'][3]['proof']['period']['event'].update(keyCode=46))
    def test_period_cannot_have_shift_modifier(self):self.reject(lambda r:r['stages'][3]['proof']['period']['event'].update(shiftKey=True))
    def test_period_undo_includes_third_literal_period(self):self.reject(lambda r:r['stages'][3]['proof']['periodUndo']['event']['after'].update(ops=V.literal('Wait ..')))
    def test_default_space_not_inserted_twice(self):self.reject(lambda r:r['stages'][2]['proof']['symbols'][0]['space']['event'].update(defaultApplied=True))
    def test_off_requires_literal_default_input(self):self.reject(lambda r:r['stages'][2]['proof']['symbols'][0]['space']['event'].update(defaultApplied=False),1)
    def test_assignment_off_is_original_observation(self):self.reject(lambda r:r['stages'][1]['model']['fixture']['policy'][2]['response']['assignments']['composer_smart_symbols'].update(group='on'),1)
    def test_disabled_pref_does_not_disable_assignment(self):self.reject(lambda r:r['stages'][1]['model']['fixture']['policy'][2]['response']['assignments']['composer_smart_symbols'].update(group='off'),2)
    def test_disable_pref_required(self):self.reject(lambda r:r['stages'][1]['plain']['preferences']['prefs'].update(disable_smart_symbols=False),2)
    def test_old_original_root_replacement_refused(self):self.reject(lambda r:r['stages'][2]['proof']['symbols'][0]['space']['event'].update(sameNode=False))
    def test_settled_control_paint_required(self):self.reject(lambda r:r['stages'][4]['proof']['guards'][-1]['settled']['native']['controls'].update(saveDisabled=True))
    def test_selection_guard_noncollapsed(self):self.reject(lambda r:r['stages'][4]['proof']['guards'][3]['selection']['selection'].update(collapsed=True))
    def test_composition_does_not_insert_default_space(self):self.reject(lambda r:r['stages'][4]['proof']['guards'][-1]['key'].update(defaultApplied=True))
    def test_quote_input_not_keyboard_substitution(self):self.reject(lambda r:r['stages'][5]['proof']['quotes']['event'].update(action='key'))
    def test_quote_queue_pending_witness(self):self.reject(lambda r:r['stages'][5]['proof']['quotes']['event']['after'].update(pending=False))
    def test_quote_original_immediate_input_before_repair(self):self.reject(lambda r:r['stages'][5]['proof']['quotes']['event']['after'].update(ops=V.literal(V.QUOTE_RESULT)))
    def test_quote_composition_end_pending(self):self.reject(lambda r:r['stages'][5]['proof']['compositionQuotes']['end']['after'].update(pending=False))
    def test_quote_scope_not_borrowed(self):self.reject(lambda r:r['stages'][5]['proof']['quotes']['settled']['native'].update(scope='other'))
    def test_live_arrow_has_no_selector(self):self.reject(lambda r:r['stages'][7]['geometry']['edit']['delta'].update(ops=V.literal(V.TEXT)))
    def test_save_wire_requires_selector_and_curly_quotes_ellipsis(self):self.reject(lambda r:r['stages'][7]['model']['fixture']['prepared'][0]['args'].update(blocks='[]'))
    def test_unique_update_after_cancel(self):self.reject(lambda r:r['stages'][6]['model']['fixture']['updates'].append({}))
    def test_original_reply_node_preserved(self):self.reject(lambda r:r['stages'][6]['geometry']['reply'].update(sameNode=False))
    def test_held_save_controls_disabled(self):self.reject(lambda r:r['stages'][7]['geometry']['controls'][0].update(disabled=False))
    def test_closed_history_retains_live_delta_and_wire_selector_separately(self):
        _,r,c=sample();c['editHistory'][0]['record']['plain_delta']['ops']=V.literal(V.TEXT)
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_isolated_wrapper_admits_only_named_cases(self):
        m=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-smart-symbols-native.py')));h=m['harness']()
        for case in V.CASES:self.assertEqual(h['case_contract'](case)[0],V.STAGES[case])
        self.assertEqual(runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))['CASES'],('own-edit-plain','own-edit-plain-storage'))

if __name__=='__main__':unittest.main()
