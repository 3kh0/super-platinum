"""Scoped missing-channel loading and original held-reply completion evidence."""
from workflow_activity_channel_spec import ChannelSpec

OPENING = "Opening conversation\u2026"
OFFLINE = "Connect to Slack to open this conversation."


class HeldSuccessSpec(ChannelSpec):
    @property
    def case(self):
        return "channel-held-success"


HELD_SUCCESS = HeldSuccessSpec()


def validate_held(exercise, model, channel, geometry, viewport, require, rectangle):
    require(isinstance(exercise, dict) and exercise.get("observedMs") == 500
            and exercise.get("unchangedRustState") is True
            and exercise.get("unchangedEndpoint") is True,
            "Held channel status changed the original model or endpoint")
    before, after = exercise.get("nativeBefore"), exercise.get("nativeAfter")
    require(isinstance(before, dict) and before == after and before.get("sameOriginalNodes") is True,
            "Held channel status replaced or changed its original native owner")
    owner, scope = before.get("owner", {}), before.get("scope", {})
    docs = channel.get("documents", [])
    require(len(docs) == 1 and type(owner.get("chain")) is int and owner["chain"] > 0
            and owner.get("phase") == "pending" and isinstance(owner.get("key"), str)
            and owner["key"] and owner.get("button") == "workflow-activity-channel-step-0-1"
            and owner.get("channel") == "CACTMISS01",
            "Loading notice does not belong to the original missing-channel attempt")
    require(all(isinstance(scope.get(k), str) and scope[k].isdigit()
                for k in ("visit", "document", "revision", "detail"))
            and scope["visit"] == str(channel.get("managementVisit"))
            and scope["document"] == str(docs[0].get("id"))
            and scope["revision"] == str(docs[0].get("revision"))
            and owner.get("detail") == scope["detail"],
            "Loading status borrowed another detail or dirty document")
    require(model.get("metadata", {}).get("busy") is False
            and model["metadata"].get("pending") is False,
            "Pending channel navigation blocked metadata editing")
    native = before.get("native", {})
    require(native.get("composing") is False and native.get("leaseAbsent") is True
            and type(native.get("refusals")) is int and native["refusals"] >= 0,
            "Held read retained a native lease or changed composition/refusal state")
    buffers = before.get("buffers", {})
    search, date = buffers.get("search", {}), buffers.get("date", {})
    require(isinstance(search.get("value"), str)
            and isinstance(search.get("generation"), str) and search["generation"].isdigit()
            and all(search.get(k) is None or type(search[k]) is int
                    and 0 <= search[k] <= len(search["value"]) for k in ("start", "end"))
            and all(isinstance(buffers.get(k), str) for k in ("status", "user", "limit"))
            and set(date) == {"from", "to", "revision", "epoch", "open", "dirty", "pending",
                              "origin", "defaultFrom", "defaultTo"}
            and all(isinstance(v, str) for v in date.values()),
            "Missing original native search/filter/date buffers")
    notice = before.get("notice", {})
    require(notice.get("text") == OPENING and notice.get("role") == "status"
            and notice.get("visible") is True and notice.get("outsideScroll") is True
            and notice.get("visiblePending") == 1,
            "Loading guidance is missing, duplicated, stale or hidden")
    rectangle(notice.get("rect"), viewport)
    notices = geometry.get("notices", [])
    require(sum(n.get("text") == OPENING for n in notices) == 1
            and not any(n.get("text") == OFFLINE for n in notices),
            "Layout retained offline guidance or lost the actual pending status")


def validate_retired(exercise, geometry, require):
    require(exercise == dict(delivered=False, cancelledBeforeReply=True,
                            cleanup=dict(oldPhase="settled", oldDetailConnected=False,
                                         visiblePending=0, currentPending=False)),
            "Retired read kept its pending owner or remained publishable")
    require(not any(n.get("text") == OPENING for n in geometry.get("notices", [])),
            "Retired loading guidance leaked into the successor")


def validate_completion(exercise, require):
    require(isinstance(exercise, dict) and exercise.get("heldCompletion") ==
            dict(serial=3, requestCount=3, delivered=True, cancelledBeforeReply=False),
            "Successful held completion did not release the original third request")
