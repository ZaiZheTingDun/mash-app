# Enhancement Automation Template State Machine

Enhancement automation uses a template state machine that is separate from the battle flow. The battle flow still uses the existing `cv.json` screen detection; this design only reuses the sidecar `find_element` / `find_element_full` template matching capability and does not change the IPC wire protocol.

## Recognition Model

State has three layers:

- `screen`: `Main`, `Enhancement`, `ServantEnhancement`, `Ascension`
- `variant`: `Main`, `ServantSelect`, `MaterialSelect`
- `status`: `MenuOpen`, `FilterDialogOpen`, `NotReady`, and similar statuses indicated by element probes inside a variant

Each screen is first confirmed by a unique anchor template, then its view is identified by variant detection, and status is identified by element probes inside the variant. Search regions originally came from temporary `rois.json` `paddedRoi` entries and have since been folded into `src-tauri/resources/servers/jp/cv.json` under `screens.*.detect` and `screens.*.variants`.

Default thresholds:

- Screen anchor: `0.85`
- Variant detection and element status/button probes: `0.80`

## Template Mapping

Screen anchors:

- `Main`: `button_notification`
- `Enhancement`: `text_enhancement`
- `ServantEnhancement`: `text_enhancement_servant`
- `Ascension`: `screen_enhancement_ascension`

Variant detects and status elements:

- `Main / Main / MenuOpen status`: `button_enhancement`
- `ServantEnhancement / Main`: `text_enhancement_result`
- `ServantEnhancement / ServantSelect`: `text_enhancement_servant_select`
- `ServantEnhancement / MaterialSelect`: `text_enhancement_material`
- `ServantEnhancement / MaterialSelect / FilterDialogOpen status`: `dialog_filter_setting`
- `Ascension / Main`: `text_ascension_main_variant`
- `Ascension / ServantSelect`: `text_enhancement_ascension_servant_select`
- `Ascension / Main / NotReady status`: `enhancement_ascension_not_ready`
- Max display density confirmation: `button_scale_level_3`

Action button probes:

- `Main / Main / menu collapsed status` -> tap `button_menu`
- `Main / Main / menu open status` -> tap `button_enhancement`
- `AscensionResult` fallback return -> tap `button_enhancement_ascension_to_servant` when present

## Transitions

- `Main / Main / menu collapsed status`: tap `button_menu` and enter menu-open status.
- `Main / Main / menu open status`: tap `button_enhancement` and enter `Enhancement`.
- `Enhancement / Main`: tap the servant enhancement entry coordinate and enter `ServantEnhancement`.
- `ServantEnhancement / Main`: read the level via OCR. If the servant is not max-level, enter material selection; if it is max-level and the ascension entry is available, enter `Ascension`.
- `ServantEnhancement / ServantSelect`: first confirm `button_scale_level_3`. If it is missing, tap the density toggle button up to 3 times until it appears. Then match the target servant by portrait template.
- `ServantEnhancement / MaterialSelect`: use OCR to confirm the current list contains only EXP materials. After confirming `button_scale_level_3`, drag-select or tap-select 20 materials.
- `ServantEnhancement / MaterialSelect / FilterDialogOpen status`: confirm `dialog_filter_setting` and `text_filter_setting_type`, then use the existing OCR path to find and tap the EXP material filter option.
- `Ascension / Main`: tap the lower-right enhancement button and follow the existing second-confirmation flow.
- `Ascension / Main / NotReady status`: stop automation and report that materials or state are not executable, avoiding repeated taps on the lower-right enhancement button.

## Fallbacks And Missing Templates

These capabilities still keep OCR or compatible coordinate fallbacks:

- Level `Lv. current/max` reading
- Selected material count `0/20` reading
- Enhancement / ascension second-confirmation dialog
- Data update dialog
- Return from ascension result screen

Templates needed to remove OCR or fully prove state:

- Unique template for the enhancement / ascension second-confirmation dialog
- Unique template for the data update dialog
- Complete return-to-servant-enhancement state template for the ascension result page
- Level digits or max-level state template
- Selected material count template
- Three-state EXP filter confirmation templates: current `text_filter_setting_type` is only the filter type label and does not prove that the middle EXP option is active while the left/right options are inactive

## Naming

Code uses English enum names such as `MaterialSelect`; it does not use names like `select_material`. Documentation refers to the nested base view as `Main screen / Main variant`. Existing PNG filenames are not renamed to avoid unrelated asset churn. New templates should continue using the `screen_*`, `text_*`, `button_*`, and `dialog_*` prefixes.
