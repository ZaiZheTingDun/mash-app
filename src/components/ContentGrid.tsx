import { useEffect, useState } from "react";
import {
  Box,
  Button,
  Dialog,
  Flex,
  Text,
} from "@radix-ui/themes";
import { PlusIcon, PersonIcon, Cross2Icon } from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "../tauri";
import type React from "react";
import {
  ThresholdLevelPicker,
  ThresholdLevelLegend,
} from "./ThresholdLevelPicker";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  arrayMove,
  rectSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ServantSelectDialog } from "./ServantSelectDialog";
import { CraftEssenceSelectDialog } from "./CraftEssenceSelectDialog";
import type { Servant } from "../types/servant";
import type { CraftEssence } from "../types/craftEssence";
import type {
  Project,
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceIds,
  SupportGrandCraftEssenceMlbRequired,
  SupportAppendSkillLevelMins,
  SupportSkillLevelMins,
} from "../types/project";
import mlbIconSrc from "../../src-tauri/resources/images/icon_mlb_mark.png";
import grandBondIconSrc from "../../src-tauri/resources/images/icon_grand_bond_ce.png";
import grandBondNpIconSrc from "../../src-tauri/resources/images/icon_grand_bond_ce_np.png";

export interface SlotItem {
  id: string;
  type: "servant" | "support";
  servant: Servant | null;
  /**
   * Pinned craft essence for this slot. Persisted on the project but
   * only consumed by the runner for the support slot today (party-slot
   * CEs are stored for future auto-equip work). Renders as a small
   * picker tile beneath each servant slot.
  */
  craftEssence: CraftEssence | null;
  craftEssenceMlbRequired?: boolean;
}

interface ContentGridProps {
  servants: Servant[];
  craftEssences: CraftEssence[];
  slots: SlotItem[];
  onSlotsChange: (slots: SlotItem[]) => void;
  /**
   * Active project, the source of truth for the pinned support servant.
   * When `null`, the support slot still renders but selecting a servant
   * is a no-op until a project is created/selected.
   */
  activeProject: Project | null;
  onUpdateActiveProject: (next: Project) => Promise<void> | void;
}

interface CraftEssenceOverlayProps {
  craftEssence: CraftEssence | null;
  cardSrc: string | null | undefined;
  mlbRequired: boolean;
  mlbIconSrc: string | null | undefined;
  onSelect: () => void;
  onClear: () => void;
}

interface GrandCraftEssenceOverlayProps {
  craftEssences: (CraftEssence | null)[];
  cardSrcs: (string | null | undefined)[];
  mlbRequired: SupportGrandCraftEssenceMlbRequired;
  mlbIconSrc: string | null | undefined;
  grandBondCeMode: SupportGrandBondCeMode;
  bondIconSrc: string | null | undefined;
  bondNpIconSrc: string | null | undefined;
  onSelect: (index: number) => void;
  onClear: (index: number) => void;
}

const EMPTY_SUPPORT_SKILL_LEVELS: SupportSkillLevelMins = [null, null, null];
const EMPTY_SUPPORT_APPEND_SKILL_LEVELS: SupportAppendSkillLevelMins = [
  null,
  null,
  null,
  null,
  null,
];
type SupportLevelKind = "np" | "skill" | "append";
type SupportLevelPickerState =
  | { kind: "np" }
  | { kind: "skill" | "append"; index: number };

function normalizeSupportSkillLevels(
  levels: Project["supportSkillLevelMins"],
): SupportSkillLevelMins {
  return [0, 1, 2].map((index) => levels?.[index] ?? null) as SupportSkillLevelMins;
}

function normalizeSupportAppendSkillLevels(
  levels: Project["supportAppendSkillLevelMins"],
): SupportAppendSkillLevelMins {
  return [0, 1, 2, 3, 4].map((index) => levels?.[index] ?? null) as SupportAppendSkillLevelMins;
}

function hasConfiguredLevels(levels: readonly (number | null | undefined)[]) {
  return levels.some((level) => level != null);
}

function normalizeSupportGrandCraftEssenceIds(
  ids: Project["supportGrandCraftEssenceIds"],
): SupportGrandCraftEssenceIds {
  return [0, 1, 2].map((index) => ids?.[index] ?? null) as SupportGrandCraftEssenceIds;
}

function normalizeSupportGrandCraftEssenceMlbRequired(
  values: Project["supportGrandCraftEssenceMlbRequired"],
): SupportGrandCraftEssenceMlbRequired {
  return [0, 1, 2].map((index) => values?.[index] ?? true) as SupportGrandCraftEssenceMlbRequired;
}

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
 * meaning is preserved — without this, configuring only "skill 3 ≥ 5"
 * would render a lone chip aligned to the left of the row, looking as
 * if it were skill 1. Unset slots show a dimmed `-` placeholder.
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
 * Compact 2×5 grid summary of the configured support requirements,
 * pinned over the support portrait. Layout:
 *
 *   row 1: [skill 1 | skill 2 | skill 3 | NP (span 2)            ]
 *   row 2: [append 1 | append 2 | append 3 | append 4 | append 5 ]
 *
 * Row 1 renders whenever any owned skill or NP is set; row 2 renders
 * whenever any append skill is set. Within a rendered row every slot
 * is always emitted (configured or `-` placeholder) so chips keep
 * their positional meaning and the NP chip stays anchored at cols 4-5.
 */
function SupportRequirementSummary({
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

function SupportSettingsDialog({
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

/**
 * Plate pinned to the bottom of the servant portrait, sized to the
 * natural CE card aspect (`--ce-card-aspect` in CSS).
 *
 * Empty state:  translucent gray scrim + four red L-shaped corner
 *               brackets (the FGO "this is where the CE goes"
 *               affordance).
 * Filled state: just the CE card artwork (or a fallback scrim with
 *               the CE name when the asset isn't on disk) plus a
 *               hover-revealed `×` clear button. The brackets are
 *               intentionally hidden once a CE is equipped so they
 *               don't compete visually with the artwork.
 *
 * Click target spans the full plate; `stopPropagation` keeps clicks
 * from bubbling up to the portrait's own click handler (which opens
 * the servant picker instead).
 */
function CraftEssenceOverlay({
  craftEssence,
  cardSrc,
  mlbRequired,
  mlbIconSrc,
  onSelect,
  onClear,
}: CraftEssenceOverlayProps) {
  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect();
  };

  return (
    <div
      className={`ce-overlay${craftEssence ? " filled" : " empty"}`}
      onClick={handleClick}
      role="button"
      aria-label={craftEssence ? `礼装：${craftEssence.name}` : "选择礼装"}
    >
      {craftEssence && cardSrc ? (
        <img
          className="ce-overlay-img"
          src={cardSrc}
          alt={craftEssence.name}
          draggable={false}
        />
      ) : (
        <div className="ce-overlay-scrim">
          {craftEssence ? (
            <Text
              size="1"
              weight="bold"
              align="center"
              className="ce-overlay-fallback-label"
              truncate
            >
              {craftEssence.name}
            </Text>
          ) : (
            <PlusIcon width={20} height={20} className="ce-overlay-empty-icon" />
          )}
        </div>
      )}
      {!craftEssence && (
        <>
          <span className="ce-overlay-corner tl" aria-hidden />
          <span className="ce-overlay-corner tr" aria-hidden />
          <span className="ce-overlay-corner bl" aria-hidden />
          <span className="ce-overlay-corner br" aria-hidden />
        </>
      )}
      {craftEssence && (
        <button
          type="button"
          className="ce-overlay-clear"
          aria-label="清除礼装"
          onClick={(e) => {
            e.stopPropagation();
            onClear();
          }}
        >
          <Cross2Icon width={11} height={11} />
        </button>
      )}
      {craftEssence && mlbRequired && (
        mlbIconSrc ? (
          <img
            className="ce-condition-icon ce-condition-icon-mlb"
            src={mlbIconSrc}
            alt="满破"
            draggable={false}
          />
        ) : (
          <span className="ce-condition-badge ce-condition-icon-mlb">满</span>
        )
      )}
    </div>
  );
}

function GrandCraftEssenceOverlay({
  craftEssences,
  cardSrcs,
  mlbRequired,
  mlbIconSrc,
  grandBondCeMode,
  bondIconSrc,
  bondNpIconSrc,
  onSelect,
  onClear,
}: GrandCraftEssenceOverlayProps) {
  const [failedCardSrcs, setFailedCardSrcs] = useState<Record<number, string>>({});
  const bondSrc = grandBondCeMode === "bond" ? bondIconSrc : bondNpIconSrc;
  const bondLabel = grandBondCeMode === "bond" ? "原始牵绊" : "冠位连接牵绊";
  return (
    <div className="grand-ce-overlay" aria-label="冠位礼装设置">
      {craftEssences.map((craftEssence, index) => {
        const cardSrc = cardSrcs[index];
        const showCardImage =
          craftEssence != null && cardSrc != null && failedCardSrcs[index] !== cardSrc;
        return (
          <div
            key={index}
            className={`grand-ce-slot${craftEssence ? " filled" : " empty"}`}
            role="button"
            tabIndex={0}
            aria-label={
              craftEssence
                ? `冠位礼装 ${index + 1}：${craftEssence.name}`
                : `选择冠位礼装 ${index + 1}`
            }
            onClick={(event) => {
              event.stopPropagation();
              onSelect(index);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                event.stopPropagation();
                onSelect(index);
              }
            }}
          >
            {showCardImage ? (
              <img
                className="grand-ce-slot-img"
                src={cardSrc}
                alt={craftEssence.name}
                draggable={false}
                onError={() => {
                  setFailedCardSrcs((prev) => ({ ...prev, [index]: cardSrc }));
                }}
              />
            ) : (
              <span className="grand-ce-slot-scrim">
                <span className="grand-ce-slot-index">{index + 1}</span>
                <span className="grand-ce-slot-label">
                  {craftEssence ? craftEssence.name : "选择礼装"}
                </span>
              </span>
            )}
            {craftEssence && (
              <button
                type="button"
                className="grand-ce-slot-clear"
                aria-label={`清除冠位礼装 ${index + 1}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onClear(index);
                }}
              >
                <Cross2Icon width={10} height={10} />
              </button>
            )}
            {craftEssence && mlbRequired[index] && (
              mlbIconSrc ? (
                <img
                  className="ce-condition-icon ce-condition-icon-mlb"
                  src={mlbIconSrc}
                  alt="满破"
                  draggable={false}
                />
              ) : (
                <span className="ce-condition-badge ce-condition-icon-mlb">满</span>
              )
            )}
            {craftEssence && index === 1 && grandBondCeMode !== "any" && (
              bondSrc ? (
                <img
                  className="ce-condition-icon ce-condition-icon-bond"
                  src={bondSrc}
                  alt={bondLabel}
                  draggable={false}
                />
              ) : (
                <span className="ce-condition-badge ce-condition-icon-bond">绊</span>
              )
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * Generic asset-path resolver hook. Walks a Rust command that takes a
 * single integer id and returns either an absolute file path or
 * `null`, then wraps the path with `convertFileSrc` so the result is
 * ready to drop into `<img src>`.
 *
 * Returns a map keyed by id; values are either a `convertFileSrc` URL,
 * `null` when the resolver returned `None`, or `undefined` while the
 * fetch is still in flight. Refires only when the id set actually
 * changes (slot reorders that don't add/remove ids are no-ops).
 *
 * The CE card resolver uses this because it only needs one numeric id.
 * Servant portraits are variant-aware and use a dedicated hook below.
 */
function useAssetPaths(
  command: string,
  argKey: string,
  ids: number[],
): Record<number, string | null | undefined> {
  const [cache, setCache] = useState<Record<number, string | null>>({});

  const key = ids
    .filter((id, i, arr) => arr.indexOf(id) === i)
    .sort((a, b) => a - b)
    .join(",");

  useEffect(() => {
    const parsedIds = key
      ? key.split(",").map((s) => Number(s)).filter((n) => Number.isFinite(n))
      : [];
    const missing = parsedIds.filter((id) => !(id in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((id) =>
        invoke<string | null>(command, { [argKey]: id })
          .then((path) => [id, path ? convertFileSrc(path) : null] as const)
          .catch(() => [id, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [id, src] of results) {
          next[id] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded — re-running on cache writes would
    // create an infinite loop. The effect re-fires only when the id set
    // changes, which is exactly when we need to fetch new ones.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, command, argKey]);

  return cache;
}

/**
 * Resolve full-art portrait paths for servants that don't have a cached
 * entry yet. Variants share the same base servant id, so the cache is
 * keyed by `variantKey` and the backend receives the variant asset id
 * (`faceId`) when one exists.
 */
function usePortraits(servants: Servant[]): Record<string, string | null | undefined> {
  const [cache, setCache] = useState<Record<string, string | null>>({});
  const byVariant = new Map<string, Servant>();
  for (const servant of servants) {
    byVariant.set(servant.variantKey, servant);
  }
  const requests = Array.from(byVariant.values())
    .map((servant) => ({
      variantKey: servant.variantKey,
      servantId: servant.id,
      faceId: servant.faceId ?? null,
    }))
    .sort((a, b) => a.variantKey.localeCompare(b.variantKey));
  const key = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(key) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_portrait_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then(
            (path) =>
              [
                request.variantKey,
                path ? convertFileSrc(path) : null,
              ] as const
          )
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [variantKey, src] of results) {
          next[variantKey] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded — re-running on cache writes would
    // create an infinite loop. The effect re-fires only when the servant
    // variant set changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return cache;
}

/**
 * Resolve craft-essence card art paths. The resolver lives in Rust
 * (`get_craft_essence_card_path`) and looks up
 * `assets/ces/{id}/card_ce.png`. Cards are checked into the repo today
 * for the full Atlas Academy CE list, so this almost always resolves
 * to a real file — the `null` branch in `CraftEssenceOverlay` exists
 * only as a defensive fallback for missing assets.
 */
function useCeCards(ceIds: number[]): Record<number, string | null | undefined> {
  return useAssetPaths("get_craft_essence_card_path", "craftEssenceId", ceIds);
}

interface SortableSlotProps {
  slot: SlotItem;
  portraitSrc: string | null | undefined;
  ceCardSrc: string | null | undefined;
  supportGrandMode: boolean;
  supportGrandCraftEssences: (CraftEssence | null)[];
  supportGrandCeCardSrcs: (string | null | undefined)[];
  supportGrandCeMlbRequired: SupportGrandCraftEssenceMlbRequired;
  supportGrandBondCeMode: SupportGrandBondCeMode;
  mlbIconSrc: string | null | undefined;
  bondIconSrc: string | null | undefined;
  bondNpIconSrc: string | null | undefined;
  supportNpLevel: number | null | undefined;
  supportSkillLevels: SupportSkillLevelMins;
  supportAppendSkillLevels: SupportAppendSkillLevelMins;
  onSelect: () => void;
  onCeSelect: () => void;
  onCeClear: () => void;
  onGrandModeToggle: () => void;
  onGrandCeSelect: (index: number) => void;
  onGrandCeClear: (index: number) => void;
  onSupportSettingsOpen: () => void;
}

/**
 * Map a servant's rarity to the CSS class that drives its portrait
 * frame gradient. Rarity buckets follow FGO's metallic frame scheme:
 *
 *   1 ★ / 2 ★  → brass
 *   3 ★        → silver
 *   4 ★ / 5 ★  → gold
 *
 * Returns an empty string for unknown / out-of-range values so the
 * caller can opt out of the gradient (the default flat-grey frame
 * still renders).
 */
function rarityFrameClass(rarity: number): string {
  if (rarity >= 4) return "rarity-gold";
  if (rarity === 3) return "rarity-silver";
  if (rarity >= 1) return "rarity-brass";
  return "";
}

/**
 * One unified servant+CE card. The vertical stack is:
 *   1. `.slot-header` — empty reserved band (future: ascension/lv/etc).
 *   2. `.servant-portrait` — aspect-ratio-locked image area that owns:
 *        - the portrait `<img>` (or placeholder),
 *        - the top-right `SUPPORT` badge on the support slot,
 *        - a `.ce-overlay` strip pinned to the bottom edge that covers
 *          the lower slice of the portrait (replaces the standalone CE
 *          tile that used to sit below this card),
 *        - on the support slot, either the requirement-summary chips
 *          or the "技能/宝具设置" button as an overlay above the CE strip.
 *
 * Empty / empty-support states keep the same skeleton so the card
 * height matches a filled card; only the middle portrait region swaps
 * for a `+ 选择从者` / `助战` placeholder.
 */
function SortableSlot({
  slot,
  portraitSrc,
  ceCardSrc,
  supportGrandMode,
  supportGrandCraftEssences,
  supportGrandCeCardSrcs,
  supportGrandCeMlbRequired,
  supportGrandBondCeMode,
  mlbIconSrc,
  bondIconSrc,
  bondNpIconSrc,
  supportNpLevel,
  supportSkillLevels,
  supportAppendSkillLevels,
  onSelect,
  onCeSelect,
  onCeClear,
  onGrandModeToggle,
  onGrandCeSelect,
  onGrandCeClear,
  onSupportSettingsOpen,
}: SortableSlotProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: slot.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
    zIndex: isDragging ? 10 : undefined,
  };

  const { servant } = slot;
  const isSupport = slot.type === "support";
  const rarityClass = servant ? rarityFrameClass(servant.rarity) : "";
  const hasSupportRequirements =
    supportNpLevel != null ||
    hasConfiguredLevels(supportSkillLevels) ||
    hasConfiguredLevels(supportAppendSkillLevels);

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="slot-drag-wrapper"
      {...attributes}
      {...listeners}
    >
      <Flex direction="column" className="slot-card">
        <div className="slot-header" />
        <div
          className={`servant-portrait${servant ? " filled" : " empty"}${isSupport ? " support" : ""}${isSupport && supportGrandMode ? " grand-support" : ""}${rarityClass ? ` ${rarityClass}` : ""}`}
          onClick={onSelect}
        >
          {servant ? (
            portraitSrc ? (
              <img
                className="servant-portrait-img"
                src={portraitSrc}
                alt={servant.name_cn}
                draggable={false}
              />
            ) : (
              <Flex
                direction="column"
                align="center"
                justify="center"
                className="servant-portrait-placeholder"
              >
                <Text size="2" weight="bold" align="center">
                  {servant.name_cn}
                </Text>
              </Flex>
            )
          ) : isSupport ? (
            <Flex
              direction="column"
              align="center"
              justify="center"
              gap="1"
              className="servant-portrait-placeholder"
            >
              <PersonIcon width={28} height={28} className="support-slot-icon" />
              <Text size="1" weight="medium" className="support-slot-label">
                助战
              </Text>
            </Flex>
          ) : (
            <Flex
              direction="column"
              align="center"
              justify="center"
              gap="1"
              className="servant-portrait-placeholder"
            >
              <PlusIcon width={28} height={28} className="servant-slot-icon" />
              <Text size="1" color="gray">
                选择从者
              </Text>
            </Flex>
          )}
          {isSupport && (
            <span className="support-corner-badge">SUPPORT</span>
          )}
          {isSupport && (
            <button
              type="button"
              className={`support-grand-toggle${supportGrandMode ? " active" : ""}`}
              aria-pressed={supportGrandMode}
              aria-label={supportGrandMode ? "关闭冠位模式" : "开启冠位模式"}
              onClick={(event) => {
                event.stopPropagation();
                onGrandModeToggle();
              }}
            >
              冠位
            </button>
          )}
          {isSupport && hasSupportRequirements && (
            <SupportRequirementSummary
              npLevel={supportNpLevel}
              skillLevels={supportSkillLevels}
              appendSkillLevels={supportAppendSkillLevels}
              onOpen={onSupportSettingsOpen}
            />
          )}
          {isSupport && !hasSupportRequirements && (
            <Button
              type="button"
              size="1"
              variant="surface"
              color="gray"
              className="support-settings-button support-settings-overlay-button"
              onClick={(event) => {
                event.stopPropagation();
                onSupportSettingsOpen();
              }}
            >
              技能/宝具设置
            </Button>
          )}
          {isSupport && supportGrandMode ? (
            <GrandCraftEssenceOverlay
              craftEssences={supportGrandCraftEssences}
              cardSrcs={supportGrandCeCardSrcs}
              mlbRequired={supportGrandCeMlbRequired}
              mlbIconSrc={mlbIconSrc}
              grandBondCeMode={supportGrandBondCeMode}
              bondIconSrc={bondIconSrc}
              bondNpIconSrc={bondNpIconSrc}
              onSelect={onGrandCeSelect}
              onClear={onGrandCeClear}
            />
          ) : (
            <CraftEssenceOverlay
              craftEssence={slot.craftEssence}
              cardSrc={ceCardSrc}
              mlbRequired={isSupport ? slot.craftEssenceMlbRequired ?? true : false}
              mlbIconSrc={mlbIconSrc}
              onSelect={onCeSelect}
              onClear={onCeClear}
            />
          )}
        </div>
      </Flex>
    </div>
  );
}

export function ContentGrid({
  servants,
  craftEssences,
  slots,
  onSlotsChange,
  activeProject,
  onUpdateActiveProject,
}: ContentGridProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [activeSlotId, setActiveSlotId] = useState<string | null>(null);
  const [ceDialogOpen, setCeDialogOpen] = useState(false);
  const [activeCeTarget, setActiveCeTarget] = useState<
    { type: "slot"; slotId: string } | { type: "grand"; index: number } | null
  >(null);
  const [supportSettingsOpen, setSupportSettingsOpen] = useState(false);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  // The support slot is project-owned: ignore whatever happens to be in
  // `slots[?].servant` for the support row and resolve from the active
  // project's pinned id instead. Falls back to `null` (empty slot) when no
  // project is active or the pin doesn't resolve to a known servant.
  const supportPinned: Servant | null = (() => {
    const id = activeProject?.supportServantId;
    if (id == null) return null;
    return (
      servants.find((s) => s.variantKey === activeProject?.supportServantVariantKey) ??
      servants.find((s) => s.id === id) ??
      null
    );
  })();

  const displaySlots: SlotItem[] = slots.map((s) =>
    s.type === "support" ? { ...s, servant: supportPinned } : s
  );

  const portraitMap = usePortraits(
    displaySlots
      .map((s) => s.servant)
      .filter((servant): servant is Servant => servant != null)
  );

  const ceCardMap = useCeCards(
    [
      ...displaySlots.map((s) => s.craftEssence?.id),
      ...(activeProject?.supportGrandCraftEssenceIds ?? []),
    ]
      .filter((id): id is number => id != null)
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const oldIndex = slots.findIndex((s) => s.id === active.id);
    const newIndex = slots.findIndex((s) => s.id === over.id);
    onSlotsChange(arrayMove(slots, oldIndex, newIndex));
  };

  const handleSlotClick = (slot: SlotItem) => {
    setActiveSlotId(slot.id);
    setDialogOpen(true);
  };

  const handleCeSlotClick = (slotId: string) => {
    setActiveCeTarget({ type: "slot", slotId });
    setCeDialogOpen(true);
  };

  const supportGrandCeIds = normalizeSupportGrandCraftEssenceIds(
    activeProject?.supportGrandCraftEssenceIds,
  );
  const supportGrandCraftEssences = supportGrandCeIds.map((id) =>
    id == null ? null : craftEssences.find((ce) => ce.id === id) ?? null
  );
  const supportGrandCeCardSrcs = supportGrandCeIds.map((id) =>
    id == null ? null : ceCardMap[id]
  );
  const supportGrandCeMlbRequired = normalizeSupportGrandCraftEssenceMlbRequired(
    activeProject?.supportGrandCraftEssenceMlbRequired,
  );
  const supportGrandBondCeMode = activeProject?.supportGrandBondCeMode ?? "any";

  const handleGrandModeToggle = () => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandMode: !(activeProject.supportGrandMode ?? false),
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode,
    });
  };

  const handleGrandCeSlotClick = (index: number) => {
    setActiveCeTarget({ type: "grand", index });
    setCeDialogOpen(true);
  };

  const handleCeSelect = (ce: CraftEssence) => {
    if (!activeCeTarget) return;
    if (activeCeTarget.type === "slot") {
      onSlotsChange(
        slots.map((s) =>
          s.id === activeCeTarget.slotId
            ? {
                ...s,
                craftEssence: ce,
                craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
              }
            : s
        )
      );
      return;
    }
    if (!activeProject) return;
    const nextIds = [...supportGrandCeIds] as SupportGrandCraftEssenceIds;
    nextIds[activeCeTarget.index] = ce.id;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: nextIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode,
    });
  };

  const handleCeClear = (slotId: string) => {
    onSlotsChange(
      slots.map((s) =>
        s.id === slotId
          ? { ...s, craftEssence: null, craftEssenceMlbRequired: true }
          : s
      )
    );
  };

  const handleGrandCeClear = (index: number) => {
    if (!activeProject) return;
    const nextIds = [...supportGrandCeIds] as SupportGrandCraftEssenceIds;
    const nextMlb = [...supportGrandCeMlbRequired] as SupportGrandCraftEssenceMlbRequired;
    nextIds[index] = null;
    nextMlb[index] = true;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: nextIds,
      supportGrandCraftEssenceMlbRequired: nextMlb,
      supportGrandBondCeMode: index === 1 ? "any" : supportGrandBondCeMode,
    });
  };

  const handleCeMlbRequiredChange = (required: boolean) => {
    if (!activeCeTarget) return;
    if (activeCeTarget.type === "slot") {
      onSlotsChange(
        slots.map((s) =>
          s.id === activeCeTarget.slotId
            ? { ...s, craftEssenceMlbRequired: required }
            : s
        )
      );
      return;
    }
    if (!activeProject) return;
    const nextMlb = [...supportGrandCeMlbRequired] as SupportGrandCraftEssenceMlbRequired;
    nextMlb[activeCeTarget.index] = required;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: nextMlb,
      supportGrandBondCeMode,
    });
  };

  const handleGrandBondCeModeChange = (mode: SupportGrandBondCeMode) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode: mode,
    });
  };

  const handleSelect = (servant: Servant) => {
    const target = slots.find((s) => s.id === activeSlotId);
    if (!target) return;
    if (target.type === "support") {
      // Persist the pin on the project so the runner can read it via
      // `RunConfig::support_servant_id`. Leave `slots` untouched for the
      // support row — the support visual derives from the project.
      if (activeProject) {
        void onUpdateActiveProject({
          ...activeProject,
          supportServantId: servant.id,
          supportServantVariantKey: servant.variantKey,
        });
      }
      return;
    }
    onSlotsChange(
      slots.map((s) => (s.id === activeSlotId ? { ...s, servant } : s))
    );
  };

  // Disallow picking the same servant into two non-support slots. The
  // support slot is intentionally exempt — the player can stack their own
  // copy of a friend's servant — so we only build the block-list for
  // party slots and exclude the slot currently being edited (so the user
  // can re-open the dialog on a filled slot without that slot's own
  // servant disappearing from the list).
  const activeSlot = slots.find((s) => s.id === activeSlotId);
  const activeCeSlot =
    activeCeTarget?.type === "slot"
      ? slots.find((s) => s.id === activeCeTarget.slotId)
      : null;
  const activeCeTargetMlbRequired =
    activeCeTarget?.type === "grand"
      ? supportGrandCeMlbRequired[activeCeTarget.index]
      : activeCeSlot?.craftEssenceMlbRequired ?? true;
  const showCeMlbOption =
    activeCeTarget?.type === "grand" || activeCeSlot?.type === "support";
  const showGrandBondOption =
    activeCeTarget?.type === "grand" && activeCeTarget.index === 1;
  const disabledIds: number[] | undefined =
    activeSlot && activeSlot.type !== "support"
      ? slots
        .filter(
          (s) =>
            s.type !== "support" &&
            s.id !== activeSlotId &&
            s.servant != null
        )
        .map((s) => s.servant!.id)
      : undefined;

  const leftSlots = displaySlots.slice(0, 3);
  const rightSlots = displaySlots.slice(3, 6);
  const slotIds = displaySlots.map((s) => s.id);
  const supportSkillLevels = normalizeSupportSkillLevels(
    activeProject?.supportSkillLevelMins,
  );
  const supportAppendSkillLevels = normalizeSupportAppendSkillLevels(
    activeProject?.supportAppendSkillLevelMins,
  );

  const handleSupportSettingsConfirm = (next: {
    npLevel: number | null;
    skillLevels: SupportSkillLevelMins;
    appendSkillLevels: SupportAppendSkillLevelMins;
  }) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportNoblePhantasmLevelMin: next.npLevel,
      supportSkillLevelMins: next.skillLevels,
      supportAppendSkillLevelMins: next.appendSkillLevels,
    });
  };

  const renderSlot = (slot: SlotItem) => (
    <SortableSlot
      key={slot.id}
      slot={slot}
      portraitSrc={slot.servant ? portraitMap[slot.servant.variantKey] : null}
      ceCardSrc={slot.craftEssence ? ceCardMap[slot.craftEssence.id] : null}
      supportGrandMode={activeProject?.supportGrandMode ?? false}
      supportGrandCraftEssences={supportGrandCraftEssences}
      supportGrandCeCardSrcs={supportGrandCeCardSrcs}
      supportGrandCeMlbRequired={supportGrandCeMlbRequired}
      supportGrandBondCeMode={supportGrandBondCeMode}
      mlbIconSrc={mlbIconSrc}
      bondIconSrc={grandBondIconSrc}
      bondNpIconSrc={grandBondNpIconSrc}
      supportNpLevel={activeProject?.supportNoblePhantasmLevelMin ?? null}
      supportSkillLevels={supportSkillLevels}
      supportAppendSkillLevels={supportAppendSkillLevels}
      onSelect={() => handleSlotClick(slot)}
      onCeSelect={() => handleCeSlotClick(slot.id)}
      onCeClear={() => handleCeClear(slot.id)}
      onGrandModeToggle={handleGrandModeToggle}
      onGrandCeSelect={handleGrandCeSlotClick}
      onGrandCeClear={handleGrandCeClear}
      onSupportSettingsOpen={() => setSupportSettingsOpen(true)}
    />
  );

  return (
    <>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext items={slotIds} strategy={rectSortingStrategy}>
          <Box className="content-unified-grid">
            {leftSlots.map(renderSlot)}
            {rightSlots.map(renderSlot)}
          </Box>
        </SortableContext>
      </DndContext>

      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onSelect={handleSelect}
        servants={servants}
        disabledIds={disabledIds}
      />

      <CraftEssenceSelectDialog
        open={ceDialogOpen}
        onOpenChange={(open) => {
          setCeDialogOpen(open);
          if (!open) setActiveCeTarget(null);
        }}
        onSelect={handleCeSelect}
        craftEssences={craftEssences}
        mlbRequired={activeCeTargetMlbRequired}
        onMlbRequiredChange={showCeMlbOption ? handleCeMlbRequiredChange : undefined}
        grandBondCeMode={supportGrandBondCeMode}
        onGrandBondCeModeChange={
          showGrandBondOption ? handleGrandBondCeModeChange : undefined
        }
      />

      {supportSettingsOpen && (
        <SupportSettingsDialog
          open={supportSettingsOpen}
          project={activeProject}
          onOpenChange={setSupportSettingsOpen}
          onConfirm={handleSupportSettingsConfirm}
        />
      )}
    </>
  );
}
