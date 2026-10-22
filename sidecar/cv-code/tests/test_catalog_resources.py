"""Catalog data must be supplied explicitly by the installed resource pack."""
import json

import mash_cv.cv as cv_module


def test_servants_catalog_does_not_search_legacy_bundled_data(monkeypatch, tmp_path):
    legacy = tmp_path / "src-tauri/src/resources/servants.json"
    legacy.parent.mkdir(parents=True)
    legacy.write_text('[{"id": 18}]', encoding="utf-8")
    monkeypatch.setattr(cv_module, "__file__", str(tmp_path / "sidecar/mash_cv/cv.py"))
    monkeypatch.delenv("MASH_CV_SERVANTS_JSON_PATH", raising=False)
    assert cv_module._servants_json_path() is None
    monkeypatch.setenv("MASH_CV_SERVANTS_JSON_PATH", str(tmp_path / "missing.json"))
    assert cv_module._servants_json_path() is None


def test_servants_catalog_reads_the_explicit_installed_path(monkeypatch, tmp_path):
    path = tmp_path / "catalogs/servants.json"
    path.parent.mkdir()
    path.write_text(json.dumps([{"id": 19}]), encoding="utf-8")
    monkeypatch.setenv("MASH_CV_SERVANTS_JSON_PATH", str(path))
    monkeypatch.setattr(cv_module, "_servant_catalog_cache", None)
    assert cv_module._servants_json_path() == str(path)
    assert cv_module._load_servant_catalog() == [{"id": 19}]
