"""Independent expected raw titles and native painted-fragment acceptance."""
import copy
import re

import workflow_activity_files as F

LIST_LABEL = "Révision & 中 :wave: :activity_ready_alias: :unknown:"
CANVAS_LABEL = "Canvas & café :wave: :activity_cold: :unknown:"
LIST_TEXT = "Révision & 中 👋  :unknown:"
CANVAS_TEXT = "Canvas & café 👋 :activity_cold: :unknown:"
LIST_COLD_LABEL = "Révision & 中 :wave: :activity_cold: :unknown:"
LIST_COLD_TEXT = "Révision & 中 👋 :activity_cold: :unknown:"
ORIGINALS = copy.deepcopy(F.ORIGINALS)
ORIGINALS["list"]["file"]["title"] = "Révision &amp; 中 :wave: :activity_ready_alias: :unknown:"
ORIGINALS["canvas"]["file"]["title"] = "Canvas &amp; café :wave: :activity_cold: :unknown:"
COLD_ORIGINALS = copy.deepcopy(ORIGINALS)
COLD_ORIGINALS["list"]["file"]["title"] = "Révision &amp; 中 :wave: :activity_cold: :unknown:"
REFUSAL = "Finish editing the current text before opening this List. Your draft is unchanged."


def local_image(src):
    if not isinstance(src, str):
        return False
    return re.fullmatch(
        r"(?:http://super-platinum-media\.emoji/|super-platinum-media://emoji/)[a-zA-Z0-9-]+\?v=\d+", src
    ) is not None


def validate_paint(row, viewport, h, *, cold=False):
    require = h["require"]
    is_list = row["kind"] == "list"
    label = (LIST_COLD_LABEL if cold else LIST_LABEL) if is_list else CANVAS_LABEL
    require(row.get("semanticLabel") == label,
            "Painted title lost its original semantic label")
    expected = [
        ("text", "Révision & 中 " if is_list else "Canvas & café ", None),
        ("emoji", "👋", ":wave:"),
        ("text", " ", None),
        ("image", "", None) if is_list and not cold else ("emoji", ":activity_cold:", ":activity_cold:"),
        ("text", " ", None),
        ("text", ":unknown:", None),
    ]
    fragments = row.get("fragments")
    require(isinstance(fragments, list) and len(fragments) == len(expected)
            and row.get("directChildren") == len(expected) and row.get("nestedActions") == 0,
            "File title lost its exact typed child boundary")
    for index, (part, spec) in enumerate(zip(fragments, expected)):
        require(part.get("ok") is True and type(part.get("index")) is int
                and part["index"] == index and part.get("tag") == "SPAN"
                and tuple(part.get(k) for k in ("kind", "text", "title")) == spec
                and part.get("visible") is True,
                "File title fragment order/text/tooltip changed")
        h["rectangle"](part.get("rect"), viewport)
        image = part.get("image")
        if spec[0] != "image":
            require(image is None, "Text or cold emoji invented an image")
            continue
        require(isinstance(image, dict) and image.get("tag") == "IMG"
                and image.get("alt") == image.get("title") == ":activity_ready_alias:"
                and image.get("complete") is True and image.get("visible") is True
                and image.get("naturalWidth") == image.get("naturalHeight") == 16
                and local_image(image.get("src")),
                "Ready custom alias lost native media identity or did not decode")
        h["rectangle"](image.get("rect"), viewport)


def validate_refused_paint(value, notices, require):
    require(value.get("kind") == "title-image-alt"
            and all(value.get(k) is True for k in ("unchangedRustState", "noFileRead", "noDestination")),
            "Mutated title did not preserve original source and read boundary")
    attempt, native, restored = (value.get(k, {}) for k in ("attempt", "native", "restored"))
    count = attempt.get("refusals")
    require(type(count) is int and count >= 0
            and attempt.get("originalAlt") == ":activity_ready_alias:"
            and attempt.get("mutatedAlt") == "forged Activity file emoji"
            and attempt.get("semanticLabel") == LIST_LABEL and attempt.get("part") == "1"
            and attempt.get("originalConnected") is True and local_image(attempt.get("src")),
            "Missing actual mutation of the original image")
    require(native.get("refusals") == count + 1 and native.get("notice") == REFUSAL
            and native.get("alt") == attempt["mutatedAlt"]
            and all(native.get(k) is True for k in ("originalImage", "originalName", "originalAnchor"))
            and REFUSAL in [notice.get("text") for notice in notices],
            "Altered title did not reach original scoped navigation refusal")
    require(restored.get("alt") == attempt["originalAlt"] and restored.get("src") == attempt["src"]
            and all(restored.get(k) is True for k in
                    ("originalImage", "originalName", "originalAnchor", "originalChildren")),
            "Restoring title replaced the original native objects")
