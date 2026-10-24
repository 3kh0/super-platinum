"""Owned native Enter split, whole-pair save/readback and continuation."""
import argparse
from pathlib import Path

import canvas_native
from canvas_native import CanvasCheck
from canvas_split_input import CanvasSplitInput
from canvas_split_storage import SplitStorage
from native_fixture import DEFAULT_BINARY, ROOT

STAGES = {
    "success": ("split", "cancelled", "saving", "confirmed", "child-editing",
                "continued-saving", "continued-confirmed"),
    "stale": ("split", "cancelled", "refused", "cancelled-stale"),
    "partial": ("split", "cancelled", "saving", "uncertain"),
}
base_validate = canvas_native.validate
base_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt, corpus="ascii", mode="middle", typing="none", reuse_base="positive", reuse_content="empty", parent_mode="bootstrap", empty_history="bootstrap", empty_layout="no-following", repeat_history="bootstrap", repeat_layout="no-following", sequence_count=2, sequence_history="bootstrap", sequence_layout="no-following", sequence_cadence="straight", pre_enter="none", middle_parent="none", middle_cadence="parent-first", second_middle="none", second_middle_cadence="straight"):
    base_validate(spec, fixture, receipt)
    if second_middle != "none":
        from canvas_second_middle_oracle import validate as validate_second
        return validate_second(receipt,corpus,parent_mode,second_middle,second_middle_cadence)
    if middle_parent != "none":
        from canvas_middle_parent_oracle import validate as validate_parent
        return validate_parent(receipt,corpus=corpus,typing=typing,parent_mode=parent_mode,pre_enter=pre_enter,middle_parent=middle_parent,middle_cadence=middle_cadence)
    if pre_enter != "none":
        from canvas_premiddle_oracle import validate as validate_premiddle
        return validate_premiddle(receipt,corpus=corpus,typing=typing,parent_mode=parent_mode,pre_enter=pre_enter)
    if mode == "sequence":
        from canvas_sequence_oracle import validate as validate_sequence
        return validate_sequence(receipt, append_count=sequence_count, history=sequence_history,
                                 layout=sequence_layout, cadence=sequence_cadence, corpus=corpus)
    if mode == "repeat" and typing != "none":
        from canvas_repeat_typing_oracle import validate as validate_repeat_typing
        return validate_repeat_typing(receipt, typing=typing, repeat_history=repeat_history,
                                      repeat_layout=repeat_layout, corpus=corpus)
    if mode == "repeat":
        from canvas_repeat_enter_oracle import validate as validate_repeat
        return validate_repeat(receipt, repeat_history=repeat_history, repeat_layout=repeat_layout, corpus=corpus)
    if mode == "empty" and typing != "none":
        from canvas_empty_typing_oracle import validate as validate_empty_typing
        return validate_empty_typing(receipt, typing=typing, empty_history=empty_history,
                                     empty_layout=empty_layout, corpus=corpus)
    if mode == "empty":
        from canvas_empty_enter_oracle import validate as validate_empty
        return validate_empty(receipt, empty_history=empty_history, empty_layout=empty_layout)
    if mode == "start" and typing != "none":
        from canvas_start_typing_oracle import validate as validate_start_typing
        return validate_start_typing(receipt, corpus=corpus, typing=typing, parent_mode=parent_mode)
    if mode == "start":
        from canvas_start_oracle import validate as validate_start
        return validate_start(receipt, corpus=corpus, parent_mode=parent_mode)
    if reuse_content != "empty":
        from canvas_end_clearing_oracle import validate as validate_clearing
        return validate_clearing(receipt, corpus=corpus, typing=typing,
                                 reuse_base=reuse_base, reuse_content=reuse_content)
    if typing != "none":
        if mode == "middle":
            from canvas_middle_typing_oracle import validate as validate_middle_typing
            return validate_middle_typing(receipt, corpus=corpus, typing=typing, parent_mode=parent_mode)
        from canvas_end_typing_oracle import validate as validate_typing
        return validate_typing(receipt, corpus=corpus, mode=mode, typing=typing, reuse_base=reuse_base)
    if mode == "reuse":
        from canvas_end_reuse_oracle import validate as validate_split
    elif mode == "end":
        from canvas_end_split_oracle import validate as validate_split
    else:
        from canvas_split_oracle import validate as validate_split
    if mode == "reuse":
        validate_split(receipt, corpus=corpus, reuse_base=reuse_base)
    elif mode == "middle":
        validate_split(receipt, corpus=corpus, parent_mode=parent_mode)
    else:
        validate_split(receipt, corpus=corpus)


def validate_closed(spec, fixture, log, stages, terminal):
    base_closed(spec, fixture, log, stages, terminal)
    if terminal.get("stages") != [{k:v for k,v in stage.items() if k != "screenshot"} for stage in stages]:
        raise RuntimeError("Terminal split proof differs from original closed stages")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui")
    parser.add_argument("--fixture", choices=("canvas-structure", "canvas-structure-narrow"), required=True)
    parser.add_argument("--scenario", choices=STAGES, required=True)
    parser.add_argument("--corpus", choices=("ascii", "unicode"), default="ascii")
    parser.add_argument("--mode", choices=("sequence", "repeat", "empty", "start", "middle", "end", "reuse"), default="middle")
    parser.add_argument("--repeat-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--repeat-layout", choices=("no-following", "text-following", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--empty-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--empty-layout", choices=("no-following", "text-following-ascii", "text-following-unicode", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--typing", choices=("none", "correction", "erase", "restore"), default="none")
    parser.add_argument("--parent-mode", choices=("bootstrap", "retained", "retained-zero"), default="bootstrap")
    parser.add_argument("--pre-enter", choices=("none", "correction", "revert", "retype"), default="none")
    parser.add_argument("--middle-parent", choices=("none", "correction", "erase", "revert"), default="none")
    parser.add_argument("--middle-cadence", choices=("parent-first", "child-first", "interleaved"), default="parent-first")
    parser.add_argument("--reuse-base", choices=("positive", "zero"), default="positive")
    parser.add_argument("--reuse-content", choices=("empty", "spaces", "nbsp", "br", "mixed", "mixed-br", "separators", "feff-leading", "feff-interior", "feff-post-br"), default="empty")
    parser.add_argument("--sequence-count", type=int, choices=range(1, 17), default=2)
    parser.add_argument("--sequence-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--sequence-layout", choices=("no-following", "text-following", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--sequence-cadence", choices=("straight", "correction-before-enter", "erase-before-enter", "revert-before-enter", "trailing-empty", "revisit-correction", "revisit-erase", "revisit-revert", "parent-correction", "parent-erase", "parent-revert", "preenter-correction", "preenter-erase", "preenter-revert"), default="straight")
    parser.add_argument("--second-middle", choices=("none", "parent", "child"), default="none")
    parser.add_argument("--second-middle-cadence", choices=("straight", "edited"), default="straight")
    args = parser.parse_args()
    from canvas_second_middle_wire import selectors
    try: selectors(args.mode,args.typing,args.pre_enter,args.middle_parent,args.middle_cadence,args.second_middle,args.second_middle_cadence)
    except RuntimeError as error: parser.error(str(error))
    if args.middle_parent != "none" and args.mode != "middle":
        parser.error("Parent-after-middle input requires --mode middle")
    if args.middle_parent == "none" and args.middle_cadence != "parent-first":
        parser.error("Middle cadence requires --middle-parent")
    if args.pre_enter != "none" and args.mode != "middle":
        parser.error("Pre-Enter input requires middle mode")
    if args.parent_mode == "retained-zero" and (args.mode != "middle" or (args.pre_enter == "none" and args.middle_parent == "none" and args.second_middle == "none")):
        parser.error("Zero-base parent requires pre-middle input")
    if args.mode != "sequence" and (args.sequence_count != 2 or args.sequence_history != "bootstrap"
            or args.sequence_layout != "no-following" or args.sequence_cadence != "straight"):
        parser.error("Sequence selectors require sequence mode")
    if args.mode == "sequence" and args.typing != "none":
        parser.error("Sequence input uses sequence cadence, not legacy typing selectors")
    if (args.mode == "sequence" and args.sequence_cadence in ("correction-before-enter", "erase-before-enter", "revert-before-enter")
            and args.sequence_count not in (2, 3, 8, 16)):
        parser.error("Sequence correction cadence requires a pinned 2, 3, 8 or 16 append cohort")
    if args.mode == "sequence" and args.sequence_count < 2 and not args.sequence_cadence.startswith("preenter-"):
        parser.error("Existing sequence routes require at least two appended members")
    if args.mode == "sequence" and args.sequence_cadence.startswith(("revisit-", "parent-")) and args.sequence_count < 3:
        parser.error("Sequence revisit cadence requires at least 3 appends")
    if args.mode != "repeat" and (args.repeat_history != "bootstrap" or args.repeat_layout != "no-following"):
        parser.error("Repeat source selectors require repeat mode")
    if args.mode == "empty" and args.typing == "none" and args.corpus != "ascii":
        parser.error("Empty source uses ascii corpus; Unicode neighbor is selected by empty layout")
    if args.mode != "empty" and (args.empty_history != "bootstrap" or args.empty_layout != "no-following"):
        parser.error("Empty source selectors require empty mode")
    if args.typing == "restore" and args.mode not in ("repeat", "empty", "start", "middle"):
        parser.error("Restoring original text requires repeat, empty, start or middle mode")
    if args.parent_mode != "bootstrap" and args.mode not in ("start", "middle"):
        parser.error("Retained parent history requires start or middle mode")
    if args.reuse_base == "zero" and args.mode != "reuse":
        parser.error("Zero-base history requires reuse mode")
    if args.reuse_content != "empty" and args.mode != "reuse":
        parser.error("Clearing existing content requires reuse mode")
    scenario = args.scenario
    class Storage(SplitStorage):
        @classmethod
        def prepare(cls, directory, fixture, checks):
            if args.mode == "sequence":
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT"] = str(args.sequence_count)
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_HISTORY"] = args.sequence_history
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_LAYOUT"] = args.sequence_layout
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE"] = args.sequence_cadence
            if args.mode == "repeat":
                checks["SUPER_PLATINUM_CANVAS_REPEAT_HISTORY"] = args.repeat_history
                checks["SUPER_PLATINUM_CANVAS_REPEAT_LAYOUT"] = args.repeat_layout
            if args.mode == "empty":
                checks["SUPER_PLATINUM_CANVAS_EMPTY_HISTORY"] = args.empty_history
                checks["SUPER_PLATINUM_CANVAS_EMPTY_LAYOUT"] = args.empty_layout
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MODE"] = args.mode
            checks["SUPER_PLATINUM_CANVAS_SPLIT_TYPING"] = args.typing
            checks["SUPER_PLATINUM_CANVAS_SECOND_MIDDLE"] = args.second_middle
            checks["SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE"] = args.second_middle_cadence
            checks["SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER"] = args.pre_enter
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT"] = args.middle_parent
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE"] = args.middle_cadence
            checks["SUPER_PLATINUM_CANVAS_SPLIT_PARENT"] = args.parent_mode
            checks["SUPER_PLATINUM_CANVAS_REUSE_BASE"] = args.reuse_base
            checks["SUPER_PLATINUM_CANVAS_REUSE_CONTENT"] = args.reuse_content
            return super().prepare(directory, fixture, checks)
    input_base = CanvasSplitInput
    if args.mode == "sequence":
        from canvas_sequence_input import CanvasSequenceInput
        input_base = CanvasSequenceInput
    elif args.mode == "repeat" and args.typing != "none":
        from canvas_repeat_typing_input import CanvasRepeatTypingInput
        input_base = CanvasRepeatTypingInput
    elif args.mode == "repeat":
        from canvas_repeat_enter_input import CanvasRepeatEnterInput
        input_base = CanvasRepeatEnterInput
    if args.mode == "empty":
        from canvas_empty_enter_input import CanvasEmptyEnterInput
        input_base = CanvasEmptyEnterInput
    class Input(input_base):
        @classmethod
        def prepare(cls, directory, fixture, checks):
            if args.mode == "sequence":
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT"] = str(args.sequence_count)
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_HISTORY"] = args.sequence_history
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_LAYOUT"] = args.sequence_layout
                checks["SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE"] = args.sequence_cadence
            if args.mode == "repeat":
                checks["SUPER_PLATINUM_CANVAS_REPEAT_HISTORY"] = args.repeat_history
                checks["SUPER_PLATINUM_CANVAS_REPEAT_LAYOUT"] = args.repeat_layout
            if args.mode == "empty":
                checks["SUPER_PLATINUM_CANVAS_EMPTY_HISTORY"] = args.empty_history
                checks["SUPER_PLATINUM_CANVAS_EMPTY_LAYOUT"] = args.empty_layout
            checks["SUPER_PLATINUM_CANVAS_SPLIT_SCENARIO"] = scenario
            checks["SUPER_PLATINUM_CANVAS_SPLIT_CORPUS"] = args.corpus
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MODE"] = args.mode
            checks["SUPER_PLATINUM_CANVAS_SPLIT_TYPING"] = args.typing
            checks["SUPER_PLATINUM_CANVAS_SECOND_MIDDLE"] = args.second_middle
            checks["SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE"] = args.second_middle_cadence
            checks["SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER"] = args.pre_enter
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT"] = args.middle_parent
            checks["SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE"] = args.middle_cadence
            checks["SUPER_PLATINUM_CANVAS_SPLIT_PARENT"] = args.parent_mode
            checks["SUPER_PLATINUM_CANVAS_REUSE_BASE"] = args.reuse_base
            checks["SUPER_PLATINUM_CANVAS_REUSE_CONTENT"] = args.reuse_content
            return super().prepare(directory, fixture, checks)
    stages = tuple("typed-child" if s == "split" and args.typing != "none" else s
                   for s in STAGES[scenario])
    if args.mode == "sequence":
        stages = (("sequence", "cancelled", "saving", "confirmed")
                  + tuple(f"member-{i}-{suffix}" for i in reversed(range(args.sequence_count))
                          for suffix in ("editing", "saving", "confirmed"))
                  + ("parent-editing", "parent-saving", "parent-confirmed")
                  if scenario == "success" else ("sequence",) + STAGES[scenario][1:])
    if args.mode == "repeat":
        first = "repeat" if args.typing == "none" else "typed-third"
        stages = ((first, "cancelled", "saving", "confirmed", "second-editing", "second-saving",
                   "second-confirmed", "child-editing", "continued-saving", "continued-confirmed",
                   "parent-editing", "parent-saving", "parent-confirmed")
                  if scenario == "success" else (first,) + STAGES[scenario][1:])
    if args.mode == "empty":
        stages = ("empty" if args.typing == "none" else "typed-child",) + STAGES[scenario][1:]
        if scenario == "success":
            stages += ("parent-editing", "parent-saving", "parent-confirmed")
    if args.mode == "start":
        first = "start" if args.typing == "none" else "typed-original"
        stages = ((first, "cancelled", "saving", "confirmed", "new-editing", "new-saving",
                   "new-confirmed", "original-editing", "original-saving", "original-confirmed")
                  if scenario == "success" else (first,) + STAGES[scenario][1:])
    if args.mode == "middle" and (args.typing != "none" or args.pre_enter != "none" or args.middle_parent != "none") and scenario == "success":
        stages += ("parent-editing", "parent-saving", "parent-confirmed")
    if args.middle_parent != "none":stages=("parent-typed",)+stages[1:]
    if args.second_middle != "none":
        stages=(("second-split","cancelled","saving","confirmed") + tuple(f"member-{i}-{suffix}" for i in range(2) for suffix in ("editing","saving","confirmed")) + ("parent-editing","parent-saving","parent-confirmed") if scenario=="success" else ("second-split",)+STAGES[scenario][1:])
    spec = CanvasCheck(fixture="canvas-structure", prefix="canvas-split",
                       flag="SUPER_PLATINUM_CANVAS_SPLIT_CHECK", required=("trustedNativeEnter",),
                       counts={}, stages=stages)
    canvas_native.validate = lambda spec, fixture, receipt: validate(spec, fixture, receipt, args.corpus, args.mode, args.typing, args.reuse_base, args.reuse_content, args.parent_mode, args.empty_history, args.empty_layout, args.repeat_history, args.repeat_layout, args.sequence_count, args.sequence_history, args.sequence_layout, args.sequence_cadence, args.pre_enter, args.middle_parent, args.middle_cadence, args.second_middle, args.second_middle_cadence)
    canvas_native.validate_closed = validate_closed
    canvas_native.run(spec, args.fixture, args.binary.resolve(), args.captures.resolve(),
                      storage_factory=Storage, input_factory=Input)


if __name__ == "__main__":
    main()
