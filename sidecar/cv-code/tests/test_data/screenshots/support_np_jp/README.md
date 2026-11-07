# Japanese support NP-level fixtures

`level_1.png` through `level_5.png` are horizontal NP-line crops at their
source resolution, including the `Lv.N` label and adjacent name/background.
They cover white text and strengthened yellow text. `support_select_np_5.png`
is the original 1920×1080 failing support screen, with two level-5 rows.

The production `screen_support/np_level_jp_N.png` references retain `Lv.` as
visual context and are resized to a common 1920-pixel frame width. They are
JP-only; CN's font must have its own references before enabling that backup.
No OCR models or native runtime changes are required.

| Level | Source | Label crop (left, top, right, bottom) | Source width |
| --- | --- | --- | --- |
| 1 | `support_score_grand_jp_14_16_62_16.png` | 1419, 637, 1521, 686 | 2560 |
| 2 | `support_score_grand_jp_9_15_5_8.png` | 1329, 1231, 1438, 1281 | 2560 |
| 3 | User-provided Tiamat/Demeter support rows | 848, 178, 917, 210 | 1472 |
| 4 | User-provided Nemo/Noah support row | 586, 174, 656, 206 | 1444 |
| 5 | `support_select_np_5.png`, upper full row | 793, 603, 877, 641 | 1920 |

Tests exercise 1280-, 1920-, and 2560-pixel frame widths, JPEG compression,
color changes, ambiguous/missing labels, multi-digit labels, OCR priority,
and separation from the CN server.
