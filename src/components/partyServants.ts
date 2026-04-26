import type { SlotItem } from "./ContentGrid";
import type { Project } from "../types/project";
import type { Servant } from "../types/servant";

/**
 * Derive the front-line party (positions 1-3) from the team-builder slots.
 *
 * The display order of `slots` is authoritative — whichever cell sits in
 * the first three positions is part of the front-line, including the
 * support slot. When the support slot lands in positions 1-3 it
 * contributes the project's pinned support servant
 * (`Project.supportServantId`); the per-slot `servantId` for support is
 * always null and must not be read here.
 *
 * The previous implementation filtered the support slot out before
 * slicing, which silently shifted later slots forward and made the
 * command editor label position 3 with the 4th servant whenever the user
 * had dragged support to position 3.
 */
export function derivePartyServants(
  slots: SlotItem[],
  activeProject: Project | null,
  servants: Servant[]
): (Servant | null)[] {
  const supportPinned =
    activeProject?.supportServantId != null
      ? (servants.find((s) => s.id === activeProject.supportServantId) ?? null)
      : null;
  return slots
    .slice(0, 3)
    .map((s) => (s.type === "support" ? supportPinned : s.servant));
}
