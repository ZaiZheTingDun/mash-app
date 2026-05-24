# Grand Battle Auto Strategy

This document describes the automatic attack-card strategy used when a project
enables advanced mode and configures `grandServants`.

## Inputs

- `grandServants[0]` is the main output servant.
- `grandServants[1]`, when present, is the deputy output servant.
- Each Grand servant stores:
  - `slotIndex`: project team slot, resolved to the current front-line servant id.
  - `npCard`: `auto`, `buster`, `arts`, or `quick`.
  - `priority`: `damage` or `np`.
- Ready Noble Phantasms and recognized command cards are scored together.
- Hand-written advanced `rules` still take precedence. This strategy only runs
  for the automatic advanced flow.

If `npCard` is `auto`, the runner uses the servant resource's
`noblePhantasmCard` value. If the configured Grand servant is not currently in
the front line, that role is ignored for the current turn.

## Chain Priority

The picker chooses three attacks by the following chain priority:

1. Main servant three-card chain.
2. Deputy servant three-card chain.
3. Same-color chain containing the main servant.
4. Same-color chain containing the deputy servant.
5. Fallback output ordering.

Within a same-servant three-card chain:

1. Exquisite chain: one buster, one arts, one quick.
2. Same-color force/quick/skill chain: three buster, three quick, or three arts.
3. Ordinary brave chain.

The main servant always outranks the deputy at the same chain class.

## Attack Order

Exquisite damage-priority chain:

- Put the target NP last.
- Avoid putting a same-color command card before that NP when possible, because
  the Grand battle chain bonus for that color would be consumed before the NP.

Exquisite NP-priority chain:

- If a red command card exists and the target NP is not red, place the red card
  before the NP to preserve NP gain while still giving the NP the red first-card
  damage benefit.
- If the target NP is red, use the damage-priority NP-last order.

Same-color force/quick/skill chain with a target NP:

- Put the NP first.
- Command-card second/third position performance does not apply to NPs, so the
  normal cards should receive later card positions instead.

Fallback ordering:

- Prefer the main servant's attacks over the deputy's attacks.
- Ready deputy or auxiliary NPs may be placed before the target NP to provide
  overcharge.
- Command cards from output servants are kept later when no chain is available,
  because second and third command cards get higher position performance.
- The first command card may be a non-output servant's card when it provides the
  preferred first-card color:
  - `damage`: buster.
  - `np`: arts.

## Non-Grand Behavior

When no `grandServants` are configured, the runner keeps the legacy advanced
automatic scoring based on the configured `mainOutput`, output type, and NP
color.
