import { useState } from "react";
import { Button, Dialog, Flex, IconButton, Select, Text } from "@radix-ui/themes";
import { Cross2Icon } from "@radix-ui/react-icons";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
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
  compact?: boolean;
}

export function GrandOutputSettings({
  partyMembers,
  faces,
  grandServants,
  grandClassDefinition,
  onChange,
  compact = false,
}: GrandOutputSettingsProps) {
  const [replacementSlot, setReplacementSlot] = useState<number | null>(null);
  const [selectingRole, setSelectingRole] = useState<string | null>(null);
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
  const selectGrandServant = (slotIndex: number) => {
    const member = partyMembers[slotIndex];
    if (!selectingRole || !member?.servant) return;
    const existing = normalized.find(config => config.role === selectingRole);
    const next = { ...existing, memberId: member.memberId ?? null, slotIndex, servantId: member.servant.id, isSupport: member.isSupport, npCard: existing?.npCard ?? "auto" as const, priority: existing?.priority ?? "damage" as const, role: selectingRole };
    persist([...normalized.filter(config => config.role !== selectingRole), next]);
    setSelectingRole(null);
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
              const servant = config ? partyLineup[config.slotIndex] ?? null : null;
              const roleLabel = roleDefinition.label;
              if (compact) {
                return (
                  <div className="command-grand-role" key={role}>
                    <div className="command-grand-identity">
                      <BattleActorIcon kind="servant" label={servant?.name_cn ?? "未选择"}
                        src={servant ? faces[servant.variantKey] : null} isSupport={config?.isSupport} />
                      <button type="button" className="command-grand-summary"
                        aria-label={config ? `${roleLabel}冠位${servant ? `：${servant.name_cn}` : ""}` : `选择${roleLabel}冠位`}
                        aria-pressed={!config ? activeRole === role : undefined}
                        onClick={() => {
                          if (config) setSettingsIndex(index);
                          else { setSelectedRole(role); setReplacementSlot(null); setSelectingRole(role); }
                        }}>
                        <span>{roleLabel}冠位 <small>{roleDefinition.required ? "必选" : "可选"}</small></span>
                        <small>{servant ? `${servant.name_cn} · 队伍位置 ${config!.slotIndex + 1}` : "未选择"}</small>
                      </button>
                      <Button type="button" variant="ghost" color="gray" onClick={() => {
                        setReplacementSlot(config?.slotIndex ?? null); setSelectingRole(role);
                      }}>更换</Button>
                      {config && <IconButton type="button" variant="ghost" color="gray"
                        aria-label={`移除${roleLabel}冠位`} onClick={() => removeGrandServant(index)}><Cross2Icon /></IconButton>}
                    </div>
                    {config && <>
                      <label className="command-grand-field"><span>宝具颜色</span>
                        <Select.Root value={config.npCard ?? "auto"} onValueChange={value => updateGrandServant(index, { npCard: value as GrandNpCard })}>
                          <Select.Trigger variant="ghost" aria-label={`${roleLabel}冠位宝具颜色`} />
                          <Select.Content><Select.Item value="auto">{autoNpOptionLabel(servant)}</Select.Item><Select.Item value="buster">红卡</Select.Item><Select.Item value="arts">蓝卡</Select.Item><Select.Item value="quick">绿卡</Select.Item></Select.Content>
                        </Select.Root>
                      </label>
                      {grandClassDefinition?.cardPriorityEnabled !== false && <label className="command-grand-field"><span>出卡策略</span>
                        <Select.Root value={config.priority ?? "damage"} onValueChange={value => updateGrandServant(index, { priority: value as GrandCardPriority })}>
                          <Select.Trigger variant="ghost" aria-label={`${roleLabel}冠位出卡策略`} />
                          <Select.Content><Select.Item value="damage">伤害优先</Select.Item><Select.Item value="np">NP 优先</Select.Item></Select.Content>
                        </Select.Root>
                      </label>}
                    </>}
                  </div>
                );
              }
              if (!config) {
                const label = roleDefinition.label;
                return (
                  <button
                    key={role}
                    type="button"
                    className={`grand-servant-tile grand-servant-role-empty${activeRole === role ? " selected" : ""}`}
                    aria-label={`选择${label}冠位`}
                    aria-pressed={activeRole === role}
                    onClick={() => {setSelectedRole(role);setReplacementSlot(null);setSelectingRole(role);}}
                  >
                    <span className="grand-role-badge">{label}</span>
                    <span className="grand-servant-placeholder">未选择</span>
                  </button>
                );
              }
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
      </div>
      <Dialog.Root open={selectingRole != null} onOpenChange={open => {if(!open)setSelectingRole(null);}}>
        <Dialog.Content maxWidth="620px" className="grand-replace-dialog"><Dialog.Title>更换冠位从者</Dialog.Title><Dialog.Description className="sr-only">选择队伍内的从者</Dialog.Description><div className="grand-replace-candidates">
          {Array.from({length:6}, (_, index) => partyMembers[index] ?? {servant:null,isSupport:false}).map((member,index) => <FaceChip key={member.memberId ?? index} servant={member.servant} index={index} src={member.servant ? faces[member.servant.variantKey] : null} isSupport={member.isSupport} selected={replacementSlot === index} disabled={selectedSlots.has(index) && !normalized.some(config => config.role === selectingRole && config.slotIndex === index)} onClick={() => setReplacementSlot(index)} />)}
        </div><Flex justify="end" gap="3" mt="4"><Dialog.Close><Button variant="ghost" color="gray">取消</Button></Dialog.Close><Button disabled={replacementSlot == null} onClick={() => {if(replacementSlot != null) selectGrandServant(replacementSlot);}}>确认更换</Button></Flex></Dialog.Content>
      </Dialog.Root>

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
              <Button variant="ghost" onClick={() => {setReplacementSlot(settings.slotIndex);setSelectingRole(settings.role ?? roles[0].role);setSettingsIndex(null);}}>更换从者</Button>
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
