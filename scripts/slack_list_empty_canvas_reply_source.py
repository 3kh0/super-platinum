"""Original empty Canvas query and independent display receipt assertions."""
import copy
import json
from pathlib import Path
import slack_list_empty_canvas_transcript as T
import slack_list_canvas_user_titles_source as U

require=T.require
CANVAS,LIST=U.CANVAS,U.LIST
DESCRIPTOR=T.DESCRIPTOR
BASE=Path(__file__).resolve().parents[1]/'src/desktop/slack_lists/comments/fixture'

def blocks():
    return [dict(type='rich_text',block_id='empty-reply-title',opaque=dict(keep='block'),elements=[
        dict(type='rich_text_section',elements=[dict(type='text',text=' \t ',style=dict(italic=True),
            opaque=dict(keep='whitespace'))])])]

def response(original,plain=False):
    marker='"title":"Alex Canvas"'
    require(original.count(marker)==1,'Original Canvas scalar is not unique')
    return original.replace(marker,'"title":'+json.dumps(' \t ' if plain else 'Alex Canvas')+
        ',"title_blocks":'+json.dumps([] if plain else blocks(),sort_keys=True,separators=(',',':')),1)

def search_text():
    return response((BASE/'completion/files-response.json').read_bytes().decode('utf-8'))

def singleton(id,plain=False):
    text=(BASE/'cold_files'/('canvas-response.json' if id==CANVAS else 'list-response.json')).read_bytes().decode('utf-8')
    return response(text,plain) if id==CANVAS else text

def fallback(value,plain,current=True):
    require(value==dict(current=current,descriptor=DESCRIPTOR,quipTitle=None if plain else
        dict(fileId=CANVAS,current=True,value='nullish')) and value.get('current') is current,
        'Original English descriptor/consumed Quip observation changed')
    if not plain:require(value['quipTitle']['current'] is True,'Quip currentness is not actual boolean')

def query(proof):
    require(isinstance(proof,dict) and set(proof)=={CANVAS},'Fallback borrowed by unrelated List/file choice')
    fallback(proof[CANVAS],False)

def file_rows(rows,envelope,ids,*,singleton_receipt=False):
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids,'Title source identities/order changed')
    originals={r['id']:r for r in envelope['files']} if 'files' in envelope else {envelope['file']['id']:envelope['file']}
    for row in rows:
        raw=originals[row['id']];expected=U.title_projection(raw)
        if singleton_receipt:expected['is_deleted']=False
        require(row==dict(id=row['id'],source=raw,effective=expected,receipt=envelope),
            'Original title projection/raw envelope changed or borrowed candidate facts')

def observed(value,tokens,envelope,plain=False):
    require(isinstance(value,dict) and value.get('current') is True and value.get('policy') is None,
        'Independent no-user display borrowed policy or stale ownership')
    titles=value.get('titles',{})
    require(titles.get('current') is True and titles.get('tokens')==tokens and titles.get('required')==[]
        and titles.get('users') is None,'Original display tokens/independent dependency set changed')
    ids=list(dict.fromkeys(t['file_id'] for t in tokens))
    file_rows(titles.get('files'),envelope,ids,singleton_receipt='file' in envelope)
    expected={CANVAS} if CANVAS in ids else set()
    require(set(titles.get('canvasFallback',{}))==expected,'Unrelated List borrowed Canvas fallback')
    if expected:fallback(titles['canvasFallback'][CANVAS],plain)
    labels=titles.get('labels');require(isinstance(labels,dict),'Original title labels missing')
    parsed=[(json.loads(k),v) for k,v in labels.items()]
    require(len(parsed)==len(tokens) and all((t,'Untitled' if t['file_id']==CANVAS else 'Alex List') in parsed for t in tokens),
        'Derived label replaced original raw token or occurrence')

def retired(value):
    require(value is None or (value.get('current') is False and value.get('observed') is None),
        'Retired reply acquired a successor display receipt')
