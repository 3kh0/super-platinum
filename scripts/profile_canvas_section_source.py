"""Exact section-reference code-unit and source expectations, independent of lossy display."""
from profile_channel_title import ACCOUNT, FOREIGN, FILE, URL, require, same
import profile_canvas_section_rich as rich
import profile_canvas_section_status as status


def utf16(text):
    data = text.encode('utf-16-le', 'surrogatepass')
    return [data[i] + 256 * data[i + 1] for i in range(0, len(data), 2)]


def validate_units(actual, expected):
    require(isinstance(actual, list) and all(type(unit) is int and 0 <= unit <= 65535 for unit in actual),
            'Invalid exact UTF16 code-unit carrier')
    require(actual == expected, 'Exact UTF16 label changed despite possible equal display normalization')
IDS = ('temp:C:0123456789abcdef01234567', 'temp:C:89abcdef0123456701234567')
TITLE = 'Section canvas'
METHOD = 'quip.getCanvasBulkSectionIdsData'


def blocks(prefix='Working with '):
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=prefix),
        *[dict(type='canvas', file_id=FILE, section_id=section, text=URL, url=URL,
               fixture_occurrence=i + 1, style={style: True}, opaque=dict(keep=[i + 1, 'original']))
          for i, (section, style) in enumerate(zip(IDS, ('bold', 'italic')))]] )])]


def file_response():
    return dict(ok=True, file=dict(id=FILE, mode='quip', filetype='quip', title=TITLE,
                                  updated=101, opaque=dict(retained=True)))


def section_response(reply):
    if reply == 'failure': return dict(error='offline')
    if reply == 'missing':
        return dict(raw=dict(ok=True, sections=[dict(id=IDS[0], rich_text_blocks=[])]))
    if reply == 'mixed':
        return dict(raw=dict(ok=True,sections=[dict(id=section,rich_text_blocks=[rich.mixed()]) for section in IDS]))
    require(reply in ('present', 'changed'), 'Unknown finite section response')
    rows = []
    for i, section in enumerate(IDS):
        text = (f'Changed section {i + 1}' if reply == 'changed' else 'a' * 24 + ('\U0001f600' if i == 0 else '\U0001f980') + ' tail') + '\nIgnored second line'
        block = dict(type='rich_text', opaque=dict(keep=i), elements=[
            dict(type='rich_text_section', elements=[dict(type='text', text=text, style=dict(bold=True))]),
            dict(type='rich_text_section', elements=[dict(type='text', text='Ignored top level')])])
        rows.append(dict(id=section, rich_text_blocks=[block]))
    return dict(raw=dict(ok=True, sections=rows))


def label_units(reply):
    if reply in ('private','deleted'): return [utf16('Private canvas' if reply=='private' else 'Deleted canvas') for _ in IDS]
    if reply == 'mixed':
        return [utf16(TITLE + ': ') + utf16('@U #c @g :wave: Section canvas')[:25] + [8230] for _ in IDS]
    require(reply in ('present', 'changed', 'missing', 'failure'), 'Unknown label expectation')
    return [utf16(TITLE + ': ') + (utf16('a' * 24) + [0xd83d + i, 0x2026]
            if reply == 'present' else utf16(f'Changed section {i + 1}')
            if reply == 'changed' else [0x2026]) for i in range(2)]


def request_args(ids=IDS):
    require(isinstance(ids, (tuple, list)) and bool(ids) and all(type(s) is str and s in IDS for s in ids)
            and len(set(ids)) == len(ids), 'Unexpected finite section request identity')
    return dict(file_id=FILE, section_ids=','.join(ids))


def validate_request_args(actual, ids=IDS):
    require(same(actual, request_args(ids)), 'Wrong exact section form arguments or identity')

STAGES = {
    'success': ('section-loading', 'opened', 'refreshed-mounted', 'used', 'confirmed'),
    'missing': ('section-loading', 'opened', 'reopened-present', 'used', 'confirmed'),
    'failure': ('section-loading', 'opened', 'refreshed-mounted', 'failed-reopened', 'reopened-present', 'used', 'confirmed'),
    'cancel': ('section-loading', 'cancelled', 'opened', 'used', 'confirmed'),
    'retirement': ('section-loading', 'opened', 'pre-mount-retired', 'socket-held', 'socket-retired', 'reopened', 'account-held', 'account-replaced'),
}
STAGES['status'] = status.STAGES
COUNTS = {'success': (1,1,2,2,2), 'missing': (1,1,2,2,2), 'failure': (1,1,2,3,4,4,4),
          'cancel': (1,1,2,2,2), 'retirement': (1,1,2,4,4,5,6,6)}
LOADING = 'Loading document titles. Your description is unchanged.'
UNAVAILABLE = 'Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'


def state_for(case, stage):
    if case=='status': return status.state(stage)
    i = STAGES[case].index(stage)
    retired = stage == 'account-replaced'
    held = stage in ('section-loading', 'socket-held', 'account-held')
    editor = stage in ('opened', 'refreshed-mounted', 'failed-reopened', 'reopened-present', 'confirmed', 'reopened', 'account-held')
    source_reply = 'present'
    if stage in ('section-loading', 'cancelled', 'socket-held', 'socket-retired') or retired:
        source_reply = 'absent'
    elif case == 'missing' and stage == 'opened': source_reply = 'missing'
    elif case == 'success' and i >= 2: source_reply = 'mixed'
    elif stage == 'pre-mount-retired': source_reply = 'changed'
    elif case == 'failure' and stage in ('refreshed-mounted', 'failed-reopened'): source_reply = 'absent'
    label = 'missing' if (case == 'missing' and stage == 'opened') or stage == 'failed-reopened' else 'mixed' if case == 'success' and stage == 'confirmed' else 'present'
    return dict(index=i, retired=retired, held=held, editor=editor, source_reply=source_reply, label=label,
        section_count=COUNTS[case][i], file_count=2 if case == 'retirement' and i >= 5 else 1,
        cancelled=int(case == 'cancel' and i >= 1), socket=int(case=='success' or (case == 'retirement' and i >= 4 and not retired)),
        saved=stage == 'confirmed', can_save=stage == 'used',
        model_prefix='Saved with ' if stage in ('used','confirmed') else 'Working with ',
        dom_prefix='Retained draft with ' if stage == 'refreshed-mounted' else 'Saved with ' if stage == 'confirmed' else 'Working with ')


def file_ledger(case, stage):
    count = state_for(case, stage)['file_count']
    return dict(requests=[dict(serial=i, method='files.info', args=dict(file=FILE,page='1',count='500',truncate='true'),
                         response=dict(raw=status.response() if case=='status' else file_response())) for i in range(1,count+1)],
                completed=count, cancelled=0, held=False, active=0, networkCalls=0)


def section_ledger(case, stage):
    if case=='status': return dict(requests=[],completed=0,cancelled=0,held=False,active=0,networkCalls=0)
    e = state_for(case,stage)
    replies = {'success': ('present','mixed'), 'missing': ('missing','present'),
               'failure': ('present','failure','failure','present'), 'cancel': ('present','present'),
               'retirement': ('present','changed','failure','present','present','changed')}[case]
    rows=[]
    for serial, reply in enumerate(replies[:e['section_count']],1):
        socket=int(case=='success' or (case == 'retirement' and serial >= 5))
        row=dict(serial=serial,method=METHOD,args=request_args(),socketGeneration=socket)
        was_held=serial == 1 or (case == 'retirement' and serial in (4,6))
        pending=e['held'] and serial == e['section_count']
        if was_held and not pending: row['releasedResponse']=section_response(reply)
        if not pending and not (case=='cancel' and serial==1): row['response']=section_response(reply)
        rows.append(row)
    return dict(requests=rows,completed=e['section_count']-int(e['held']),cancelled=e['cancelled'],
                held=e['held'],active=int(e['held']),networkCalls=0)


def section_source(case,stage):
    e=state_for(case,stage)
    if e['retired']: return None
    rows=[]
    for i,section in enumerate(IDS):
        reply=e['source_reply'];block=None;has=False
        if reply in ('present','changed','mixed'):
            block=section_response(reply)['raw']['sections'][i]['rich_text_blocks'][0];has=True
        elif reply=='missing': has=i==0
        deps=dict(members=None,channels=None,groups=None,files=[])
        if reply=='mixed':
            cache=expected_cache(case,stage)
            deps=rich.dependencies(cache['source'],cache['effective'],file_response()) if stage=='confirmed' else None
        rows.append(dict(id=section,current=True,hasEntry=has,cached=block is not None,block=block,previewDependencies=deps))
    return dict(rows=rows)


def source(case,stage):
    e=state_for(case,stage)
    return dict(case=case,files=file_ledger(case,stage),sections=section_ledger(case,stage),sectionSource=section_source(case,stage),
        account=FOREIGN if e['retired'] else ACCOUNT,epoch=int(e['retired']),socketGeneration=e['socket'],networkCalls=0,richPreview=rich.snapshot(stage) if case=='success' else None)


def expected_cache(case,stage):
    if case=='status': return status.cache(stage)
    e=state_for(case,stage)
    if e['retired']: return None
    raw=file_response()['file'];current=stage!='socket-retired'
    projected={key:raw[key] for key in ('id','mode','filetype','title','updated')}
    return dict(source=raw,effective=dict(projected,is_deleted=False,is_tombstoned=False,isUnknown=False,isNonExistent=False),current=current,
        canvasFallback=dict(current=current,descriptor=dict(locale='en-US',namespace='canvas',source='Untitled',value='Untitled',origin='application-presentation')),
        emojiText=dict(current=current,descriptor=dict(locale='en-US',format='canonical-colon-name',catalog='retained-v16-v17-canonical-intersection',origin='application-presentation')))


def validate_source(value,case,stage):
    require(same(value,source(case,stage)), 'Wrong exact metadata/section source, request, retirement or owner ledger')


def validate_cache(value,case,stage):
    require(same(value,expected_cache(case,stage)), 'Wrong exact current metadata source or status authority')
