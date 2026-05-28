# Fallback Registry

This document records fallback paths that exist only to keep automation
usable across uncertain screenshots, old data, or missing templates. When a
primary path is proven stable, remove its fallback here and in code together.

## Support Selection

| Area | Primary path | Fallback path | Removal signal |
| --- | --- | --- | --- |
| Support row anchor | Match `button_support_form_confirm.png` inside the right-side support confirmation column. | Shape-detect the same button by rectangle contour. | Real Grand and normal support screenshots consistently match the button template at target resolution/server variants. |
| Grand support CE regions | Derive the three CE search boxes from the visible confirmation button top. | If only the right-side panel anchor is available, derive CE boxes from the panel top for skill OCR/debug only; runner Grand CE filtering still requires a visible confirmation button. | Confirmation button template covers all rows that should be selectable. |
| Grand support CE filtering | Match only configured CE positions, in order. | Unconfigured positions are skipped; if all three Grand CE positions are empty, CE filtering is disabled. | Keep unless the UI later requires all three CE positions. |
| Grand CE template resolution | Use the configured CE template path. | Missing CE template currently makes that position uncheckable and logs a missing-template diagnostic in debug. | All selectable CE assets have bundled templates or the UI prevents choosing missing-template CEs. |
| CE decoration icons | When enabled, match `icon_mlb_mark`, `icon_grand_bond_ce`, or `icon_grand_bond_ce_np` inside CE-relative subregions. | Disabled decoration checks are skipped; missing required icon templates fail that icon check instead of accepting the row. | Icon regions and templates are stable across support-list variants. |
| Normal support CE filtering | Use `supportCraftEssenceId` only when Grand mode is off. | Grand mode hides and ignores the normal single-CE setting without deleting it. | Keep while users may switch Grand mode off and back on. |
| Support skill slots | Project fixed skill-slot offsets from the confirmation-button anchor and use saturation at the append-only slot to classify owned vs append panels. | Ambiguous saturation leaves the panel unresolved and uses the owned slot layout. | Button-template anchoring and saturation probe are validated across enough normal, append, and Grand rows. |
| Support OCR rows | Use full row match with name, NP, skill, and CE requirements. | Name-only rows are synthesized when OCR fragments cannot form full support rows. | OCR row grouping becomes reliable enough on the target server/resolution. |
| Servant names on CN | Prefer `name_cn_server` for CN display-name matching. | Fall back to `name_cn` when server-specific name is missing; id-indexed lookup prevents aliases from leaking across servants. | Keep as long as the data source has incomplete `name_cn_server` fields. |

## Battle Automation

| Area | Primary path | Fallback path | Removal signal |
| --- | --- | --- | --- |
| Advanced startup condition | Wait until configured command-card conditions match. | If not started, run one control action per turn and still attack by priority without NP. | User adds explicit waiting/skip-turn controls that replace this default behavior. |
| Command-card owner recognition | Wait and retry until all visible command cards have recognized servants. | If retries are exhausted, log unrecognized cards and avoid treating them as condition matches. | Card owner CV is stable enough to remove retry exhaustion handling. |
| Advanced auto attack | Use configured main attacker and output type to build the 3-card choice. | If no configured strategy can fill three cards, fill left-to-right from remaining cards; if no advanced rule matches, default to non-blocking attack. | A future strategy layer defines explicit behavior for every partial-card case. |
| NP color | Use configured/manual NP color when available. | Treat NP as selectable high-priority attack card without color-chain assumptions when color is unknown. | Servant/NP metadata reliably includes card color. |

## Enhancement Automation

| Area | Primary path | Fallback path | Removal signal |
| --- | --- | --- | --- |
| ADB screen dimensions | Use scrcpy-reported frame dimensions. | Query ADB device dimensions when scrcpy reports odd or missing dimensions. | Scrcpy stream metadata is always valid on supported devices. |
| Enhancement dialogs | Detect and tap explicit dialog/result buttons. | Coordinate fallback taps remain for known post-action result screens. | Template probes cover every result/dialog variant. |
