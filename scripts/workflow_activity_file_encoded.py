"""Independent raw/Activity/header vectors for the default no-highlight path."""
import copy
import workflow_activity_files as F

LIST_TEXT = "R\u00e9vision <b> &lt;"
CANVAS_TEXT = "Canvas &lt;i&gt; &amp;lt;"
HEADER_TEXT = "R\u00e9vision &lt;b&gt; &amp;lt;"
ORIGINALS = copy.deepcopy(F.ORIGINALS)
ORIGINALS["list"]["file"]["title"] = "R\u00e9vision &amp;lt;b&amp;gt; &amp;amp;lt;"
ORIGINALS["canvas"]["file"]["title"] = "Canvas &amp;lt;i&amp;gt; &amp;amp;lt;"


def validate_literal(row, require):
    text = LIST_TEXT if row.get("kind") == "list" else CANVAS_TEXT
    proof = row.get("literalProof")
    require(isinstance(proof, dict) and proof.get("ok") is True
            and proof.get("semanticLabel") == text and proof.get("text") == text
            and proof.get("directChildren") == 1 and proof.get("nestedElements") == 0
            and proof.get("nestedActions") == 0,
            "Encoded title is not the exact literal semantic label")
    require(proof.get("fragments") == [
        dict(index=0, kind="text", tag="SPAN", text=text, title=None,
             childElements=0, childNodes=1, nodeType=3)
    ], "Encoded title gained HTML, extra decoding, emoji or hidden child nodes")
