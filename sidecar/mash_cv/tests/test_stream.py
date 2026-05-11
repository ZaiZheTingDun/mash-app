from mash_cv.stream import _adb_base, _is_tcp_serial


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
