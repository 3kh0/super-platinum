"""Exact colliding source, ordered history string, and real mixed candidates."""
import copy,json
import slack_list_plain_member_save_facts as M
require,exact,R,O=M.require,M.exact,M.R,M.O
SELF,MEMBER=M.SELF,M.MEMBER
GROUP=dict(M.GROUP,handle='harbor');EFFECTIVE=dict(M.EFFECTIVE,handle='harbor');GROUPS=[dict(source=GROUP,effective=EFFECTIVE)]
CLOCK='SystemTime::now captured once by the offline fixture'

def history_wire(case,time):
    return '{"harbor":[{"id":"ULISTMEM1","count":1,"visits":['+str(time)+']},{"id":"SLISTGRP1","count":'+('10' if case.endswith('-group') else '1')+',"visits":['+str(time)+']}]}'

def mixed_proof(v,case,wire):
    require(set(v)=={'enabled','case','capturedAtMillis','historyWire','clock','noLiveNetwork'} and v['enabled'] is True and v['case']==case and v['clock']==CLOCK and v['noLiveNetwork'] is True,'Original fixture history provenance changed')
    time=v['capturedAtMillis'];require(type(time) is int and 1_000_000_000_000<=time<100_000_000_000_000,'Actual millisecond visit missing or converted to seconds')
    require(type(wire) is str and v['historyWire']==wire==history_wire(case,time),'Fresh original encoded history/order/count/visits changed')

def observations_for(value,owner,storage,base,case,wire):
    mixed_proof(value['mixed'],case,wire)
    v=copy.deepcopy(value);c=v['completion'];seed=c['seed']
    require(exact(v['canonical'],dict(locale='en-US',membership=[],rows=GROUPS,current=True)) and exact(c['groups'],dict(rows=GROUPS,membership=[],selfRestricted=False)),'Actual colliding group canonical snapshot changed')
    require(exact(seed['groups'],dict(incoming=[GROUP],rows=GROUPS,membership=[])),'Original colliding group seed changed')
    require(v['preferences']['prefs'].get('frecency')==wire and seed['boot']['prefs'].get('frecency')==wire,'Fresh complete preferences do not retain original encoded history')
    # Only validated group-handle differences are mapped into unchanged source helpers.
    v['canonical']['rows']=M.GROUPS;c['groups']['rows']=M.GROUPS;seed['groups']=dict(incoming=[M.GROUP],rows=M.GROUPS,membership=[])
    M.observations_for(v,owner,storage,base)

def selectors(value,matched):
    # Retain the original helper's complete selector schema, with the actual
    # unchanged Boot101 human (no recovery) when matching harbor.
    private=copy.deepcopy(value)
    if matched:
        expected=M.selector_row(M.canonical(M.raw_member()))
        require(exact(private['members'].get(MEMBER),expected),'Rank borrowed repaired/foreign human')
        private['members'][MEMBER]=M.selector_row(M.repaired())
    M.selectors(private,matched)

def renderer(v,query,source):
    require(v['rawQuery']==query and v['formattedQuery']==query and v['locale']=='en-US' and exact(v['query'],dict(normalized=query,matcherNormalized=query)),'Original mixed query changed')
    require(exact(v['originalMembers'],source) and exact(v['members'],[M.derived(r) for r in source]) and exact(v['self'],M.derived(M.USER)) and exact(v['groups'],GROUPS),'Actual renderer mixed inputs changed')
    texts={'',query,'harbor','Harbor Person','harbor@fixture.invalid','fixture-self','username-must-not-win','Alex group SLISTGRP1','alex group slistgrp1','@harbor'}
    fields=v['fields'];require(len(fields)==len(texts) and {r['text'] for r in fields}==texts and all(set(r)=={'text','lowered','localeLowered'} and r['lowered']==r['localeLowered']==r['text'].lower() for r in fields),'Original colliding renderer fields changed')
    classes=v['classes'];require(set(classes)=={'fixture-self','harbor','Alex group SLISTGRP1','alex group slistgrp1'} and all(type(x) is int and x>=0 for x in classes.values()) and classes['alex group slistgrp1']<classes['Alex group SLISTGRP1']<classes['fixture-self']<classes['harbor'],'Original mixed collation changed')
    # Validate unchanged toggle/self/name proof with original helper after exact
    # colliding-input checks, rather than changing accepted baseline assertions.
    w=copy.deepcopy(v);w['groups']=M.GROUPS
    expected={'',query,'harbor','Harbor Person','harbor@fixture.invalid','fixture-self','username-must-not-win','Alex group SLISTGRP1','alex group slistgrp1','alex-team','@alex-team'}
    w['fields']=[dict(text=t,lowered=t.lower(),localeLowered=t.lower()) for t in expected]
    w['classes']={n:i for i,n in enumerate(['alex group slistgrp1','Alex group SLISTGRP1','alex-team','fixture-self','harbor'])}
    M.renderer(w,query,source,M.GROUPS)

def mixed_stage(row,owner,index,rows,case,wire):
    p=row['plain'];mixed_proof(p['mixed'],case,wire);require(exact(p['mixed'],rows[0]['plain']['mixed']),'Captured history rewritten')
    source=p['memberSource'];first=rows[0]['plain']['memberSource'];clean=copy.deepcopy(source)
    require(exact(source['groups'],dict(rows=GROUPS,membership=[],current=True)),'Current colliding group missing')
    clean['groups']=dict(rows=M.GROUPS,membership=[],current=True);M.source_check(clean,owner,first,0)
    require(exact(source,first),'Original member/group source changed during Save')
    ledger=p['members'];require(set(ledger)=={'enabled','lookups','requests','held','repair'} and ledger['enabled'] is True and ledger['requests']==[] and ledger['held'] is None and ledger['repair'] is None,'Unexpected member repair or source reads')
    lookups=ledger['lookups'];require(len(lookups)==(2 if index>=2 else 0),'Missing/premature mixed lookup')
    if index:
        old=rows[index-1]['plain']['members']['lookups'];require(exact(lookups[:len(old)],old),'Original lookup history rewritten')
    if index<2:require(p['completion']['requests']==[],'Premature name/rank read');return
    originals=[r['source'] for r in source['rows']]
    for serial,(entry,query) in enumerate(zip(lookups,('harbor','not-found')),1):
        require(set(entry)=={'serial','observation'} and entry['serial']==serial,'Original lookup serial changed')
        o=entry['observation'];require(exact(o['lookup'],dict(kind='member',query=query)) and all(o[k] is True for k in ('sourceCurrent','groupsCurrent','rankCurrent')),'Mixed lookup lost original authority')
        require(exact(o['source'],originals) and exact(o['groups'],GROUPS),'Original mixed raw source/order changed')
        require(exact(o['requested'],dict(query=query,options=dict(entities=dict(userGroups=dict(includeFromAnotherTeam=True),members=dict(includeSelf=True,includeSlackbot=False,excludeSalesforceApp=False)),limit=25,sort={'source':'texty-autocomplete'}))),'Mixed original request categories changed')
        require(exact(o['namePolicy'],dict(gantry=M.GANTRY,enterprise=None,defaultWorkspace='T1')),'Original name policy changed')
        M.eligibility(o['eligibility'],originals);renderer(o['options'],query,originals);selectors(o['selectors'],serial==1)
        result=o['results']
        if serial==1:
            require(len(result)==2 and {r.get('id') for r in result}=={MEMBER,GROUP['id']} and all(exact(r,M.canonical(M.raw_member()) if r['id']==MEMBER else EFFECTIVE) for r in result),'Both exact effective colliding candidates must survive actual ranked lookup')
        else:require(result==[],'Observed unknown must remain actual no-match')
    require(exact(lookups[0]['observation']['selectors']['navigationHistory'],lookups[1]['observation']['selectors']['navigationHistory']),'Actual navigation changed across queries')
