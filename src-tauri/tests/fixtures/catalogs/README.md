# Catalog test fixtures

Small, curated samples of the v19 catalog schemas. These are test inputs, not a
complete resource-pack snapshot. Retain only records and fields exercised by the
offline Rust tests and Python bond-level OCR tests.

- `servants.json`: 32 servants covering selectable types, NPC exclusion, variant
  names, JP/CN aliases, overlapping names and OCR screenshot subjects. Trait and
  other unused data are omitted.
- `servants_variants*.json`: matching portrait/NP variants. Only Mash's JP skill
  forms are retained for the static-versus-runtime skill regression.
- `craft_essences.json`: 19 entries covering categories, localization, bond
  ownership, permanent rewards and server-specific event bonuses.
- `events.json`: schedules for the five events referenced by those entries.
- `event_items.json`: an empty catalog. Item-name and icon scenarios create their
  own small catalogs in the individual tests.

Add a focused example when a test needs a new case. Do not refresh these files by
copying the full production catalogs or assert production catalog sizes. Rust reads
these fixtures only under `cfg(test)`; Python tests receive the explicit fixture
path. Production requires `app_data_dir()/assets/catalogs/`.
