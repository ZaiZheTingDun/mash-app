import type {
  GrandClass,
  GrandClassDefinition,
  GrandServantConfig,
} from "../../types/project";

export function grandClassDefinition(
  definitions: GrandClassDefinition[],
  grandClass: GrandClass | undefined,
): GrandClassDefinition | undefined {
  return definitions.find((definition) => definition.id === (grandClass ?? "saber"));
}

export function normalizedGrandRole(
  config: GrandServantConfig,
  index: number,
  definition: GrandClassDefinition,
): string | null {
  if (config.role && definition.roles.some((role) => role.role === config.role)) {
    return config.role;
  }
  if (config.lancerRole && definition.roles.some((role) => role.role === config.lancerRole)) {
    return config.lancerRole;
  }
  return definition.roles[index]?.role ?? null;
}

export function validateGrandServants(
  servants: GrandServantConfig[],
  definition: GrandClassDefinition,
): boolean {
  const valid = servants.filter(
    (servant) => Number.isInteger(servant.slotIndex) && servant.slotIndex >= 0 && servant.slotIndex < 6,
  );
  const slots = new Set(valid.map((servant) => servant.slotIndex));
  const roles = new Set(
    valid.map((servant, index) => normalizedGrandRole(servant, index, definition)).filter(Boolean),
  );
  return (
    valid.length > 0 &&
    valid.length <= definition.roles.length &&
    slots.size === valid.length &&
    roles.size === valid.length &&
    definition.roles.filter((role) => role.required).every((role) => roles.has(role.role))
  );
}
