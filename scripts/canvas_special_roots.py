"""Closed hidden-root receipt oracle; raw source and painted output are separate."""
import math
import json
from urllib.parse import urlsplit

STAGES = ('initial', 'thread', 'thread-refreshed', 'visible-true',
          'visible-absent', 'visible-null', 'error', 'recovered')
READS = (2, 3, 4, 6, 6, 6, 7, 8)
DELETED = 'This content was deleted.'
HIDDEN = '1719800000.000100'
REPLY = '1719800001.000100'
CONTROLS = ('1719800020.000100', '1719800030.000100', '1719800040.000100')
LABELS = ('Visible true', 'Visibility absent', 'Visibility null')
URLS = tuple('https://example.test/hidden-root/' + name for name in
             ('block.png', 'file.png', 'download.png', 'thumb.png', 'attachment.png',
              'attachment-thumb.png', 'author.png'))


def require(value, message):
    if not value:
        raise RuntimeError(message)


def same(left, right):
    # Preserve JSON null/bool/number distinctions; object member order is irrelevant.
    return json.dumps(left, sort_keys=True, allow_nan=False) == json.dumps(right, sort_keys=True, allow_nan=False)


def retained_fields(actual, original):
    """Require each original field through serde's added typed defaults."""
    if isinstance(original, dict):
        return isinstance(actual, dict) and all(k in actual and retained_fields(actual[k], v)
                                                for k, v in original.items())
    if isinstance(original, list):
        return isinstance(actual, list) and len(actual) == len(original) and all(
            retained_fields(a, b) for a, b in zip(actual, original))
    return type(actual) is type(original) and actual == original


def retained(value, index):
    require(isinstance(value, dict) and value.get('ownerCurrent') is True
            and value.get('loading') is False and type(value.get('serial')) is int and value['serial'] > 0,
            'Actual retained comments owner/read is unavailable')
    thread = index in (1, 2)
    require(same(value.get('query'), {'kind': 'thread' if thread else 'stream',
                                     'rootTs': HIDDEN if thread else None}), 'Retained control query changed')
    error = 'Comments could not be loaded. Your canvas is unchanged. Retry when ready.' if index == 6 else None
    require(value.get('error') == error, 'Retained read failure differs from displayed state')
    expected = ([hidden()], responses()[2]['messages'], responses()[3]['messages'], controls(),
                controls(), controls(), [], [hidden()])[index]
    rows = value.get('rows')
    require(isinstance(rows, list) and len(rows) == len(expected), 'Retained Row.message inventory differs')
    for row, original in zip(rows, expected):
        message = row.get('message')
        require(row.get('matchesOriginal') is True and retained_fields(message, original),
                'Retained Row.message lost original source fields')
        for key in ('blocks', 'document_comment'):
            if key in original:
                require(same(message.get(key), original[key]), 'Retained raw root fields changed')
    if thread:
        require(retained_fields(value.get('root'), hidden())
                and same(value['root'].get('document_comment'), hidden()['document_comment'])
                and same(value['root'].get('blocks'), hidden()['blocks']), 'Actual State.root changed or disappeared')
    else:
        require(value.get('root') is None, 'Stream retained a foreign thread root')


def root(ts, text):
    return {'type': 'message', 'subtype': 'document_comment_root', 'ts': ts, 'user': 'U1',
            'text': text, 'reply_count': 2, 'document_comment': {'thread_id': 'quip-' + ts}}


def hidden():
    row = root(HIDDEN, 'DECOY hidden fallback')
    row['markdown_text'] = 'DECOY hidden markdown'
    row['document_comment'].update(is_visible=False, most_recent_editor='U2', opaque={'preserve': True})
    row['blocks'] = [
        {'type': 'section', 'text': {'type': 'plain_text', 'text': 'DECOY hidden blocks'}},
        {'type': 'image', 'image_url': URLS[0], 'alt_text': 'DECOY hidden image'}]
    row['files'] = [{'id': 'FHIDDEN001', 'name': 'DECOY hidden file', 'title': 'DECOY hidden title',
                     'filetype': 'png', 'mimetype': 'image/png', 'size': 42, 'url_private': URLS[1],
                     'url_private_download': URLS[2], 'thumb_360': URLS[3], 'original_w': 120, 'original_h': 80}]
    row['attachments'] = [{'id': 1, 'text': 'DECOY hidden attachment', 'title': 'DECOY hidden attachment title',
                          'title_link': 'https://example.test/hidden-root/attachment-link', 'image_url': URLS[4],
                          'image_width': 120, 'image_height': 80, 'thumb_url': URLS[5],
                          'author_name': 'DECOY hidden attachment author', 'author_icon': URLS[6]}]
    return row


def controls():
    rows = []
    for index, (ts, label) in enumerate(zip(CONTROLS, LABELS)):
        row = root(ts, label)
        if index == 0:
            row['document_comment']['is_visible'] = True
        if index == 2:
            row['document_comment']['is_visible'] = None
        row['blocks'] = [{'type': 'rich_text', 'elements': [{'type': 'rich_text_section', 'elements': [
            {'type': 'text', 'text': label + ' & \u4e2d\U0001f98a', 'style': {'bold': True, 'underline': True}}]}]}]
        row['files'] = [{'id': f'FCONTROL0{index}', 'name': label + ' file', 'title': label + ' file',
                         'filetype': 'pdf', 'mimetype': 'application/pdf'}]
        rows.append(row)
    return rows


def responses():
    stream = {'ok': True, 'has_more': True, 'response_metadata': {'next_cursor': 'special-controls'},
              'messages': [hidden()]}
    thread = {'ok': True, 'has_more': False, 'messages': [hidden(),
              {'type': 'message', 'ts': REPLY, 'thread_ts': HIDDEN, 'user': 'U2',
               'text': 'Visible reply remains readable.'}]}
    return [stream, stream, thread, thread, stream,
            {'ok': True, 'has_more': False, 'messages': controls()}, None, stream]


def proof(value, count):
    require(isinstance(value, dict) and same(value.get('hidden'), hidden())
            and same(value.get('controls'), controls()), 'Original special-root source changed')
    require(same(value.get('responses'), responses()[:count]), 'Original special-root response prefix changed')
    requests = value.get('requests')
    require(isinstance(requests, list) and len(requests) == count, 'Wrong special-root read count')
    for index, item in enumerate(requests):
        request(item, 'replies' if index in (2, 3) else 'history',
                root_ts=HIDDEN if index in (2, 3) else None,
                cursor='special-controls' if index == 5 else None)
    media = value.get('media')
    require(isinstance(media, list) and len(media) == len(URLS), 'Missing hidden media registry evidence')
    for item, url in zip(media, URLS):
        require(item.get('url') == url and type(item.get('registered')) is int
                and item['registered'] == 0, 'Hidden media was registered or source substituted')


def bounded(rect, viewport):
    keys = ('x', 'y', 'width', 'height', 'right', 'bottom')
    return (isinstance(rect, dict)
            and all(type(rect.get(k)) in (int, float) and math.isfinite(rect[k]) for k in keys)
            and rect['width'] > 0 and rect['height'] > 0
            and rect['x'] >= -1 and rect['y'] >= -1
            and abs(rect['right'] - rect['x'] - rect['width']) < .01
            and abs(rect['bottom'] - rect['y'] - rect['height']) < .01
            and rect['right'] <= viewport[0] + 1 and rect['bottom'] <= viewport[1] + 1)


def request(value, method, root_ts=None, cursor=None):
    url = urlsplit(value.get('url', ''))
    require(url.scheme == 'https' and url.netloc == 'fixture.slack.com'
            and url.path == '/api/conversations.' + method and not url.fragment
            and value.get('method') == 'POST', 'Wrong special-root read endpoint')
    pairs = value.get('fields')
    require(isinstance(pairs, list) and all(isinstance(p, list) and len(p) == 2
            and all(isinstance(x, str) for x in p) for p in pairs), 'Malformed special-root form')
    fields = dict(pairs)
    require(len(fields) == len(pairs), 'Duplicate special-root form keys')
    wanted = {'channel': 'Cfixture', 'inclusive': 'true',
              'limit': '20' if method == 'history' else '28', 'cached_latest_updates': '{}'}
    if cursor is not None:
        wanted['cursor'] = cursor
    if method == 'history':
        wanted.update(document_stream='latest_reply', ignore_replies='true', include_pin_count='true',
                      no_user_profile='true', include_stories='true', include_free_team_extra_messages='true',
                      include_date_joined='false')
    else:
        wanted['ts'] = root_ts
    require(fields == wanted, 'Special-root read form differs from original target')


def shown(value, viewport, clip):
    require(isinstance(value, dict) and value.get('inView') is True and value.get('hit') is True
            and bounded(value.get('rect'), viewport), 'Special-root target is clipped or not hit-testable')
    r = value['rect']
    require(r['x'] >= clip['x'] - 1 and r['y'] >= clip['y'] - 1
            and r['right'] <= clip['right'] + 1 and r['bottom'] <= clip['bottom'] + 1,
            'Special-root target escapes actual scroll viewport')


def layout(value, index, viewport):
    require(isinstance(value, dict) and value.get('ok') is True and value.get('focused') is True
            and value.get('noOverflow') is True and value.get('viewport') == viewport
            and bounded(value.get('dialog'), viewport) and bounded(value.get('panel'), viewport),
            'Invalid special-root panel geometry')
    controls_ = value.get('controls')
    require(isinstance(controls_, list) and len(controls_) == 3
            and controls_[0].get('label') == 'Back to canvas'
            and controls_[-1].get('label') == 'Refresh comments'
            and all(c.get('hit') is True and bounded(c.get('rect'), viewport) for c in controls_),
            'Special-root navigation controls are missing or clipped')
    require(type(value.get('scrollHeight')) in (int, float) and value['scrollHeight'] >= 40,
            'Missing bounded comment scroll area')
    body = value.get('bodyProof')
    require(isinstance(body, dict) and body.get('ok') is True and bounded(body.get('scroll'), viewport),
            'Missing actual body/scroll proof')
    ids = ([HIDDEN], [HIDDEN, REPLY], [HIDDEN, REPLY], list(CONTROLS),
           list(CONTROLS), list(CONTROLS), [], [HIDDEN])[index]
    target_ts = (HIDDEN, HIDDEN, HIDDEN, *CONTROLS, None, HIDDEN)[index]
    rows = body.get('rows')
    require(isinstance(rows, list) and [r.get('ts') for r in rows] == ids
            and body.get('targetTs') == target_ts, 'Original special-root row/target identity changed')
    expected_text = {HIDDEN: DELETED, REPLY: 'Visible reply remains readable.'}
    expected_text.update({ts: label + ' & \u4e2d\U0001f98a'
                          for ts, label in zip(CONTROLS, LABELS)})
    for row in rows:
        ts = row['ts']
        require(row.get('text') == expected_text[ts] and row.get('body', {}).get('text') == expected_text[ts],
                'Special-root painted text leaked or visible control disappeared')
        require(row.get('links') == [] and row.get('images') == [] and row.get('attachments') == [],
                'Special-root file/link/media/attachment content leaked')
        button = row.get('threadButton')
        if index in (1, 2):
            require(button is None, 'Thread gained a stream-only root action')
        else:
            require(isinstance(button, dict) and button.get('disabled') is False
                    and button.get('text') == 'Open thread',
                    'Original root lost its enabled thread action')
        if ts == target_ts:
            require(same(body.get('target'), row['body']), 'Target proof substituted another body')
            if button is not None:
                shown(button, viewport, body['scroll'])
    error = 'Comments could not be loaded. Your canvas is unchanged. Retry when ready.'
    require(body.get('alerts') == ([error] if index == 6 else []), 'Failure guidance differs or falsely empty')
    if index == 6:
        require(body.get('target', {}).get('text') == error, 'Failure target is not actual guidance')
    shown(body.get('target'), viewport, body['scroll'])


def validate(receipt):
    stages = receipt.get('stages')
    viewport = receipt.get('layout', {}).get('viewport')
    require(isinstance(viewport, list) and len(viewport) == 2
            and all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in viewport),
            'Missing special-root viewport')
    require(isinstance(stages, list) and len(stages) == len(STAGES), 'Missing special-root stages')
    previous_requests = []
    first_serial = None
    for index, (stage, name, count) in enumerate(zip(stages, STAGES, READS)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int and stage['ordinal'] == index
                and all(stage.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Special-root stage order or isolation changed')
        p = stage.get('proof')
        proof(p, count)
        retained(p.get('retained'), index)
        serial = p['retained']['serial']
        if first_serial is None:
            first_serial = serial
        require(serial == first_serial + count - READS[0], 'Original retained read serial drifted')
        require(same(p['requests'][:len(previous_requests)], previous_requests), 'Captured requests changed later')
        previous_requests = p['requests']
        layout(stage.get('layout'), index, viewport)
    require(same(receipt.get('proof'), stages[-1]['proof'])
            and same(receipt.get('layout'), stages[-1]['layout']), 'Terminal special-root proof changed')
