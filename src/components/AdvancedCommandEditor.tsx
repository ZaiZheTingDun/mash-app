import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { Dialog, Flex, Text, Button } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  ChevronUpIcon,
  Cross2Icon,
  PersonIcon,
  PlusIcon,
} from "@radix-ui/react-icons";
import { convertFileSrc, invoke } from "../tauri";
import { deriveLineupAfterPreparationActions } from "./partyServants";
import type {
  AdvancedBattleScene,
  AdvancedCommandCardCondition,
  CommandSpellAction,
  EquipmentAction,
  OrderChangeSelection,
  PreparationAction,
  ServantAction,
} from "../types/command";
import type {
  GrandCardPriority,
  GrandCardStrategy,
  GrandChainPriorityItem,
  GrandNpCard,
  GrandServantConfig,
} from "../types/project";
import type { Servant } from "../types/servant";
import orderChangeIcon from "../../src-tauri/resources/images/icon_order_change.png";
import commandBgArts from "../../src-tauri/resources/images/command_bg/command_bg_a.png";
import commandBgBuster from "../../src-tauri/resources/images/command_bg/command_bg_b.png";
import commandBgQuick from "../../src-tauri/resources/images/command_bg/command_bg_q.png";

interface AdvancedCommandEditorProps {
  projectId: string | null;
  partyLineup: (Servant | null)[];
  grandServants?: GrandServantConfig[];
  grandCardStrategy?: GrandCardStrategy;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

type PartySlot = `servant_${1 | 2 | 3 | 4 | 5 | 6}`;
type FrontServant = Extract<PartySlot, "servant_1" | "servant_2" | "servant_3">;
type PrepSource = "equipment" | "commandSpell" | PartySlot;
type PrepDraft =
  | { step: "source" }
  | { step: "option"; source: PrepSource }
  | { step: "target"; source: PrepSource; option: string }
  | { step: "orderChange"; source: "equipment"; option: string; front: PartySlot | null };

const SKILLS = ["skill_1", "skill_2", "skill_3"] as const;
const SKILL_LABELS: Record<string, string> = {
  skill_1: "技能 1",
  skill_2: "技能 2",
  skill_3: "技能 3",
};
const COMMAND_SPELL_LABELS: Record<string, string> = {
  np_release: "宝具解放",
  restore: "灵基修复",
};
const EMPTY_STARTUP_ACTIONS: PreparationAction[] = [];
const COMMAND_BG_BY_SUIT: Record<Exclude<AdvancedCommandCardCondition["suit"], "any">, string> = {
  arts: commandBgArts,
  buster: commandBgBuster,
  quick: commandBgQuick,
};
const DEFAULT_GRAND_CHAIN_PRIORITY: GrandChainPriorityItem[] = [
  "mainBraveChain",
  "mainReadyNp",
  "deputyBraveChain",
  "mainColorChain",
  "deputyColorChain",
  "fallback",
];
const GRAND_CHAIN_PRIORITY_LABELS: Record<GrandChainPriorityItem, string> = {
  mainBraveChain: "主冠位三卡链",
  mainReadyNp: "主冠位宝具已就绪",
  deputyBraveChain: "副冠位三卡链",
  mainColorChain: "包含主冠位的同色链",
  deputyColorChain: "包含副冠位的同色链",
  fallback: "兜底输出排序",
};

let nextAdvancedSceneId = 1;

function createId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

function createDefaultScene(): AdvancedBattleScene {
  return {
    id: `advanced_scene_${nextAdvancedSceneId++}_${Date.now()}`,
    mainOutput: { servant: null, outputType: null, npCard: "auto" },
    grandAutoOrderChange: null,
    commandConditions: [0, 1, 2, 3, 4].map(defaultCommandCard),
    controlActions: [],
    startupActions: [],
    rules: [],
  };
}

function normalizeScene(scene: AdvancedBattleScene): AdvancedBattleScene {
  return {
    ...scene,
    mainOutput: scene.mainOutput
      ? { npCard: "auto", ...scene.mainOutput }
      : { servant: null, outputType: null, npCard: "auto" },
    grandAutoOrderChange: scene.grandAutoOrderChange ?? null,
    commandConditions:
      scene.commandConditions && scene.commandConditions.length === 5
        ? scene.commandConditions.map((card) => ({ ...card, minCritChance: null }))
        : [0, 1, 2, 3, 4].map(defaultCommandCard),
    controlActions: scene.controlActions ?? [],
    startupActions: scene.startupActions ?? [],
    rules: [],
  };
}

function defaultCommandCard(slot: number): AdvancedCommandCardCondition {
  return {
    slot,
    servant: "any",
    suit: "any",
    minCritChance: null,
  };
}

function servantLabel(index: number, servant: Servant | null): string {
  return servant?.name_cn || `从者 ${index + 1}`;
}

function servantSlotIndex(source: string | null | undefined): number | null {
  const match = source?.match(/^servant_([1-6])$/);
  return match ? Number(match[1]) - 1 : null;
}

function mainGrandBackSlot(grandServants: GrandServantConfig[]): number | null {
  const slotIndex = grandServants[0]?.slotIndex;
  return Number.isInteger(slotIndex) && slotIndex >= 3 && slotIndex < 6 ? slotIndex : null;
}

function normalizeGrandServants(values: GrandServantConfig[] | undefined): GrandServantConfig[] {
  const seen = new Set<number>();
  return (values ?? [])
    .filter((item) => Number.isInteger(item.slotIndex) && item.slotIndex >= 0 && item.slotIndex < 6)
    .filter((item) => {
      if (seen.has(item.slotIndex)) return false;
      seen.add(item.slotIndex);
      return true;
    })
    .slice(0, 2)
    .map((item) => ({
      slotIndex: item.slotIndex,
      npCard: item.npCard ?? "auto",
      priority: item.priority ?? "damage",
    }));
}

function normalizeGrandCardStrategy(strategy: GrandCardStrategy | undefined): GrandCardStrategy {
  const configured = strategy?.chainPriority ?? [];
  const priority = configured.filter(
    (item, index): item is GrandChainPriorityItem =>
      DEFAULT_GRAND_CHAIN_PRIORITY.includes(item as GrandChainPriorityItem) &&
      configured.indexOf(item) === index
  );
  for (const item of DEFAULT_GRAND_CHAIN_PRIORITY) {
    if (!priority.includes(item)) {
      priority.push(item);
    }
  }
  return { chainPriority: priority };
}

function cardColorLabel(card: Servant["noblePhantasmCard"] | undefined): string | null {
  switch (card) {
    case "buster":
      return "红";
    case "arts":
      return "蓝";
    case "quick":
      return "绿";
    default:
      return null;
  }
}

function npCardLabel(card: GrandNpCard | undefined, inferredCard?: Servant["noblePhantasmCard"]): string {
  switch (card) {
    case "buster":
      return "红";
    case "arts":
      return "蓝";
    case "quick":
      return "绿";
    default:
      return cardColorLabel(inferredCard) ? `自动${cardColorLabel(inferredCard)}` : "自动";
  }
}

function autoNpOptionLabel(servant: Servant | null): string {
  const label = cardColorLabel(servant?.noblePhantasmCard);
  return label ? `自动读取（${label}）` : "自动读取";
}

function priorityLabel(priority: GrandCardPriority | undefined): string {
  return priority === "np" ? "NP" : "伤害";
}

function prepSummary(action: PreparationAction, partyLineup: (Servant | null)[]): string {
  if (action.type === "commandSpell") {
    const target = servantSlotIndex(action.target);
    const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
    return `令咒 ${COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动"}${suffix}`;
  }
  if (action.type === "equipment") {
    if (action.orderChange) {
      const front = servantSlotIndex(action.orderChange.front);
      const back = servantSlotIndex(action.orderChange.back);
      const swap =
        front == null || back == null
          ? ""
          : ` ${servantLabel(front, partyLineup[front] ?? null)} ↔ ${servantLabel(back, partyLineup[back] ?? null)}`;
      return `御主礼装 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change${swap}`;
    }
    const target = servantSlotIndex(action.target);
    const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
    return `御主礼装 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${suffix}`;
  }
  const source = servantSlotIndex(action.servant) ?? 0;
  const target = servantSlotIndex(action.target);
  const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
  return `${servantLabel(source, partyLineup[source] ?? null)} ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${suffix}`;
}

function commandCardSummary(card: AdvancedCommandCardCondition): string {
  const servant = card.servant === "any" ? "ANY" : `S${card.servant.slice(-1)}`;
  const suit = card.suit === "any" ? "ANY" : card.suit[0].toUpperCase();
  return `${servant}${suit}`;
}

function commandCardAria(card: AdvancedCommandCardCondition): string {
  return `设置指令卡 ${card.slot + 1}，${commandCardSummary(card)}`;
}

function useServantFaces(partyLineup: (Servant | null)[]) {
  const [faces, setFaces] = useState<Record<string, string | null>>({});
  const requests = useMemo(
    () =>
      partyLineup
        .filter((servant): servant is Servant => Boolean(servant))
        .map((servant) => ({
          key: servant.variantKey,
          servantId: servant.id,
          faceId: servant.faceId ?? null,
        })),
    [partyLineup]
  );

  useEffect(() => {
    let cancelled = false;
    const missing = requests.filter(({ key }) => !(key in faces));
    if (missing.length === 0) return;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then((path) => [request.key, path ? convertFileSrc(path) : null] as const)
          .catch(() => [request.key, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [key, src] of results) next[key] = src;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [faces, requests]);

  return faces;
}

function FaceChip({
  servant,
  index,
  src,
  active = true,
  selected = false,
  disabled = false,
  onClick,
}: {
  servant: Servant | null;
  index: number;
  src: string | null | undefined;
  active?: boolean;
  selected?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      className={`advanced-face-chip${active ? "" : " dim"}${selected ? " selected" : ""}`}
      aria-label={servantLabel(index, servant)}
      disabled={disabled}
      onClick={onClick}
    >
      {src ? <img src={src} alt="" draggable={false} /> : <span>{index + 1}</span>}
    </button>
  );
}

function AdvancedInlineFace({
  servant,
  index,
  src,
}: {
  servant: Servant | null;
  index: number;
  src: string | null | undefined;
}) {
  return (
    <span className="battle-inline-face" aria-label={servantLabel(index, servant)}>
      {src ? <img src={src} alt="" draggable={false} /> : <PersonIcon width={18} height={18} />}
    </span>
  );
}

function GrandOutputSettings({
  partyLineup,
  faces,
  grandServants,
  onChange,
}: {
  partyLineup: (Servant | null)[];
  faces: Record<string, string | null>;
  grandServants: GrandServantConfig[];
  onChange?: (grandServants: GrandServantConfig[]) => void;
}) {
  const [settingsIndex, setSettingsIndex] = useState<number | null>(null);
  const normalized = normalizeGrandServants(grandServants);
  const selectedSlots = new Set(normalized.map((item) => item.slotIndex));
  const settings = settingsIndex == null ? null : normalized[settingsIndex] ?? null;
  const settingsServant = settings == null ? null : partyLineup[settings.slotIndex] ?? null;

  const persist = (next: GrandServantConfig[]) => {
    onChange?.(normalizeGrandServants(next));
  };
  const addGrandServant = (slotIndex: number) => {
    if (normalized.length >= 2 || selectedSlots.has(slotIndex) || partyLineup[slotIndex] == null) {
      return;
    }
    persist([...normalized, { slotIndex, npCard: "auto", priority: "damage" }]);
  };
  const removeGrandServant = (index: number) => {
    persist(normalized.filter((_, itemIndex) => itemIndex !== index));
    setSettingsIndex(null);
  };
  const updateGrandServant = (
    index: number,
    patch: Partial<Pick<GrandServantConfig, "npCard" | "priority">>,
  ) => {
    persist(normalized.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item)));
  };
  const moveToMain = (index: number) => {
    if (index <= 0) return;
    const next = [...normalized];
    const [item] = next.splice(index, 1);
    next.unshift(item);
    persist(next);
    setSettingsIndex(0);
  };

  return (
    <>
      <div className="advanced-grand-output">
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">冠位</Text>
          <div className="advanced-grand-output-slots">
            {normalized.map((config, index) => {
              const servant = partyLineup[config.slotIndex] ?? null;
              return (
                <button
                  key={`${config.slotIndex}-${index}`}
                  type="button"
                  className="grand-servant-tile"
                  aria-label={`${index === 0 ? "主" : "副"}冠位${servant ? `：${servant.name_cn}` : ""}`}
                  onClick={() => setSettingsIndex(index)}
                >
                  <span className="grand-role-badge">{index === 0 ? "主" : "副"}</span>
                  {servant && faces[servant.variantKey] ? (
                    <img src={faces[servant.variantKey] ?? undefined} alt={servant.name_cn} draggable={false} />
                  ) : (
                    <span className="grand-servant-placeholder">{servant?.name_cn ?? "未选择"}</span>
                  )}
                  <span className="grand-np-badge">{npCardLabel(config.npCard, servant?.noblePhantasmCard)}</span>
                  <span className="grand-priority-badge">{priorityLabel(config.priority)}</span>
                </button>
              );
            })}
            {normalized.length < 2 && <div className="grand-servant-empty">选择冠位从者</div>}
          </div>
        </div>
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">辅助</Text>
          <div className="battle-choice-row">
            {partyLineup.slice(0, 6).map((servant, index) => (
              <FaceChip
                key={index}
                servant={servant}
                index={index}
                src={servant ? faces[servant.variantKey] : null}
                selected={selectedSlots.has(index)}
                disabled={servant == null || selectedSlots.has(index) || normalized.length >= 2}
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
          {settings && settingsIndex != null && (
            <Flex direction="column" gap="4">
              <label className="grand-setting-field">
                <Text size="2" weight="medium">宝具颜色</Text>
                <select
                  value={settings.npCard ?? "auto"}
                  onChange={(event) =>
                    updateGrandServant(settingsIndex, {
                      npCard: event.target.value as GrandNpCard,
                    })
                  }
                >
                  <option value="auto">{autoNpOptionLabel(settingsServant)}</option>
                  <option value="buster">红卡</option>
                  <option value="arts">蓝卡</option>
                  <option value="quick">绿卡</option>
                </select>
              </label>
              <label className="grand-setting-field">
                <Text size="2" weight="medium">出卡策略</Text>
                <select
                  value={settings.priority ?? "damage"}
                  onChange={(event) =>
                    updateGrandServant(settingsIndex, {
                      priority: event.target.value as GrandCardPriority,
                    })
                  }
                >
                  <option value="damage">伤害优先</option>
                  <option value="np">NP 优先</option>
                </select>
              </label>
              <Flex justify="between" gap="3">
                <Button type="button" variant="soft" color="red" onClick={() => removeGrandServant(settingsIndex)}>
                  移除
                </Button>
                <Flex gap="3">
                  {settingsIndex > 0 && (
                    <Button type="button" variant="soft" onClick={() => moveToMain(settingsIndex)}>
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

function AdvancedPreparationActionSummary({
  action,
  partyLineup,
  faces,
}: {
  action: PreparationAction;
  partyLineup: (Servant | null)[];
  faces: Record<string, string | null>;
}) {
  const targetIndex = servantSlotIndex(action.target);
  const orderChangeSlots =
    action.type === "equipment" && action.orderChange
      ? {
          front: servantSlotIndex(action.orderChange.front),
          back: servantSlotIndex(action.orderChange.back),
        }
      : null;

  let sourceFace;
  let sourceText: string;
  let actionText: string;

  if (action.type === "servant") {
    const source = servantSlotIndex(action.servant) ?? 0;
    const servant = partyLineup[source] ?? null;
    sourceFace = (
      <AdvancedInlineFace
        servant={servant}
        index={source}
        src={servant ? faces[servant.variantKey] : null}
      />
    );
    sourceText = servantLabel(source, servant);
    actionText = `释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`;
  } else {
    sourceFace = (
      <span className="battle-inline-square">
        {action.type === "equipment" ? "御主" : "令咒"}
      </span>
    );
    sourceText = action.type === "equipment" ? "御主礼装" : "令咒";
    actionText =
      action.type === "equipment"
        ? `释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
        : COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动";
  }

  return (
    <span className="battle-action-summary" aria-label={prepSummary(action, partyLineup)}>
      {sourceFace}
      <Text size="2" weight="medium" className="battle-action-name">
        {sourceText} {actionText}
      </Text>
      {orderChangeSlots?.front != null && orderChangeSlots.back != null ? (
        <>
          <span className="battle-action-to">Order Change</span>
          <AdvancedInlineFace
            servant={partyLineup[orderChangeSlots.front] ?? null}
            index={orderChangeSlots.front}
            src={
              partyLineup[orderChangeSlots.front]
                ? faces[partyLineup[orderChangeSlots.front]!.variantKey]
                : null
            }
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.front, partyLineup[orderChangeSlots.front] ?? null)}
          </Text>
          <span className="battle-action-to">↔</span>
          <AdvancedInlineFace
            servant={partyLineup[orderChangeSlots.back] ?? null}
            index={orderChangeSlots.back}
            src={
              partyLineup[orderChangeSlots.back]
                ? faces[partyLineup[orderChangeSlots.back]!.variantKey]
                : null
            }
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.back, partyLineup[orderChangeSlots.back] ?? null)}
          </Text>
        </>
      ) : (
        targetIndex != null && (
          <>
            <span className="battle-action-to">to</span>
            <AdvancedInlineFace
              servant={partyLineup[targetIndex] ?? null}
              index={targetIndex}
              src={partyLineup[targetIndex] ? faces[partyLineup[targetIndex]!.variantKey] : null}
            />
            <Text size="2" weight="medium" className="battle-action-name">
              {servantLabel(targetIndex, partyLineup[targetIndex] ?? null)}
            </Text>
          </>
        )
      )}
    </span>
  );
}

function AdvancedCommandCardButton({
  card,
  partyLineup,
  faces,
  onClick,
}: {
  card: AdvancedCommandCardCondition;
  partyLineup: (Servant | null)[];
  faces: Record<string, string | null>;
  onClick: () => void;
}) {
  const servantIndex = servantSlotIndex(card.servant);
  const servant = servantIndex == null ? null : partyLineup[servantIndex] ?? null;
  const faceSrc = servant ? faces[servant.variantKey] : null;
  const unset = card.servant === "any" && card.suit === "any";
  const grayscale = card.suit === "any";
  const style = {
    "--command-card-bg": `url(${card.suit === "any" ? commandBgArts : COMMAND_BG_BY_SUIT[card.suit]})`,
  } as CSSProperties;

  return (
    <button
      type="button"
      className={`advanced-command-card${grayscale ? " unset" : ""}`}
      style={style}
      aria-label={commandCardAria(card)}
      onClick={onClick}
    >
      {!unset && servantIndex != null && (
        <span className="advanced-command-card-face">
          {faceSrc ? <img src={faceSrc} alt="" draggable={false} /> : <span>{servantIndex + 1}</span>}
        </span>
      )}
    </button>
  );
}

function GrandCardStrategyPanel({
  strategy,
  onChange,
}: {
  strategy?: GrandCardStrategy;
  onChange?: (strategy: GrandCardStrategy) => void;
}) {
  const [open, setOpen] = useState(false);
  const normalized = normalizeGrandCardStrategy(strategy);
  const priority = normalized.chainPriority ?? DEFAULT_GRAND_CHAIN_PRIORITY;
  const firstPriority = priority[0] ?? "mainBraveChain";

  const moveItem = (index: number, delta: -1 | 1) => {
    const nextIndex = index + delta;
    if (nextIndex < 0 || nextIndex >= priority.length) return;
    const next = [...priority];
    const [item] = next.splice(index, 1);
    next.splice(nextIndex, 0, item);
    onChange?.({ chainPriority: next });
  };

  const reset = () => {
    onChange?.({ chainPriority: [...DEFAULT_GRAND_CHAIN_PRIORITY] });
  };

  return (
    <section className="battle-phase advanced-strategy-section grand-card-strategy-section">
      <button
        type="button"
        className="grand-strategy-summary"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="grand-strategy-chevron" aria-hidden>
          {open ? <ChevronUpIcon width={16} height={16} /> : <ChevronDownIcon width={16} height={16} />}
        </span>
        <span className="grand-strategy-title">冠位出牌优先级</span>
        <span className="grand-strategy-current">{GRAND_CHAIN_PRIORITY_LABELS[firstPriority]}</span>
      </button>
      {open && (
        <div className="grand-strategy-body">
          <div className="grand-strategy-list">
            {priority.map((item, index) => (
              <div className="grand-strategy-item" key={item}>
                <span className="grand-strategy-rank">{index + 1}</span>
                <Text size="2" weight="medium" className="grand-strategy-label">
                  {GRAND_CHAIN_PRIORITY_LABELS[item]}
                </Text>
                <div className="grand-strategy-actions">
                  <button
                    type="button"
                    className="grand-strategy-move"
                    aria-label={`上移${GRAND_CHAIN_PRIORITY_LABELS[item]}`}
                    disabled={index === 0}
                    onClick={() => moveItem(index, -1)}
                  >
                    <ChevronUpIcon width={15} height={15} />
                  </button>
                  <button
                    type="button"
                    className="grand-strategy-move"
                    aria-label={`下移${GRAND_CHAIN_PRIORITY_LABELS[item]}`}
                    disabled={index === priority.length - 1}
                    onClick={() => moveItem(index, 1)}
                  >
                    <ChevronDownIcon width={15} height={15} />
                  </button>
                </div>
              </div>
            ))}
          </div>
          <Button type="button" variant="soft" color="gray" onClick={reset}>
            恢复默认
          </Button>
        </div>
      )}
    </section>
  );
}

function AdvancedStrategyEditor({
  scene,
  partyLineup,
  faces,
  grandServants,
  grandCardStrategy,
  onGrandServantsChange,
  onGrandCardStrategyChange,
  onChange,
}: {
  scene: AdvancedBattleScene;
  partyLineup: (Servant | null)[];
  faces: Record<string, string | null>;
  grandServants: GrandServantConfig[];
  grandCardStrategy?: GrandCardStrategy;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
  onChange: (scene: AdvancedBattleScene) => void;
}) {
  const [editingCardSlot, setEditingCardSlot] = useState<number | null>(null);
  const [controlDraft, setControlDraft] = useState<PrepDraft | null>(null);
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const grandAutoOrderChange = scene.grandAutoOrderChange ?? null;
  const mainGrandSlot = mainGrandBackSlot(grandServants);
  const mainGrandServant = mainGrandSlot == null ? null : partyLineup[mainGrandSlot] ?? null;
  const startupSelectableSlots = useMemo(() => {
    const slots = [0, 1, 2];
    if (mainGrandSlot != null && grandAutoOrderChange === true) {
      slots.push(mainGrandSlot);
    }
    return slots;
  }, [grandAutoOrderChange, mainGrandSlot]);
  const commandConditions = scene.commandConditions ?? [0, 1, 2, 3, 4].map(defaultCommandCard);
  const controlActions = scene.controlActions ?? EMPTY_STARTUP_ACTIONS;
  const startupActions = scene.startupActions ?? EMPTY_STARTUP_ACTIONS;
  const controlActionLineups = useMemo(() => {
    const lineups: (Servant | null)[][] = [];
    let lineup = partyLineup;
    for (const action of controlActions) {
      lineups.push(lineup);
      lineup = deriveLineupAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [partyLineup, controlActions]);
  const postControlLineup = useMemo(
    () => deriveLineupAfterPreparationActions(partyLineup, controlActions),
    [partyLineup, controlActions]
  );
  const startupActionLineups = useMemo(() => {
    const lineups: (Servant | null)[][] = [];
    let lineup = postControlLineup;
    for (const action of startupActions) {
      lineups.push(lineup);
      lineup = deriveLineupAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [postControlLineup, startupActions]);
  const currentPartyLineup = useMemo(
    () => deriveLineupAfterPreparationActions(postControlLineup, startupActions),
    [postControlLineup, startupActions]
  );
  const editingCard =
    editingCardSlot == null
      ? null
      : commandConditions.find((card) => card.slot === editingCardSlot) ??
        defaultCommandCard(editingCardSlot);

  const updateCommandCard = (
    slot: number,
    updater: (card: AdvancedCommandCardCondition) => AdvancedCommandCardCondition
  ) => {
    onChange({
      ...scene,
      commandConditions: commandConditions.map((card) =>
        card.slot === slot ? updater(card) : card
      ),
    });
  };

  const updateStartupActions = (actions: PreparationAction[]) => {
    onChange({ ...scene, startupActions: actions });
  };

  const updateControlActions = (actions: PreparationAction[]) => {
    onChange({ ...scene, controlActions: actions });
  };

  const makePrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ): PreparationAction => {
    if (draft.source === "equipment") {
      return {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target,
        orderChange: null,
      } satisfies EquipmentAction;
    }
    if (draft.source === "commandSpell") {
      return {
        type: "commandSpell",
        id: createId("cs"),
        spell: draft.option as CommandSpellAction["spell"],
        target,
      } satisfies CommandSpellAction;
    }
    return {
      type: "servant",
      id: createId("sa"),
      servant: draft.source,
      skill: draft.option,
      target,
    } satisfies ServantAction;
  };

  const finishPrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateStartupActions([...startupActions, makePrepAction(draft, target)]);
    setPrepDraft(null);
  };

  const finishControlAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateControlActions([...controlActions, makePrepAction(draft, target)]);
    setControlDraft(null);
  };

  const finishOrderChangeAction = (
    draft: Extract<PrepDraft, { step: "orderChange" }>,
    back: PartySlot
  ) => {
    if (draft.front == null) return;
    updateStartupActions([
      ...startupActions,
      {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target: null,
        orderChange: {
          front: draft.front,
          back,
        } satisfies OrderChangeSelection,
      } satisfies EquipmentAction,
    ]);
    setPrepDraft(null);
  };

  const finishControlOrderChangeAction = (
    draft: Extract<PrepDraft, { step: "orderChange" }>,
    back: PartySlot
  ) => {
    if (draft.front == null) return;
    updateControlActions([
      ...controlActions,
      {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target: null,
        orderChange: {
          front: draft.front,
          back,
        } satisfies OrderChangeSelection,
      } satisfies EquipmentAction,
    ]);
    setControlDraft(null);
  };

  return (
    <div className="advanced-strategy-editor">
      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">主力输出</div>
        <div className="advanced-main-output-grid">
          <span className="advanced-delete-spacer" aria-hidden />
          <GrandOutputSettings
            partyLineup={partyLineup}
            faces={faces}
            grandServants={grandServants}
            onChange={onGrandServantsChange}
          />
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">启动条件</div>
        <div className="advanced-condition-row">
          <span className="advanced-delete-spacer" aria-hidden />
          {mainGrandSlot != null && grandAutoOrderChange == null ? (
            <div className="advanced-grand-order-choice">
              <Text size="2" weight="medium">
                主冠位从者配置在后排，是否自动换位至前排？
              </Text>
              <div className="battle-choice-row">
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: true })}
                >
                  是
                </button>
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: false })}
                >
                  否
                </button>
              </div>
            </div>
          ) : mainGrandSlot != null && grandAutoOrderChange === true ? (
            <div className="advanced-grand-order-note">
              <Text size="2" weight="medium">
                第一回合会自动将
                {mainGrandServant ? ` ${mainGrandServant.name_cn} ` : "主冠位从者"}
                和前排指令卡最多的从者交换。
              </Text>
              <button
                type="button"
                className="battle-option-btn"
                onClick={() => onChange({ ...scene, grandAutoOrderChange: false })}
              >
                改为配置指令卡
              </button>
            </div>
          ) : (
            <div className="advanced-manual-startup-condition">
              <div className="advanced-card-row">
                {commandConditions.map((card) => (
                  <AdvancedCommandCardButton
                    key={card.slot}
                    card={card}
                    partyLineup={partyLineup}
                    faces={faces}
                    onClick={() => setEditingCardSlot(card.slot)}
                  />
                ))}
              </div>
              {mainGrandSlot != null && grandAutoOrderChange === false && (
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: true })}
                >
                  改为自动换位
                </button>
              )}
            </div>
          )}
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">控制栏</div>
        <div className="advanced-rule-section">
          {controlActions.map((action, index) => (
            <div className="battle-action-row committed advanced-action-row" key={action.id}>
              <button
                type="button"
                className="advanced-inline-delete"
                aria-label="删除控制行动"
                onClick={() => updateControlActions(controlActions.filter((_, i) => i !== index))}
              >
                <Cross2Icon width={13} height={13} />
              </button>
              <AdvancedPreparationActionSummary
                action={action}
                partyLineup={controlActionLineups[index] ?? partyLineup}
                faces={faces}
              />
            </div>
          ))}
          {!controlDraft ? (
            <button
              type="button"
              className="battle-add-trigger"
              onClick={() => setControlDraft({ step: "source" })}
            >
              <span className="advanced-delete-spacer" aria-hidden />
              <span className="battle-plus-box">
                <PlusIcon width={16} height={16} />
              </span>
              <Text size="2" weight="medium">添加控制行动</Text>
            </button>
          ) : (
            <div className="battle-choice-row">
              <span className="advanced-delete-spacer" aria-hidden />
              {controlDraft.step === "source" ? (
                <>
                  {deriveLineupAfterPreparationActions(partyLineup, controlActions)
                    .slice(0, 3)
                    .map((servant, index) => (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        onClick={() =>
                          setControlDraft({
                            step: "option",
                            source: `servant_${index + 1}` as FrontServant,
                          })
                        }
                      />
                    ))}
                  <button type="button" className="battle-option-btn" onClick={() => setControlDraft({ step: "option", source: "equipment" })}>
                    御主礼装
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setControlDraft({ step: "option", source: "commandSpell" })}>
                    令咒
                  </button>
                </>
              ) : controlDraft.step === "option" ? (
                <>
                  {controlDraft.source === "commandSpell"
                    ? Object.entries(COMMAND_SPELL_LABELS).map(([value, label]) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={value}
                          onClick={() => setControlDraft({ step: "target", source: controlDraft.source, option: value })}
                        >
                          {label}
                        </button>
                      ))
                    : SKILLS.map((skill) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={skill}
                          onClick={() => setControlDraft({ step: "target", source: controlDraft.source, option: skill })}
                        >
                          {SKILL_LABELS[skill]}
                        </button>
                      ))}
                </>
              ) : controlDraft.step === "target" ? (
                <>
                  <button type="button" className="battle-option-btn" onClick={() => finishControlAction(controlDraft, null)}>
                    无目标
                  </button>
                  {deriveLineupAfterPreparationActions(partyLineup, controlActions)
                    .slice(0, 3)
                    .map((servant, index) => (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        onClick={() => finishControlAction(controlDraft, `servant_${index + 1}`)}
                      />
                    ))}
                  {controlDraft.source === "equipment" && (
                    <>
                      <span className="battle-choice-separator" aria-hidden />
                      <button
                        type="button"
                        className="battle-option-btn order-change"
                        aria-label="Order Change"
                        onClick={() =>
                          setControlDraft({
                            step: "orderChange",
                            source: "equipment",
                            option: controlDraft.option,
                            front: null,
                          })
                        }
                      >
                        <img src={orderChangeIcon} alt="" draggable={false} />
                      </button>
                    </>
                  )}
                </>
              ) : (
                <>
                  {Array.from(
                    { length: 6 },
                    (_, index) => deriveLineupAfterPreparationActions(partyLineup, controlActions)[index] ?? null
                  ).map((servant, index) => {
                    const slot = `servant_${index + 1}` as PartySlot;
                    const needsFront = controlDraft.front == null;
                    const selectable = Boolean(servant) && (needsFront ? index < 3 : index >= 3);
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        active={selectable || controlDraft.front === slot}
                        onClick={() => {
                          if (!selectable) return;
                          if (needsFront) {
                            setControlDraft({ ...controlDraft, front: slot });
                          } else {
                            finishControlOrderChangeAction(controlDraft, slot);
                          }
                        }}
                      />
                    );
                  })}
                  <Text size="2" weight="medium" className="battle-order-change-hint">
                    {controlDraft.front == null ? "选择前排" : "选择后排"}
                  </Text>
                </>
              )}
            </div>
          )}
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">启动阶段</div>
        <div className="advanced-rule-section">
          {startupActions.map((action, index) => (
            <div className="battle-action-row committed advanced-action-row" key={action.id}>
              <button
                type="button"
                className="advanced-inline-delete"
                aria-label="删除启动行动"
                onClick={() => updateStartupActions(startupActions.filter((_, i) => i !== index))}
              >
                <Cross2Icon width={13} height={13} />
              </button>
              <AdvancedPreparationActionSummary
                action={action}
                partyLineup={startupActionLineups[index] ?? partyLineup}
                faces={faces}
              />
            </div>
          ))}
          {!prepDraft ? (
            <button
              type="button"
              className="battle-add-trigger"
              onClick={() => setPrepDraft({ step: "source" })}
            >
              <span className="advanced-delete-spacer" aria-hidden />
              <span className="battle-plus-box">
                <PlusIcon width={16} height={16} />
              </span>
              <Text size="2" weight="medium">添加启动行动</Text>
            </button>
          ) : (
            <div className="battle-choice-row">
              <span className="advanced-delete-spacer" aria-hidden />
              {prepDraft.step === "source" ? (
                <>
                  {startupSelectableSlots.map((index) => {
                    const servant = currentPartyLineup[index] ?? null;
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        onClick={() =>
                          setPrepDraft({
                            step: "option",
                            source: `servant_${index + 1}` as PartySlot,
                          })
                        }
                      />
                    );
                  })}
                  <button type="button" className="battle-option-btn" onClick={() => setPrepDraft({ step: "option", source: "equipment" })}>
                    御主礼装
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setPrepDraft({ step: "option", source: "commandSpell" })}>
                    令咒
                  </button>
                </>
              ) : prepDraft.step === "option" ? (
                <>
                  {prepDraft.source === "commandSpell"
                    ? Object.entries(COMMAND_SPELL_LABELS).map(([value, label]) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={value}
                          onClick={() => setPrepDraft({ step: "target", source: prepDraft.source, option: value })}
                        >
                          {label}
                        </button>
                      ))
                    : SKILLS.map((skill) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={skill}
                          onClick={() => setPrepDraft({ step: "target", source: prepDraft.source, option: skill })}
                        >
                          {SKILL_LABELS[skill]}
                        </button>
                      ))}
                </>
              ) : prepDraft.step === "target" ? (
                <>
                  <button type="button" className="battle-option-btn" onClick={() => finishPrepAction(prepDraft, null)}>
                    无目标
                  </button>
                  {startupSelectableSlots.map((index) => {
                    const servant = currentPartyLineup[index] ?? null;
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        onClick={() => finishPrepAction(prepDraft, `servant_${index + 1}`)}
                      />
                    );
                  })}
                  {prepDraft.source === "equipment" && (
                    <>
                      <span className="battle-choice-separator" aria-hidden />
                      <button
                        type="button"
                        className="battle-option-btn order-change"
                        aria-label="Order Change"
                        onClick={() =>
                          setPrepDraft({
                            step: "orderChange",
                            source: "equipment",
                            option: prepDraft.option,
                            front: null,
                          })
                        }
                      >
                        <img src={orderChangeIcon} alt="" draggable={false} />
                      </button>
                    </>
                  )}
                </>
              ) : (
                <>
                  {Array.from({ length: 6 }, (_, index) => currentPartyLineup[index] ?? null).map((servant, index) => {
                    const slot = `servant_${index + 1}` as PartySlot;
                    const needsFront = prepDraft.front == null;
                    const selectable =
                      Boolean(servant) &&
                      (needsFront
                        ? index < 3 || (grandAutoOrderChange === true && index === mainGrandSlot)
                        : index >= 3);
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        active={selectable || prepDraft.front === slot}
                        onClick={() => {
                          if (!selectable) return;
                          if (needsFront) {
                            setPrepDraft({ ...prepDraft, front: slot });
                          } else {
                            finishOrderChangeAction(prepDraft, slot);
                          }
                        }}
                      />
                    );
                  })}
                  <Text size="2" weight="medium" className="battle-order-change-hint">
                    {prepDraft.front == null ? "选择前排" : "选择后排"}
                  </Text>
                </>
              )}
            </div>
          )}
        </div>
      </section>

      <GrandCardStrategyPanel
        strategy={grandCardStrategy}
        onChange={onGrandCardStrategyChange}
      />

      <Dialog.Root
        open={editingCardSlot != null && editingCard != null}
        onOpenChange={(open) => {
          if (!open) setEditingCardSlot(null);
        }}
      >
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>设置指令卡</Dialog.Title>
          {editingCard && (
            <div className="advanced-card-editor">
              <label>
                从者
                <select
                  value={editingCard.servant}
                  onChange={(event) =>
                    updateCommandCard(editingCard.slot, (prev) => ({
                      ...prev,
                      servant: event.target.value as AdvancedCommandCardCondition["servant"],
                      minCritChance: null,
                    }))
                  }
                >
                  <option value="any">任意</option>
                  <option value="servant_1">{servantLabel(0, partyLineup[0] ?? null)}</option>
                  <option value="servant_2">{servantLabel(1, partyLineup[1] ?? null)}</option>
                  <option value="servant_3">{servantLabel(2, partyLineup[2] ?? null)}</option>
                </select>
              </label>
              <label>
                色卡
                <select
                  value={editingCard.suit}
                  onChange={(event) =>
                    updateCommandCard(editingCard.slot, (prev) => ({
                      ...prev,
                      suit: event.target.value as AdvancedCommandCardCondition["suit"],
                      minCritChance: null,
                    }))
                  }
                >
                  <option value="any">任意</option>
                  <option value="buster">红卡</option>
                  <option value="arts">蓝卡</option>
                  <option value="quick">绿卡</option>
                </select>
              </label>
            </div>
          )}
          <Flex justify="end" mt="4">
            <Dialog.Close>
              <Button type="button">完成</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </div>
  );
}

export function AdvancedCommandEditor({
  projectId,
  partyLineup,
  grandServants = [],
  grandCardStrategy,
  onGrandServantsChange,
  onGrandCardStrategyChange,
}: AdvancedCommandEditorProps) {
  // Coronation mode is single-scene by design — the runner only ever
  // executes one battle. We still persist as an array on disk so the
  // backend schema (`save_advanced_battle_scenes`) stays compatible
  // with existing project files; older multi-scene drafts collapse to
  // the first scene.
  const [scene, setScene] = useState<AdvancedBattleScene>(() => createDefaultScene());
  const [loaded, setLoaded] = useState(() => !projectId);
  const faces = useServantFaces(partyLineup);

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    invoke<AdvancedBattleScene[]>("load_advanced_battle_scenes", { projectId })
      .then((saved) => {
        if (cancelled) return;
        const first = saved.length > 0 ? normalizeScene(saved[0]) : createDefaultScene();
        setScene(first);
      })
      .catch(() => {
        if (cancelled) return;
        setScene(createDefaultScene());
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const handleSceneChange = useCallback(
    (updatedScene: AdvancedBattleScene) => {
      setScene(updatedScene);
      if (!projectId) return;
      invoke("save_advanced_battle_scenes", {
        projectId,
        scenes: [updatedScene],
      }).catch(console.error);
    },
    [projectId]
  );

  if (!loaded) return null;

  return (
    <Flex direction="column" className="command-editor advanced-command-editor">
      <div className="command-scroll-region">
        <AdvancedStrategyEditor
          scene={scene}
          partyLineup={partyLineup}
          faces={faces}
          grandServants={grandServants}
          grandCardStrategy={grandCardStrategy}
          onGrandServantsChange={onGrandServantsChange}
          onGrandCardStrategyChange={onGrandCardStrategyChange}
          onChange={handleSceneChange}
        />
      </div>
    </Flex>
  );
}
