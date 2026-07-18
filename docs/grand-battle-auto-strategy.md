# Grand Battle Auto Strategy

This document describes the automatic attack-card strategy used when a project
enables advanced mode and configures `grandServants`.

## Inputs

- Each configured servant carries a class-defined `role`. Saber and Berserker
  use `main` / `deputy`; Lancer uses `single` / `aoe`.
- Legacy Lancer `lancerRole` values are migrated into `role` during project
  normalization and are not written back.
- The user configures these roles in the advanced command editor's main-output
  section, not on the team lineup page.
- Each Grand servant stores:
  - `slotIndex`: project team slot, resolved to the current front-line servant id.
  - `npCard`: `auto`, `buster`, `arts`, or `quick`.
  - `priority`: `damage` or `np`.
- Ready Noble Phantasms and recognized command cards are scored together.
- Hand-written advanced `rules` still take precedence. This strategy only runs
  for the automatic advanced flow.
- `grandClass` selects the automatic rule set. Missing legacy values default to
  `saber`.
- The project-level `grandCardStrategy.chainPriority` list can reorder the
  Saber automatic rule order. Berserker uses its fixed rule order.
- When `grandCardStrategy.customRules` is non-empty, those user rules are tried
  before the class-specific built-in rules. Built-in Saber/Berserker rules
  always remain as the fallback.

If `npCard` is `auto`, the runner uses the servant resource's
`noblePhantasmCard` value. If the configured Grand servant is not currently in
the front line, that role is ignored for the current turn.

## Strategy Modules

The generic candidate builder, custom-rule matcher, and scoring engine live in
`runner/grand.rs`. Class-owned metadata and built-in behavior live in
`runner/grand/saber.rs`, `lancer.rs`, and `berserker.rs`, behind the
`GrandClassStrategy` interface. The same registered strategy supplies project
normalization, startup validation, runtime role order, automatic Order Change
targets, and the `get_grand_class_definitions` UI payload.

To add a class, add the `GrandClass` enum value, implement one strategy module,
and register it in `grand_class_strategies`. React renders its class option,
role slots, support filter, validation message, and settings from the returned
definition without class-specific branches.

## Startup Order Change

When a scene has no effective command-card startup conditions, the runner
normally executes its first control action and `startupActions` directly from
the Battle screen, then opens the attack-card screen once for the actual
attack. This avoids opening the card page only to return immediately.

When the main Grand servant is configured in a back-line slot, an advanced
scene can set `grandAutoOrderChange` as its startup condition. This setting is
per scene and does not change the configured control actions or startup actions.
The direct-start optimization is disabled in this case because the runner still
needs the first attack-card screen to count card ownership and choose which
front-line servant to replace.

On the first attack-card screen for that scene, the runner:

1. Counts recognized command cards owned by each current front-line servant.
2. Selects the front-line servant with the highest count; ties use the leftmost
   servant.
3. Uses Mystic Code `skill_3` to Order Change that front-line servant with the
   back-line main Grand servant.
4. Re-reads the attack-card screen, treats startup as satisfied, then executes
   the scene's configured startup actions.

Startup actions configured against the back-line main Grand servant are resolved
by servant identity at runtime, then rewritten to that servant's current
front-line position. Actions configured against a servant that was moved out of
the front line are skipped instead of being applied to the new occupant of that
position.

If the main Grand servant is already in the front line, cannot be located, or
the swap target cannot be resolved, the runner skips the automatic swap without
failing the battle loop.

## Rule Model

Grand automatic card selection is rule-based. Each rule has exactly three
slots, and the slot order is the click order. A candidate combo must satisfy
the rule's slot constraints plus any rule-wide constraints:

- Owner: main Grand, deputy Grand, any Grand, or any servant.
- Kind: command card, Noble Phantasm, or either.
- Color: exact B/A/Q, any color, or the configured NP color of a Grand role.
- `sameColor`: all three chosen attacks have the same color.
- `colorSetBAQ`: the three chosen attacks contain one buster, one arts, and one
  quick. This is the "exquisite" chain.
- `include` / `exclude`: required or forbidden attacks in the three-card combo.

When several combos match the same rule, the picker prefers the combo with more
target-role attacks, then main/deputy Grand attacks, then NPs and lower original
card order. When a rule's slots allow multiple valid attack orders, non-Grand
command cards are placed before Grand servant command cards where possible.

## User Custom Rules

User custom rules use the same three-slot matcher as the built-in rules. Each
rule has exactly three slots, and each slot can either bind to a concrete
servant id from the configured team or to any configured Grand servant:

- Servant: the exact `servantId` that must own the selected attack, or
  `grandServant: true` for any Grand servant.
- Kind: any attack, command-card-only, or Noble Phantasm.
- Color: any color, buster, arts, or quick. NP slots do not use color for
  runtime matching.

Slots using `grandServant: true` also carry the hidden owner priority used by
Berserker broad slots: main Grand attacks first, then deputy Grand attacks.

The row order in the UI is the matching order. Invalid or incomplete custom
rules are ignored at runtime, so the picker proceeds to the next custom rule or
the built-in fallback rules.

## Saber Rules

Saber mode keeps the user-configurable `grandCardStrategy.chainPriority` order.
Each priority item expands to rule templates:

1. Main exquisite brave chain with NP:
   main command, main command, main NP; rule-wide color set must be B/A/Q.
2. Main exquisite brave chain without NP:
   main buster command, main arts command, main quick command; main NP excluded.
3. Main ready NP:
   any attack, any attack, main NP.
4. Deputy exquisite brave chain with NP:
   deputy command, deputy command, deputy NP; rule-wide color set must be B/A/Q.
5. Deputy exquisite brave chain without NP:
   deputy buster command, deputy arts command, deputy quick command; deputy NP
   excluded.
6. Main same-color chain:
   any three attacks of the same color, including at least one main Grand
   attack.
7. Deputy same-color chain:
   any three attacks of the same color, including at least one deputy Grand
   attack.
8. Fallback:
   any three attacks.

The default Saber priority order is main exquisite brave chain, main ready NP,
deputy exquisite brave chain, main same-color chain, deputy same-color chain,
then fallback.

## Berserker Rules

Berserker mode uses a fixed order:

For every built-in Berserker slot whose owner is "any servant", matching combos
prefer main Grand attacks first, then deputy Grand attacks, then non-Grand
attacks. This preference is configured on those rule slots, not hard-coded in
the generic matcher.

1. Main Grand NP same-color chain:
   command card matching the main NP color, command card matching the main NP
   color, main NP. The second command slot is configured to accept a ready
   deputy Grand NP as a command-equivalent attack when its NP color also matches
   the main NP color, and prefers placing that deputy NP as the second attack.
2. Main Grand NP:
   any attack, any attack, main NP. The second free slot is configured to prefer
   a ready deputy Grand NP, so the click order becomes ordinary/free attack,
   deputy NP, main NP when that combo is available.
3. Deputy Grand NP same-color chain:
   command card matching the deputy NP color, command card matching the deputy
   NP color, deputy NP.
4. Main Grand other same-color chain:
   any three attacks of the same color, including the main Grand servant, but
   excluding the main Grand NP.
5. Deputy Grand other same-color chain:
   any three attacks of the same color, including the deputy Grand servant, but
   excluding the deputy Grand NP.
6. Grand exquisite chain:
   buster, arts, quick; includes at least one Grand servant attack.
7. Fallback:
   any three attacks.

## Lancer Rules

Lancer mode also uses the shared `GrandCardRule` matcher. Its fixed rule order
is dual-NP exquisite chain, dual-NP same-color chain, dual-NP fallback,
single-target NP, AoE NP, then three command cards. Dual-NP rules click AoE NP,
single-target NP, and the filler in that order. Command-card slots use the
reusable main/deputy/other then Arts/Quick/Buster candidate priority, which
preserves the former Lancer filler order. Non-Grand NPs never satisfy these
command-card slots.

## Non-Grand Behavior

When no `grandServants` are configured, the runner keeps the legacy advanced
automatic scoring based on the configured `mainOutput`, output type, and NP
color.
