"""Disconnected loading motion at both native sizes and process-local preferences."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import sys

from canvas_native import CanvasCheck, run
from native_fixture import DEFAULT_BINARY, ROOT
from native_check import expected_viewport

FIXTURES = ("profile-loading", "profile-loading-narrow")
NAMES = ("profile", "workspace", "connecting", "no-network")
SIZES = (16, 26, 12, 12)
DURATIONS = ("0.8s", "0.8s", "0.9s", "0.9s")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def validate_motion(evidence, fixture, reduce):
    require(evidence.get("ok") is True, "Native runner failed")
    native = evidence["native"]
    require(native.get("fixture") == fixture and native.get("reduce") is reduce,
            "Wrong fixture or preference")
    require(native.get("states") == 4 and native.get("samplesPerState") == 8,
            "Wrong temporal matrix")
    require(native.get("probeRemoved") is True, "Owned probe remained")
    observations = native.get("observations")
    require(isinstance(observations, list) and len(observations) == 4,
            "Missing observations")
    viewport = expected_viewport(fixture)
    for index, observation in enumerate(observations):
        require(observation.get("name") == NAMES[index]
                and observation.get("reduce") is reduce and observation.get("ok") is True,
                "Wrong observation order")
        samples = observation.get("samples")
        require(isinstance(samples, list) and len(samples) == 8, "Wrong sample count")
        for sample in samples:
            require(number(sample.get("at")) and sample.get("reduce") is reduce,
                    "Malformed time or observed preference")
            require(sample.get("name") == ("none" if reduce else "spin")
                    and sample.get("duration") == ("0s" if reduce else DURATIONS[index]),
                    "Computed animation mismatch")
            require(sample.get("width") == SIZES[index] and sample.get("height") == SIZES[index],
                    "Fixed geometry changed")
            require(type(sample.get("activeAnimations")) is int
                    and sample.get("activeAnimations") == (0 if reduce else 1),
                    "Unexpected active animation count")
            rect = sample.get("rect", {})
            require(all(number(rect.get(key)) for key in ("left", "top", "right", "bottom")),
                    "Malformed indicator rect")
            require(0 <= rect["left"] < rect["right"] <= viewport[0]
                    and 0 <= rect["top"] < rect["bottom"] <= viewport[1],
                    "Indicator is outside native viewport")
            require(sample.get("borderTopColor") != sample.get("borderRightColor"),
                    "Loading ring leading segment lost")
            if reduce:
                require(sample.get("animationTime") is None and sample.get("transform") == "none",
                        "Reduced indicator moved")
            else:
                require(number(sample.get("animationTime")), "Missing animation time")
        elapsed = samples[-1]["at"] - samples[0]["at"]
        require(900 <= elapsed < 15000 and abs(elapsed-observation["elapsed"]) < .001,
                "Invalid temporal interval")
        require(all(b["at"] > a["at"] for a, b in zip(samples, samples[1:])),
                "Sample timestamps did not advance")
        if not reduce:
            require(samples[-1]["animationTime"]-samples[0]["animationTime"] >= 500,
                    "Normal animation clock did not advance")
            require(all(b["animationTime"] >= a["animationTime"] for a, b in zip(samples, samples[1:])),
                    "Normal animation restarted")
            require(any(sample["transform"] != samples[0]["transform"] for sample in samples[1:]),
                    "Normal indicator did not rotate")
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32", "This checker requires Windows WebView2")
    require(not any(key.startswith("WEBVIEW2_") for key in os.environ),
            "Inherited WEBVIEW2 overrides would invalidate the owned fixture environment")
    binary, captures = args.binary.resolve(), args.captures.resolve()
    require(not captures.exists(), "Use a fresh capture directory")
    captures.mkdir(parents=True)
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    result = {"ok":False, "binary":str(binary), "binarySha256":binary_hash, "cases":[]}
    observations = {}
    try:
        for mode, reduce, flag in (
            ("reduce", True, "SUPER_PLATINUM_MOTION_REDUCE_CHECK"),
            ("normal", False, "SUPER_PLATINUM_MOTION_NORMAL_CHECK"),
        ):
            spec = CanvasCheck("profile-loading", flag,
                               ("motionVerified", "temporalVerified", "profileLoading", "probeRemoved"),
                               {"states":4, "samplesPerState":8}, NAMES)
            for fixture in FIXTURES:
                run(spec, fixture, binary, captures / mode)
                receipt = captures / mode / f"{fixture}.json"
                evidence = json.loads(receipt.read_text(encoding="utf-8"))
                observations[(mode, fixture)] = validate_motion(evidence, fixture, reduce)
                result["cases"].append({"mode":mode, "fixture":fixture, "receipt":str(receipt),
                                        "sha256":hashlib.sha256(receipt.read_bytes()).hexdigest()})
        for fixture in FIXTURES:
            for normal, reduced in zip(observations[("normal", fixture)], observations[("reduce", fixture)]):
                for a, b in zip(normal["samples"], reduced["samples"]):
                    require(all(a[key] == b[key] for key in ("width", "height", "borderTopColor", "borderRightColor")),
                            "Ring geometry or colors differed between preferences")
        require(hashlib.sha256(binary.read_bytes()).hexdigest() == binary_hash, "Native binary changed")
        result.update(ok=True, processes=4, states=16, temporalSamples=128, expectedCaptures=20,
                      nativeVisualReview="REQUIRED_SEPARATELY", globalWindowsSettingsChanged=False)
    except BaseException as error:
        result["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        (captures / "motion-matrix.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")


if __name__ == "__main__":
    main()
