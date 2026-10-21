"""Independent ordinary-root body oracle; explicit synthetic HTTP carriers only."""
RICH_TS = '1719800000.000100'
FALLBACK_TS = '1719800020.000100'
OLDER_TS = '1719800030.000100'
LITERAL_TS = '1719700000.000100'
HIDDEN_TS = '1719700020.000100'
REPLY_TS = '1719800001.000100'
MORE_TS = '1719800002.000100'
RICH = 'Rich & \u4e2d\U0001f98a'
MENTION = '@Maya Chen'
FALLBACK = 'Fallback bold & \u65e5\U0001f600 New &\U0001f98a '
OLDER = 'An older thread with italic & \u4e2d\U0001f98a.'
LITERAL = 'Resolved: *literal* _marks_ @Maya Chen \U0001f44b & <@U1>'
STAGES = ('initial', 'thread', 'thread-more', 'previous-page', 'resolved', 'unread', 'error', 'recovered', 'stream-more')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def root(ts, text):
    return {'type': 'message', 'subtype': 'document_comment_root', 'ts': ts, 'user': 'U1',
            'text': text, 'reply_count': 2, 'document_comment': {'thread_id': 'quip-' + ts}}


def roots():
    rich = root(RICH_TS, 'DECOY text must not render')
    rich.update(markdown_text='DECOY markdown must not render', blocks=[{'type': 'rich_text', 'elements': [
        {'type': 'rich_text_section', 'elements': [
            {'type': 'text', 'text': RICH, 'style': {'bold': True, 'underline': True}},
            {'type': 'text', 'text': ' for '}, {'type': 'user', 'user_id': 'U1'}]}]}])
    rich['document_comment']['snippet'] = {'rich_text': 'DECOY snippet must not render'}
    fallback = root(FALLBACK_TS, 'Fallback *bold* &amp; \u65e5\U0001f600 _New &\U0001f98a _')
    fallback.update(blocks=[], markdown_text='DECOY secondary fallback must not render')
    older = root(OLDER_TS, '')
    older['markdown_text'] = 'An older thread with _italic_ &amp; \u4e2d\U0001f98a.'
    literal = root(LITERAL_TS, 'Resolved: *literal* _marks_ <@U1> :wave: &amp; &lt;@U1&gt;')
    literal['mrkdwn'] = False
    hidden = root(HIDDEN_TS, 'DECOY hidden text must not render')
    hidden['markdown_text'] = 'DECOY hidden markdown must not render'
    hidden['blocks'] = [{'type': 'section', 'text': {'type': 'plain_text', 'text': 'DECOY hidden blocks must not render'}}]
    hidden['document_comment']['is_visible'] = False
    return rich, fallback, older, literal, hidden


def reply(ts, text):
    return {'type': 'message', 'ts': ts, 'thread_ts': RICH_TS, 'user': 'U2', 'text': text}


def responses():
    rich, fallback, older, literal, hidden = roots()
    first = reply(REPLY_TS, 'The keyboard review is complete.\nReady for the final pass.')
    recent = {'ok': True, 'has_more': True, 'response_metadata': {'next_cursor': 'stream-next'},
              'messages': [rich, first, fallback]}
    thread = {'ok': True, 'has_more': True, 'response_metadata': {'next_cursor': 'reply-next'},
              'messages': [rich, first]}
    return [recent, recent, thread,
            {'ok': True, 'messages': [reply(MORE_TS, 'Unicode remains readable: \u65e5\u672c\u8a9e \U0001f98a e\u0301.')]},
            thread, recent, {'ok': True, 'messages': [literal, hidden]}, {'ok': True, 'messages': []}, None,
            recent, {'ok': True, 'has_more': False, 'messages': [older]}]


def paint(text, bold=False, italic=False, underline=False):
    return [[c, bold, italic, underline] for c in text]


def expected():
    return {
        RICH_TS: (RICH + ' for ' + MENTION, paint(RICH, bold=True, underline=True) + paint(' for ' + MENTION), [MENTION]),
        FALLBACK_TS: (FALLBACK, paint('Fallback ') + paint('bold', bold=True) + paint(' & \u65e5\U0001f600 ') + paint('New &\U0001f98a ', italic=True), []),
        OLDER_TS: (OLDER, paint('An older thread with ') + paint('italic', italic=True) + paint(' & \u4e2d\U0001f98a.'), []),
        LITERAL_TS: (LITERAL, paint(LITERAL), [MENTION]),
        HIDDEN_TS: ('This content was deleted.', paint('This content was deleted.'), []),
    }


def validate(receipt):
    require(receipt.get('responses') == responses(), 'Original synthetic response carriers changed')
    layouts = receipt.get('layouts')
    require(isinstance(layouts, list) and len(layouts) == len(STAGES), 'Missing root body stages')
    ids = [[RICH_TS, FALLBACK_TS], [RICH_TS, REPLY_TS], [MORE_TS], [RICH_TS, REPLY_TS],
           [LITERAL_TS, HIDDEN_TS], [], [], [RICH_TS, FALLBACK_TS], [OLDER_TS]]
    rich_original = None
    for name, layout, wanted in zip(STAGES, layouts, ids):
        proof = layout.get('bodyProof')
        require(isinstance(proof, dict) and proof.get('ok') is True, 'Missing actual DOM body proof')
        rows = proof.get('rows')
        require(isinstance(rows, list) and [r.get('ts') for r in rows] == wanted, 'Root/reply row identity changed at ' + name)
        require(proof.get('threadButtons') == (wanted if name not in ('thread', 'thread-more', 'previous-page') else []), 'Original root thread actions changed')
        for row in rows:
            require('DECOY' not in row.get('text', ''), 'Fallback or snippet decoy leaked')
            if row['ts'] not in expected():
                continue
            text, painted, mentions = expected()[row['ts']]
            require(row.get('text') == text and row.get('painted') == painted, 'Root text/computed marks differ at ' + name)
            require(row.get('mentions') == mentions and row.get('links') == [], 'Root special-token rendering differs')
            if row['ts'] == RICH_TS:
                if rich_original is None:
                    rich_original = row
                require(row == rich_original, 'Stream/thread root rendering identity drift')
