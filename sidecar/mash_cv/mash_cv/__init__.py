from mash_cv import cv as _cv_module
from mash_cv.cv import (
    _detect_screen,
    _find_element,
    _find_element_by_name,
    _load_config,
    _load_templates,
    _match_template_region,
    _read_turn,
    _reply,
    _respond,
    main,
    templates,
)


def _set_config(new_config: dict) -> None:
    """Replace the module-level config (used by tests)."""
    _cv_module.config = new_config


def _get_config() -> dict:
    return _cv_module.config


__all__ = [
    "_detect_screen",
    "_find_element",
    "_find_element_by_name",
    "_get_config",
    "_load_config",
    "_load_templates",
    "_match_template_region",
    "_read_turn",
    "_reply",
    "_respond",
    "_set_config",
    "main",
    "templates",
]
