from mash_cv import cv as _cv_module
from mash_cv.cv import (
    DEFAULT_COMMAND_CARD_SLOTS,
    FACE_CROP_REL_H,
    _classify_suit_in_slot,
    _detect_screen,
    _ensure_icon_color_sigs,
    _face_cache,
    _face_search_bbox,
    _find_command_cards,
    _find_element,
    _find_element_by_name,
    _icon_color_sig,
    _identify_servant_in_slot,
    _load_config,
    _load_templates,
    _match_template_region,
    _read_turn,
    _reply,
    _respond,
    _slot_to_pixels,
    _suit_sample_bbox,
    main,
    templates,
)


def _set_config(new_config: dict) -> None:
    """Replace the module-level config (used by tests)."""
    _cv_module.config = new_config


def _get_config() -> dict:
    return _cv_module.config


__all__ = [
    "DEFAULT_COMMAND_CARD_SLOTS",
    "FACE_CROP_REL_H",
    "_classify_suit_in_slot",
    "_detect_screen",
    "_ensure_icon_color_sigs",
    "_face_cache",
    "_face_search_bbox",
    "_find_command_cards",
    "_find_element",
    "_find_element_by_name",
    "_get_config",
    "_icon_color_sig",
    "_identify_servant_in_slot",
    "_load_config",
    "_load_templates",
    "_match_template_region",
    "_read_turn",
    "_reply",
    "_respond",
    "_set_config",
    "_slot_to_pixels",
    "_suit_sample_bbox",
    "main",
    "templates",
]
