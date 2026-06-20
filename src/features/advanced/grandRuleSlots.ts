import type { SlotItem } from "../team/ContentGrid";
import type { GrandCardRuleSlotConfig, GrandCardStrategy, GrandServantConfig } from "../../types/project";

function slotServantId(slot: SlotItem, supportServantId: number | null | undefined): number | null {
  return slot.type === "support" ? supportServantId ?? null : slot.servant?.id ?? null;
}

function memberMetadata(slot: SlotItem, supportServantId: number | null | undefined) {
  return {
    memberId: slot.id,
    servantId: slotServantId(slot, supportServantId),
    isSupport: slot.type === "support",
  };
}

function resolveNextSlotIndex(
  ref: {
    memberId?: string | null;
    servantId?: number | null;
    isSupport?: boolean | null;
    slotIndex?: number | null;
  },
  nextSlots: SlotItem[],
  supportServantId: number | null | undefined
): number {
  if (ref.memberId) {
    const index = nextSlots.findIndex((candidate) => candidate.id === ref.memberId);
    if (index >= 0) return index;
  }
  if (ref.servantId != null) {
    const isSupport = ref.isSupport === true;
    const index = nextSlots.findIndex(
      (candidate) =>
        candidate.type === (isSupport ? "support" : "servant") &&
        slotServantId(candidate, supportServantId) === ref.servantId
    );
    if (index >= 0) return index;
  }
  return ref.slotIndex ?? -1;
}

export function relocateGrandRuleSlot(
  slot: GrandCardRuleSlotConfig,
  nextSlots: SlotItem[],
  supportServantId: number | null | undefined
): GrandCardRuleSlotConfig {
  if (slot.grandServant === true || (slot.memberId == null && slot.servantId == null)) return slot;
  const nextSlotIndex = resolveNextSlotIndex(slot, nextSlots, supportServantId);
  const nextSlot = nextSlotIndex >= 0 ? nextSlots[nextSlotIndex] : null;
  const metadata = nextSlot ? memberMetadata(nextSlot, supportServantId) : null;
  return {
    ...slot,
    slotIndex: nextSlotIndex >= 0 ? nextSlotIndex : slot.slotIndex,
    memberId: metadata?.memberId ?? slot.memberId ?? null,
    servantId: metadata?.servantId ?? slot.servantId,
    isSupport: metadata?.isSupport ?? slot.isSupport === true,
  };
}

export function relocateGrandServants(
  grandServants: GrandServantConfig[] | undefined,
  nextSlots: SlotItem[],
  supportServantId: number | null | undefined
): GrandServantConfig[] | undefined {
  if (!grandServants) return grandServants;
  return grandServants.map((config) => {
    const nextSlotIndex = resolveNextSlotIndex(config, nextSlots, supportServantId);
    const nextSlot = nextSlotIndex >= 0 ? nextSlots[nextSlotIndex] : null;
    const metadata = nextSlot ? memberMetadata(nextSlot, supportServantId) : null;
    return {
      ...config,
      slotIndex: nextSlotIndex >= 0 ? nextSlotIndex : config.slotIndex,
      memberId: metadata?.memberId ?? config.memberId ?? null,
      servantId: metadata?.servantId ?? config.servantId ?? null,
      isSupport: metadata?.isSupport ?? config.isSupport === true,
    };
  });
}

export function relocateGrandCardStrategySlots(
  strategy: GrandCardStrategy | undefined,
  nextSlots: SlotItem[],
  supportServantId: number | null | undefined
): GrandCardStrategy | undefined {
  if (!strategy) return strategy;
  return {
    ...strategy,
    customRules: strategy.customRules?.map((rule) => ({
      ...rule,
      slots: rule.slots.map((slot) =>
        relocateGrandRuleSlot(slot, nextSlots, supportServantId)
      ),
    })),
  };
}
