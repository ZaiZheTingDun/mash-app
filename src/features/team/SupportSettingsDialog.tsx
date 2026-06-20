import { useState } from "react";
import {
  Box,
  Button,
  Dialog,
  Flex,
  Text,
} from "@radix-ui/themes";
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
  npLevel: number | null | undefined;
  skillLevels: SupportSkillLevelMins;
  appendSkillLevels: SupportAppendSkillLevelMins;
  onOpen: () => void;
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
  npLevel,
  skillLevels,
  appendSkillLevels,
  onOpen,
}: SupportRequirementSummaryProps) {
  const showSkills = hasConfiguredLevels(skillLevels);
  const showAppend = hasConfiguredLevels(appendSkillLevels);
  const showNp = npLevel != null;
  if (!showSkills && !showAppend && !showNp) return null;

  const showRow1 = showSkills || showNp;

  return (
    <button
      type="button"
      className="support-requirement-summary"
      aria-label="编辑技能宝具设置"
      onClick={(event) => {
        event.stopPropagation();
        onOpen();
      }}
    >
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
  onOpenChange: (open: boolean) => void;
  onConfirm: (next: {
    npLevel: number | null;
    skillLevels: SupportSkillLevelMins;
    appendSkillLevels: SupportAppendSkillLevelMins;
  }) => void;
}

export function SupportSettingsDialog({
  open,
  project,
  onOpenChange,
  onConfirm,
}: SupportSettingsDialogProps) {
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
    setNpLevel(null);
    setSkillLevels([...EMPTY_SUPPORT_SKILL_LEVELS] as SupportSkillLevelMins);
    setAppendSkillLevels(
      [...EMPTY_SUPPORT_APPEND_SKILL_LEVELS] as SupportAppendSkillLevelMins,
    );
  };

  return (
    <>
      <Dialog.Root open={open} onOpenChange={onOpenChange}>
        <Dialog.Content maxWidth="560px">
          <Dialog.Title>技能/宝具设置</Dialog.Title>
          <Flex direction="column" gap="5">
            <Flex gap="5">
              <Box>
                <Text as="div" size="2" weight="medium" mb="2">宝具等级</Text>
                <button
                  type="button"
                  data-kind="np"
                  aria-label="宝具等级"
                  className="support-skill-level-button"
                  onClick={() => openLevelPicker({ kind: "np" })}
                >
                  {supportLevelLabel(npLevel)}
                </button>
              </Box>
              <Box>
                <Text as="div" size="2" weight="medium" mb="2">持有技能</Text>
                <Flex gap="2" wrap="wrap">
                  {skillLevels.map((level, index) => (
                    <button
                      key={index}
                      type="button"
                      data-kind="skill"
                      aria-label={`持有技能 ${index + 1}`}
                      className="support-skill-level-button"
                      onClick={() => openLevelPicker({ kind: "skill", index })}
                    >
                      {supportLevelLabel(level)}
                    </button>
                  ))}
                </Flex>
              </Box>
              <Box>
                <Text as="div" size="2" weight="medium" mb="2">追加技能</Text>
                <Flex gap="2" wrap="wrap">
                  {appendSkillLevels.map((level, index) => (
                    <button
                      key={index}
                      type="button"
                      data-kind="append"
                      aria-label={`追加技能 ${index + 1}`}
                      className="support-skill-level-button"
                      onClick={() => openLevelPicker({ kind: "append", index })}
                    >
                      {supportLevelLabel(level)}
                    </button>
                  ))}
                </Flex>
              </Box>
            </Flex>

            <Flex justify="between" gap="3" align="center">
              <Button type="button" variant="soft" color="gray" onClick={reset}>
                重置
              </Button>
              <Flex gap="2">
                <Dialog.Close>
                  <Button type="button" variant="soft" color="gray">取消</Button>
                </Dialog.Close>
                <Button
                  type="button"
                  onClick={() => {
                    onConfirm({ npLevel, skillLevels, appendSkillLevels });
                    onOpenChange(false);
                  }}
                >
                  确认
                </Button>
              </Flex>
            </Flex>
          </Flex>
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
