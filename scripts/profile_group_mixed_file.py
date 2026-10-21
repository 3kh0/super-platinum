"""Independent mixed Profile reference oracle; no native process or network."""
FILE = 'FTITLE0001'
URL = 'https://fixture.slack.com/docs/FTITLE0001'


def file_tokens():
    return [dict(type='file', file_id=FILE, text=URL, url=URL,
                 fixture_occurrence=i, style={style: True}, opaque={'keep': [i, 'original']})
            for i, style in ((1, 'bold'), (2, 'italic'))]


def file_raw():
    return dict(id=FILE, mode='list', filetype='list', created=101, updated=101,
                title='  List &amp; Plan  ', name='wrong-name-fallback',
                fixture_opaque={'preserve': [FILE, 1, 2]})


def file_cache():
    raw = file_raw()
    effective = {k: v for k, v in raw.items() if k != 'fixture_opaque'}
    # Singleton envelope mapping supplies is_deleted; processFile projects a
    # tombstone boolean and clears unknown/nonexistent for observed timestamps.
    effective.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
    return dict(source=raw, effective=effective, current=True)


def file_source(count, completed, held):
    return dict(requests=[dict(serial=i, method='files.info', args=dict(
        file=FILE, page='1', count='500', truncate='true')) for i in range(1, count + 1)],
        completed=completed, cancelled=0, held=held, active=count-completed, networkCalls=0)


def validate_mixed(receipt, require):
    expected = {
        'initial': (0, 0, False), 'file-held': (1, 0, True),
        'group-held': (1, 1, False), 'offline': (1, 1, False),
        'retry-file-held': (2, 1, True), 'success': (2, 2, False),
        'edited': (2, 2, False), 'saved': (2, 2, False), 'reopened': (2, 2, False),
    }
    openings = {}
    for row in receipt['stages']:
        stage, model, proof = row['stage'], row['model'], row['proof']
        mixed = model.get('mixedFile')
        require(isinstance(mixed, dict) and set(mixed) == {'source', 'cache', 'opening'},
                'Missing separate mixed file source/cache evidence')
        require(mixed['source'] == file_source(*expected[stage]),
                'Mixed files.info exact request, completion, cancellation or no-I/O count changed')
        require(mixed['cache'] == (None if stage in ('initial', 'file-held') else file_cache()),
                'Mixed canonical file receipt/effective source/currentness changed')
        if stage in ('file-held', 'group-held', 'offline', 'retry-file-held'):
            require(proof.get('noAutomaticLoop') is True and proof.get('quietObservationMs') == 500,
                    'Mixed wait lacks bounded quiet no-loop observation')
            require(proof.get('openingRetained') is True and proof.get('memberRetained') is True,
                    'Mixed original opening or known member custody missing')
        if stage in ('group-held', 'success'):
            require(proof.get('fileRelease') == dict(delivered=True, cancelledBeforeReply=False),
                    'Original held file response was not delivered exactly')
        opening = mixed['opening']
        require(isinstance(opening, dict) and set(opening) == {'visit', 'serial', 'key', 'reading', 'mounted'}
                and all(type(opening[k]) is int and opening[k] >= 0 for k in ('visit', 'serial', 'key')),
                'Missing original mixed opening identity')
        require(opening['reading'] is (stage in ('file-held', 'group-held', 'retry-file-held'))
                and opening['mounted'] is (stage in ('success', 'edited', 'reopened')),
                'Mixed native editor mounted early or opening reading state changed')
        openings[stage] = opening
        reads = model['requests']
        final = receipt['requests']
        require(len(reads) <= len(final), 'Mixed group ledger exceeded terminal reads')
        for index, request in enumerate(reads):
            expected_request = dict(final[index])
            if stage == 'group-held' and index == len(reads) - 1:
                expected_request['released'] = False
                for key in ('receiverRetired', 'delivered', 'outcome', 'retryAfterSeconds'):
                    expected_request.pop(key, None)
            require(request == expected_request, 'Mixed original group receipt prefix changed')
    first, retry = openings['file-held'], openings['retry-file-held']
    require(first['key'] > openings['initial']['key'] and retry['key'] > openings['offline']['key'],
            'Mixed explicit Retry reused original opening or initial open missing')
    for stage in ('group-held',):
        require(all(openings[stage][k] == first[k] for k in ('visit', 'serial', 'key')),
                'File-to-group handoff replaced the original opening')
    # Group read bookkeeping returns Ok even when both endpoints returned
    # offline. Re-tracking then mints a new key before missing labels refuse
    # admission. The original Profile section and raw draft still remain.
    offline = openings['offline']
    require(offline['key'] > first['key']
            and (offline['visit'], offline['serial']) == (first['visit'], first['serial']),
            'Mixed failed acquisition did not re-track within its original section')
    # Successful resume re-tracks all references through prepare(), which mints
    # a fresh opening key before mounting; it does not reuse the failed witness.
    success = openings['success']
    require(success['key'] > retry['key']
            and (success['visit'], success['serial']) == (retry['visit'], retry['serial']),
            'Mixed successful adoption did not recapture all original reference sources')
    require(all(openings['edited'][k] == success[k] for k in ('visit', 'serial', 'key')),
            'Mixed native edit replaced its admitted editor')
    require((first['visit'], first['serial']) == (retry['visit'], retry['serial']),
            'Explicit retry replaced the original Profile section')
    group_reads = [r for r in receipt['requests'] if r['kind'] != 'discovery']
    require([r.get('outcome') for r in group_reads] == ['offline', 'offline', 'response']
            and all(r.get('delivered') is True and r.get('receiverRetired') is False for r in group_reads),
            'Mixed fallback failure or original response delivery changed')
