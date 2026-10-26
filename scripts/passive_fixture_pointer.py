"""Observe an owned fixture without moving or restoring the OS pointer.

Uses the existing bounded caption probe and post-response owner validation.
The pointer must already be outside the WebView client; interference fails.
"""
from visual_pointer import FixturePointer, titlebar_point
from windows_capture import contains_rect


class PassiveFixturePointer(FixturePointer):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.probe_point = None
        self.data.update(mode="passive", actual_position_changed=False,
                         restoration_required=False, closed=False)

    def park(self):
        raise RuntimeError("Passive guard cannot park the pointer")

    @staticmethod
    def clear(window, position):
        if contains_rect(window["client_rect_physical"], (*position, 1, 1)):
            raise RuntimeError("Pointer overlaps the owned WebView; passive capture refused")

    def arm(self):
        self.verify_identity()
        self.window = self.observe()
        self.original = self.placed = self.io.position()
        self.probe_point = titlebar_point(self.window)
        self.data.update(original=self.original, observed_position=self.original,
                         window=self.window, response_probe_point=self.probe_point)
        self.clear(self.window, self.original)
        self.check("armed", require_clear=True)

    def check(self, stage, require_clear=False):
        position = self.io.position()
        self.data["movement_observed"] |= position != self.original
        self.verify_identity()
        window = self.observe()
        self.data["checkpoints"].append(dict(stage=stage, position=position, window=window))
        if self.data["movement_observed"]:
            raise RuntimeError("Pointer movement observed; passive capture rejected")
        if self.identity(window) != self.identity(self.window):
            raise RuntimeError("Owned window changed during passive capture")
        self.clear(window, position)
        if not self.io.caption(window["hwnd"], self.probe_point):
            raise RuntimeError("Owned title bar response/occlusion check failed")
        self._after_caption(window, position, stage, require_clear)
        if require_clear and window.get("taskbar_client_overlap") is not False:
            raise RuntimeError("Taskbar overlap during passive capture")
        return window

    def close(self):
        if self.original is None:
            return
        try:
            position = self.io.position()
            self.data.update(position_at_close=position, closed=True)
            self.data["movement_observed"] |= position != self.original
            if self.data["movement_observed"]:
                raise RuntimeError("Pointer movement observed before passive cleanup")
        except (OSError, RuntimeError) as error:
            self.data["cleanup_error"] = str(error)
            raise


def validate_passive_cleanup(data):
    required = {"mode": "passive", "move_attempted": False, "move_observed": False,
                "actual_position_changed": False, "movement_observed": False,
                "restore_attempted": False, "restore_observed": False,
                "restoration_required": False, "closed": True}
    if (any((data.get(k) is not v) if type(v) is bool else (data.get(k) != v)
            for k, v in required.items())
            or data.get("cleanup_error") or data.get("restore_error")
            or data.get("position_at_close") != data.get("original")
            or data.get("original") is None):
        raise RuntimeError("Passive pointer/cleanup evidence is incomplete or changed")
