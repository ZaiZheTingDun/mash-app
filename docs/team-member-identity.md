# Team Member Identity

Team configuration must distinguish a servant catalog entry from a concrete
member instance in a project. A project can contain the same `servantId` as both
an owned servant and a support servant, so `servantId` alone is not a safe key
for mutations or runtime action resolution.

## Identity Fields

| Field | Meaning | Stability and usage |
| --- | --- | --- |
| `memberId` | Stable identity of one team member instance. It is normally the `ProjectSlot.id`, such as `slot-2`. | Primary key for configuration references, updates, deletion, ordering, Order Change, and resolving actions after the lineup moves. |
| `servantId` | Atlas Academy/FGO servant catalog id. | Selects servant assets and recognition templates. It is descriptive metadata and a compatibility fallback, not a unique team-member key. |
| `isSupport` | Whether this member is the borrowed support. | Disambiguates owned and support instances with the same `servantId`, and controls support-specific behavior. It is not unique without another field. |
| `slotIndex` | Current or configured zero-based project position. | Positional metadata used by lineup construction and legacy configurations. It can change after drag-and-drop or Order Change and must not replace `memberId`. |

The canonical persisted member reference is:

```json
{
  "memberId": "slot-2",
  "servantId": 309,
  "isSupport": true
}
```

Some action structures prefix the same fields by role, for example
`servantMemberId`, `targetMemberId`, `frontMemberId`, and `backMemberId`.
Their semantics are identical.

## Project Storage

`Project.slots` owns the six stable slot ids and their arrangement. For regular
slots, the selected servant is stored in `ProjectSlot.servantId`. The support
slot is special:

- `ProjectSlot.id` remains the support member's `memberId`.
- `ProjectSlot.type` is `support`.
- The selected support servant is stored in `Project.supportServantId`, not in
  `ProjectSlot.servantId`.
- `RunConfig.supportMemberId` and `RunConfig.supportSlotIndex` carry the support
  member identity and position into the runner.

Code that reads a project member must therefore derive `servantId` from
`Project.supportServantId` for the support slot and from
`ProjectSlot.servantId` for regular slots.

## Resolution Rules

When the runner resolves a configured action against the current lineup, it
uses this order:

1. Match `memberId`.
2. For legacy data without a usable `memberId`, match the pair
   (`servantId`, `isSupport`).
3. For older positional configurations, fall back to the stored
   `servant_1`…`servant_6` selection and map the original member to its current
   position.

This order keeps actions attached to the same member after project reordering
or an Order Change. If a referenced member has left the field, normal fallback
and availability rules decide whether the action is skipped or may use the
original position.

Command-card recognition is different: computer vision can report only the
recognized `servantId` and whether the portrait belongs to the support. The
runner combines that pair with its current `PartyMemberRuntime` lineup when it
needs the configured member instance.

## Engineering Rules

- Never update, delete, swap, or reorder a team member by `servantId` alone.
- Persist `memberId`, `servantId`, and `isSupport` together on new references.
- Treat `memberId` as authoritative and the other fields as validation,
  display, recognition, and legacy-fallback metadata.
- Keep `memberId` stable when the slot moves; do not regenerate it during
  drag-and-drop.
- When normalizing legacy data, resolve a valid existing `memberId` first, then
  use `slotIndex`, then use (`servantId`, `isSupport`) as the recovery path.
- Tests involving duplicate servants should include one owned instance and one
  support instance with the same `servantId`.
