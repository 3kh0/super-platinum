"""Original pending group FIFO, paired native commit, mixed rank and semantic wire proofs."""
import copy
import slack_list_plain_member_save_facts as M
require,exact,R,O=M.require,M.exact,M.R,M.O
SELF,MEMBER=M.SELF,M.MEMBER
GROUP=M.GROUP

def groups(updated=101):
    raw=dict(GROUP,date_update=updated)
    effective=dict(M.EFFECTIVE,date_update=updated)
    return [dict(source=raw,effective=effective)]

def native(updated=101):
    return dict(id=GROUP['id'],team_id='T1',name=GROUP['name'],handle=GROUP['handle'],description='',users=[],members_loaded=updated==102,prefs=dict(channels=[],groups=[]),date_delete=0,label_source=dict(GROUP,date_update=updated))

def observations_for(value,owner,storage,base):
    # The actual group state is checked before mapping only this named mode to
    # the accepted ordinary source contract. Seed remains the original Fetch101.
    v=copy.deepcopy(value);s=v['groupSource'];pending=s['pending'];rows=s['rows']
    require(exact(v['canonical'],None if pending else dict(locale='en-US',membership=[],rows=rows,current=True)), 'Original canonical group availability differs')
    require(exact(v['completion']['groups'],{'unavailable':True} if pending else dict(rows=rows,membership=[],selfRestricted=False)), 'Original completion group projection differs')
    v['canonical']=dict(locale='en-US',membership=[],rows=M.GROUPS,current=True)
    v['completion']['groups']=dict(rows=M.GROUPS,membership=[],selfRestricted=False)
    M.observations_for(v,owner,storage,base)

def group_stage(row,owner,index,rows):
    p=row['plain'];proof=row['proof'];s=p['groupSource'];first=rows[0]['plain']['groupSource']
    pending=index in (2,3);updated=102 if index>=4 else 101;expected=groups(updated)
    require(set(s)=={'account','epoch','team','user','socket','pending','current','rows','membership','native'}
        and s['account']==owner['account'] and s['epoch']==owner['epoch'] and s['team']=='T1' and s['user']==SELF
        and R['integer'](s['socket'],1) and s['socket']==first['socket'], 'Original group owner/socket changed')
    require(s['pending'] is pending and s['current'] is (not pending)
        and exact(s['rows'],None if pending else expected) and exact(s['membership'],None if pending else [])
        and exact(s['native'],native(updated)), 'Original group pending/source/native paired state changed')
    member=p['memberSource'];original=rows[0]['plain']['memberSource']
    require(exact(member['groups'],None if pending else dict(rows=expected,membership=[],current=True)), 'Member snapshot borrowed stale group inventory')
    clean=copy.deepcopy(member);clean['groups']=dict(rows=M.GROUPS,membership=[],current=True)
    M.source_check(clean,owner,original,0)
    require(exact({k:v for k,v in member.items() if k!='groups'},{k:v for k,v in original.items() if k!='groups'}), 'Independent original human source changed')
    ledger=p['members'];require(set(ledger)=={'enabled','lookups','requests','held','repair'} and ledger['enabled'] is True
        and ledger['requests']==[] and ledger['held'] is None and ledger['repair'] is None, 'Group drain invented member network repair')
    if index>=2:
        event=proof['groupEvent']
        require(exact(event,dict(raw=dict(type='subteam_updated',subteam=dict(GROUP,date_update=102)),
            before=rows[1]['plain']['groupSource'],source=rows[2]['plain']['groupSource'],originalCurrent=False,contextCurrent=True)), 'Original held group event/owner/preimage changed')
    if index>=4:
        require(exact(proof['groupRelease'],dict(source=rows[4]['plain']['groupSource'],originalCurrent=False,contextCurrent=True,pairedCommit=True)), 'Old group Snapshot resurrected or original paired commit lost')
    lookups=ledger['lookups'];require(len(lookups)==(2 if index>=5 else 0),'Premature or missing atomic group lookup')
    if index:
        old=rows[index-1]['plain']['members']['lookups'];require(exact(lookups[:len(old)],old),'Original lookup ledger rewritten')
    if index<5:
        require(p['completion']['requests']==[], 'Unavailable group source reached name/rank acquisition')
        return
    source=[r['source'] for r in original['rows']]
    for serial,(entry,query) in enumerate(zip(lookups,('alex-team','not-found')),1):
        require(set(entry)=={'serial','observation'} and entry['serial']==serial,'Original mixed lookup serial changed')
        o=entry['observation']
        require(exact(o['lookup'],dict(kind='member',query=query)) and all(o[k] is True for k in ('sourceCurrent','groupsCurrent','rankCurrent')), 'Original mixed source/rank authority missing')
        require(exact(o['source'],source) and exact(o['groups'],groups(102)), 'Original mixed source rows/order changed')
        require(exact(o['requested'],dict(query=query,options=dict(entities=dict(userGroups=dict(includeFromAnotherTeam=True),members=dict(includeSelf=True,includeSlackbot=False,excludeSalesforceApp=False)),limit=25,sort={'source':'texty-autocomplete'}))), 'Original requested categories changed')
        require(exact(o['namePolicy'],dict(gantry=M.GANTRY,enterprise=None,defaultWorkspace='T1')),'Original current name-policy source changed')
        M.eligibility(o['eligibility'],source);M.renderer(o['options'],query,source,groups(102));M.selectors(o['selectors'],False)
        require(exact(o['results'],[groups(102)[0]['effective']] if serial==1 else []), 'Original effective group result or observed no-match changed')
    require(exact(lookups[0]['observation']['selectors']['navigationHistory'],lookups[1]['observation']['selectors']['navigationHistory']), 'Navigation changed between original lookups')
