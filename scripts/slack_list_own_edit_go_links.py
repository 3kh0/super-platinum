"""Bounded literal go/team with original plain ownership, wire and closed SQLite."""
import copy
from pathlib import Path

CASE='own-edit-plain-go-links'
TEXT='Visit go/team'
# Specialize only the authored text and case in a private namespace. Every
# original six-stage ownership/wire/capture/database assertion is retained.
_BASE=Path(__file__).with_name('slack_list_own_edit_plain.py')
_SOURCE=_BASE.read_text(encoding='utf-8')
if _SOURCE.count('Changed plain comment')!=7 or _SOURCE.count("'own-edit-plain'")!=4:
    raise RuntimeError('Original bounded plain oracle source changed')
P={'__file__':str(_BASE),'__name__':'_isolated_go_links_plain'}
exec(compile(_SOURCE.replace('Changed plain comment',TEXT).replace("'own-edit-plain'",repr(CASE)),str(_BASE)+'[go-links text/case]', 'exec'),P)
W,O,T,N,G,R,C,READ,H,S=(P[k] for k in ('W','O','T','N','G','R','C','READ','H','S'))
require,exact=P['require'],P['exact']
STAGES={CASE:P['STAGES'][CASE]}
INITIAL_BYTES,INITIAL,INITIAL_OPS,CHANGED_OPS,BODY,FUTURE,FUTURE_BYTES,ACK,PREFS=(P[k] for k in ('INITIAL_BYTES','INITIAL','INITIAL_OPS','CHANGED_OPS','BODY','FUTURE','FUTURE_BYTES','ACK','PREFS'))
READS,POLICIES=P['READS'],P['POLICIES']
seed_value=P['seed_value']
original_policy=P['policy_cycle']
def policy_cycle():
    rows=original_policy();rows[2][1]['assignments']['detect_go_links']={'group':'on'};return rows
P['policy_cycle']=policy_cycle
literal=lambda text:[{'insert':text+'\n'}]

def selection(v):
    require(isinstance(v,dict) and set(v)=={'collapsed','contained','anchorOffset','focusOffset','text'} and v['collapsed'] is True and v['contained'] is True
        and R['integer'](v['anchorOffset']) and R['integer'](v['focusOffset']) and v['text']=='','Original collapsed input selection changed')

def event(v,action,before,after,mount,viewport):
    require(v.get('ok') is True and v.get('action')==action and v.get('sameNode') is True and v.get('scope')==mount['scope'] and v.get('generation')==mount['generation'],'Go-link event borrowed original owner/node')
    P['ticket'](v['before'],literal(before),mount);P['ticket'](v['after'],literal(after),mount)
    require(v['after']['revision']>=v['before']['revision'],'Original native revision regressed')
    P['exposed'](v['exposed'],viewport)

def settled(v,text,mount,viewport,scope,seed):
    n=v['native'];event(n,'state',text,text,mount,viewport);selection(n['selection'])
    require(exact(n.get('controls'),{'saveDisabled':False,'cancelDisabled':False}) and n.get('anchors')==[],'Original controls unsettled or go-link DOM anchor appeared')
    P['inventory'](v['inventory'],scope,seed,literal(text))

def key(v,name,before,after,mount,viewport,scope,seed):
    e=v['event'];event(e,'key',before,after,mount,viewport)
    consumed=name!='Space';shift=name in ('ShiftEnter','Redo');ctrl=name in ('Undo','Redo')
    require(e.get('key')==name and e.get('keyCode')=={'Space':32,'ShiftEnter':13,'Undo':90,'Redo':90}[name]
        and e.get('isTrusted') is False and e.get('defaultPrevented') is consumed and e.get('defaultApplied') is (not consumed)
        and e.get('shiftKey') is shift and e.get('ctrlKey') is ctrl and e.get('altKey') is False and e.get('metaKey') is False,'Original key/default/newline/history dispatch changed')
    selection(e['selectionBefore']);selection(e['selectionAfter']);settled(v['settled'],after,mount,viewport,scope,seed)

def validate(fixture,receipt,directory,*,expected_case=CASE,dimensions=None,png=True):
    require(expected_case==CASE,'Unexpected go-links case')
    P['validate'](fixture,receipt,directory,expected_case=CASE,dimensions=dimensions,png=png)
    first=receipt['stages'][0];scope=first['journal']['scope'];seed=seed_value(first['model']['comments'])
    for i,row in enumerate(receipt['stages']):
        p=row['proof']
        require('customCatalog' not in row['plain'],'Go links acquired unrelated custom catalog')
        if not i:
            require('goLinks' not in p,'Go-link input predates original Edit');continue
        v=p['goLinks'];mount=p['opens'][0]['mount'];viewport=row['geometry']['viewport']
        key(v['space'],'Space',TEXT,TEXT+' ',mount,viewport,scope,seed)
        key(v['newline'],'ShiftEnter',TEXT+' ',TEXT+' \n',mount,viewport,scope,seed)
        require(R['integer'](v.get('historyPauseMillis'),1100),'Original normal history interval was not observed')
        e=v['continuation']['event'];event(e,'insert',TEXT+' \n',TEXT+' \ngo/team',mount,viewport)
        require(e.get('inputCommand')=='insertText' and e.get('text')=='go/team' and e.get('composing') is False and 'key' not in e,'Continuation was not original literal input')
        selection(e['selectionBefore']);selection(e['selectionAfter']);settled(v['continuation']['settled'],TEXT+' \ngo/team',mount,viewport,scope,seed)
        key(v['undo'],'Undo',TEXT+' \ngo/team',TEXT+' \n',mount,viewport,scope,seed)
        key(v['redo'],'Redo',TEXT+' \n',TEXT+' \ngo/team',mount,viewport,scope,seed)
        reset=v['reset'];require(reset.get('ok') is True and reset.get('action')=='native-select-all-insertText' and reset.get('text')==TEXT and reset.get('sameNode') is True,'Original reset input changed')
        P['ticket'](reset['before'],literal(TEXT+' \ngo/team'),mount);P['ticket'](reset['after'],CHANGED_OPS,mount);P['exposed'](reset['exposed'],viewport)
        settled(v['resetSettled'],TEXT,mount,viewport,scope,seed)
    return receipt

def validate_closed(snapshot,receipt):return P['validate_closed'](snapshot,receipt)
def validator(case):
    require(case==CASE,'Unexpected go-links case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
