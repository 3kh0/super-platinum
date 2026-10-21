"""Independent info/get literals and original sender accounting; no execution grant."""
import copy
from workflow_management_delete import exact, require, integer
import workflow_management_channel_workflows as W

SETUP=W.PREFIX
STAGES={
 'first-details-success':('browse','details-a','closed','details-b','closed-b','returned'),
 'first-details-errors':('browse','denied','not-found','failed','recovered','closed','returned'),
 'first-details-close-held':('browse','info-held','closed-info','late-info','get-held','closed-get','late-get','returned'),
 'first-details-supersession':('browse','a-held','b-ready','late-a','b-held','channel-successor','late-b','returned'),
 'first-details-account-retirement':('browse','held','successor','late-settled'),
 'first-details-rate-limit':('browse','rate-limited','closed','reopened-blocked','expired','recovered','closed-final','returned'),
}
COUNTS={
 'first-details-success':(0,2,2,4,4,4),
 'first-details-errors':(0,1,2,4,6,6,6),
 'first-details-close-held':(0,1,1,1,3,3,3,3),
 'first-details-supersession':(0,1,3,3,5,5,5,5),
 'first-details-account-retirement':(0,1,1,1),
 'first-details-rate-limit':(0,1,1,1,1,3,3,3),
}
INFO='workflows.triggers.info';GET='functions.workflows.get';A='TR_REVIEW';B='FtFolder01'
def workflow_id(trigger):return 'WfREVIEW01' if trigger==A else 'WfFOLDER01'
def trigger_raw(trigger):
    a=trigger==A
    return dict(ok=True,trigger=dict(id=trigger,type='shortcut',name='Info review \u4e2d' if a else '',description='Original trigger description' if a else '',owning_team_id='TWORKFLOW',
      workflow=dict(workflow_id=workflow_id(trigger),title='Inline workflow title' if a else '',description='Inline workflow description' if a else '',app_id='A_HOME',app=dict(id='A_HOME',name='Review requests')),
      workflow_details=dict(collaborators=['UWORKFLOW'],steps=[dict(id='inline',type='function',function=dict(title='Inline step must be replaced'))]),opaque=dict(original=True,order=[3,1])),opaque_receipt='original-details-info')
def workflow_raw(trigger):
    return dict(ok=True,workflow=dict(id=workflow_id(trigger),title='Full review \u4e2d' if trigger==A else 'Folder details caf\u00e9',description='Actual complete workflow description \u00e9' if trigger==A else 'Full folder workflow description',collaborators=['UWORKFLOW'],steps=[dict(id='step-original',type='function',function=dict(title='Review the original request'))],opaque_original=dict(retained=[False,7])),opaque_receipt='original-details-workflow')
def projection(trigger):
    t=trigger_raw(trigger)['trigger'];w=workflow_raw(trigger)['workflow']
    return dict(title=t['name'] or t['workflow']['title'] or w['title'],description=t['description'] or t['workflow']['description'] or w['description'],triggerType='shortcut',workflowId=w['id'],app=t['workflow']['app'],owningTeam='TWORKFLOW',collaborators=w['collaborators'],steps=w['steps'])
def fields(method,trigger):
    return dict(trigger_id=trigger,include_workflow_details='true',_x_reason='hermes_triggers_ufm') if method==INFO else dict(workflow_id=workflow_id(trigger),_x_reason='hermes_workflows_store_unknown_fetching')
def plan(case):
    return {
      'first-details-success':[(INFO,A),(GET,A),(INFO,B),(GET,B)],
      'first-details-errors':[(INFO,A),(INFO,A),(INFO,A),(GET,A),(INFO,A),(GET,A)],
      'first-details-close-held':[(INFO,A),(INFO,A),(GET,A)],
      'first-details-supersession':[(INFO,A),(INFO,B),(GET,B),(INFO,B),(GET,B)],
      'first-details-account-retirement':[(INFO,A)],
      'first-details-rate-limit':[(INFO,A),(INFO,A),(GET,A)],
    }[case]
def result(case,n,method,trigger):
    if case=='first-details-errors' and n in (1,2):return dict(raw=dict(ok=False,error='access_denied' if n==1 else 'trigger_not_found',opaque_receipt=f'original-details-error-{n}'))
    if case=='first-details-errors' and n==4:return dict(error='offline')
    if case=='first-details-rate-limit' and n==1:return dict(error='rate-limited',retryAfterSeconds=12)
    return dict(raw=trigger_raw(trigger) if method==INFO else workflow_raw(trigger))
def holds(case):
    return {'first-details-close-held':[(1,'info-held','closed-info','late-info'),(3,'get-held','closed-get','late-get')],
      'first-details-supersession':[(1,'a-held','b-ready','late-a'),(5,'b-held','channel-successor','late-b')],
      'first-details-account-retirement':[(1,'held','successor','late-settled')]}.get(case,[])
def state(case,stage):
    if stage in SETUP:return 0,[],[],[]
    index=STAGES[case].index(stage);count=COUNTS[case][index];pending=[];cancelled=[];released=[]
    for serial,start,retired,late in holds(case):
        begin,end,release=(STAGES[case].index(s) for s in (start,retired,late))
        if begin<=index<end:pending.append(serial)
        if index>=end:cancelled.append(serial)
        if begin<=index<release:released.append(dict(serial=serial,receiverRetired=index>=end))
    return count,pending,cancelled,released
def selection(value,trigger,browse_owner=None,thread_root=None):
    require(isinstance(value,dict),'Original details selection absent')
    integer(value.get('id'),'Selection identity',1);integer(value.get('browseSerial'),'Browse serial',1)
    require(value.get('triggerId')==trigger and value.get('group')==('featured' if trigger==A else 'bookmarked') and value.get('index')==0,'Original available row changed')
    f,b=W.rows();row=(f if trigger==A else b)[0]
    require(exact(value.get('source'),row['source']) and exact(value.get('previewRaw'),row['previewRaw']),'Row origin raw changed')
    require(value.get('context')==dict(location='bookmark',channelId=W.A.DEST,bookmarkId=trigger),'Original bookmark context fabricated')
    require(value.get('threadRoot')==thread_root and all(value.get(k) is None for k in ('profileUser','groupId')),'Unexpected synthetic secondary owner')
    owner=value.get('owner',{});integer(owner.get('id'),'Browse owner ID',1)
    require(owner.get('account')==W.F.OWNER['account'] and owner.get('accountEpoch')==W.F.OWNER['epoch'] and owner.get('team')=='TWORKFLOW' and owner.get('channel')==W.A.DEST,'Original account/team/channel lost')
    integer(owner.get('socket'),'Original source socket generation',0)
    integer(owner.get('channelGeneration'),'Channel generation',1)
    if browse_owner is not None:require(exact(owner,browse_owner),'Original browse owner changed')
def ledger(case,stage,value,browse_owner=None,thread_root=None):
    require(isinstance(value,dict) and value.get('networkCalls')==0,'Missing closed details ledger')
    count,pending,cancelled,held=state(case,stage)
    req,res,settle,release=(value.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(v,list) for v in (req,res,settle,release)) and len(req)==len(res)==count,'Details exact request count/replay')
    for n,((method,trigger),r,response) in enumerate(zip(plan(case),req,res),1):
        require(r.get('serial')==response.get('serial')==n and r.get('method')==method and r.get('retrySafe') is False,'Details original factory/serial')
        args=r.get('fields');require(isinstance(args,list) and len(args)==len(fields(method,trigger)) and dict(args)==fields(method,trigger),'Details exact body/reason')
        selection(r.get('owner'),trigger,browse_owner,thread_root);integer(r.get('attempt'),'Original details attempt serial',1)
        require(exact(response.get('result'),result(case,n,method,trigger)),'Details raw/error was rewritten')
    wanted={n:n not in cancelled for n in range(1,count+1) if n not in pending}
    require(len(settle)==len(wanted) and {r.get('serial'):r.get('returned') for r in settle}==wanted and all(type(r.get('returned')) is bool for r in settle),'Actual details settlement/cancellation')
    require(exact(value.get('held'),held),'Original held receiver custody')
    released=[dict(serial=n,receiverRetired=True,delivered=False) for n,_,_,late in holds(case) if stage not in SETUP and STAGES[case].index(stage)>=STAGES[case].index(late)]
    require(exact(release,released),'Original late response delivered or replayed')
    return req
