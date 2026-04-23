import { Flex, Text } from "@radix-ui/themes";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  arrayMove,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  TrashIcon,
  DragHandleDots2Icon,
  PlusIcon,
  PersonIcon,
  HeartIcon,
  LightningBoltIcon,
} from "@radix-ui/react-icons";
import type {
  BattleScene,
  ServantAction,
  EquipmentAction,
  AttackCard,
} from "../types/command";
import type { Servant } from "../types/servant";

interface BattleSceneBlockProps {
  scene: BattleScene;
  index: number;
  partyServants: (Servant | null)[];
  onChange: (updated: BattleScene) => void;
  onDelete: () => void;
  canDelete: boolean;
}

const SKILLS = ["skill_1", "skill_2", "skill_3"];
const SKILL_LABELS: Record<string, string> = {
  skill_1: "Skill 1",
  skill_2: "Skill 2",
  skill_3: "Skill 3",
};

const CARD_TYPES = ["quick", "arts", "buster"] as const;
const CARD_LABELS: Record<string, string> = {
  quick: "Quick",
  arts: "Arts",
  buster: "Buster",
};

function getServantLabel(index: number, servant: Servant | null): string {
  return servant ? servant.name_cn : `Servant ${index + 1}`;
}

function getServantOptions(partyServants: (Servant | null)[]) {
  return partyServants.map((s, i) => ({
    value: `servant_${i + 1}`,
    label: getServantLabel(i, s),
  }));
}

function getAttackCardOptions(partyServants: (Servant | null)[]) {
  const options: { value: string; label: string }[] = [];
  for (let i = 0; i < 3; i++) {
    const name = getServantLabel(i, partyServants[i] ?? null);
    for (const cardType of CARD_TYPES) {
      options.push({
        value: `servant_${i + 1}_${cardType}`,
        label: `${name} - ${CARD_LABELS[cardType]}`,
      });
    }
  }
  for (let i = 0; i < 3; i++) {
    const name = getServantLabel(i, partyServants[i] ?? null);
    options.push({
      value: `servant_${i + 1}_np`,
      label: `${name} - NP`,
    });
  }
  return options;
}

function ServantActionRow({
  action,
  partyServants,
  onChange,
  onDelete,
}: {
  action: ServantAction;
  partyServants: (Servant | null)[];
  onChange: (a: ServantAction) => void;
  onDelete: () => void;
}) {
  const servantOpts = getServantOptions(partyServants);

  return (
    <Flex align="center" gap="2" className="action-row action-row-servant">
      <select
        className="action-select"
        value={action.servant ?? ""}
        onChange={(e) => onChange({ ...action, servant: e.target.value || null })}
      >
        <option value="">-- Servant --</option>
        {servantOpts.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <Text size="2" className="action-label">
        Use
      </Text>
      <select
        className="action-select"
        value={action.skill ?? ""}
        onChange={(e) => onChange({ ...action, skill: e.target.value || null })}
      >
        <option value="">-- Skill --</option>
        {SKILLS.map((s) => (
          <option key={s} value={s}>
            {SKILL_LABELS[s]}
          </option>
        ))}
      </select>
      <Text size="2" className="action-label">
        to
      </Text>
      <select
        className="action-select"
        value={action.target ?? ""}
        onChange={(e) => onChange({ ...action, target: e.target.value || null })}
      >
        <option value="">-- Target --</option>
        {servantOpts.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <button className="action-delete-btn" onClick={onDelete}>
        <TrashIcon />
      </button>
    </Flex>
  );
}

function EquipmentActionRow({
  action,
  partyServants,
  onChange,
  onDelete,
}: {
  action: EquipmentAction;
  partyServants: (Servant | null)[];
  onChange: (a: EquipmentAction) => void;
  onDelete: () => void;
}) {
  const servantOpts = getServantOptions(partyServants);

  return (
    <Flex align="center" gap="2" className="action-row action-row-equipment">
      <Text size="2" className="action-label">
        Master use
      </Text>
      <select
        className="action-select"
        value={action.skill ?? ""}
        onChange={(e) => onChange({ ...action, skill: e.target.value || null })}
      >
        <option value="">-- Skill --</option>
        {SKILLS.map((s) => (
          <option key={s} value={s}>
            {SKILL_LABELS[s]}
          </option>
        ))}
      </select>
      <Text size="2" className="action-label">
        to
      </Text>
      <select
        className="action-select"
        value={action.target ?? ""}
        onChange={(e) => onChange({ ...action, target: e.target.value || null })}
      >
        <option value="">-- None --</option>
        {servantOpts.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <button className="action-delete-btn" onClick={onDelete}>
        <TrashIcon />
      </button>
    </Flex>
  );
}

function SortableAttackRow({
  card,
  partyServants,
  onChange,
  onDelete,
  canDelete,
}: {
  card: AttackCard;
  partyServants: (Servant | null)[];
  onChange: (c: AttackCard) => void;
  onDelete: () => void;
  canDelete: boolean;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: card.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
    zIndex: isDragging ? 10 : undefined,
  };

  const cardOptions = getAttackCardOptions(partyServants);

  return (
    <Flex
      ref={setNodeRef}
      style={style}
      align="center"
      gap="2"
      className="attack-priority-row"
    >
      <span className="drag-handle" {...attributes} {...listeners}>
        <DragHandleDots2Icon />
      </span>
      <select
        className="action-select attack-select"
        value={card.card ?? ""}
        onChange={(e) => onChange({ ...card, card: e.target.value || null })}
      >
        <option value="">-- Card --</option>
        {cardOptions.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <button
        className="action-delete-btn"
        onClick={onDelete}
        disabled={!canDelete}
      >
        <TrashIcon />
      </button>
    </Flex>
  );
}

export function BattleSceneBlock({
  scene,
  index,
  partyServants,
  onChange,
  onDelete,
  canDelete,
}: BattleSceneBlockProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor)
  );

  const addServantAction = () => {
    const newAction: ServantAction = {
      type: "servant",
      id: `sa_${Date.now()}`,
      servant: null,
      skill: null,
      target: null,
    };
    onChange({
      ...scene,
      servantActions: [...scene.servantActions, newAction],
    });
  };

  const addEquipmentAction = () => {
    const newAction: EquipmentAction = {
      type: "equipment",
      id: `eq_${Date.now()}`,
      skill: null,
      target: null,
    };
    onChange({
      ...scene,
      equipmentActions: [...scene.equipmentActions, newAction],
    });
  };

  const updateServantAction = (idx: number, updated: ServantAction) => {
    const next = [...scene.servantActions];
    next[idx] = updated;
    onChange({ ...scene, servantActions: next });
  };

  const deleteServantAction = (idx: number) => {
    onChange({
      ...scene,
      servantActions: scene.servantActions.filter((_, i) => i !== idx),
    });
  };

  const updateEquipmentAction = (idx: number, updated: EquipmentAction) => {
    const next = [...scene.equipmentActions];
    next[idx] = updated;
    onChange({ ...scene, equipmentActions: next });
  };

  const deleteEquipmentAction = (idx: number) => {
    onChange({
      ...scene,
      equipmentActions: scene.equipmentActions.filter((_, i) => i !== idx),
    });
  };

  const updateAttackCard = (idx: number, updated: AttackCard) => {
    const next = [...scene.attackPriority];
    next[idx] = updated;
    onChange({ ...scene, attackPriority: next });
  };

  const deleteAttackCard = (idx: number) => {
    onChange({
      ...scene,
      attackPriority: scene.attackPriority.filter((_, i) => i !== idx),
    });
  };

  const addAttackCard = () => {
    onChange({
      ...scene,
      attackPriority: [
        ...scene.attackPriority,
        { id: `atk_${Date.now()}_${scene.attackPriority.length}`, card: null },
      ],
    });
  };

  const handleAttackDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = scene.attackPriority.findIndex((c) => c.id === active.id);
    const newIndex = scene.attackPriority.findIndex((c) => c.id === over.id);
    onChange({
      ...scene,
      attackPriority: arrayMove(scene.attackPriority, oldIndex, newIndex),
    });
  };

  return (
    <div className="scene-block">
      <Flex align="center" justify="between" className="scene-header">
        <Text size="4" weight="bold">
          场景 {index + 1}
        </Text>
        <Flex gap="2" align="center">
          <button className="scene-action-btn scene-action-btn-servant" onClick={addServantAction}>
            <PersonIcon />
            <span>Servant</span>
          </button>
          <button className="scene-action-btn scene-action-btn-equipment" onClick={addEquipmentAction}>
            <HeartIcon />
            <span>Equipment</span>
          </button>
          {canDelete && (
            <button className="scene-delete-btn" onClick={onDelete}>
              <TrashIcon />
            </button>
          )}
        </Flex>
      </Flex>

      <div className="scene-actions">
        {scene.servantActions.map((action, i) => (
          <ServantActionRow
            key={action.id}
            action={action}
            partyServants={partyServants}
            onChange={(a) => updateServantAction(i, a)}
            onDelete={() => deleteServantAction(i)}
          />
        ))}
        {scene.equipmentActions.map((action, i) => (
          <EquipmentActionRow
            key={action.id}
            action={action}
            partyServants={partyServants}
            onChange={(a) => updateEquipmentAction(i, a)}
            onDelete={() => deleteEquipmentAction(i)}
          />
        ))}
      </div>

      <div className="attack-priority-section">
        <Flex align="center" justify="between" className="attack-priority-header">
          <Flex align="center" gap="1">
            <LightningBoltIcon />
            <Text size="2" weight="bold">
              Attack Priority
            </Text>
          </Flex>
          <button className="add-priority-btn" onClick={addAttackCard}>
            <PlusIcon />
            <span>Add</span>
          </button>
        </Flex>
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={handleAttackDragEnd}
        >
          <SortableContext
            items={scene.attackPriority.map((c) => c.id)}
            strategy={verticalListSortingStrategy}
          >
            {scene.attackPriority.map((card, i) => (
              <SortableAttackRow
                key={card.id}
                card={card}
                partyServants={partyServants}
                onChange={(c) => updateAttackCard(i, c)}
                onDelete={() => deleteAttackCard(i)}
                canDelete={scene.attackPriority.length > 3}
              />
            ))}
          </SortableContext>
        </DndContext>
      </div>
    </div>
  );
}
