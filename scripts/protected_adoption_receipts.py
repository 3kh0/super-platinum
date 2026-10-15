"""Strict isolated adoption evidence. Preparation cannot launch before promotion."""
from pathlib import Path
import runpy

SCRIPTS = next(parent / "scripts" for parent in Path(__file__).resolve().parents
               if (parent / "Cargo.toml").is_file() and (parent / "scripts/native_fixture.py").is_file())
BASE = runpy.run_path(str(SCRIPTS / "protected_drafts_receipts.py"))
for name in ("NativeFixture", "DEFAULT_BINARY", "ROOT", "expected_viewport", "sha", "png",
             "validate_exit", "normalize", "validate_pointer_cleanup", "closed_receipts", "strict_equal",
             "number", "rectangle", "require", "same", "native", "uuid4", "keys"):
    globals()[name] = BASE[name]

FIXTURES = ("protected-adoption", "protected-adoption-narrow")
SCENARIOS = ("adopt-channel", "adopt-thread", "late-input-channel", "native-refusals-channel",
             "owner-replacement-channel", "cancel-boundaries-channel", "failed-managed-write-channel")
FLAG = "SUPER_PLATINUM_PROTECTED_ADOPTION_CHECK"
PREFIX, WAITING, STAGE = ("protected-adoption-" + name + ": " for name in ("check", "waiting", "stage"))
CONTRACT_READY = True


def require_contract():
    require(CONTRACT_READY and Path(__file__).resolve().parent == SCRIPTS,
            "Adoption preparation cannot launch before contract completion and promotion")


def phases(scenario):
    require(scenario in SCENARIOS, "Unknown adoption scenario")
    return ("exercise",)


ACCOUNT = "00000000-0000-4000-8000-000000000351"
OTHER = "00000000-0000-4000-8000-000000000352"
THREAD = "1719800400.000000"
INITIAL = "Local draft ready for recovery"
NEWER = "Later local input \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
FINAL = NEWER + " \u2014 after retry"
local, encoded = BASE['local'], BASE['encoded']


def scope(target):
    return dict(account_id=ACCOUNT,key=dict(team="T1",channel="C2",thread=THREAD if target=="thread" else None))


def stages(scenario, phase="exercise"):
    require(phase in phases(scenario), "Unknown adoption phase")
    common = ("initial", "dialog", "enrollment", "reloaded")
    if scenario.startswith("adopt-"): return common + ("prepared", "installed", "newer")
    if scenario=="late-input-channel": return common + ("registering", "interleaved", "refused")
    if scenario=="owner-replacement-channel": return common + ("prepared", "replaced", "refused", "recovered")
    if scenario=="cancel-boundaries-channel": return common + ("before-prepared", "canceled-before", "registered", "after-installed", "canceled-after", "recovered", "newer")
    if scenario=="failed-managed-write-channel": return common + ("prepared", "installed", "failed", "away", "returned", "retried", "resumed", "newer")
    return common + ("selection-prepared", "selection-changed", "selection-refused", "composition-prepared", "composition-active", "composition-refused", "expiry-locked", "expiry-restored", "expiry-refused", "aba-prepared", "aba-input", "aba-refused")


def capture_names(scenario, phase="exercise"):
    return tuple(name for stage in stages(scenario,phase) for name in (("dialog-channel","dialog-thread") if stage=="dialog" else (stage,)))


def validate_closed_storage(storage, receipt):
    module=runpy.run_path(str(Path(__file__).with_name("protected_adoption_storage.py")))
    return module['validate'](storage,receipt,globals())


def validate(*args, **kwargs):
    module=runpy.run_path(str(Path(__file__).with_name("protected_adoption_semantics.py")))
    return module['validate'](*args, **kwargs, helpers=globals())
