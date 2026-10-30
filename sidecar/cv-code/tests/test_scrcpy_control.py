import io
import json
import os
import socket
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest

from mash_cv.scrcpy_control import ScrcpyControl, touch_packet
from mash_cv import cv, scrcpy_control
from mash_cv.stream import ScrcpyStream


def gesture():
    return [
        {"action": 0, "x": 10, "y": 20, "atMs": 0},
        {"action": 2, "x": 30, "y": 40, "atMs": 100},
        {"action": 2, "x": 30, "y": 40, "atMs": 280},
        {"action": 1, "x": 30, "y": 40, "atMs": 280},
    ]


def test_wire_packet_matches_scrcpy_27_big_endian_layout():
    packet = touch_packet(0, 256, 512, 1920, 1080)
    assert packet.hex() == "02000000000000000000000001000000020007800438ffff0000000000000000"
    assert len(packet) == 32
    assert struct.unpack(">BBQiiHHHII", touch_packet(1, 256, 512, 1920, 1080)) == (
        2, 1, 0, 256, 512, 1920, 1080, 0, 0, 0,
    )


def test_gesture_uses_absolute_deadlines_and_real_socket(monkeypatch):
    now = [0.0]
    waits = []
    monkeypatch.setattr(scrcpy_control.time, "monotonic", lambda: now[0])

    def sleep(seconds):
        waits.append(seconds)
        now[0] += seconds

    monkeypatch.setattr(scrcpy_control.time, "sleep", sleep)
    writer, reader = socket.socketpair()
    reader.settimeout(1)
    control = ScrcpyControl(writer)
    try:
        control.send(gesture(), 1920, 1080)
        data = bytearray()
        while len(data) < 128:
            data.extend(reader.recv(128 - len(data)))
        packets = [struct.unpack(">BBQiiHHHII", data[offset:offset + 32]) for offset in range(0, 128, 32)]
        assert [packet[1] for packet in packets] == [0, 2, 2, 1]
        assert [(packet[3], packet[4]) for packet in packets] == [(10, 20), (30, 40), (30, 40), (30, 40)]
        assert waits == pytest.approx([0.1, 0.18])
    finally:
        control.close()
        reader.close()


def test_scheduler_stall_does_not_burst_moves_or_shorten_the_hold(monkeypatch):
    now = [0.0]
    sent = []
    waits = []
    monkeypatch.setattr(scrcpy_control.time, "monotonic", lambda: now[0])

    def sleep(seconds):
        waits.append(seconds)
        now[0] += seconds
        if len(waits) == 1:
            now[0] += 0.2  # Simulate the host waking up late.

    class Socket:
        def sendall(self, packet):
            sent.append((packet[1], now[0]))
            if len(sent) == 2:
                now[0] += 0.05  # A slow write must not consume the next gap.

    monkeypatch.setattr(scrcpy_control.time, "sleep", sleep)
    events = [
        {"action": 0, "x": 0, "y": 0, "atMs": 0},
        {"action": 2, "x": 10, "y": 0, "atMs": 16},
        {"action": 2, "x": 20, "y": 0, "atMs": 32},
        {"action": 2, "x": 20, "y": 0, "atMs": 282},
        {"action": 1, "x": 20, "y": 0, "atMs": 282},
    ]
    ScrcpyControl(Socket()).send(events, 1920, 1080)
    assert sent[1][1] == pytest.approx(0.216)
    assert sent[2][1] - sent[1][1] >= 0.065
    assert sent[3][1] - sent[2][1] == pytest.approx(0.25)
    assert sent[-1][1] == pytest.approx(0.532)


class RecordingSocket:
    def __init__(self, fail_at=None):
        self.packets = []
        self.fail_at = fail_at

    def sendall(self, packet):
        self.packets.append(packet)
        if len(self.packets) == self.fail_at:
            raise BrokenPipeError("test disconnect")


def test_interrupted_gesture_releases_contact_and_rejects_replay(monkeypatch):
    monkeypatch.setattr(scrcpy_control.time, "sleep", lambda _: None)
    sock = RecordingSocket(fail_at=2)
    control = ScrcpyControl(sock)
    with pytest.raises(BrokenPipeError):
        control.send(gesture(), 1920, 1080)
    assert [packet[1] for packet in sock.packets] == [0, 2, 1]
    with pytest.raises(RuntimeError, match="restart"):
        control.send(gesture(), 1920, 1080)
    assert len(sock.packets) == 3


@pytest.mark.parametrize("field,value", [("action", 0), ("x", 1920), ("y", -1), ("atMs", 99), ("atMs", 60001), ("atMs", True)])
def test_entire_gesture_is_validated_before_down(field, value):
    events = gesture()
    events[-1][field] = value
    sock = RecordingSocket()
    with pytest.raises(ValueError):
        ScrcpyControl(sock).send(events, 1920, 1080)
    assert not sock.packets


def test_stream_rejects_stale_session_and_changed_dimensions_before_sending(monkeypatch):
    stream = ScrcpyStream("unused", control=True)
    sock = RecordingSocket()
    stream._control = ScrcpyControl(sock)
    stream._publish_frame(np.zeros((1080, 1920, 3), dtype=np.uint8))
    monkeypatch.setattr(stream, "is_decoder_alive", lambda: True)
    command = {"streamId": "old-session", "width": 1920, "height": 1080, "events": gesture()}
    with pytest.raises(ValueError, match="stale"):
        stream.send_touch(command)
    command["streamId"] = stream.control_info()["streamId"]
    command["width"] = 2560
    with pytest.raises(RuntimeError, match="dimensions"):
        stream.send_touch(command)
    assert not sock.packets


@pytest.mark.parametrize("failure,error", [
    ("stale_session", "stale scrcpy stream session"),
    ("changed_dimensions", "scrcpy frame dimensions changed or stream stopped; restart the stream"),
    ("stopped_decoder", "scrcpy frame dimensions changed or stream stopped; restart the stream"),
    ("unavailable_control", "scrcpy control unavailable"),
    ("invalid_events", "expected a complete scrcpy gesture"),
    ("broken_socket", "test disconnect"),
])
def test_repl_touch_failure_preserves_stream_and_handles_following_requests(
    monkeypatch, capsys, failure, error,
):
    stream = ScrcpyStream("unused", control=True)
    sock = RecordingSocket(fail_at=1 if failure == "broken_socket" else None)
    stream._control = ScrcpyControl(sock)
    stream._publish_frame(np.zeros((1080, 1920, 3), dtype=np.uint8))
    monkeypatch.setattr(stream, "is_decoder_alive", lambda: failure != "stopped_decoder")
    command = {
        "cmd": "scrcpy_touch", "id": 41,
        "streamId": stream.control_info()["streamId"],
        "width": 1920, "height": 1080, "events": gesture(),
    }
    if failure == "stale_session":
        command["streamId"] = "old-session"
    elif failure == "changed_dimensions":
        command["width"] = 2560
    elif failure == "unavailable_control":
        stream._control = None
    elif failure == "invalid_events":
        command["events"] = []
    commands = [command, {"cmd": "ping", "id": 42},
                {"cmd": "get_frame", "format": "png", "id": 43}]
    monkeypatch.setattr(cv, "stream", stream)
    monkeypatch.setattr(cv.sys, "stdin", io.StringIO("\n".join(json.dumps(c) for c in commands)))

    cv._main_repl()

    responses = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert responses[0] == {"ok": False, "error": error, "id": 41}
    assert responses[1] == {"ok": True, "recognitionDiagnosticsVersion": 1, "id": 42}
    assert responses[2]["id"] == 43
    assert responses[2]["ok"] is True
    assert responses[2]["pngB64"]
    assert cv.stream is stream
    assert not stream._stop_event.is_set()
    if failure == "broken_socket":
        assert [packet[1] for packet in sock.packets] == [0, 1]
    else:
        assert not sock.packets


def test_repl_touch_success_preserves_request_id(monkeypatch, capsys):
    stream = ScrcpyStream("unused", control=True)
    sock = RecordingSocket()
    stream._control = ScrcpyControl(sock)
    stream._publish_frame(np.zeros((1080, 1920, 3), dtype=np.uint8))
    monkeypatch.setattr(stream, "is_decoder_alive", lambda: True)
    command = {
        "cmd": "scrcpy_touch", "id": 41,
        "streamId": stream.control_info()["streamId"],
        "width": 1920, "height": 1080, "events": gesture(),
    }
    monkeypatch.setattr(scrcpy_control.time, "sleep", lambda _: None)
    monkeypatch.setattr(cv, "stream", stream)
    monkeypatch.setattr(cv.sys, "stdin", io.StringIO(json.dumps(command)))

    cv._main_repl()

    assert json.loads(capsys.readouterr().out) == {"ok": True, "id": 41}
    assert [packet[1] for packet in sock.packets] == [0, 2, 2, 1]


def test_repl_reports_unavailable_control_and_preserves_request_id():
    command = {"cmd": "scrcpy_touch", "id": 41, "events": gesture()}
    result = subprocess.run(
        [sys.executable, "-m", "mash_cv"],
        input=json.dumps(command) + '\n{"cmd":"quit"}\n',
        text=True, capture_output=True, timeout=30,
        env={**os.environ, "PYTHONPATH": str(Path(__file__).parents[1])},
    )
    assert result.returncode == 0
    response = json.loads(result.stdout.splitlines()[0])
    assert response == {"ok": False, "error": "stream not started", "id": 41}
