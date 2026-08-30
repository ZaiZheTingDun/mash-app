import {
  Button,
  ContextMenu,
  Flex,
  Text,
} from "@radix-ui/themes";
import { PlusIcon, PersonIcon } from "@radix-ui/react-icons";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  CraftEssenceOverlay,
  GrandCraftEssenceOverlay,
} from "./ContentGridOverlays";
import { SupportRequirementSummary } from "./SupportSettingsDialog";
import { hasConfiguredLevels } from "./supportSettingsModel";
import type { CraftEssence } from "../../types/craftEssence";
import type {
  SupportAppendSkillLevelMins,
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceMlbRequired,
  SupportSkillLevelMins,
} from "../../types/project";
import type { SlotItem } from "./contentGridTypes";

interface SortableSlotProps {
  slot: SlotItem;
  portraitSrc: string | null | undefined;
  ceCardSrc: string | null | undefined;
  craftEssences: CraftEssence[];
  ceCardSrcs: (string | null | undefined)[];
  supportGrandMode: boolean;
  supportGrandCraftEssenceGroups: CraftEssence[][];
  supportGrandCeCardSrcGroups: (string | null | undefined)[][];
  supportGrandCeMlbRequired: SupportGrandCraftEssenceMlbRequired;
  supportGrandBondCeMode: SupportGrandBondCeMode;
  mlbIconSrc: string | null | undefined;
  bondIconSrc: string | null | undefined;
  bondNpIconSrc: string | null | undefined;
  supportServantLevel: number | null | undefined;
  supportNpLevel: number | null | undefined;
  supportStarMapScore: number | null | undefined;
  supportGrandStarMapScore: number | null | undefined;
  supportSkillLevels: SupportSkillLevelMins;
  supportAppendSkillLevels: SupportAppendSkillLevelMins;
  onSelect: () => void;
  onCeSelect: () => void;
  onCeAdd: () => void;
  onCeManage: () => void;
  onCeClear: () => void;
  onGrandModeToggle: () => void;
  onGrandCeSelect: (index: number) => void;
  onGrandCeAdd: (index: number) => void;
  onGrandCeManage: (index: number) => void;
  onGrandCeClear: (index: number) => void;
  onSupportSettingsOpen: () => void;
  onDeleteRequest?: () => void;
  onPortraitSettingsOpen?: () => void;
}

function rarityFrameClass(rarity: number): string {
  if (rarity >= 4) return "rarity-gold";
  if (rarity === 3) return "rarity-silver";
  if (rarity >= 1) return "rarity-brass";
  return "";
}

/**
 * One unified servant+CE card. It owns portrait rendering, support
 * badges, per-slot CE overlays, and the support requirement affordance.
 */
export function SortableSlot({
  slot,
  portraitSrc,
  ceCardSrc,
  craftEssences,
  ceCardSrcs,
  supportGrandMode,
  supportGrandCraftEssenceGroups,
  supportGrandCeCardSrcGroups,
  supportGrandCeMlbRequired,
  supportGrandBondCeMode,
  mlbIconSrc,
  bondIconSrc,
  bondNpIconSrc,
  supportServantLevel,
  supportNpLevel,
  supportStarMapScore,
  supportGrandStarMapScore,
  supportSkillLevels,
  supportAppendSkillLevels,
  onSelect,
  onCeSelect,
  onCeAdd,
  onCeManage,
  onCeClear,
  onGrandModeToggle,
  onGrandCeSelect,
  onGrandCeAdd,
  onGrandCeManage,
  onGrandCeClear,
  onSupportSettingsOpen,
  onDeleteRequest,
  onPortraitSettingsOpen,
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
    supportServantLevel != null ||
    supportStarMapScore != null ||
    (supportGrandMode && supportGrandStarMapScore != null) ||
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
        <ContextMenu.Root>
          <ContextMenu.Trigger disabled={!servant}>
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
                  grandMode={supportGrandMode}
                  servantLevel={supportServantLevel}
                  starMapScore={supportStarMapScore}
                  grandStarMapScore={supportGrandStarMapScore}
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
                  助战筛选设置
                </Button>
              )}
              {isSupport && supportGrandMode ? (
                <GrandCraftEssenceOverlay
                  craftEssenceGroups={supportGrandCraftEssenceGroups}
                  cardSrcGroups={supportGrandCeCardSrcGroups}
                  mlbRequired={supportGrandCeMlbRequired}
                  mlbIconSrc={mlbIconSrc}
                  grandBondCeMode={supportGrandBondCeMode}
                  bondIconSrc={bondIconSrc}
                  bondNpIconSrc={bondNpIconSrc}
                  onSelect={onGrandCeSelect}
                  onAdd={onGrandCeAdd}
                  onManage={onGrandCeManage}
                  onClear={onGrandCeClear}
                />
              ) : (
                <CraftEssenceOverlay
                  craftEssence={slot.craftEssence}
                  cardSrc={ceCardSrc}
                  craftEssences={craftEssences}
                  cardSrcs={ceCardSrcs}
                  mlbRequired={isSupport ? slot.craftEssenceMlbRequired ?? true : false}
                  mlbIconSrc={mlbIconSrc}
                  onSelect={onCeSelect}
                  onAdd={isSupport ? onCeAdd : undefined}
                  onManage={isSupport ? onCeManage : undefined}
                  onClear={onCeClear}
                />
              )}
            </div>
          </ContextMenu.Trigger>
          <ContextMenu.Content>

            <ContextMenu.Item
              disabled={!onPortraitSettingsOpen}
              onSelect={() => onPortraitSettingsOpen?.()}
            >
              立绘设置
            </ContextMenu.Item>
            <ContextMenu.Separator />
            <ContextMenu.Item
              color="red"
              onSelect={() => onDeleteRequest?.()}
            >
              删除
            </ContextMenu.Item>
          </ContextMenu.Content>
        </ContextMenu.Root>
      </Flex>
    </div>
  );
}
