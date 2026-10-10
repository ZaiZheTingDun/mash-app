import { useState } from "react";
import {
  Avatar,
  Button,
  Dialog,
  DropdownMenu,
  Flex,
  IconButton,
  Text,
  TextField,
} from "@radix-ui/themes";
import { ChevronDownIcon, ChevronRightIcon, Cross2Icon, MinusIcon, PlusIcon } from "@radix-ui/react-icons";
import { DiamondSwitch } from "../../components/common/DiamondSwitch";
import { useServantSkillIcons } from "./useServantSkillIcons";
import { useAppendSkillIcons } from "./useAppendSkillIcons";
import type { Servant } from "../../types/servant";
import {
  ThresholdLevelPicker,
  ThresholdLevelLegend,
} from "../../components/common/ThresholdLevelPicker";
import {
  EMPTY_SUPPORT_APPEND_SKILL_LEVELS,
  EMPTY_SUPPORT_SKILL_LEVELS,
  hasConfiguredLevels,
  normalizeSupportAppendSkillLevels,
  normalizeSupportSkillLevels,
} from "./supportSettingsModel";
import type {
  Project,
  SupportAppendSkillLevelMins,
  SupportSkillLevelMins,
} from "../../types/project";

type SupportLevelKind = "np" | "skill" | "append";
type SupportLevelPickerState =
  | { kind: "np" }
  | { kind: "skill" | "append"; index: number };

function supportLevelLabel(level: number | null | undefined) {
  return level == null ? "任意" : String(level);
}

function supportLevelPickerTitle(kind: SupportLevelKind | undefined) {
  if (kind === "np") return "宝具等级";
  return kind === "append" ? "追加技能等级" : "持有技能等级";
}

interface SupportRequirementSummaryProps {
  grandMode: boolean;
  servantLevel: number | null | undefined;
  starMapScore: number | null | undefined;
  grandStarMapScore: number | null | undefined;
  npLevel: number | null | undefined;
  skillLevels: SupportSkillLevelMins;
  appendSkillLevels: SupportAppendSkillLevelMins;
  onOpen: () => void;
}

function scoreChip(
  label: string,
  value: number | null | undefined,
  className: string,
) {
  const isUnset = value == null;
  return (
    <span
      className={`support-requirement-chip score ${className}${isUnset ? " unset" : ""}`}
      aria-label={isUnset ? `${label}任意` : `${label}至少 ${value}`}
    >
      {isUnset ? `${label} -` : `${label} ${value}`}
    </span>
  );
}

function servantLevelChip(value: number) {
  return (
    <span
      className="support-requirement-chip score servant-level"
      aria-label={`从者至少 ${value} 级`}
    >
      Lv.{value}
    </span>
  );
}

/**
 * Render the chip stack for a single skill row. We always render a slot
 * for every position in the row (3 owned, 5 append) so positional
 * meaning is preserved.
 */
function renderSkillChips(
  levels: readonly (number | null)[],
  variant: "skill" | "append",
  labelPrefix: string,
) {
  return levels.map((level, index) => {
    const slotLabel = `${labelPrefix} ${index + 1}`;
    const isUnset = level == null;
    return (
      <span
        key={`${variant}-${index}`}
        className={`support-requirement-chip ${variant}${isUnset ? " unset" : ""}`}
        aria-label={isUnset ? `${slotLabel} 任意等级` : `${slotLabel} 至少 ${level} 级`}
      >
        {isUnset ? "-" : level}
      </span>
    );
  });
}

function renderNpChip(npLevel: number | null | undefined) {
  const isUnset = npLevel == null;
  return (
    <span
      className={`support-requirement-chip np${isUnset ? " unset" : ""}`}
      aria-label={isUnset ? "宝具任意等级" : `宝具至少 ${npLevel} 级`}
    >
      {isUnset ? "宝具 -" : `宝具 ${npLevel}`}
    </span>
  );
}

/**
 * Compact 2x5 grid summary of the configured support requirements,
 * pinned over the support portrait.
 */
export function SupportRequirementSummary({
  grandMode,
  servantLevel,
  starMapScore,
  grandStarMapScore,
  npLevel,
  skillLevels,
  appendSkillLevels,
  onOpen,
}: SupportRequirementSummaryProps) {
  const showSkills = hasConfiguredLevels(skillLevels);
  const showAppend = hasConfiguredLevels(appendSkillLevels);
  const showServantLevel = servantLevel != null;
  const showNp = npLevel != null;
  const showScores = starMapScore != null || (grandMode && grandStarMapScore != null);
  if (!showSkills && !showAppend && !showNp && !showScores && !showServantLevel) return null;

  const showRow1 = showSkills || showNp;

  return (
    <button
      type="button"
      className="support-requirement-summary"
      aria-label="编辑助战筛选设置"
      onClick={(event) => {
        event.stopPropagation();
        onOpen();
      }}
    >
      {(showScores || showServantLevel) && (
        <span className="support-requirement-score-row">
          {showServantLevel && servantLevelChip(servantLevel)}
          {showScores && (
            <>
              {scoreChip("星图", starMapScore, grandMode ? "primary" : "single")}
              {grandMode && scoreChip("冠位", grandStarMapScore, "grand")}
            </>
          )}
        </span>
      )}
      {showRow1 && (
        <>
          {renderSkillChips(skillLevels, "skill", "持有技能")}
          {renderNpChip(npLevel)}
        </>
      )}
      {showAppend && renderSkillChips(appendSkillLevels, "append", "追加技能")}
    </button>
  );
}

interface SupportSettingsDialogProps {
  open: boolean;
  project: Project | null;
  servant: Servant | null;
  portraitSrc: string | null | undefined;
  slotNumber: number;
  craftEssenceGroups: { count: number; images: (string | null | undefined)[] }[];
  onCraftEssenceOpen: (index: number, grandMode: boolean) => void;
  onOpenChange: (open: boolean) => void;
  onConfirm: (next: {
    grandMode: boolean;
    servantLevel: number | null;
    starMapScore: number | null;
    grandStarMapScore: number | null;
    npLevel: number | null;
    skillLevels: SupportSkillLevelMins;
    appendSkillLevels: SupportAppendSkillLevelMins;
  }) => void;
}

export function SupportSettingsDialog({
  open,
  project,
  servant,
  portraitSrc,
  slotNumber,
  craftEssenceGroups,
  onCraftEssenceOpen,
  onOpenChange,
  onConfirm,
}: SupportSettingsDialogProps) {
  const [grandMode, setGrandMode] = useState(project?.supportGrandMode ?? false);
  const skillIcons = useServantSkillIcons([servant]);
  const appendSkillIcons = useAppendSkillIcons();
  const [servantLevel, setServantLevel] = useState<number | null>(
    () => project?.supportServantLevelMin ?? null,
  );
  const [starMapScore, setStarMapScore] = useState<number | null>(
    () => project?.supportStarMapScoreMin ?? null,
  );
  const [grandStarMapScore, setGrandStarMapScore] = useState<number | null>(
    () => project?.supportGrandStarMapScoreMin ?? null,
  );
  const [npLevel, setNpLevel] = useState<number | null>(
    () => project?.supportNoblePhantasmLevelMin ?? null,
  );
  const [skillLevels, setSkillLevels] = useState<SupportSkillLevelMins>(() =>
    normalizeSupportSkillLevels(project?.supportSkillLevelMins),
  );
  const [appendSkillLevels, setAppendSkillLevels] =
    useState<SupportAppendSkillLevelMins>(() =>
      normalizeSupportAppendSkillLevels(project?.supportAppendSkillLevelMins),
    );
  const [levelPicker, setLevelPicker] =
    useState<SupportLevelPickerState | null>(null);
  const [pickerDraftLevel, setPickerDraftLevel] = useState<number | null>(null);

  const openLevelPicker = (nextPicker: SupportLevelPickerState) => {
    const nextLevel =
      nextPicker.kind === "np"
        ? npLevel
        : nextPicker.kind === "skill"
          ? skillLevels[nextPicker.index]
          : appendSkillLevels[nextPicker.index];
    setLevelPicker(nextPicker);
    setPickerDraftLevel(nextLevel);
  };

  const confirmPickedLevel = () => {
    if (!levelPicker) return;
    if (levelPicker.kind === "np") {
      setNpLevel(pickerDraftLevel);
    } else if (levelPicker.kind === "skill") {
      setSkillLevels((prev) => {
        const next = [...prev] as SupportSkillLevelMins;
        next[levelPicker.index] = pickerDraftLevel;
        return next;
      });
    } else {
      setAppendSkillLevels((prev) => {
        const next = [...prev] as SupportAppendSkillLevelMins;
        next[levelPicker.index] = pickerDraftLevel;
        return next;
      });
    }
    setLevelPicker(null);
  };

  const reset = () => {
    setServantLevel(null);
    setStarMapScore(null);
    setGrandStarMapScore(null);
    setNpLevel(null);
    setSkillLevels([...EMPTY_SUPPORT_SKILL_LEVELS] as SupportSkillLevelMins);
    setAppendSkillLevels(
      [...EMPTY_SUPPORT_APPEND_SKILL_LEVELS] as SupportAppendSkillLevelMins,
    );
  };

  return (
    <>
      <Dialog.Root open={open} onOpenChange={onOpenChange}>
        <Dialog.Content className="support-settings-drawer" aria-describedby={undefined}>
          <header className="support-settings-header">
            <Dialog.Title className="support-settings-accessible-title">助战筛选设置</Dialog.Title>
            <div className="support-settings-identity">
              <span className="support-settings-slot-number">{String(slotNumber).padStart(2, "0")}</span>
              <span className="support-settings-badge">支援</span>
            </div>
            <div className="support-settings-servant-name" title={servant?.name_cn}>{servant?.name_cn ?? "未选择支援从者"}</div>
            <Text as="div" className="support-settings-subtitle">支援从者配置</Text>
            {portraitSrc && <div className="support-settings-portrait"><img src={portraitSrc} alt="" /></div>}
            <Dialog.Close>
              <IconButton type="button" variant="ghost" color="gray" className="support-settings-close" aria-label="关闭助战筛选设置"><Cross2Icon width={12} height={12} /></IconButton>
            </Dialog.Close>
          </header>

          <div className="support-settings-body">
            <section className="support-settings-basic">
              <h3 className="support-settings-heading">基础配置</h3>
              <div className="support-settings-base-row">
                <label className="support-settings-field">
                  <span>从者等级</span>
                  <TextField.Root size="1" type="number" min={1} max={120} value={servantLevel ?? ""} placeholder="任意" aria-label="从者等级" onChange={(event) => {
                    const value = event.currentTarget.valueAsNumber;
                    setServantLevel(Number.isFinite(value) ? Math.min(120, Math.max(1, Math.trunc(value))) : null);
                  }} />
                </label>
                <div className="support-settings-field">
                  <span>宝具等级</span>
                  <Button type="button" size="1" variant="surface" color="gray" aria-label="宝具等级" className="support-settings-np" onClick={() => openLevelPicker({ kind: "np" })}>
                    {supportLevelLabel(npLevel)}<ChevronDownIcon width={12} height={12} />
                  </Button>
                </div>
              </div>
              <div className="support-settings-grand-row">
                <span>冠位从者</span>
                <span>开启后可设置冠位星图</span>
                <DiamondSwitch size="1" checked={grandMode} onCheckedChange={setGrandMode} aria-label="冠位从者" />
                <span>{grandMode ? "冠位" : "非冠位"}</span>
              </div>
              <div className="support-settings-score-row">
                <div>
                  <label className="support-settings-field">
                    <span>星图分值</span>
                    <TextField.Root size="1" type="number" min={0} max={62} value={starMapScore ?? ""} placeholder="任意" aria-label="星图分值" onChange={(event) => {
                      const value = event.currentTarget.valueAsNumber;
                      setStarMapScore(Number.isFinite(value) ? Math.min(62, Math.max(0, Math.trunc(value))) : null);
                    }} />
                  </label>
                  <div className="support-settings-limit">最高 62</div>
                </div>
                {grandMode && <div>
                  <label className="support-settings-field">
                    <span>冠位星图</span>
                    <TextField.Root size="1" type="number" min={0} max={16} value={grandStarMapScore ?? ""} placeholder="任意" aria-label="冠位星图分值" onChange={(event) => {
                      const value = event.currentTarget.valueAsNumber;
                      setGrandStarMapScore(Number.isFinite(value) ? Math.min(16, Math.max(0, Math.trunc(value))) : null);
                    }} />
                  </label>
                  <div className="support-settings-limit">最高 16</div>
                </div>}
              </div>
              <Text as="p" className="support-settings-hint">未设置时不限制；已设置的等级为最低要求。</Text>
            </section>

            <section className="support-settings-owned">
              <h3 className="support-settings-heading">技能等级</h3>
              <div className="support-settings-skills">
                {skillLevels.map((level, index) => <SupportSkillControl key={index} index={index} kind="skill" level={level} iconSrc={servant ? skillIcons[servant.variantKey]?.[index]?.src : null} onPick={() => openLevelPicker({ kind: "skill", index })} onChange={(nextLevel) => setSkillLevels(prev => prev.map((value, i) => i === index ? nextLevel : value) as SupportSkillLevelMins)} />)}
              </div>
            </section>

            <section className="support-settings-append">
              <h3 className="support-settings-heading">追加技能</h3>
              <div className="support-settings-skills">
                {appendSkillLevels.map((level, index) => <SupportSkillControl key={index} index={index} kind="append" level={level} iconSrc={appendSkillIcons[index]?.src} onPick={() => openLevelPicker({ kind: "append", index })} onChange={(nextLevel) => setAppendSkillLevels(prev => prev.map((value, i) => i === index ? nextLevel : value) as SupportAppendSkillLevelMins)} />)}
              </div>
            </section>

            <section className="support-settings-ce-section">
              <h3 className="support-settings-heading">概念礼装</h3>
              {grandMode ? <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  <Button type="button" variant="surface" color="gray" className="support-settings-ce" aria-label="配置冠位助战礼装">
                    <SupportCraftEssenceSummary groups={craftEssenceGroups.slice(1, 4)} />
                  </Button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Content>
                  {["普通礼装", "羁绊礼装", "冠位礼装"].map((label, index) => <DropdownMenu.Item key={label} onSelect={() => onCraftEssenceOpen(index, true)}>{label}</DropdownMenu.Item>)}
                </DropdownMenu.Content>
              </DropdownMenu.Root> : <Button type="button" variant="surface" color="gray" className="support-settings-ce" aria-label="配置助战礼装" onClick={() => onCraftEssenceOpen(0, false)}>
                <SupportCraftEssenceSummary groups={craftEssenceGroups.slice(0, 1)} />
              </Button>}

            </section>
          </div>

          <footer className="support-settings-footer">
            <Button type="button" variant="ghost" color="red" onClick={reset}>清空配置</Button>
            <Button type="button" className="support-settings-apply" onClick={() => {
              onConfirm({ grandMode, servantLevel, starMapScore, grandStarMapScore, npLevel, skillLevels, appendSkillLevels });
              onOpenChange(false);
            }}>应用并关闭</Button>
          </footer>
        </Dialog.Content>
      </Dialog.Root>

      <Dialog.Root
        open={levelPicker != null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) {
            setLevelPicker(null);
            setPickerDraftLevel(null);
          }
        }}
      >
        <Dialog.Content
          maxWidth={levelPicker?.kind === "np" ? "440px" : "680px"}
          className="support-level-dialog"
        >
          <Dialog.Title>
            {supportLevelPickerTitle(levelPicker?.kind)}
          </Dialog.Title>
          <ThresholdLevelPicker
            value={pickerDraftLevel}
            maxLevel={levelPicker?.kind === "np" ? 5 : 10}
            ariaLabel={levelPicker?.kind === "np" ? "宝具等级选择" : "技能等级选择"}
            onChange={setPickerDraftLevel}
          />
          <ThresholdLevelLegend
            value={pickerDraftLevel}
            maxLevel={levelPicker?.kind === "np" ? 5 : 10}
          />

          <Flex justify="end" gap="3" mt="4">
            <Dialog.Close>
              <Button
                type="button"
                variant="surface"
                color="gray"
                aria-label="取消等级选择"
              >
                取消
              </Button>
            </Dialog.Close>
            <Button type="button" onClick={confirmPickedLevel}>
              确认
            </Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}


function SupportSkillControl({ index, kind, level, iconSrc, onPick, onChange }: {
  index: number;
  kind: "skill" | "append";
  level: number | null;
  iconSrc: string | null | undefined;
  onPick: () => void;
  onChange: (level: number | null) => void;
}) {
  const label = `${kind === "skill" ? "持有技能" : "追加技能"} ${index + 1}`;
  return <div className="support-settings-skill">
    <Avatar src={iconSrc ?? undefined} fallback={String(index + 1)} radius="small" className="support-settings-skill-icon" />
    <span className="support-settings-skill-label">{kind === "skill" ? "技能" : "追加"} {index + 1}</span>
    <div className="support-settings-stepper">
      <IconButton type="button" variant="ghost" color="gray" aria-label={`降低${label}等级`} disabled={level == null} onClick={() => onChange(level != null && level > 1 ? level - 1 : null)}><MinusIcon width={12} height={12} /></IconButton>
      <Button type="button" variant="ghost" color="gray" aria-label={label} data-configured={level != null} onClick={onPick}>{level ?? "—"}</Button>
      <IconButton type="button" variant="ghost" color="gray" aria-label={`提高${label}等级`} disabled={level === 10} onClick={() => onChange(level == null ? 1 : level + 1)}><PlusIcon width={12} height={12} /></IconButton>
    </div>
  </div>;
}


function SupportCraftEssenceSummary({ groups }: {
  groups: SupportSettingsDialogProps["craftEssenceGroups"];
}) {
  const configured = groups.filter(group => group.count > 0).length;
  const count = groups.reduce((total, group) => total + group.count, 0);
  const images = groups.flatMap(group => group.images).filter((src): src is string => Boolean(src)).slice(0, 3);
  return <>
    <span className="support-settings-ce-count">{configured}/{groups.length} 已配置<ChevronRightIcon width={12} height={12} /></span>
    <span className="support-settings-ce-content">
      <span className="support-settings-ce-images">{images.map((src, index) => <img key={index} src={src} alt="" style={{ left: index * 10.5, top: index * 6, zIndex: 3 - index }} />)}</span>
      <span><span>{count ? `已选择 ${count} 张礼装` : "未选择礼装"}</span><span className="support-settings-ce-link">点击配置礼装</span></span>
    </span>
  </>;
}
