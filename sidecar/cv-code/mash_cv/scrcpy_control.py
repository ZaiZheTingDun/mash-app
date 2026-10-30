"""scrcpy 2.7 touch wire format and execution of Rust-scheduled events.

This module has no CV or gesture policy. A request contains a complete single
contact cycle in video coordinates, with offsets from the gesture start.
Protocol: Genymobile/scrcpy, tag v2.7, app/src/control_msg.c.
"""

import socket
import struct
import time


def touch_packet(action: int, x: int, y: int, width: int, height: int) -> bytes:
    # type, action, pointer id, x/y, video width/height, pressure, buttons.
    # A nonnegative pointer id represents a finger (not scrcpy's mouse id -1).
    return struct.pack(
        ">BBQiiHHHII", 2, action, 0, x, y, width, height,
        0 if action == 1 else 0xFFFF, 0, 0,
    )


def validate_events(events: list, width: int, height: int) -> None:
    if not (0 < width <= 65535 and 0 < height <= 65535):
        raise ValueError("invalid scrcpy video dimensions")
    if not isinstance(events, list) or not 2 <= len(events) <= 604:
        raise ValueError("expected a complete scrcpy gesture")
    previous = 0
    for index, event in enumerate(events):
        if not isinstance(event, dict):
            raise ValueError("invalid scrcpy touch event")
        action, x, y, at_ms = (event.get(key) for key in ("action", "x", "y", "atMs"))
        if any(type(value) is not int for value in (action, x, y, at_ms)):
            raise ValueError("touch event fields must be integers")
        expected = 0 if index == 0 else 1 if index == len(events) - 1 else 2
        if action != expected or not (0 <= x < width and 0 <= y < height):
            raise ValueError("invalid scrcpy contact sequence or coordinates")
        if not previous <= at_ms <= 60_000 or (index == 0 and at_ms != 0):
            raise ValueError("invalid scrcpy touch schedule")
        previous = at_ms


class ScrcpyControl:
    def __init__(self, sock: socket.socket):
        self._sock = sock
        self._pressed = None
        self._failed = False

    def send(self, events: list, width: int, height: int) -> None:
        validate_events(events, width, height)  # Validate everything before DOWN.
        if self._failed:
            raise RuntimeError("scrcpy control connection failed; restart the stream")
        deadline = time.monotonic()
        previous_ms = 0
        try:
            for event in events:
                deadline += (event["atMs"] - previous_ms) / 1000
                remaining = deadline - time.monotonic()
                if remaining > 0:
                    time.sleep(remaining)
                point = (event["x"], event["y"], width, height)
                # Record before writing: a failing sendall may have sent DOWN.
                if event["action"] != 1:
                    self._pressed = point
                self._sock.sendall(touch_packet(event["action"], *point))
                if event["action"] == 1:
                    self._pressed = None
                # scrcpy timestamps events on receipt. Never burst overdue
                # MOVE events or shorten the stationary tail after a host
                # scheduling stall / slow write; let the gesture run longer.
                deadline = max(deadline, time.monotonic())
                previous_ms = event["atMs"]
        except Exception:
            self._failed = True
            raise
        finally:
            self.release()

    def release(self) -> None:
        if self._pressed is not None:
            try:
                self._sock.sendall(touch_packet(1, *self._pressed))
            except OSError:
                pass
            self._pressed = None

    def close(self) -> None:
        self.release()
        self._failed = True
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self._sock.close()
