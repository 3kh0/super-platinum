"""Independent endpoint-debt assertions for the closed native rate scenario.

The shared group oracle still validates raw rows, native tokens, controls,
requests, save/readback, capture paths and owned process lifecycle.
"""

DIRECTORY_SECONDS = 8
DIRECT_SECONDS = 15
ACCOUNT = '00000000-0000-4000-8000-000000000081'


def validate_rate(receipt, require):
    stages = {row['stage']: row for row in receipt['stages']}
    rows = receipt['requests']
    reads = [row for row in rows if row['kind'] != 'discovery']
    require(len(reads) == 3, 'Rate scenario must issue exactly three group reads')
    for row, kind, seconds in zip(reads[:2], ('directory', 'direct'),
                                  (DIRECTORY_SECONDS, DIRECT_SECONDS)):
        require(row['kind'] == kind and row.get('outcome') == 'rate-limited'
                and type(row.get('retryAfterSeconds')) is int and row['retryAfterSeconds'] == seconds
                and row.get('delivered') is True and row.get('receiverRetired') is False,
                'Missing original delivered endpoint Retry-After receipt')
    require(reads[2]['kind'] == 'directory' and reads[2].get('outcome') == 'response'
            and reads[2].get('delivered') is True and reads[2].get('receiverRetired') is False,
            'Recovery must use available directory without a second direct fallback')
    all_keys = {'scope', 'directory', 'direct', 'blocked', 'observed'}
    scope = dict(account=ACCOUNT, team='TINIT0001', user='UINIT0001')
    directory = stages['direct-held']['model']['rate']['directory']
    direct = stages['limited']['model']['rate']['direct']
    for debt, seconds in ((directory, DIRECTORY_SECONDS), (direct, DIRECT_SECONDS)):
        require(isinstance(debt, dict) and set(debt) == {'revision', 'retryAfterSecs', 'blocked'}
                and type(debt.get('revision')) is int and debt['revision'] > 0
                and debt.get('retryAfterSecs') == seconds and debt.get('blocked') is True,
                'Missing distinct original endpoint debt')
    require(directory['revision'] != direct['revision'], 'Endpoint error receipts share a revision')
    directory_identity = {k: directory[k] for k in ('revision', 'retryAfterSecs')}
    direct_identity = {k: direct[k] for k in ('revision', 'retryAfterSecs')}
    directory_expired = direct_expired = False
    for row in receipt['stages']:
        stage = row['stage']
        rate = row['model'].get('rate')
        require(isinstance(rate, dict) and set(rate) == all_keys and rate['scope'] == scope,
                'Rate scope or snapshot shape changed')
        require(type(rate['blocked']) is bool and type(rate['observed']) is bool,
                'Invalid endpoint admission flags')
        if stage in ('initial', 'held'):
            require(rate == dict(scope=scope, directory=None, direct=None, blocked=False, observed=False),
                    'Rate debt appeared before an actual error')
            continue
        require(isinstance(rate['directory'],dict)
                and {k: rate['directory'].get(k) for k in directory_identity} == directory_identity
                and type(rate['directory'].get('blocked')) is bool,
                'Directory debt identity changed or vanished after expiry')
        if stage == 'direct-held':
            require(rate['directory']['blocked'] and rate['direct'] is None
                    and rate['blocked'] is False and rate['observed'] is True,
                    'Directory debt incorrectly blocked independent direct fallback')
            pending = row['model']['pending']
            require(len(pending) == 1 and pending[0]['kind'] == 'direct',
                    'Independent direct request not actually held')
            require(row['proof'].get('directoryRequest') == reads[0]['serial']
                    and row['proof'].get('directoryRetryAfterSeconds') == DIRECTORY_SECONDS,
                    'Direct fallback proof lost original directory request')
            continue
        require(isinstance(rate['direct'],dict)
                and {k: rate['direct'].get(k) for k in direct_identity} == direct_identity
                and type(rate['direct'].get('blocked')) is bool and rate['observed'] is True,
                'Direct debt identity changed or vanished after expiry')
        require(rate['blocked'] is (rate['directory']['blocked'] and rate['direct']['blocked']),
                'Global Retry availability does not match both endpoint debts')
        if stage in ('limited', 'cancelled', 'reopened-limited'):
            require(rate['directory']['blocked'] and rate['direct']['blocked'] and rate['blocked'],
                    'Both debts must survive early Cancel/reopen')
        else:
            require(not rate['directory']['blocked'] and not rate['blocked'],
                    'Directory debt remained blocked after actual expiry')
        if directory_expired:
            require(not rate['directory']['blocked'], 'Directory debt was extended without another error')
        if direct_expired:
            require(not rate['direct']['blocked'], 'Direct debt was extended without another error')
        directory_expired |= not rate['directory']['blocked']
        direct_expired |= not rate['direct']['blocked']
    for stage in ('limited', 'reopened-limited'):
        proof = stages[stage]['proof']
        require(proof.get('disabledRetry') == dict(ok=True, disabled=True, clicks=2)
                and proof.get('quietObservationMs') == 500,
                'Early disabled Retry was not exercised without dispatch')
    # Exact original ledger, including immutable error fields, survives Cancel,
    # reopening and the first endpoint's expiry before explicit Retry.
    baseline = stages['limited']['model']['requests']
    for stage in ('cancelled', 'reopened-limited', 'expired'):
        require(stages[stage]['model']['requests'] == baseline,
                'Cancel/reopen/expiry automatically dispatched or changed an error receipt')
    expiry = stages['expired']
    require(expiry['model']['rate']['direct']['blocked'] is True,
            'Expiry did not prove directory independence while direct still cooling')
    elapsed = expiry['proof'].get('elapsed', {})
    require(type(elapsed.get('directoryElapsedMs')) is int
            and elapsed['directoryElapsedMs'] >= DIRECTORY_SECONDS * 1000
            and type(elapsed.get('directElapsedMs')) is int
            and 0 < elapsed['directElapsedMs'] < DIRECT_SECONDS * 1000
            and expiry['proof'].get('quietObservationMs') == 500,
            'Missing real elapsed first-endpoint expiry')
    require(stages['reopened']['model']['requests'] == rows,
            'Saved description reopening dispatched another acquisition')
