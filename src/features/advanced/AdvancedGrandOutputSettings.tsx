import { useState } from "react";
import { Button, Dialog, Flex, Select, Text } from "@radix-ui/themes";
import { FaceChip } from "./AdvancedFaceChip";
import {
  autoNpOptionLabel,
  normalizeGrandServants,
  npCardLabel,
  priorityLabel,
} from "./advancedCommandModel";
import {
  partyMembersToServants,
  type PartyMember,
} from "../team/partyServants";
import type {
  GrandClassDefinition,
  GrandCardPriority,
  GrandNpCard,
  GrandServantConfig,
} from "../../types/project";

interface GrandOutputSettingsProps {
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  grandServants: GrandServantConfig[];
  grandClassDefinition?: GrandClassDefinition;
  onChange?: (grandServants: GrandServantConfig[]) => void;
}

export function GrandOutputSettings({
  partyMembers,
  faces,
  grandServants,
  grandClassDefinition,
  onChange,
}: GrandOutputSettingsProps) {
  const [settingsIndex, setSettingsIndex] = useState<number | null>(null);
  const partyLineup = partyMembersToServants(partyMembers);
  const roles = grandClassDefinition?.roles ?? [
    { role: "main", label: "主", required: true },
    { role: "deputy", label: "副", required: false },
  ];
  const [selectedRole, setSelectedRole] = useState(roles[0]?.role ?? "main");
  const normalized = normalizeGrandServants(grandServants, grandClassDefinition);
  const activeRole = normalized.some((config) => config.role === selectedRole)
    ? roles.find((role) => !normalized.some((config) => config.role === role.role))?.role ?? selectedRole
    : selectedRole;
  const selectedSlots = new Set(normalized.map((item) => item.slotIndex));
  const settings =
    settingsIndex == null ? null : normalized[settingsIndex] ?? null;
  const settingsServant =
    settings == null ? null : partyLineup[settings.slotIndex] ?? null;

  const persist = (next: GrandServantConfig[]) => {
    onChange?.(normalizeGrandServants(next, grandClassDefinition));
  };
  const addGrandServant = (slotIndex: number) => {
    const member = partyMembers[slotIndex];
    if (
      normalized.length >= roles.length ||
      selectedSlots.has(slotIndex) ||
      member?.servant == null
    ) {
      return;
    }
    persist([
      ...normalized,
      {
        memberId: member.memberId ?? null,
        slotIndex,
        servantId: member.servant.id,
        isSupport: member.isSupport,
        npCard: "auto",
        priority: "damage",
        role: activeRole,
      },
    ]);
    const nextRole = roles.find((role) => role.role !== activeRole && !normalized.some((config) => config.role === role.role));
    if (nextRole) setSelectedRole(nextRole.role);
  };
  const removeGrandServant = (index: number) => {
    persist(normalized.filter((_, itemIndex) => itemIndex !== index));
    setSettingsIndex(null);
  };
  const updateGrandServant = (
    index: number,
    patch: Partial<Pick<GrandServantConfig, "npCard" | "priority">>
  ) => {
    persist(
      normalized.map((item, itemIndex) =>
        itemIndex === index ? { ...item, ...patch } : item
      )
    );
  };
  const moveToMain = (index: number) => {
    const primaryRole = roles[0]?.role;
    if (!primaryRole || normalized[index]?.role === primaryRole) return;
    const next = normalized.map((item) => ({ ...item }));
    const primaryIndex = next.findIndex((item) => item.role === primaryRole);
    const previousRole = next[index].role;
    next[index].role = primaryRole;
    if (primaryIndex >= 0) next[primaryIndex].role = previousRole;
    persist(next);
    setSettingsIndex(primaryIndex >= 0 ? primaryIndex : index);
  };

  return (
    <>
      <div className="advanced-grand-output">
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">
            冠位
          </Text>
          <div className="advanced-grand-output-slots">
            {roles.map((roleDefinition) => {
              const role = roleDefinition.role;
              const index = normalized.findIndex((config) => config.role === role);
              const config = index >= 0 ? normalized[index] : null;
              if (!config) {
                const label = roleDefinition.label;
                return (
                  <button
                    key={role}
                    type="button"
                    className={`grand-servant-tile grand-servant-role-empty${activeRole === role ? " selected" : ""}`}
                    aria-label={`选择${label}冠位`}
                    aria-pressed={activeRole === role}
                    onClick={() => setSelectedRole(role)}
                  >
                    <span className="grand-role-badge">{label}</span>
                    <span className="grand-servant-placeholder">未选择</span>
                  </button>
                );
              }
              const servant = partyLineup[config.slotIndex] ?? null;
              const roleLabel = roleDefinition.label;
              return (
                <button
                  key={`${config.slotIndex}-${index}`}
                  type="button"
                  className="grand-servant-tile"
                  aria-label={`${roleLabel}冠位${
                    servant ? `：${servant.name_cn}` : ""
                  }`}
                  onClick={() => setSettingsIndex(index)}
                >
                  <span className="grand-role-badge">
                    {roleLabel}
                  </span>
                  {servant && faces[servant.variantKey] ? (
                    <img
                      src={faces[servant.variantKey] ?? undefined}
                      alt={servant.name_cn}
                      draggable={false}
                    />
                  ) : (
                    <span className="grand-servant-placeholder">
                      {servant?.name_cn ?? "未选择"}
                    </span>
                  )}
                  <span className="grand-np-badge">
                    {npCardLabel(config.npCard, servant?.noblePhantasmCard)}
                  </span>
                  {grandClassDefinition?.cardPriorityEnabled !== false && (
                    <span className="grand-priority-badge">
                      {priorityLabel(config.priority)}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">
            辅助
          </Text>
          <div className="battle-choice-row">
            {partyLineup.slice(0, 6).map((servant, index) => (
              <FaceChip
                key={index}
                servant={servant}
                index={index}
                src={servant ? faces[servant.variantKey] : null}
                selected={selectedSlots.has(index)}
                disabled={
                  servant == null ||
                  selectedSlots.has(index) ||
                  normalized.length >= roles.length
                }
                isSupport={partyMembers[index]?.isSupport ?? false}
                onClick={() => addGrandServant(index)}
              />
            ))}
          </div>
        </div>
      </div>

      <Dialog.Root
        open={settings != null}
        onOpenChange={(open) => {
          if (!open) setSettingsIndex(null);
        }}
      >
        <Dialog.Content maxWidth="420px">
          <Dialog.Title>冠位从者设置</Dialog.Title>
          <Dialog.Description className="sr-only">
            设置冠位从者的宝具颜色和出卡策略。
          </Dialog.Description>
          {settings && settingsIndex != null && (
            <Flex direction="column" gap="4">
              <label className="grand-setting-field">
                <Text size="2" weight="medium">
                  宝具颜色
                </Text>
                <Select.Root
                  value={settings.npCard ?? "auto"}
                  onValueChange={(value) =>
                    updateGrandServant(settingsIndex, {
                      npCard: value as GrandNpCard,
                    })
                  }
                >
                  <Select.Trigger aria-label="宝具颜色" />
                  <Select.Content>
                    <Select.Item value="auto">
                      {autoNpOptionLabel(settingsServant)}
                    </Select.Item>
                    <Select.Item value="buster">红卡</Select.Item>
                    <Select.Item value="arts">蓝卡</Select.Item>
                    <Select.Item value="quick">绿卡</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
              {grandClassDefinition?.cardPriorityEnabled !== false && <label className="grand-setting-field">
                <Text size="2" weight="medium">
                  出卡策略
                </Text>
                <Select.Root
                  value={settings.priority ?? "damage"}
                  onValueChange={(value) =>
                    updateGrandServant(settingsIndex, {
                      priority: value as GrandCardPriority,
                    })
                  }
                >
                  <Select.Trigger aria-label="出卡策略" />
                  <Select.Content>
                    <Select.Item value="damage">伤害优先</Select.Item>
                    <Select.Item value="np">NP 优先</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>}
              <Flex justify="between" gap="3">
                <Button
                  type="button"
                  variant="soft"
                  color="red"
                  onClick={() => removeGrandServant(settingsIndex)}
                >
                  移除
                </Button>
                <Flex gap="3">
                  {grandClassDefinition?.cardPriorityEnabled !== false && settings.role !== roles[0]?.role && (
                    <Button
                      type="button"
                      variant="soft"
                      onClick={() => moveToMain(settingsIndex)}
                    >
                      设为主
                    </Button>
                  )}
                  <Dialog.Close>
                    <Button type="button">完成</Button>
                  </Dialog.Close>
                </Flex>
              </Flex>
            </Flex>
          )}
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}
