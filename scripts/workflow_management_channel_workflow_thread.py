"""Original native thread and in-memory draft custody; no disk/send claim."""
from workflow_management_delete import exact, require, integer
import workflow_management_channel_workflows as W

ROOT='1781524900.000001'
REPLY='1781524901.000002'
DRAFT='Unsent workflow review 中\nKeep café details 🙂.'
KEY=dict(team='TWORKFLOW',channel=W.A.DEST,thread=ROOT)

def channels():
    rows=W.channels()
    rows[0]['properties']['tabz']=[dict(id='workflows',type='workflows',label='')]
    return rows

def message(ts,text,reply=False):
    row={k:None for k in ('user','bot_id','username','bot_profile','icons','type','subtype','ts','client_msg_id','text','team','channel','thread_ts','parent_user_id','reply_count','reply_users_count','latest_reply','edited','message')}
    row.update({k:[] for k in ('reply_users','reactions','blocks','files','attachments')})
    row.update(type='message',ts=ts,channel=W.A.DEST,user='UWORKFLOW',text=text,thread_ts=ROOT)
    if not reply:row.update(reply_count=1,latest_reply=REPLY)
    return row

PARENT=message(ROOT,'Original workflow discussion.')
RESPONSE=message(REPLY,'Keep this discussion while inspecting workflow details.',True)

def population(model):
    require(isinstance(model,dict) and model.get('root')==ROOT,'Original thread fixture missing')
    require(model.get('historyLoaded') is True and model.get('threadLoaded') is True,'Closed history not loaded')
    require(exact(model.get('history'),[PARENT]) and exact(model.get('thread'),[PARENT,RESPONSE]),'Original history/thread rows changed or message was sent')
    require(model.get('historyPending')==[] and model.get('threadPending')==[] and model.get('attachments')==0,'Thread unexpectedly queued a send or attachment')
    require(exact(model.get('draftKey'),KEY),'Original thread draft destination changed')
    integer(model.get('threadGeneration'),'Original thread generation',0)

def composer(value,text):
    require(isinstance(value,dict) and value.get('text')==text and value.get('reply_broadcast') is False,'Original unsent composer text/destination intent changed')
    selection=value.get('selection',{})
    start,end=selection.get('start'),selection.get('end')
    integer(start,'Composer byte selection',0);integer(end,'Composer byte selection',start)
    boundaries={len(text[:i].encode('utf-8')) for i in range(len(text)+1)}
    require(start in boundaries and end in boundaries,'Composer selection is not a valid original byte boundary')

def model_proof(model,active,typed,baseline=None):
    population(model)
    require(model.get('activeRoot')==(ROOT if active else None) and model.get('loading') is False,'Actual thread lifecycle did not settle')
    require(exact(model.get('activeDraftKey'),KEY if active else None),'Active thread draft binding changed')
    composer(model.get('threadComposer'),DRAFT if typed and active else '')
    expected_rows=[dict(id=ts,ts=ts,pending=False) for ts in (ROOT,REPLY)] if active else []
    require(exact(model.get('renderedRows'),expected_rows),'Original thread transcript projection changed')
    draft=model.get('storedDraft')
    if typed:
        require(isinstance(draft,dict) and draft.get('attachments')==[],'Unsent keyed draft absent')
        composer(draft.get('composer'),DRAFT)
        if active:require(exact(draft['composer'],model['threadComposer']),'Stored draft differs from actual composer')
        if baseline:require(exact(draft,baseline['storedDraft']),'Original keyed draft changed during pane interaction')
    else:require(draft is None,'Fixture seeded an unsent draft instead of native input')
    if baseline and active:
        for key in ('threadGeneration','scope','threadComposer','activeDraftKey','storedDraft','renderedRows'):
            require(exact(model[key],baseline[key]),'Details changed original thread '+key)

def native_proof(native,active,hidden=False,baseline=None,typed=True):
    require(isinstance(native,dict),'Original native thread observation absent')
    if not active:
        require(all(native.get(k) is None for k in ('pane','body','composer')),'Retired thread DOM remains')
        require(native.get('originalNodes') is False,'Absent thread claimed original nodes')
        if baseline:require(native.get('savedDetached') is True,'Original thread nodes not retired on About navigation')
        return
    nodes=[]
    for key,identity in (('pane',''),('body','thread-body'),('composer','thread-composer')):
        node=native.get(key,{})
        require(node.get('id')==identity and node.get('connected') is True,'Original '+key+' node missing')
        integer(node.get('nodeId'),'Original '+key+' node identity',1);nodes.append(node['nodeId'])
        require(node.get('displayed') is (not hidden),'Original thread visibility differs from pane state')
        rect=node.get('rect',{})
        require((rect.get('width')==0 and rect.get('height')==0) if hidden else rect.get('width',0)>0 and rect.get('height',0)>0,'Original thread geometry inconsistent with visibility')
        if baseline:require(node['nodeId']==baseline[key]['nodeId'],'Details replaced original '+key+' node')
    require(len(set(nodes))==3,'Thread pane/body/composer reused identity')
    require(native['pane']['display']==('none' if hidden else 'flex'),'Thread pane display state changed')
    field=native['composer']
    require(field.get('value')==(DRAFT if typed else '') and field.get('readonly') is False and field.get('disabled') is False,'Actual thread field changed')
    require(isinstance(field.get('draftScope'),str) and field['draftScope'] and isinstance(field.get('projectionVisit'),str) and field['projectionVisit'],'Mounted composer scope absent')
    rows=native['body'].get('rows',[])
    require([r.get('id') for r in rows]==[ROOT,REPLY] and all(text in native['body'].get('text','') for text in (PARENT['text'],RESPONSE['text'])),'Original native thread messages absent')
    if typed:
        require((field.get('selectionStart'),field.get('selectionEnd'),field.get('selectionDirection'))==(7,22,'forward'),'Native Unicode draft selection changed')
        require(native.get('originalNodes') is True and native.get('savedDetached') is False,'Original typed thread mount replaced')
    if baseline:
        for key in ('value','selectionStart','selectionEnd','selectionDirection','draftScope','projectionVisit'):
            require(exact(field[key],baseline['composer'][key]),'Original native composer custody changed: '+key)
        require(exact(rows,baseline['body']['rows']) and native['body']['text']==baseline['body']['text'],'Original mounted transcript changed')

def action_proof(action,kind,viewport,label=None,node=None):
    require(isinstance(action,dict) and action.get('ok') is True and action.get('kind')==kind,'Actual original activation absent')
    integer(action.get('nodeId'),'Activated original node',1)
    if node is not None:require(action['nodeId']==node,'Activation targeted a different original node')
    if label is not None:require(action.get('label')==label,'Wrong original control activated')
    box=action.get('rect',{});point=action.get('hitPoint',{})
    for k in ('left','right','top','bottom','width','height'):
        require(type(box.get(k)) in (int,float),'Missing original activation rectangle')
    x,y=point.get('x'),point.get('y')
    require(type(x) in (int,float) and type(y) in (int,float) and box['width']>0 and box['height']>0 and 0<=x<viewport[0] and 0<=y<viewport[1] and box['left']<=x<=box['right'] and box['top']<=y<=box['bottom'],'Original control had no in-viewport hitpoint')

def validate(receipt,viewport,rectangle):
    rows=receipt['stages'];browse=rows[0];setup=browse['proof']['exercise'].get('threadSetup')
    require(isinstance(setup,dict),'Native Reply/input setup absent')
    require(set(setup)=={'before','messagesActivation','messages','replyActivation','opened','inputActivation','drafted','tabActivation','after'},'Original thread setup inventory changed')
    before,messages,opened,drafted,after=(setup[k] for k in ('before','messages','opened','drafted','after'))
    for obs in (before,messages):
        model_proof(obs['model']['detailsThread'],False,False)
        native_proof(obs['native']['detailsThread'],False)
    model_proof(opened['model']['detailsThread'],True,False)
    native_proof(opened['native']['detailsThread'],True,typed=False)
    model_proof(drafted['model']['detailsThread'],True,True)
    native_proof(drafted['native']['detailsThread'],True)
    model_proof(after['model']['detailsThread'],True,True)
    native_proof(after['native']['detailsThread'],True,baseline=drafted['native']['detailsThread'])
    require(exact(drafted['model']['detailsThread']['storedDraft'],after['model']['detailsThread']['storedDraft']),'Workflows reentry changed typed draft')
    require(after['model']['detailsThread']['threadGeneration']==opened['model']['detailsThread']['threadGeneration'],'Tab reentry reopened thread')
    action_proof(setup['messagesActivation'],'dom-click',viewport,'Messages')
    action_proof(setup['replyActivation'],'dom-click',viewport,'Reply')
    require(setup['replyActivation'].get('messageId')==ROOT,'Reply targeted another message')
    action_proof(setup['inputActivation'],'dom-input',viewport,node=opened['native']['detailsThread']['composer']['nodeId'])
    field=drafted['native']['detailsThread']['composer']
    require(all(exact(setup['inputActivation'].get(k),field[k]) for k in ('nodeId','value','selectionStart','selectionEnd','selectionDirection')),'Native input differs from actual typed field')
    action_proof(setup['tabActivation'],'dom-click',viewport,'Workflows',after['native']['workflows']['tab']['nodeId'])
    # Keep the original request pair and require exactly the second pair that
    # belongs to reentry; same account/socket and two actual tab transitions.
    first=before['server']['channelWorkflows'];last=after['server']['channelWorkflows']
    W.ledger('first-tab-success','workflows',first)
    W.ledger('first-tab-success','refreshed',last)
    for key in ('requests','responses','settlements'):
        require(exact(last[key][:2],first[key]),'Thread setup rewrote original browse receipts')
    destinations=[o['model']['confirmationDestination'] for o in (before,messages,opened,drafted,after)]
    require([d['tab'] for d in destinations]==['Workflows','Messages','Messages','Messages','Workflows'],'Original thread setup tab path changed')
    generation=destinations[0]['channelGeneration']
    require([d['channelGeneration'] for d in destinations]==[generation,generation+1,generation+1,generation+1,generation+2],'Original tab navigation generations changed')
    for d in destinations:
        require(d['channel']==W.A.DEST and d['team']=='TWORKFLOW','Thread setup left original destination')
    baseline=browse['model']['detailsThread'];native=browse['proof']['native']['detailsThread']
    require(exact(baseline,after['model']['detailsThread']),'Browse did not retain actual setup model')
    for row in receipt['setup']:
        model_proof(row['model']['detailsThread'],False,False)
        native_proof(row['proof']['native']['detailsThread'],False)
    for row in rows:
        returned=row['stage']=='returned';m=row['model']['detailsThread'];n=row['proof']['native']['detailsThread']
        model_proof(m,not returned,True,baseline)
        native_proof(n,not returned,row['proof']['native'].get('details') is not None,native)
    require(receipt.get('threadCustody') is True,'Thread evidence not explicitly requested')
