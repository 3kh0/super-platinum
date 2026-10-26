"""Independent expected known preview source and protocol snapshots."""
from profile_channel_title import ACCOUNT, TEAM, SELF, FILE, CHANNEL_PRESENTATION, projected_channel

USER, CHANNEL, GROUP = 'USECTION01', 'CSECTION01', 'SSECTION01'


def user_raw():
    return dict(id=USER,name='section-user',team_id=TEAM,profile=dict(display_name='U',real_name='Preview User'))


def channel_raw():
    return dict(id=CHANNEL,name='c',is_channel=True,is_member=True,updated=101)


def group_raw():
    return dict(id=GROUP,team_id=TEAM,handle='g',name='Section group',date_update=101,date_delete=0,is_visible=True)


def seed():
    me=dict(id=SELF,name='fixture-self',team_id=TEAM,profile={});team=dict(id=TEAM,name='Section preview')
    return dict(boot=dict(ok=True,team=team,self=me,users=[me,user_raw()],
                    channels=[dict(id='CINIT0001',name='warm-harbor',is_channel=True,is_member=True),channel_raw()],
                    workspaces=[dict(id=TEAM)],prefs={}),
                init=dict(ok=True,self=me,team=team,workspaces=[dict(id=TEAM)],
                    onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123),
                group=group_raw(),actualSourceSink=True)


def mixed():
    return dict(type='rich_text',opaque=dict(mixed=True),elements=[dict(type='rich_text_section',elements=[
        dict(type='user',user_id=USER,style=dict(bold=True)),dict(type='text',text=' '),
        dict(type='channel',channel_id=CHANNEL),dict(type='text',text=' '),
        dict(type='usergroup',usergroup_id=GROUP),dict(type='text',text=' '),
        dict(type='emoji',name='wave'),dict(type='text',text=' '),
        dict(type='canvas',file_id=FILE,section_id='nested-section-is-stripped',style=dict(italic=True),opaque=dict(retain=True))])])


def user_source():
    raw=user_raw()
    raw['profile'].update(first_name='',last_name='',display_name_normalized='U',real_name_normalized='Preview User')
    raw.update(_name_lc='section-user',_first_name_lc='',_last_name_lc='',_real_name_lc='preview user',_display_name_lc='u',
               _real_name_normalized_lc='preview user',_display_name_normalized_lc='u',_email_normalized_lc='')
    return raw


def channel_source():
    raw=projected_channel('c',101);raw['id']=CHANNEL;del raw['fixture_metadata'];return raw


def native_user():
    raw=user_raw()
    raw.update(real_name=None,deleted=False,is_bot=False,is_admin=False,is_owner=False,is_restricted=False,
               is_ultra_restricted=False,is_stranger=False,tz=None,tz_label=None,tz_offset=None,im_mpim_ids=[],has_more_mpims=False)
    raw['profile'].update({k:None for k in ('title','pronouns','phone','email','start_date','image_24','image_32','image_48','image_72',
        'image_192','image_512','image_original','avatar_hash','team','status_text','status_emoji','status_expiration')})
    raw['profile']['fields']={};return raw


def native_channel():
    raw=channel_source()
    raw.update(is_private=False,is_archived=False,is_starred=False,is_ext_shared=False,connected_teams=[],user=None,
               unread_count=None,unread_count_display=None,mention_count=None,has_unreads=False,last_read=None,previous_names=[])
    return raw


def native_group():
    return dict(id=GROUP,team_id=TEAM,name='Section group',handle='g',description='',users=[],members_loaded=False,
        prefs=dict(channels=[],groups=[]),date_delete=0,label_source=group_raw())


def policy():
    return dict(ok=True,prefs=dict(display_real_names_override=1,hidden_users=''),opaque_policy='complete-section-policy')


def snapshot(stage):
    return dict(input=dict(seed=seed(),policy=[dict(serial=1,method='users.prefs.get',body={},account=ACCOUNT,team=TEAM,user=SELF,epoch=0,raw=policy())] if stage=='confirmed' else []),
                nativeUser=native_user(),nativeChannel=native_channel(),nativeGroup=native_group(),groupFreshness=None,
                conditionalFixtureTransportExercised=False)


def dependencies(file_raw,file_effective,file_receipt):
    raw=mixed()['elements'][0]['elements'][-1].copy();del raw['section_id']
    return dict(members=dict(members=[dict(source=user_source(),effective=user_raw())],
                 policy=dict(user=policy(),enterprise=None,revision=0),generation='profile-canvas-title-users'),
        channels=dict(ids=[CHANNEL],rows=[channel_source()],presentation=CHANNEL_PRESENTATION),
        groups=dict(ids=[GROUP],rows=[dict(source=group_raw(),effective=dict(group_raw(),user_count=0,_name_lc='section group',is_usergroup=True,isNonExistent=False,users=[]))],
                    labels={GROUP:'@g'},provenance='shared-group-history-snapshot'),
        files=[dict(raw=raw,source=file_raw,effective=file_effective,receipt=file_receipt)])
