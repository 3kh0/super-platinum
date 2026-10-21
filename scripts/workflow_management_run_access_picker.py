"""Literal fixture inputs and independent picker acquisition checks."""
import copy
from workflow_management_delete import exact, require
from workflow_management_run_access_ledger import DISCOVERY

TEAMS = [
    {'id':'TWORKFLOW','name':'Home fixture','enterprise_id':'ERUNORG01'},
    {'id':'TRUNINNER','name':'Run internal','enterprise_id':'ERUNORG01'},
    {'id':'TRUNEXTER','name':'Run partner','enterprise_id':'ERUNOTHER'},
]
PERSON = {'id':'URUNADA01','name':'ada_runner','real_name':'Ada Runner','team_id':'TWORKFLOW',
          'updated':101,'deleted':False,'is_bot':False,'is_restricted':False,'is_ultra_restricted':False,
          'is_stranger':False,'profile':{'real_name':'Ada Runner','real_name_normalized':'Ada Runner',
          'display_name':'Ada Runner','display_name_normalized':'Ada Runner'}}
CHANNEL = {'id':'CRUNTEAM1','name':'run-team','is_channel':True,'is_private':False,
           'is_archived':False,'is_member':True,'is_im':False,'is_mpim':False}
QUERIES = {
    'Members': {'query':'run','count':24,'fuzz':1,'enable_workspace_ranking':True,
                'filter':'NOT deactivated AND NOT guests','include_profile_only_users':True},
    'Channels': {'query':'run','count':24,'fuzz':1,'include_record_channels':True,'check_membership':True},
    'InternalTeams': {'query':'{"type":"and","clauses":[{"type":"is","value":"team"},{"type":"fuzzy","value":"run"}]}','count':24},
    'AvailableExternal': {'count':100,'filter':'available'},
    'ExternalTeams': {'query':'run'},
}
METHODS = {'Members':'users/search','Channels':'channels/search','InternalTeams':'search.enterprise',
           'AvailableExternal':'externalTeams/list','ExternalTeams':'externalTeams/search'}
PAGES = {
    'Members': {'ok':True,'results':[PERSON],'next_marker':''},
    'Channels': {'ok':True,'results':[CHANNEL],'member_channels':['CRUNTEAM1'],'next_marker':''},
    'InternalTeams': {'ok':True,'items':[TEAMS[1]],'next_cursor_mark':''},
    'AvailableExternal': {'ok':True,'results':[TEAMS[2]],'next_marker':''},
    'ExternalTeams': {'ok':True,'results':[TEAMS[2]],'next_marker':''},
}


def seed_proof(seed):
    require(isinstance(seed,dict) and set(seed)=={'boot','init','canonicalTeams'},'Missing original Boot/Init receipt')
    boot,init,canonical=(seed.get(k,{}) for k in ('boot','init','canonicalTeams'))
    require(boot.get('ok') is True and init.get('ok') is True,'Synthetic boot/init did not succeed')
    require(exact(boot.get('team'),TEAMS[0]) and exact(init.get('team'),TEAMS[0])
            and exact(init.get('workspaces'),TEAMS),'Original team input changed')
    require(all(exact(canonical.get(t['id']),t) for t in TEAMS),'Actual canonical team producer did not retain input')
    require(exact(boot.get('self'),init.get('self')) and boot['self'].get('id')=='UWORKFLOW'
            and boot['self'].get('team_id')=='TWORKFLOW','Original self input changed')
    require(exact([p for p in boot.get('users',[]) if p.get('id')=='URUNADA01'],[PERSON])
            and exact([c for c in boot.get('channels',[]) if c.get('id')=='CRUNTEAM1'],[CHANNEL]),
            'Original member/channel producer inputs missing or replaced')
    require(exact(init.get('onboarding'),{'user_data':{'onboarding_complete':True}})
            and init.get('user_date_created')==123,'Initial producer readiness changed')


def picker_proof(rows, seed, expected_kinds):
    seed_proof(seed)
    require([r.get('kind') if r.get('phase')=='picker' else 'Discovery' for r,_ in rows]==expected_kinds,
            'Missing, repeated or unrequested target acquisition')
    for request,response in rows:
        if request.get('phase')=='read':
            require(request.get('method')=='api.getFlannelHttpUrl','Foreign discovery')
            fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
            require(exact(request.get('fields'),fields) and exact(request.get('args'),dict(fields)),
                    'Discovery scope or reason changed')
            raw=DISCOVERY
        else:
            kind=request.get('kind')
            require(kind in QUERIES and request.get('method')==METHODS[kind],'Wrong selected target category/endpoint')
            query=QUERIES[kind]
            require(exact(request.get('query'),query),'Wrong source query defaults')
            body=copy.deepcopy(query)
            if kind=='InternalTeams':
                body=[[k,str(v)] for k,v in sorted(query.items())]+[['_x_reason','fetchAndUpsertTeamsWithQuery']]
            require(exact(request.get('body'),body),'Actual target wire differs from exact factory')
            raw=PAGES[kind]
        require(exact(response,{'serial':request['serial'],'result':{'raw':raw,'error':None}}),
                'Raw target page changed or no longer original')
