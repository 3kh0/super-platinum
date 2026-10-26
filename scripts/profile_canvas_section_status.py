"""Bounded private-to-event-deleted section evidence, with no snippet requests."""
from copy import deepcopy
from profile_channel_title import FILE, require, same
from profile_canvas_unavailable import LOCK_PATH
from profile_canvas_deleted import DELETE_PATH
from profile_canvas_unavailable_source import CONTEXT as PRIVATE_CONTEXT
from profile_canvas_deleted_source import CONTEXT as DELETED_CONTEXT

STAGES = ('private-opened','event-mounted','deleted-reopened','used','confirmed')

def response():
    return dict(ok=False,error='template_not_visible',opaque=dict(sectionStatus=True))

def raw_event():
    return dict(type='file_deleted',file_id=FILE,opaque=dict(sectionStatus=True))

def state(stage):
    return dict(index=STAGES.index(stage),retired=False,held=False,editor=stage!='used',
        source_reply='absent',label='private' if stage in STAGES[:2] else 'deleted',
        section_count=0,file_count=1,cancelled=0,socket=0,saved=stage=='confirmed',can_save=stage=='used',
        model_prefix='Saved with ' if stage in ('used','confirmed') else 'Working with ',
        dom_prefix='Retained draft with ' if stage=='event-mounted' else 'Saved with ' if stage=='confirmed' else 'Working with ')

def presentation(label):
    return dict((PRIVATE_CONTEXT if label=='private' else DELETED_CONTEXT)['token'])

def cache(stage):
    deleted=stage!='private-opened'
    row=dict(id=FILE,mode='tombstone',is_deleted=True) if deleted else dict(id=FILE,filetype='quip',mode='template_not_visible')
    context=DELETED_CONTEXT if deleted else PRIVATE_CONTEXT
    return dict(source=row,effective=deepcopy(row),current=True,reference=dict(status=context['token']['kind'],
        receipt=raw_event() if deleted else response(),presentation=deepcopy(context),
        dependencies=dict(users=[],channels=[],groups=[]),quip=None))

def event():
    def shared(deleted):
        value=cache('event-mounted' if deleted else 'private-opened')
        return dict(rows=[dict(id=FILE,source=value['source'],effective=value['effective'],
            receipt=value['reference']['receipt'],event=raw_event() if deleted else None,
            preimageSource=cache('private-opened')['source'] if deleted else None,
            preimageReceipt=response() if deleted else None,current=True,workspaceDeleted=deleted)],
            revision=int(deleted),queryCount=0)
    return dict(raw=raw_event(),acceptedOwner=True,generation=0,before=shared(False),after=shared(True),scheduledRead=False)

def token_view(value,captured,e,units):
    import math
    require(isinstance(value,dict) and set(value)=={'ok','capture','units','icons','editorEditable'}
        and value['ok'] is True and value['editorEditable'] is True,'Missing enabled surrounding status editor')
    require(same(value['capture'],captured) and same(value['units'],units),'Wrong exact mounted status units')
    require(isinstance(value['icons'],list) and len(value['icons'])==2,'Wrong status icon inventory')
    deleted=e['label']=='deleted'
    for item in value['icons']:
        require(set(item)=={'statusClass','sectionClass','disabled','editable','icon'} and item['statusClass'] is True
            and item['sectionClass'] is False and item['disabled']==str(deleted).lower() and item['editable']=='false',
            'Wrong status precedence or disabled atom capability')
        icon=item['icon']
        require(isinstance(icon,dict) and set(icon)=={'hidden','paths','width','height'} and icon['hidden']=='true'
            and icon['paths']==[DELETE_PATH if deleted else LOCK_PATH],'Wrong actual typed status icon')
        require(all(type(icon[k]) in (int,float) and math.isfinite(icon[k]) and icon[k]>0 for k in ('width','height')),
            'Missing rendered status icon bounds')

def exercise(value,stage,captured,units):
    require(isinstance(value,dict),'Missing status exercise')
    if stage=='used':
        require(same(value,dict(rawTokensPreserved=True,sectionRequests=0)),'Status Use changed raw tokens or fetched snippets')
        return
    token_view(value.get('token'),captured,state(stage),units)
    rest={k:v for k,v in value.items() if k!='token'}
    expected=dict(sectionRequests=0)
    if stage=='event-mounted':expected.update(event=event(),mountedPrivateRetained=True)
    if stage=='confirmed':expected['savedAndReadBack']=True
    require(same(rest,expected),'Wrong status event/preimage/mounted/Save custody')
