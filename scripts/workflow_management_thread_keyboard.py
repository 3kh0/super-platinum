"""Bounded explicitly requested owned-window Enter for named fixture controls."""
import math
from workflow_management_delete import require, exact

PREFIX='workflow-thread-keyboard: '

def admit(value,expected,enabled,*,share_drafts=False):
    require((enabled or share_drafts) and not (enabled and share_drafts) and expected['fixture']=='workflow-editor-narrow','Unrequested thread keyboard activation')
    require(isinstance(value,dict) and set(value)=={'version','fixture','nonce','pid','nodeId','key','label'},'Unexpected keyboard request shape')
    require(all(type(value.get(k)) is type(v) and value[k]==v for k,v in expected.items()),'Keyboard request lost owned process/nonce')
    labels=('Finish Up','Messages') if share_drafts else ('Workflows',)
    require(type(value.get('nodeId')) is int and value['nodeId']>0 and value.get('key')=='Enter' and value.get('label') in labels,'Wrong original keyboard target/key')


def admit_requests(requests,expected,enabled,*,share_drafts=False):
    require(isinstance(requests,list) and len(requests)<=(2 if share_drafts else 1),'Repeated keyboard request')
    for request in requests:admit(request,expected,enabled,share_drafts=share_drafts)
    if share_drafts:
        labels=[r['label'] for r in requests]
        require(labels==[label for label in ('Finish Up','Messages') if label in labels],
                'Repeated or reordered Share keyboard request')


def trusted_action(action,label,node=None):
    require(isinstance(action,dict) and action.get('ok') is True and action.get('kind')=='trusted-keyboard'
            and action.get('label')==label and action.get('key')=='Enter' and action.get('hitPoint') is None,
            'Wrong original trusted keyboard action')
    identity=action.get('nodeId')
    require(type(identity) is int and identity>0 and (node is None or identity==node),'Keyboard node identity changed')
    box=action.get('rect',{})
    require(all(type(box.get(k)) in (int,float) and math.isfinite(box[k]) for k in ('left','top','right','bottom','width','height'))
            and box['width']>0 and box['height']>0,'Keyboard control geometry absent')
    require(exact(action.get('keydown'),dict(trusted=True,key='Enter',code='Enter',repeat=False,targetNodeId=identity,focused=True,connected=True,enabled=True)),
            'Original trusted focused keydown absent')
    require(exact(action.get('click'),dict(trusted=True,targetNodeId=identity,detail=0,connected=True,enabled=True)),
            'Original trusted keyboard click absent')

def validate_requests(requests,expected,receipt,enabled,*,share_drafts=False):
    admit_requests(requests,expected,enabled,share_drafts=share_drafts)
    rows=receipt.get('stages',[])
    if share_drafts:
        require(receipt.get('shareDraftCustody') is True,'Missing explicit Share draft keyboard mode')
        finish=next((r for r in rows if r.get('stage')=='finish'),{})
        actions=[('Finish Up',finish.get('proof',{}).get('exercise',{}).get('draftOpen',{})),
                 ('Messages',receipt.get('shareDrafts',{}).get('returned',{}).get('messagesActivation',{}))]
        used=[(label,action) for label,action in actions if action.get('kind')=='trusted-keyboard']
        require(len(requests)==len(used),'Share keyboard request and actual activation differ')
        for request,(label,action) in zip(requests,used):
            require(request['label']==label,'Share keyboard label/order changed')
            trusted_action(action,label,request['nodeId'])
        return
    browse=next((r for r in rows if r.get('stage')=='browse'),{})
    action=browse.get('proof',{}).get('exercise',{}).get('threadSetup',{}).get('tabActivation',{})
    used=action.get('kind')=='trusted-keyboard'
    require(len(requests)==int(used),'Keyboard request and actual activation differ')
    if used:
        require(action.get('nodeId')==requests[0]['nodeId'] and action.get('label')=='Workflows' and action.get('key')=='Enter','Native keyboard witness targets another node')

def completed(data,requests=None,*,share_drafts=False):
    require(isinstance(data,dict),'Keyboard session evidence missing')
    batches=data.get('batches',[])
    count=len(requests) if share_drafts and isinstance(requests,list) else 1
    require(1<=count<=(2 if share_drafts else 1) and len(batches)==count and
            all(b==dict(action='enter',batch=0,requested=2,inserted=2) for b in batches),'Enter was repeated/partially inserted')
    restoration=data.get('foregroundRestoration',{})
    require(restoration.get('observerClean') is True and not restoration.get('interruption') and restoration.get('heldModifiers')==[],'Keyboard observer/held-key cleanup failed')
