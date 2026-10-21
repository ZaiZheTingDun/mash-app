import base64
import errno
from types import SimpleNamespace

import av
import cv2
import numpy as np
import pytest

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
        display_id=2,
    )

    command = stream._server_command()

    assert "max_size=1920" in command
    assert "video_bit_rate=12000000" in command
    assert "max_fps=15" in command
    assert "display_id=2" in command


def test_scrcpy_defaults_to_main_display_for_existing_callers():
    stream = ScrcpyStream(jar_path="/tmp/scrcpy-server.jar")
    assert "display_id=0" in stream._server_command()


def test_frame_conversion_recovers_mislabeled_android_sdr_metadata(capsys):
    stream = ScrcpyStream(jar_path="/tmp/scrcpy-server.jar")
    pixels = np.full((24, 32, 3), (32, 128, 224), dtype=np.uint8)
    frame = av.VideoFrame.from_ndarray(pixels, format="bgr24").reformat(format="yuv420p")
    frame.colorspace, frame.color_primaries, frame.color_trc = 6, 6, 6
    frame.color_range = 1
    expected = frame.to_ndarray(format="bgr24")
    frame.colorspace, frame.color_primaries, frame.color_trc = 8, 2, 10

    # Reproduce the metadata emitted by the Meizu encoder, without a device.
    with pytest.raises(av.error.OSError) as error:
        frame.to_ndarray(format="bgr24")
    assert error.value.errno == errno.ENOTSUP

    for _ in range(2):
        assert np.array_equal(stream._frame_to_bgr(frame), expected)
        assert (frame.colorspace, frame.color_primaries, frame.color_trc) == (8, 2, 10)
        assert frame.color_range == 1
    assert capsys.readouterr().err.count("using BT.601 for mislabeled SDR frames") == 1


@pytest.mark.parametrize("colorspace,primaries,trc", [(1, 1, 1), (6, 6, 6), (2, 2, 2)])
def test_frame_conversion_preserves_supported_color_metadata(colorspace, primaries, trc, capsys):
    stream = ScrcpyStream(jar_path="/tmp/scrcpy-server.jar")
    pixels = np.full((24, 32, 3), (32, 128, 224), dtype=np.uint8)
    frame = av.VideoFrame.from_ndarray(pixels, format="bgr24").reformat(format="yuv420p")
    frame.colorspace, frame.color_primaries, frame.color_trc = colorspace, primaries, trc

    assert np.array_equal(stream._frame_to_bgr(frame), frame.to_ndarray(format="bgr24"))
    assert (frame.colorspace, frame.color_primaries, frame.color_trc) == (colorspace, primaries, trc)
    assert not capsys.readouterr().err


@pytest.mark.parametrize(
    "error_number,pixel_format,metadata,color_range",
    [
        (errno.EINVAL, "yuv420p", (8, 2, 10), 1),
        (errno.ENOTSUP, "yuv420p", (1, 1, 1), 1),
        (errno.ENOTSUP, "yuv420p10le", (8, 2, 10), 1),
        (errno.ENOTSUP, "yuv420p", (8, 2, 10), 2),
        (errno.ENOTSUP, "yuv420p", (8, 2, 10), 1),
    ],
)
def test_frame_conversion_does_not_mask_other_failures(error_number, pixel_format, metadata, color_range):
    failure = av.error.OSError(error_number, "conversion failed")

    class FailingFrame:
        format = SimpleNamespace(name=pixel_format)

        def __init__(self):
            self.colorspace, self.color_primaries, self.color_trc = metadata
            self.color_range = color_range

        def to_ndarray(self, **kwargs):
            raise failure

    frame = FailingFrame()
    with pytest.raises(av.error.OSError) as error:
        ScrcpyStream(jar_path="/tmp/scrcpy-server.jar")._frame_to_bgr(frame)
    assert error.value is failure
    assert (frame.colorspace, frame.color_primaries, frame.color_trc) == metadata


def test_get_latest_png_preserves_frame_pixels(monkeypatch):
    stream = ScrcpyStream(
        jar_path="/tmp/scrcpy-server.jar",
        adb_path="/opt/platform-tools/adb",
        serial="emulator-5554",
    )
    frame = np.array(
        [
            [[0, 32, 255], [255, 64, 0]],
            [[12, 128, 240], [200, 16, 80]],
        ],
        dtype=np.uint8,
    )
    monkeypatch.setattr(stream, "get_latest_bgr", lambda: frame.copy())

    encoded = stream.get_latest_png()
    decoded = cv2.imdecode(np.frombuffer(encoded, dtype=np.uint8), cv2.IMREAD_COLOR)

    assert np.array_equal(decoded, frame)


def test_get_frame_supports_png_and_keeps_jpeg_as_default(monkeypatch):
    class FakeStream:
        width = 1920
        height = 1080

        def get_latest_png(self, wait):
            assert wait == 0.25
            return b"png-frame"

        def get_latest_jpeg(self, quality, wait):
            assert quality == 91
            assert wait == 0.5
            return b"jpeg-frame"

    monkeypatch.setattr(cv, "stream", FakeStream())

    png_result = cv._get_frame({"format": "png", "waitSeconds": 0.25})
    jpeg_result = cv._get_frame({"quality": 91, "waitSeconds": 0.5})

    assert base64.b64decode(png_result["pngB64"]) == b"png-frame"
    assert "jpegB64" not in png_result
    assert base64.b64decode(jpeg_result["jpegB64"]) == b"jpeg-frame"
    assert "pngB64" not in jpeg_result


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
            "displayId": 2,
        }
    )

    assert result == {"ok": True, "width": 1920, "height": 1080, "displayId": 2}
    assert captured["max_fps"] == 15
    assert captured["display_id"] == 2
