"""Original supplied media sources, DOM custody, and closed isolated reply journal."""
import copy
from pathlib import Path
import runpy
from urllib.parse import urlsplit, parse_qs

E=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_edits.py')))
I,R,C,READ,H=(E[k] for k in ('I','R','C','READ','H'))
require=R['require']
CASE='rich-emoji-display-source'
FLAG='SUPER_PLATINUM_LIST_EMOJI_DISPLAY_SOURCE_CHECK'
STAGES=('pending','ready','replacement-pending','replacement-ready')
TS,ROOT,CHANNEL=E['FIRST'],E['ROOT'],E['CHANNEL']
URLS=('https://fixture.invalid/shared-emoji-a.png','https://fixture.invalid/shared-emoji-b.png')
TEXT='Shared :external_original: Standard :wave:'
REPO=Path(__file__).resolve().parent.parent

def blocks(replacement=False):
    return [dict(type='rich_text',block_id='original-external-emoji',opaque={'keep':True},elements=[dict(type='rich_text_section',elements=[
        dict(type='text',text='Shared '),dict(type='emoji',name='external_original',display_alias='shared_party',display_url=URLS[int(replacement)],display_team_id='TFOREIGN001',opaque={'keep':'original appearance'}),
        dict(type='text',text=' Standard '),dict(type='emoji',name='wave')])])]

def page():
    return dict(ok=True,channel=CHANNEL,messages=[dict(ts=ROOT,subtype='list_record_comment',slack_list=dict(list_id=READ['LIST'],list_record_id=READ['KNOWN']),text='Record comment root',unknown={'keep':True}),
        dict(ts=TS,thread_ts=ROOT,user='U1',text=TEXT,blocks=blocks(),reactions=I['source_reactions'](2),display_fixture_opaque={'keep':'received'})],has_more=True,response_metadata=dict(next_cursor=READ['CURSOR']))

def uri(value):
    require(isinstance(value,str),'Missing original opaque URI')
    u=urlsplit(value);q=parse_qs(u.query,strict_parsing=True)
    require(not u.fragment and set(q)=={'v'} and len(q['v'])==1 and q['v'][0].isdigit(),'Missing observed media epoch')
    if u.scheme=='super-platinum-media': asset=u.netloc+u.path
    else:
        require(u.scheme=='http' and u.netloc=='super-platinum-media.emoji','Nonopaque/external image URI')
        asset='emoji'+u.path
    require(asset.startswith('emoji/') and len(asset)>6,'Wrong original emoji asset')
    return asset,int(q['v'][0])

def native(v,ready,original,viewport):
    require(v.get('ok') is True and v.get('ready') is ready,'Native media state not observed')
    reply=v.get('reply',{})
    require(reply==original and all(reply.get(k) is True for k in ('connected','sameNode','sameSelection','editable'))
        and reply.get('composing') is False and reply.get('anchorOffset')==1 and reply.get('focusOffset')==3
        and all(R['integer'](reply.get(k),1) for k in ('node','anchor','focus')),'Original reply node/range/content changed')
    images,spans=v.get('images'),v.get('spans')
    require(isinstance(images,list) and isinstance(spans,list) and len(images)==int(ready) and len(spans)==(1 if ready else 2),'Missing/duplicated alias or sibling')
    expected=[('\U0001f44b',':wave:')] if ready else [(':shared_party:',':shared_party:'),('\U0001f44b',':wave:')]
    require([(s.get('text'),s.get('title')) for s in spans]==expected,'Source alias/standard glyph changed')
    require(v.get('bodyText')==('Shared  Standard \U0001f44b' if ready else 'Shared :shared_party: Standard \U0001f44b'),'Body projection changed')
    for node in images+spans:
        require(node.get('visible') is True and R['integer'](node.get('node'),1),'Original appearance is not exposed')
        R['rectangle'](node.get('rect'),viewport)
    if ready:
        img=images[0];require(img.get('alt')==img.get('title')==':shared_party:' and img.get('complete') is True,'Ready image lost alias/decoding')
        uri(img.get('src'))

def delivery(d,image,replacement):
    index=int(replacement);size=32 if replacement else 16
    canonical=urlsplit(d.get('id',''))
    require(canonical.scheme=='super-platinum-media' and not canonical.query and not canonical.fragment,'Missing canonical registered ID')
    identity=canonical.netloc+canonical.path
    before,epoch=uri(d.get('uriBefore'));after,paint_epoch=uri(image.get('src'))
    require(identity==before==after and epoch==d.get('epochBefore') and paint_epoch>epoch,'Media paint did not follow original registry dirty epoch')
    require(d.get('url')==URLS[index] and d.get('replacement') is replacement and d.get('readyBefore') is False and d.get('readyAfter') is True
        and d.get('originalStillReady') is True and d.get('mime')=='image/png' and d.get('bytes')==(REPO/f'assets/icons/png/icon_{size}.png').stat().st_size,'Original registry bytes/source changed')
    require(image.get('width')==image.get('height')==size,'Wrong decoded original PNG')
    require((d.get('id')!=d.get('originalId')) is replacement,'Same alias reused original asset identity')
    return identity

def validate(fixture,receipt,directory,*,dimensions=None,png=True):
    require(fixture in C['FIXTURES'] and receipt.get('ok') is True and receipt.get('case')==CASE and receipt.get('fixture')==fixture and receipt.get('noLiveNetwork') is True,'Wrong supplied-source receipt')
    ready=receipt.get('readiness',{});H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages',[]);require([r.get('stage') for r in rows]==list(STAGES),'Missing/reordered original stages')
    first=rows[0];initial=first['model'];proof=first['proof'];draft=initial['draftStore']['drafts'][0];journal=first['journal']
    E['A']['rich_content'](proof['seed'],proof['originalA'],E['b_blocks']())
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Original record inventory changed')
    scope=dict(workspace='T1',user='U0',list=READ['LIST'],record=READ['KNOWN'],root=ROOT,created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(proof['seed']['scope']==journal.get('scope')==draft.get('scope')==scope and proof['seed']['reopened'] is True,'Original independent reply scope changed')
    require(journal.get('blocks')==draft.get('blocks')==proof['originalA'] and journal.get('attempt') is None
        and journal.get('capture')==draft.get('capture') and R['uuid'](journal.get('capture'))
        and journal.get('revision')==draft.get('savedRevision') and R['integer'](journal.get('revision'),1),'Independent durable reply does not match native')
    source=first['source'];reply=first['native']['before']['reply'];viewport=first['geometry']['viewport']
    require(reply['blocks']==proof['originalA'],'Native original reply differs from durable blocks')
    raw=page();root,original=map(I['typed'],raw['messages']);changed=E['typed_edit'](original,TEXT,blocks(True),None,None)
    for i,row in enumerate(rows):
        m,g,p,c=row['model'],row['geometry'],row['proof'],row['model']['comments']
        require(R['uuid'](m.get('account')) and m.get('account')==initial['account'] and m.get('epoch')==initial['epoch']
            and m.get('noSession') is True and m.get('noTransport') is True and m.get('writes')==m.get('networkCalls')==0,'Closed original owner changed')
        require(m['document']==m['initialDocument']==initial['initialDocument'] and m['records']==m['initialRecords']==touched and m['unrelated']==m['initialUnrelated']==unrelated and m['detail']==initial['detail'],'Canonical records changed')
        require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==ROOT and c.get('loading') is False and c.get('error') is None
            and c.get('hasMore') is True and c.get('reveal') is None and c.get('deleted')==[]
            and all(c.get(k)==initial['comments'].get(k) for k in ('read','generation','requestTs')),'Original comments owner/read changed')
        require(c.get('page')==raw and c.get('pageMessages')==[root,changed if i>=2 else original]
            and c.get('live')==c.get('accepted')==c.get('liveTs')==c.get('acceptedTs')==c.get('editCustody')==c.get('liveReactions')==[]
            and c.get('visibleTs')==[TS] and c.get('reactions')==[dict(ts=TS,reactions=[dict(name='wave',count=2)])],'Original raw/typed source or reaction projection changed')
        ledger=m['ledger'];C['validate_reads'](ledger['requests'])
        require(ledger==initial['ledger'] and len(ledger['requests'])==4 and ledger.get('completed')==4 and ledger.get('cancelled')==0 and ledger.get('held') is False,'Source appearance issued an unrelated read')
        post=m['composeLedger']
        require(post==initial['composeLedger'] and all(post.get(k)==[] for k in ('prepared','posts','messages','preflights','policies')) and post.get('heldPost') is False and post.get('published') is False,'Unexpected send/policy/mutation')
        require(m['draftStore']==initial['draftStore'] and len(m['draftStore']['drafts'])==1 and row['journal']==journal
            and draft.get('native') is True and all(draft.get(k) is False for k in ('busy','saving','loading','captureFailed','terminalPending'))
            and draft.get('error') is None and draft.get('attempt') is None,'Original draft custody changed')
        require(p['seed']==proof['seed'] and p['originalA']==proof['originalA'] and p['arm']==proof['arm'],'Original input proof rewritten')
        require(g.get('ok') is True and all(g.get(k) is True for k in ('noOverflow','dialog','comments','compose')) and g.get('viewport')==viewport,'Original native surface changed')
        e=g['editor'];require(e.get('sameNode') is True and e.get('editable') is True and e.get('visible') is True and e.get('bold') is True and e.get('blocks')==proof['originalA'] and e.get('text')==R['visible_text'](proof['originalA']),'Original rich editor changed')
        R['rectangle'](e['rect'],viewport)
        controls=g['controls'];require(len(controls)==4 and {x['id'] for x in controls}=={'slack-list-comments-back','slack-list-comments-refresh','slack-list-comments-next','slack-list-comment-send'},'Original controls changed')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True and control.get('disabled') is False,'Original control covered/disabled');R['rectangle'](control['rect'],viewport)
        n=row['native'];require(n['before']==n['after'] and n['before']['geometry']==g,'Capture changed original DOM')
        native(n['before'],i%2==1,reply,viewport)
        require(n['before']['row']==first['native']['before']['row'] and n['before']['body']==first['native']['before']['body'],'Original transcript identity replaced')
        s=row['source'];require(s.get('named') is True and s.get('noSession') is True and s.get('noTransport') is True and s.get('customEmoji')==source['customEmoji'] and s.get('customRevision')==source['customRevision'],'Private display source leaked into global catalog')
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row['capture'])==path,'Foreign original capture')
        if png:H['png_signature'](path,dimensions)
    a=delivery(rows[1]['proof']['mediaA'],rows[1]['native']['before']['images'][0],False)
    b=delivery(rows[3]['proof']['mediaB'],rows[3]['native']['before']['images'][0],True)
    require(a!=b and rows[3]['proof']['mediaB']['originalId']==rows[1]['proof']['mediaA']['id'],'Replacement aliased old bytes')
    for row in rows[1:]:require(row['proof']['mediaA']==rows[1]['proof']['mediaA'],'Original A receipt changed')
    frame=E['changed'](TS,TEXT,blocks=blocks(True))
    events=E['expected_events'](rows,[('replacement','T1',7,frame,True,1,2)])
    require(receipt.get('editEvents')==events and all(row['proof'].get('replacement')==events[0] for row in rows[2:]),'Original replacement event witness changed')
    require(receipt.get('final')==rows[-1]['model'],'Final original model changed')
    return receipt

def validate_closed(value,receipt):
    last=receipt['stages'][-1]
    require(set(value)=={'account','inventories','editInventories','editHistory','contentSha256'} and value['account']==last['model']['account']
        and value['inventories']==[last['journal']] and value['editInventories']==value['editHistory']==[],'Closed original SQLite lost reply or gained edit/send state')
    require(isinstance(value['contentSha256'],str) and len(value['contentSha256'])==64 and all(c in '0123456789abcdef' for c in value['contentSha256']),'Closed SQLite fingerprint missing')
