import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Avatar, Button, Dialog, DropdownMenu, Flex } from "@radix-ui/themes";
import { ChevronDownIcon, ChevronUpIcon, MagicWandIcon } from "@radix-ui/react-icons";
import { convertFileSrc } from "../../tauri";
import { ServantChoice } from "../../components/common/ServantChoice";
import { MysticCodeIcon } from "../../components/common/MysticCodeIcon";
import { CommandDraftHeading } from "../../components/common/CommandDraftHeading";
import { EnemyTargetButtons } from "./EnemyTargetSelector";
import { MashSkillTwoChoice } from "./MashSkillTwoChoice";
import { equipmentSkillUsageKey, servantSkillUsageKey, type PreparationSkillUsage } from "./preparationSkillUsage";
import { SKILLS, SKILL_LABELS, type PartySlot, type PrepDraft, type PrepSource } from "./battleSceneModel";
import { makePreparationAction, makeOrderChangeAction, startPreparationSkill, selectPreparationSkillOption, type PreparationContext, type PreparationResult } from "./preparationActions";
import type { PartyMember } from "../team/partyServants";
import type { SkillIcons } from "../team/useServantSkillIcons";
import type { PreparationAction } from "../../types/command";
import type { MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

type Pending = { draft: Extract<PrepDraft, { step: "target" | "skillSelection" | "orderChange" }>; target: PartySlot | null };

interface Props {
  enabled: boolean;
  actionCount: number;
  usedSkills?: PreparationSkillUsage;
  members: PartyMember[];
  faces: Record<string, string | null>;
  skillIcons: Record<string, SkillIcons>;
  mysticCode: MysticCode | null;
  gender: MysticCodeGender;
  disableAutoSkillTargetRecognition: boolean;
  skillTargetStatus: PreparationContext["skillTargetStatus"];
  skillSelection: PreparationContext["skillSelection"];
  isSkillLoading: (servant: PartyMember["servant"]) => boolean;
  disabled?: boolean;
  onAction: (action: PreparationAction) => void;
  children: ReactNode;
}

function memberKey(member: PartyMember | undefined, index: number) {
  return member?.memberId ?? `${index}:${member?.servant?.variantKey ?? "empty"}:${member?.isSupport ?? false}`;
}

/** Alternative entry layout using the same action builders and metadata as guided entry. */
export function SimplePreparationPanel({ enabled, actionCount, usedSkills, members, faces, skillIcons, mysticCode, gender, disableAutoSkillTargetRecognition, skillTargetStatus, skillSelection, isSkillLoading, disabled = false, onAction, children }: Props) {
  const [targetKey, setTargetKey] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const palette = useRef<HTMLDivElement>(null);
  const scrollRegion = useRef<HTMLDivElement>(null);
  const previousActionCount = useRef(actionCount);
  const enemyTargetDraft = useRef<HTMLDivElement>(null);
  const enemyTargetOpen = pending?.draft.step === "target" && pending.draft.source === "enemyTarget";
  const closeDraft = useCallback(() => {
    setPending(null);
  }, []);
  const contextKey = JSON.stringify([members.map(m => [m.memberId, m.servant?.variantKey, m.isSupport]), mysticCode?.id, disableAutoSkillTargetRecognition]);
  const [previous, setPrevious] = useState({ contextKey, enabled, disabled });
  if (previous.contextKey !== contextKey || previous.enabled !== enabled || previous.disabled !== disabled) {
    setPrevious({ contextKey, enabled, disabled });
    if (previous.contextKey !== contextKey || previous.enabled !== enabled) {
      setPending(null); setTargetKey(null);
    }
    if (disabled) setExpanded(false);
  }
  useEffect(() => { palette.current?.toggleAttribute("inert", expanded); }, [expanded, enabled]);
  useEffect(() => {
    const added = actionCount > previousActionCount.current;
    previousActionCount.current = actionCount;
    // Wait for the saved action to render, then scroll only the lower list.
    if (enabled && added && scrollRegion.current) {
      scrollRegion.current.scrollTop = scrollRegion.current.scrollHeight;
    }
  }, [actionCount, enabled]);
  useEffect(() => {
    if (!enemyTargetOpen) return;
    enemyTargetDraft.current?.scrollIntoView({ block: "end" });
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); closeDraft(); }
    };
    document.addEventListener("keydown", cancel);
    return () => document.removeEventListener("keydown", cancel);
  }, [enemyTargetOpen, closeDraft]);
  if (!enabled) return <>{children}</>;

  const selectedIndex = members.slice(0, 3).findIndex((m, i) => memberKey(m, i) === targetKey && m.servant);
  const selectedTarget: PartySlot | null = selectedIndex < 0 ? null : `servant_${selectedIndex + 1}` as PartySlot;
  const context: PreparationContext = { members, mysticCode, disableAutoSkillTargetRecognition, skillTargetStatus, skillSelection };
  const commit = (action: PreparationAction) => { setPending(null); onAction(action); };
  const applyResult = (result: PreparationResult, target: PartySlot | null) => {
    if (result.type === "action") commit(result.action);
    else setPending({ draft: result.draft, target });
  };
  const selectSkill = (source: PrepSource, skill: string) => {
    if (!disabled) applyResult(startPreparationSkill(context, source, skill, selectedTarget), selectedTarget);
  };
  const locked = disabled || pending != null;
  return <div className={`simple-preparation${expanded ? " is-expanded" : ""}`}>
    <div className="simple-preparation-menu">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger disabled={locked}>
          <Button variant="ghost" color="gray" className="simple-preparation-menu-trigger" aria-label="令咒 / 敌方目标">
            <i className="command-diamond" aria-hidden="true" />令咒 / 敌方目标 <ChevronDownIcon />
          </Button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Content className="simple-preparation-menu-content" align="end" onCloseAutoFocus={event => {
          const firstTarget = enemyTargetDraft.current?.querySelector<HTMLButtonElement>(".battle-enemy-target");
          if (firstTarget) { event.preventDefault(); firstTarget.focus({ preventScroll: true }); }
        }}>
          <DropdownMenu.Label>令咒</DropdownMenu.Label>
          <DropdownMenu.Item onSelect={() => selectSkill("commandSpell", "np_release")}>宝具解放</DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => selectSkill("commandSpell", "restore")}>灵基修复</DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Label>敌方目标</DropdownMenu.Label>
          <DropdownMenu.Item onSelect={() => setPending({ draft: { step: "target", source: "enemyTarget", option: "select" }, target: null })}>敌方目标</DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
    <div ref={palette} className="simple-skill-palette" aria-hidden={expanded || undefined} role="group" aria-label="简易技能设置">
      {Array.from({ length: 4 }, (_, index) => {
        const member = members[index];
        const servant = index < 3 ? member?.servant ?? null : null;
        const equipment = index === 3;
        return <div className="simple-skill-column" key={index} role="group" aria-label={equipment ? "御主礼装技能" : `前排 ${index + 1} 技能`}>
          <small className="simple-skill-eyebrow">{equipment ? "MYSTIC CODE" : `SERVANT 0${index + 1}`}</small>
          {equipment ? <div className="command-actor-choice simple-mystic-display" aria-label="御主礼装（不可选为目标）">
            <span className="battle-actor-icon"><MysticCodeIcon code={mysticCode} gender={gender} label="御主礼装" /></span>
            <span className="command-actor-copy"><b>{mysticCode?.name ?? "未配置礼装"}</b><small>御主礼装</small></span>
          </div> : <ServantChoice servant={servant} index={index} src={servant ? faces[servant.variantKey] : null} isSupport={member?.isSupport} selected={selectedIndex === index} disabled={locked}
            onClick={() => setTargetKey(selectedIndex === index ? null : memberKey(member, index))} />}
          <div className="simple-skill-buttons">
            {SKILLS.map((skill, skillIndex) => {
              const meta = equipment ? mysticCode?.skills.find(s => s.slot === skillIndex + 1) : null;
              const entry = servant ? skillIcons[servant.variantKey]?.[skillIndex] : null;
              const src = equipment ? (meta?.iconPath ? convertFileSrc(meta.iconPath) : undefined) : entry?.src ?? undefined;
              const label = meta?.name || entry?.name || SKILL_LABELS[skill];
              const source: PrepSource = equipment ? "equipment" : `servant_${index + 1}` as PrepSource;
              const usageKey = equipment ? equipmentSkillUsageKey(skill) : member?.servant ? servantSkillUsageKey(member, skill) : null;
              const used = usageKey != null && usedSkills?.has(usageKey);
              return <MashSkillTwoChoice key={skill} servant={servant} skill={skill} iconSrc={src}
                context={context} source={source} target={selectedTarget} onResult={result => applyResult(result, selectedTarget)}>
                <button type="button" className={`battle-option-btn simple-skill-button${used ? " is-used" : ""}`} aria-label={label} title={label} disabled={locked || (equipment ? !mysticCode : !servant || isSkillLoading(servant))}
                onClick={() => selectSkill(source, skill)}>
                <Avatar src={src} fallback={<MagicWandIcon />} alt={label} radius="small" size="2" />
                </button>
              </MashSkillTwoChoice>;
            })}
          </div>
        </div>;
      })}
    </div>
    <div className="simple-preparation-sequence">
      <button type="button" className="simple-preview-divider" aria-label={expanded ? "恢复技能设置" : "展开配置预览"} aria-expanded={expanded} disabled={locked}
        onClick={() => setExpanded(value => !value)}><span aria-hidden="true">{expanded ? <ChevronDownIcon /> : <ChevronUpIcon />}</span></button>
      <div ref={scrollRegion} className="simple-preparation-scroll" role="region" aria-label="已配置行动" tabIndex={0}>
        {children}
        {enemyTargetOpen && pending?.draft.step === "target" && (
          <div ref={enemyTargetDraft} className="command-inline-draft simple-enemy-target-draft" role="group" aria-label="选择敌方目标">
            <CommandDraftHeading title="选择敌方目标" hint="" onCancel={closeDraft} />
            <fieldset className="simple-enemy-target-options" disabled={disabled}>
              <EnemyTargetButtons onChange={value => { if (value && pending.draft.step === "target") commit(makePreparationAction(pending.draft, value, members)); }} />
            </fieldset>
          </div>
        )}
      </div>
    </div>
    <Dialog.Root open={pending != null && !enemyTargetOpen} onOpenChange={open => { if (!open) closeDraft(); }}>
      <Dialog.Content maxWidth="780px" className="simple-preparation-dialog">
        <Dialog.Title>{pending?.draft.step === "orderChange" ? "选择换人从者" : pending?.draft.step === "skillSelection" ? "选择技能选项" : pending?.draft.source === "commandSpell" ? "选择令咒目标" : "选择技能目标"}</Dialog.Title>
        <Dialog.Description size="2" color="gray">{pending?.draft.step === "orderChange" ? (pending.draft.front == null ? "选择前排从者" : "选择后排从者") : pending?.draft.step === "target" && pending.draft.source !== "commandSpell" ? "该技能需要指定目标，请选择一名从者。" : "选择后添加到当前回合"}</Dialog.Description>
        <Flex wrap="wrap" gap="2" mt="4">
          {pending?.draft.step === "skillSelection" ? pending.draft.options.map(option => <Button key={option.index} disabled={disabled} onClick={() => { if (pending.draft.step === "skillSelection") applyResult(selectPreparationSkillOption(context, pending.draft, option, pending.target), pending.target); }}>{option.label}</Button>)
            : pending && members.map((member, index) => {
              const draft = pending.draft;
              const orderChange = draft.step === "orderChange";
              if (!orderChange && index >= 3) return null;
              const front = orderChange ? draft.front : null;
              const selectable = !orderChange || (front == null ? index < 3 : index >= 3);
              return <ServantChoice key={index} servant={member.servant} index={index} src={member.servant ? faces[member.servant.variantKey] : null} isSupport={member.isSupport} selected={front === `servant_${index + 1}`} disabled={disabled || !selectable}
                onClick={() => {
                  const slot = `servant_${index + 1}` as PartySlot;
                  if (orderChange) {
                    if (front == null) setPending({ ...pending, draft: { ...draft, front: slot } });
                    else { const action = makeOrderChangeAction(draft, slot, members); if (action) commit(action); }
                  } else if (draft.step === "target") commit(makePreparationAction(draft, slot, members));
                }} />;
            })}
        </Flex>
        <Flex justify="end" mt="4"><Dialog.Close><Button variant="soft" color="gray">取消</Button></Dialog.Close></Flex>
      </Dialog.Content>
    </Dialog.Root>
  </div>;
}
