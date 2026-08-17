import { Text } from "@radix-ui/themes";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  horizontalListSortingStrategy,
  sortableKeyboardCoordinates,
  useSortable,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { servantLabel } from "../../components/common/battleActorLabels";
import type {
  AttackMemberPriorityItem,
  CriticalAttackStrategy,
  CriticalChainType,
} from "../../types/command";
import type { PartyMember } from "../team/partyServants";

const DEFAULT_CHAIN_PRIORITY: CriticalChainType[] = [
  "mighty",
  "buster",
  "arts",
  "quick",
];

const CHAIN_LABELS: Record<CriticalChainType, string> = {
  mighty: "精湛连携",
  buster: "力击连携",
  arts: "技击连携",
  quick: "迅击连携",
};

interface ConfiguredMember {
  key: string;
  index: number;
  member: PartyMember;
  priority: AttackMemberPriorityItem;
}

function normalizedChains(configured: CriticalChainType[] | undefined): CriticalChainType[] {
  const result = (configured ?? []).filter(
    (chain, index, values) =>
      DEFAULT_CHAIN_PRIORITY.includes(chain) && values.indexOf(chain) === index
  );
  for (const chain of DEFAULT_CHAIN_PRIORITY) {
    if (!result.includes(chain)) result.push(chain);
  }
  return result;
}

function memberMatches(
  configured: AttackMemberPriorityItem,
  member: ConfiguredMember
): boolean {
  if (configured.memberId && member.member.memberId) {
    return configured.memberId === member.member.memberId;
  }
  return (
    configured.slotIndex === member.index &&
    configured.servantId === member.member.servant?.id &&
    configured.isSupport === member.member.isSupport
  );
}

function normalizedMembers(
  partyMembers: PartyMember[],
  configured: AttackMemberPriorityItem[] | undefined
): ConfiguredMember[] {
  const available = partyMembers.flatMap((member, index) => {
    if (!member.servant) return [];
    return [{
      key: `member:${member.memberId ?? index}`,
      index,
      member,
      priority: {
        memberId: member.memberId ?? null,
        slotIndex: index,
        servantId: member.servant.id,
        isSupport: member.isSupport,
      },
    }];
  });
  const result: ConfiguredMember[] = [];
  for (const item of configured ?? []) {
    const match = available.find(
      (member) => !result.includes(member) && memberMatches(item, member)
    );
    if (match) result.push(match);
  }
  for (const member of available) {
    if (!result.includes(member)) result.push(member);
  }
  return result;
}

function SortableMember({
  item,
  face,
}: {
  item: ConfiguredMember;
  face: string | null | undefined;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: item.key });
  const label = servantLabel(item.index, item.member.servant);
  return (
    <button
      ref={setNodeRef}
      type="button"
      className={`critical-priority-member${isDragging ? " dragging" : ""}`}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      aria-label={`${label}，拖动调整优先级`}
      {...attributes}
      {...listeners}
    >
      <BattleActorIcon
        kind="servant"
        src={face}
        label={label}
        isSupport={item.member.isSupport}
        size="button"
      />
    </button>
  );
}

function SortableChain({ chain }: { chain: CriticalChainType }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: `chain:${chain}` });
  return (
    <button
      ref={setNodeRef}
      type="button"
      className={`critical-priority-chain ${chain}${isDragging ? " dragging" : ""}`}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      aria-label={`${CHAIN_LABELS[chain]}，拖动调整优先级`}
      {...attributes}
      {...listeners}
    >
      {CHAIN_LABELS[chain]}
    </button>
  );
}

export function CriticalStrategyEditor({
  strategy,
  partyMembers,
  faces,
  onChange,
}: {
  strategy?: CriticalAttackStrategy;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onChange: (strategy: CriticalAttackStrategy) => void;
}) {
  const members = normalizedMembers(partyMembers, strategy?.memberPriority);
  const chains = normalizedChains(strategy?.chainPriority);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  );

  const handleMemberDrag = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const oldIndex = members.findIndex((item) => item.key === active.id);
    const newIndex = members.findIndex((item) => item.key === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    onChange({
      memberPriority: arrayMove(members, oldIndex, newIndex).map((item) => item.priority),
      chainPriority: chains,
    });
  };

  const handleChainDrag = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const activeChain = String(active.id).replace("chain:", "") as CriticalChainType;
    const overChain = String(over.id).replace("chain:", "") as CriticalChainType;
    const oldIndex = chains.indexOf(activeChain);
    const newIndex = chains.indexOf(overChain);
    if (oldIndex < 0 || newIndex < 0) return;
    onChange({
      memberPriority: members.map((item) => item.priority),
      chainPriority: arrayMove(chains, oldIndex, newIndex),
    });
  };

  return (
    <div className="critical-strategy-editor">
      <div className="critical-priority-row">
        <Text size="2" weight="medium" className="critical-priority-label">
          从者优先级
        </Text>
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleMemberDrag}>
          <SortableContext items={members.map((item) => item.key)} strategy={horizontalListSortingStrategy}>
            <div className="critical-priority-items">
              {members.map((item) => (
                <SortableMember
                  key={item.key}
                  item={item}
                  face={item.member.servant ? faces[item.member.servant.variantKey] : null}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      </div>
      <div className="critical-priority-row">
        <Text size="2" weight="medium" className="critical-priority-label">
          连携优先级
        </Text>
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleChainDrag}>
          <SortableContext items={chains.map((chain) => `chain:${chain}`)} strategy={horizontalListSortingStrategy}>
            <div className="critical-priority-items">
              {chains.map((chain) => <SortableChain key={chain} chain={chain} />)}
            </div>
          </SortableContext>
        </DndContext>
      </div>
    </div>
  );
}
