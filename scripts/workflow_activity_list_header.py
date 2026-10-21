"""Actual loaded List heading, independently from its original raw document."""
import workflow_activity_files as F
import workflow_activity_file_emoji as E


def validate(value, viewport, h, *, emoji=False, cold=False, encoded=False):
    require = h["require"]
    semantic = (E.LIST_COLD_LABEL if cold else E.LIST_LABEL) if emoji else F.LIST_LABEL
    painted = (E.LIST_COLD_TEXT if cold else E.LIST_TEXT) if emoji else F.LIST_LABEL
    if encoded:
        from workflow_activity_file_encoded import HEADER_TEXT
        semantic = painted = HEADER_TEXT
    proof = value.get("titleProof")
    require(value.get("title") == painted and value.get("semanticTitle") == semantic
            and isinstance(proof, dict) and proof.get("ok") is True
            and proof.get("id") == "slack-list-title" and proof.get("tag") == "H2"
            and proof.get("semanticLabel") == semantic and proof.get("text") == painted
            and proof.get("visible") is True and proof.get("nestedActions") == 0,
            "Actual List heading text/semantic identity is missing or changed")
    h["rectangle"](proof.get("rect"), viewport)
    title_controls = [row for row in value.get("controls", []) if row.get("id") == "slack-list-title"]
    require(len(title_controls) == 1 and title_controls[0].get("text") == painted
            and title_controls[0].get("visible") is True,
            "List heading control observation substituted semantic text for paint")
    if emoji:
        E.validate_paint(dict(proof, kind="list"), viewport, h, cold=cold)
    else:
        parts = proof.get("fragments")
        require(proof.get("directChildren") == 1 and isinstance(parts, list) and len(parts) == 1,
                "Ordinary title gained extra fragments")
        part = parts[0]
        require(part.get("ok") is True and type(part.get("index")) is int and part["index"] == 0
                and part.get("tag") == "SPAN" and part.get("kind") == "text"
                and part.get("text") == semantic and part.get("title") is None
                and part.get("image") is None and part.get("visible") is True,
                "Plain List title changed or gained image/tooltip behavior")
        h["rectangle"](part.get("rect"), viewport)
