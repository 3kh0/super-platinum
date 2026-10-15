"""Read-only, post-exit validation of the isolated protected worker database."""
import json
import sqlite3
import stat


def validate(storage, receipt, helpers):
    same, require, uuid4, local, scope = (helpers[name] for name in ("same", "require", "uuid4", "local", "scope"))
    account, initial = helpers["ACCOUNT"], helpers["INITIAL"]
    def regular(path, directory=False):
        info = path.lstat()
        require(not path.is_symlink() and not getattr(info, "st_file_attributes", 0) & 0x400
                and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)), "Unowned linked/reparse storage path")
        return info
    for parent in (storage, *storage.parents): regular(parent, True)
    require(storage.name == "protected-drafts-" + receipt["nonce"], "Wrong owned root name")
    def read_json(path):
        require(regular(path).st_size <= 1024 * 1024, "Oversized fixture metadata")
        return unique_json(path.read_text(encoding="utf-8"))
    same(read_json(storage / "owner.json"), dict(version=1, nonce=receipt["nonce"]), "Parent ownership changed")
    worker = storage / "worker"
    regular(worker, True)
    marker = read_json(worker / "fixture.json")
    require(isinstance(marker, dict) and set(marker) == {"version", "account", "nonce"} and uuid4(marker["nonce"]), "Wrong isolated worker marker")
    same(marker["version"], 1, "Wrong isolated marker version")
    same(marker["account"], account, "Wrong isolated worker account")
    for child in worker.iterdir():
        require(child.name in ("fixture.json", "fixture.lock", "drafts.sqlite", "drafts.sqlite-journal", "drafts.sqlite-wal", "drafts.sqlite-shm"), "Unexpected isolated worker file")
        regular(child)
    db = worker / "drafts.sqlite"
    require(0 < regular(db).st_size < 32 * 1024 * 1024, "Invalid bounded fixture database")
    conn = sqlite3.connect(db.as_uri() + "?mode=ro", uri=True)
    try:
        conn.execute("PRAGMA query_only=ON")
        same(conn.execute("PRAGMA integrity_check").fetchall(), [("ok",)], "Closed database integrity failed")
        tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        populated = {"composer_drafts", "composer_draft_sync", "composer_draft_owner", "composer_draft_adapter_format",
                     "composer_draft_continuity_format", "composer_draft_continuity", "composer_draft_writer_lineage", "composer_draft_local_clock"}
        empty = {"composer_draft_blocked", "composer_draft_alternates", "composer_draft_reviewed_writers", "composer_draft_known_read_limit",
                 "composer_draft_baseline_evidence", "composer_draft_execution_result", "composer_draft_protected_archive",
                 "composer_draft_recovery_editing", "composer_draft_release_head", "composer_draft_continuity_reads",
                 "composer_draft_recovery_owners", "composer_draft_status_reads", "composer_draft_read_abandonment",
                 "composer_draft_read_late", "upload_journal_identity", "upload_receipts", "sqlite_sequence"}
        same(tables, populated | empty, "Unexpected or missing protected database table")
        for table in empty:
            same(conn.execute('SELECT * FROM "' + table + '"').fetchall(), [], "Unrelated persistent state: " + table)
        same(conn.execute("SELECT * FROM composer_draft_owner").fetchall(), [(1, account)], "Persistent owner changed")
        same(conn.execute("SELECT * FROM composer_draft_adapter_format").fetchall(), [(1, 1)], "Adapter format changed")
        same(conn.execute("SELECT * FROM composer_draft_continuity_format").fetchall(), [(1, 7)], "Protected format changed")
        final = receipt["finalInventory"]
        def key(target): return json.dumps(scope(target)["key"], sort_keys=True)
        canonical_raw = conn.execute("SELECT destination,content FROM composer_drafts").fetchall()
        require(len(canonical_raw) == 2, "Wrong exact canonical SQL row count")
        canonical = {json.dumps(unique_json(k), sort_keys=True): unique_json(v) for k, v in canonical_raw}
        same(canonical, {key(target): final[target]["local"] for target in final}, "Closed canonical drafts differ from terminal typed inventory")
        sync_raw = conn.execute("SELECT destination,storage_revision,format_version,state FROM composer_draft_sync").fetchall()
        require(len(sync_raw) == 2, "Wrong exact protected SQL row count")
        rows = {json.dumps(unique_json(k), sort_keys=True): dict(revision=r, format=f, envelope=unique_json(raw))
                for k, r, f, raw in sync_raw}
        same(rows, {key(target): dict(revision=final[target]["row"]["revision"], format=7, envelope=envelope(final[target]["row"])) for target in final}, "Closed protected envelopes differ")
        clocks_raw = conn.execute("SELECT destination,revision,generation,writer_id,serial FROM composer_draft_local_clock").fetchall()
        require(len(clocks_raw) == 2, "Wrong exact local clock SQL row count")
        clocks = {json.dumps(unique_json(k), sort_keys=True): [r, g, w, serial] for k, r, g, w, serial in clocks_raw}
        same(clocks, {key(target): [final[target]["clock"], None, None, 0] for target in final}, "Closed local clocks differ")
        writers = {}
        for writer, destination, raw in conn.execute("SELECT writer_id,destination,content FROM composer_draft_writer_lineage"):
            require(uuid4(writer) and writer not in writers, "Invalid duplicate writer identity")
            writers[writer] = unique_json(raw)
            last = writers[writer].get("last")
            if isinstance(last, dict) and isinstance(last.get("result", {}).get("row"), list):
                pair = last["result"]["row"]
                require(len(pair) == 2 and isinstance(pair[1], str), "Invalid embedded writer row")
                pair[1] = unique_json(pair[1])
            same(writers[writer].get("scope", {}).get("key"), unique_json(destination), "Writer destination differs")
        require(len(writers) == 4, "Unexpected writer count")
        for target in ("channel", "thread"):
            current = receipt["finalProjection"]["editors"][target]["writerId"]
            require(current in writers, "Final retained writer missing from SQLite")
            inventory = final[target]
            expected = dict(version=1, scope=scope(target), writer=current,
                            origin=dict(Enrollment=inventory["row"]["lineage"]["enrollment"]), processed=0, last=None)
            if target == receipt["target"]:
                expected.update(processed=2, last=dict(serial=2, local=inventory["local"],
                    direct=inventory["row"]["state"]["consumption"], continuation=None, preserved=False, result=inventory_record(inventory)))
            same(writers.pop(current), expected, "Retained writer receipt differs from exact typed command")
            seed = [(wid, record) for wid, record in writers.items() if record.get("scope") == scope(target)]
            require(len(seed) == 1, "Missing unique local seed writer")
            wid, record = seed[0]
            generation = record.get("origin", {}).get("LocalGeneration")
            require(uuid4(generation), "Missing original local generation")
            initial_local = local(initial, target == "thread")
            same(record, dict(version=1, scope=scope(target), writer=wid, origin=dict(LocalGeneration=generation), processed=1,
                last=dict(serial=1, local=initial_local, direct=None, continuation=dict(generation=generation, predecessor=0, expected=local("")),
                    preserved=False, result=dict(version=1, scope=scope(target), local=initial_local, blocked=None, alternates=[], row=None,
                                                origins=[], clock=4, generation=generation))), "Seed writer receipt changed")
            writers.pop(wid)
        same(writers, {}, "Unrelated writer persisted")
        teams = conn.execute("SELECT team,content FROM composer_draft_continuity").fetchall()
        require(len(teams) == 1 and teams[0][0] == "T1", "Unexpected continuity team")
        team = unique_json(teams[0][1])
        failure = receipt["scenario"] == "failed-write-return-channel"
        lease = team.get("lease_id")
        if not failure: require(uuid4(lease), "Missing actual isolated stream identity")
        same(team, dict(epoch=1 if failure else 2, lease_id=None if failure else lease, mode="Gap" if failure else "Open",
            sequence=0, pending=None, last_interrupted_id=None, gap_reason="activation" if failure else "new stream requires per-owner verification", read_retry_at_ms=0), "Unexpected continuity/request work")
        return dict(ok=True, database=str(db), workerMarker=marker, integrity="ok", canonical=canonical,
                    protected=rows, clocks=clocks, writerCount=4, continuity=team, emptyTables=sorted(empty))
    finally:
        conn.close()


def unique_json(raw):
    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value: raise RuntimeError("Duplicate JSON field in persisted protected state")
            value[key] = item
        return value
    return json.loads(raw, object_pairs_hook=unique, parse_constant=lambda value: (_ for _ in ()).throw(RuntimeError("Nonfinite JSON")))


def envelope(row):
    return dict(state=row["state"], enrollment_id=row["lineage"]["enrollment"], verified_epoch=row["lineage"]["verifiedEpoch"],
                basis=row["lineage"]["basis"], origin=dict(LocalIssued=dict(client_msg_id=row["state"]["client_msg_id"])))


def inventory_record(value):
    return dict(version=1, scope=value["scope"], local=value["local"], blocked=None, alternates=[],
                row=[value["row"]["revision"], envelope(value["row"])],
                origins=[], clock=value["clock"], generation=None)
