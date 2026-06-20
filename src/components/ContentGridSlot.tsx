import {
  Button,
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
import type { CraftEssence } from "../types/craftEssence";
import type {
  SupportAppendSkillLevelMins,
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceMlbRequired,
  SupportSkillLevelMins,
} from "../types/project";
import type { SlotItem } from "./contentGridTypes";

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
