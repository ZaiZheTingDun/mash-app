import os

import pytest

from mash_cv.host import process as host_process


def test_external_environment_restores_original_library_path(monkeypatch):
    monkeypatch.setenv("LD_LIBRARY_PATH", "/bundle")
    monkeypatch.setenv("LD_LIBRARY_PATH_ORIG", "/host")
    monkeypatch.setenv("DYLD_LIBRARY_PATH", "/bundle")
    env = host_process.clean_external_env()
    assert env["LD_LIBRARY_PATH"] == "/host"
    assert "LD_LIBRARY_PATH_ORIG" not in env
    assert "DYLD_LIBRARY_PATH" not in env
    assert os.environ["LD_LIBRARY_PATH"] == "/bundle"


def test_windows_external_environment_removes_only_bundle_path_entries(monkeypatch, tmp_path):
    bundle = tmp_path / "bundle"
    outside = tmp_path / "bundle-extra"
    monkeypatch.setattr(host_process.sys, "platform", "win32")
    monkeypatch.setattr(host_process.sys, "_MEIPASS", str(bundle), raising=False)
    monkeypatch.setenv("PATH", os.pathsep.join(map(str, [bundle, bundle / "libs", outside])))
    assert host_process.clean_external_env()["PATH"] == str(outside)


@pytest.mark.parametrize("fail", [False, True])
def test_windows_external_process_restores_dll_directory_after_spawn(monkeypatch, fail):
    calls = []
    monkeypatch.setattr(host_process.sys, "platform", "win32")
    monkeypatch.setattr(host_process.sys, "frozen", True, raising=False)
    monkeypatch.setattr(host_process, "_get_dll_directory", lambda: "runtime-dlls")
    monkeypatch.setattr(host_process, "_set_dll_directory", calls.append)
    try:
        with host_process.external_process_scope():
            assert calls == [None]
            if fail:
                raise RuntimeError("spawn failed")
    except RuntimeError:
        assert fail
    assert calls == [None, "runtime-dlls"]


def test_windows_children_have_no_console(monkeypatch):
    monkeypatch.setattr(host_process.sys, "platform", "win32")
    monkeypatch.setattr(host_process.subprocess, "CREATE_NO_WINDOW", 0x08000000, raising=False)
    assert host_process.hidden_process_options() == {"creationflags": 0x08000000}
    monkeypatch.setattr(host_process.sys, "platform", "darwin")
    assert host_process.hidden_process_options() == {}
