from mash_cv import cv
from mash_cv import stream as stream_module
from mash_cv.stream import ScrcpyStream, _adb_base, _is_tcp_serial


def test_adb_base_uses_explicit_adb_path_and_serial():
    assert _adb_base("/opt/platform-tools/adb", "127.0.0.1:5555") == [
        "/opt/platform-tools/adb",
        "-s",
        "127.0.0.1:5555",
    ]


def test_is_tcp_serial_only_matches_host_port_serials():
    assert _is_tcp_serial("127.0.0.1:5555")
    assert not _is_tcp_serial("emulator-5554")
    assert not _is_tcp_serial(None)


def test_scrcpy_server_command_limits_stream_to_configured_fps():
    stream = ScrcpyStream(
        jar_path="/tmp/scrcpy-server.jar",
        adb_path="/opt/platform-tools/adb",
        serial="127.0.0.1:5555",
        max_size=1920,
        bit_rate=12_000_000,
        max_fps=15,
    )

    command = stream._server_command()

    assert "max_size=1920" in command
    assert "video_bit_rate=12000000" in command
    assert "max_fps=15" in command


def test_start_stream_forwards_max_fps(monkeypatch, tmp_path):
    captured = {}

    class FakeScrcpyStream:
        def __init__(self, **kwargs):
            captured.update(kwargs)

        def start(self):
            return 1920, 1080

        def wait_for_frame(self, timeout):
            return object()

        def stop(self):
            pass

    jar_path = tmp_path / "scrcpy-server.jar"
    jar_path.write_bytes(b"")
    monkeypatch.setattr(stream_module, "ScrcpyStream", FakeScrcpyStream)
    monkeypatch.setattr(cv, "stream", None)

    result = cv._start_stream(
        {
            "jarPath": str(jar_path),
            "adbPath": "/opt/platform-tools/adb",
            "maxSize": 1920,
            "bitRate": 12_000_000,
            "maxFps": 15,
        }
    )

    assert result == {"ok": True, "width": 1920, "height": 1080}
    assert captured["max_fps"] == 15
