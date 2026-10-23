"""Independent fixed-offset labels and original mounted Profile clock receipts."""
from datetime import datetime, timedelta, timezone
from canvas_special_roots import require, same
from native_check import expected_viewport

PREFIX = 'profile-local-time'
FIXTURES = ('profile-pane', 'profile-pane-narrow', 'profile-pane-min-width',
            'profile-pane-min-width-narrow', 'profile-hover-card', 'profile-hover-card-narrow')
STAGES = ('initial', 'same-minute', 'minute', 'midnight-before', 'midnight',
          'offset', 'absent', 'closed', 'replacement')
EPOCHS = (1735801130, 1735801135, 1735801145, 1735801199, 1735801200,
          1735801200, 1735801200, 1735801260, 1735801260)
OFFSETS = (-25200, -25200, -25200, -25200, -25200, 19800, None, None, 3600)


def label(epoch, offset):
    require(type(epoch) is int, 'Clock epoch is not an observed integer')
    if offset is None:
        return None
    require(type(offset) is int and -86400 < offset < 86400, 'Invalid fixed member offset')
    return (datetime.fromtimestamp(epoch, timezone.utc) + timedelta(seconds=offset)).strftime('%H:%M local time')

def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def rectangle(value, viewport):
    import math
    require(isinstance(value, dict) and all(type(value.get(k)) in (int,float) and math.isfinite(value[k]) for k in ('x','y','width','height')), 'Missing native rectangle')
    require(value['width'] > 0 and value['height'] > 0 and value['x'] >= -1 and value['y'] >= -1
            and value['x']+value['width'] <= viewport[0]+1 and value['y']+value['height'] <= viewport[1]+1, 'Profile time node exceeds viewport')


def exposed(action, viewport):
    import math
    r=action.get('rect');point=action.get('hitPoint')
    require(isinstance(r,dict) and all(type(r.get(k)) in (int,float) and math.isfinite(r[k]) for k in ('x','y','width','height'))
            and r['width']>0 and r['height']>0 and isinstance(point,dict)
            and all(type(point.get(k)) in (int,float) and math.isfinite(point[k]) for k in ('x','y'))
            and r['x']<=point['x']<=r['x']+r['width'] and r['y']<=point['y']<=r['y']+r['height']
            and 0<=point['x']<viewport[0] and 0<=point['y']<viewport[1], 'Original exposed action point/rectangle invalid')


def initial_setup(action, model, native, fixture):
    require('setup' in action, 'Original initial setup disposition absent')
    exposed(action,native['viewport'])
    setup=action['setup']
    if setup is None:
        return
    require(fixture in ('profile-pane-narrow','profile-pane-min-width-narrow')
            and isinstance(setup,dict) and set(setup)=={'close','draft','reopen','selection'}, 'Unexpected Profile setup branch')
    for key,label in (('close','Close profile'),('reopen','Maya Chen')):
        a=setup[key]
        require(a.get('ok') is True and a.get('kind')=='synthetic-dom-click' and a.get('isTrusted') is False
                and integer(a.get('nodeId'),1) and a.get('label')==label, 'Actual exposed Profile setup control missing')
        exposed(a,native['viewport'])
    require(setup['close']['nodeId']!=setup['reopen']['nodeId']
            and setup['reopen'].get('messageId')=='m-rich-1', 'Profile reopened through foreign author/control')
    draft=setup.get('draft',{});m=draft.get('model',{});c=draft.get('native',{})
    require(m.get('user') is None and m.get('offset') is None and m.get('profileUser') is None
            and m.get('hoverUser') is None, 'Draft input occurred before Profile closed')
    changing={'user','offset','profileUser','hoverUser','profileGeneration','hoverGeneration'}
    require(same({k:v for k,v in m.items() if k not in changing},
                 {k:v for k,v in model.items() if k not in changing}), 'Setup changed owner/channel or original draft data')
    require(all(integer(m.get(k)) and m[k]<=model[k] for k in ('profileGeneration','hoverGeneration')),
            'Setup adopted a successor profile generation')
    require(c.get('nodeId')==action['nodeId']==native['composer']['nodeId'] and c.get('connected') is True
            and c.get('value')==DRAFT and c.get('selectionStart')==2 and c.get('selectionEnd')==9,
            'Actual exposed draft did not commit before reopening')
    selection=setup['selection'];before=selection.get('before',{})
    require(selection.get('ok') is True and selection.get('kind')=='native-selection-restore'
            and selection.get('nodeId')==action['nodeId'] and selection.get('after')==dict(start=2,end=9)
            and integer(before.get('start')) and integer(before.get('end'))
            and before['start']<=before['end']<=len(DRAFT.encode('utf-16-le'))//2,
            'Explicit setup-only selection restoration missing or foreign')


def setup_initial_polls(initial, following, hover):
    """Prove reopened-owner baseline now, and polls before the next clock write."""
    m=initial['model'];clock=initial['clock'];expected=face(m,hover)
    render=clock['renders'][-1]
    require(render['now']==m['now'] and same([render['face']],expected),
            'Reopened Profile lacks original current-owner render baseline')
    action=following['action'];deadline=action.get('afterTick')
    require(action.get('kind')=='synthetic-clock' and action.get('now')==EPOCHS[1]
            and integer(deadline) and deadline>=len(clock['ticks'])
            and deadline<=len(following['clock']['ticks']), 'Initial poll proof has invalid clock-advance deadline')
    polls=following['clock']['ticks'][:deadline][-2:]
    require(len(polls)==2 and all(t['now']==m['now'] and same(t['before'],expected)
            and same(t['after'],expected) and t['notify'] is False
            and t['renderSerial']>=render['serial'] for t in polls),
            'Reopened Profile lacks two actual initial-time owner polls before advance')


def face(model, hover):
    if model['user'] is None:
        return []
    return [dict(slot='hover' if hover else 'pane',generation=model['hoverGeneration' if hover else 'profileGeneration'],account=model['account'],epoch=model['epoch'],
                 team=model['team'],user=model['user'],minute=None if model['offset'] is None
                 else ((model['now']+model['offset'])%86400)//60)]


def same_owner(a,b):
    return all(a[k]==b[k] for k in ('slot','generation','account','epoch','team','user'))


def tick_history(clock, previous=None):
    require(isinstance(clock,dict) and isinstance(clock.get('ticks'),list) and clock['ticks']
            and isinstance(clock.get('renders'),list) and clock['renders'], 'Original runtime/render observations absent')
    ticks=clock['ticks'];renders=clock['renders']
    require(len(ticks)<=2048 and len(renders)<=4096 and integer(clock.get('notifications')), 'Unbounded/malformed clock ledger')
    if previous is not None:
        require(same(ticks[:len(previous['ticks'])],previous['ticks'])
                and same(renders[:len(previous['renders'])],previous['renders']), 'Original runtime/render history rewritten')
    def valid_face(item):
        require(set(item)=={'slot','generation','account','epoch','team','user','minute'}
                and item['slot'] in ('pane','hover') and integer(item['epoch']) and integer(item['generation'])
                and item['team']=='T1' and item['user'] in ('U1','U2')
                and (item['minute'] is None or integer(item['minute']) and item['minute']<1440), 'Invalid displayed clock owner')
    for i,r in enumerate(renders):
        require(set(r)=={'serial','now','face'} and r['serial']==i+1 and integer(r['now']), 'Invalid original render serial/time')
        valid_face(r['face'])
    displayed=[];pending=[];cursor=0;notifications=0
    for i,tick in enumerate(ticks):
        require(set(tick)=={'serial','renderSerial','now','before','after','notify'} and tick['serial']==i+1
                and integer(tick['now']) and isinstance(tick['after'],list) and type(tick['notify']) is bool
                and integer(tick['renderSerial']) and cursor<=tick['renderSerial']<=len(renders), 'Runtime/render serial correlation invalid')
        for r in renders[cursor:tick['renderSerial']]:
            require(r['now']<=tick['now'], 'Runtime borrowed future render')
            displayed=[v for v in displayed if v['slot']!=r['face']['slot']]+[r['face']]
        cursor=tick['renderSerial']
        for item in tick['after']:valid_face(item)
        displayed=[old for old in displayed if any(same_owner(old,new) for new in tick['after'])]
        require(same(tick['before'],displayed), 'Tick baseline is not actual ordered current-owner render evidence')
        changed=[new for new in tick['after'] if any(same_owner(old,new) and old['minute']!=new['minute'] for old in displayed)]
        notify=any(new not in pending for new in changed)
        require(tick['notify'] is notify, 'Clock missed change or repeated notification without new render')
        pending=changed;notifications+=int(notify)
    require(clock['notifications']==notifications, 'Notification count differs from original ticks')


DRAFT='Profile clock draft \u4e2d\nKeep caf\u00e9 \U0001f642.'
NOTIFICATION_DELTAS=(0,0,1,0,1,None,None,0,0)


def validate(receipt,fixture):
    require(fixture in FIXTURES,'Unknown Profile time fixture')
    rows=receipt.get('stages');viewport=expected_viewport(fixture);hover=fixture.startswith('profile-hover')
    require(receipt.get('ok') is True and receipt.get('noSession') is True and receipt.get('noTransport') is True
            and type(receipt.get('stagesCount')) is int and receipt['stagesCount']==9
            and isinstance(rows,list) and tuple(s.get('stage') for s in rows)==STAGES,'Profile time stage inventory/isolation invalid')
    baseline=rows[0];original=baseline['native']['original'];model0=baseline['model'];native0=baseline['native']
    require(integer(original.get('rootId'),1) and integer(original.get('timeId'),1) and original['rootId']!=original['timeId'], 'Original Profile nodes not identified')
    previous=None
    for i,s in enumerate(rows):
        require(s.get('ok') is True and s.get('noSession') is True and s.get('noTransport') is True
                and type(s.get('ordinal')) is int and s['ordinal']==i,'Profile stage order/isolation changed')
        m=s['model'];n=s['native'];clock=s['clock'];action=s['action'];closed=i==7;replacement=i==8
        require(m.get('now')==EPOCHS[i] and m.get('offset')==OFFSETS[i] and m.get('team')=='T1'
                and m.get('user')==(None if closed else 'U2' if replacement else 'U1'), 'Original member or synthetic boundary differs')
        require(m.get('account')==('profile-clock-replacement' if replacement else model0['account'])
                and m.get('epoch')==model0['epoch']+int(replacement), 'Original account epoch replaced or revived')
        require(m.get('profileUser')==(None if hover or closed else m['user'])
                and m.get('hoverUser')==(m['user'] if hover and not closed else None),'Observed mounted Profile identity differs')
        for k in ('composer','threadComposer','drafts','mainAttachments','threadAttachments','channel','channelGeneration'):
            require(k in m and same(m[k],model0[k]),'Clock changed independent '+k)
        require(m['composer'].get('text')==DRAFT,'Actual input draft missing')
        require(n.get('ok') is True and n.get('viewport')==viewport and n.get('documentWidth')<=viewport[0]+1,'Profile viewport overflow')
        root=n.get('root');time=n.get('time');o=n.get('original');c=n.get('composer')
        require(o.get('rootId')==original['rootId'] and o.get('timeId')==original['timeId']
                and o.get('rootConnected') is (i<7) and o.get('timeConnected') is (i<6)
                and o.get('timeText')==label(EPOCHS[min(i,5)],OFFSETS[min(i,5)]),'Original detached/mounted node custody differs')
        if closed:require(root is None and time is None and n.get('scroll') is None,'Closed Profile remains mounted')
        else:
            require(isinstance(root,dict) and integer(root.get('nodeId'),1) and root.get('connected') is True,'Current Profile root missing')
            require((root['nodeId']!=original['rootId']) if replacement else root['nodeId']==original['rootId'],'Profile root replaced during idle clock update')
            rectangle(root.get('rect'),viewport)
            if i==6:require(time is None,'Absent offset fabricated local time')
            else:
                require(isinstance(time,dict) and integer(time.get('nodeId'),1) and time.get('connected') is True
                        and time.get('text')==label(m['now'],m['offset']),'Actual local-time label differs from independent epoch+offset')
                require((time['nodeId']!=original['timeId']) if replacement else time['nodeId']==original['timeId'],'Time label node replaced during idle update')
                rectangle(time.get('rect'),viewport)
        if i<6:require(same(n.get('scroll'),native0.get('scroll')),'Idle Profile scroll changed')
        require(isinstance(c,dict) and integer(c.get('nodeId'),1) and c.get('connected') is True
                and c.get('value')==DRAFT, 'Current native composer or original draft missing')
        if replacement:
            # Account/epoch participates in the actual composer field key. This
            # is a new editor with preserved data, not preserved native selection.
            require(c['nodeId']!=native0['composer']['nodeId']
                    and integer(c.get('selectionStart')) and integer(c.get('selectionEnd'))
                    and c['selectionStart']<=c['selectionEnd']<=len(DRAFT.encode('utf-16-le'))//2,
                    'Replacement composer did not remount with valid selection')
        else:
            require(c['nodeId']==native0['composer']['nodeId'] and c.get('selectionStart')==2
                    and c.get('selectionEnd')==9, 'Original native composer/caret changed')
        tick_history(clock,previous)
        delta=clock['notifications']-(previous['notifications'] if previous else 0)
        require(delta in (0,1) if i in (5,6) else delta==NOTIFICATION_DELTAS[i], 'Unexpected clock notification for this action')
        if i==0 and action.get('setup') is not None:
            setup_initial_polls(s,rows[1],hover)
        else:
            require(len(clock['ticks'])>=2
                    and all(t['now']==m['now'] and same(t['after'],face(m,hover)) for t in clock['ticks'][-2:]), 'Runtime did not settle two actual polls at expected owner/time')
        writes=[{'now':n} for n in EPOCHS[1:min(i,4)+1]]
        if i>=7:writes.append({'now':EPOCHS[7]})
        require(clock.get('writes')==writes,'Synthetic clock write history differs')
        if i==0:
            require(action.get('ok') is True and action.get('kind')=='synthetic-dom-input' and action.get('isTrusted') is False
                    and action.get('nodeId')==c['nodeId'] and action.get('value')==DRAFT
                    and action.get('selectionStart')==2 and action.get('selectionEnd')==9,'Original input event missing')
            initial_setup(action,m,n,fixture)
        elif i<=4:
            require(set(action)=={'kind','now','afterTick'} and action['kind']=='synthetic-clock' and action['now']==EPOCHS[i]
                    and integer(action['afterTick']) and action['afterTick']>=len(previous['ticks'])
                    and len(clock['ticks'])>=action['afterTick']+2,'Minute transition was not consumed by original runtime after clock-only advance')
        elif i<=6:require(action==dict(kind='synthetic-member-update',user='U1',offset=OFFSETS[i]),'Offset mutation provenance differs')
        elif i==7:require(action==dict(ok=True,kind='synthetic-dom-mouseleave' if hover else 'synthetic-dom-click',isTrusted=False),'Original close event missing')
        else:require(action==dict(kind='synthetic-owner-replacement',user='U2'),'Replacement provenance differs')
        previous=clock
    require(same(receipt.get('model'),rows[-1]['model']) and same(receipt.get('native'),rows[-1]['native'])
            and receipt.get('layout')==dict(ok=True,viewport=viewport),'Terminal current Profile differs')
    tick_history(receipt.get('clock'),previous)
    require(receipt['clock']['notifications']==previous['notifications'] and receipt['clock']['writes']==previous['writes']
            and all(t['notify'] is False and t['after']==face(rows[-1]['model'],hover) for t in receipt['clock']['ticks'][len(previous['ticks']):]),'Late clock mutated retired owner or repeated notification')
