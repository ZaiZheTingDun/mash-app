import { useState } from "react";
import { Text } from "@radix-ui/themes";
import { Cross2Icon, PlusIcon } from "@radix-ui/react-icons";
import type React from "react";
import type { CraftEssence } from "../../types/craftEssence";
import type {
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceMlbRequired,
} from "../../types/project";

interface CraftEssenceOverlayProps {
  craftEssence: CraftEssence | null;
  cardSrc: string | null | undefined;
  craftEssences: CraftEssence[];
  cardSrcs: (string | null | undefined)[];
  mlbRequired: boolean;
  mlbIconSrc: string | null | undefined;
  onSelect: () => void;
  onAdd?: () => void;
  onManage?: () => void;
  onClear: () => void;
}

interface GrandCraftEssenceOverlayProps {
  craftEssenceGroups: CraftEssence[][];
  cardSrcGroups: (string | null | undefined)[][];
  mlbRequired: SupportGrandCraftEssenceMlbRequired;
  mlbIconSrc: string | null | undefined;
  grandBondCeMode: SupportGrandBondCeMode;
  bondIconSrc: string | null | undefined;
  bondNpIconSrc: string | null | undefined;
  onSelect: (index: number) => void;
  onAdd: (index: number) => void;
  onManage: (index: number) => void;
  onClear: (index: number) => void;
}

/**
 * Plate pinned to the bottom of the servant portrait, sized to the
 * natural CE card aspect (`--ce-card-aspect` in CSS).
 */
export function CraftEssenceOverlay({
  craftEssence,
  cardSrc,
  craftEssences,
  cardSrcs,
  mlbRequired,
  mlbIconSrc,
  onSelect,
  onAdd,
  onManage,
  onClear,
}: CraftEssenceOverlayProps) {
  const selected = craftEssences.length
    ? craftEssences
    : craftEssence
      ? [craftEssence]
      : [];
  const primary = selected[0] ?? null;
  const isStack = selected.length > 1;
  const visibleStackSize = Math.min(selected.length, 4);
  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (primary && onManage) {
      onManage();
    } else {
      onSelect();
    }
  };

  return (
    <div
      className={`ce-overlay${primary ? " filled" : " empty"}${isStack ? " stacked" : ""}`}
      onClick={handleClick}
      role="button"
      aria-label={
        primary
          ? isStack
            ? `礼装：${primary.name}等 ${selected.length} 张`
            : `礼装：${primary.name}`
          : "选择礼装"
      }
    >
      {isStack ? (
        <div className="ce-overlay-stack" aria-hidden>
          {selected.slice(0, 4).map((ce, index) => {
            const src = cardSrcs[index];
            return (
              <span
                className="ce-overlay-stack-card"
                key={ce.id}
                style={{
                  left: `${3 + (visibleStackSize - 1) * 10}px`,
                  right: "3px",
                  transform: `translateX(${-index * 10}px)`,
                  zIndex: 4 - index,
                }}
              >
                {src ? <img src={src} alt="" draggable={false} /> : ce.name}
              </span>
            );
          })}
          <span className="ce-overlay-count">{selected.length}</span>
        </div>
      ) : primary && cardSrc ? (
        <img
          className="ce-overlay-img"
          src={cardSrc}
          alt={primary.name}
          draggable={false}
        />
      ) : (
        <div className="ce-overlay-scrim">
          {primary ? (
            <Text
              size="1"
              weight="bold"
              align="center"
              className="ce-overlay-fallback-label"
              truncate
            >
              {primary.name}
            </Text>
          ) : (
            <PlusIcon width={20} height={20} className="ce-overlay-empty-icon" />
          )}
        </div>
      )}
      {!primary && (
        <>
          <span className="ce-overlay-corner tl" aria-hidden />
          <span className="ce-overlay-corner tr" aria-hidden />
          <span className="ce-overlay-corner bl" aria-hidden />
          <span className="ce-overlay-corner br" aria-hidden />
        </>
      )}
      {primary && (
        <>
          {onAdd && selected.length < 10 && (
            <button
              type="button"
              className="ce-overlay-add"
              aria-label="新增礼装"
              onClick={(e) => {
                e.stopPropagation();
                onAdd();
              }}
            >
              <PlusIcon width={11} height={11} />
            </button>
          )}
          <button
            type="button"
            className="ce-overlay-clear"
            aria-label={isStack ? "清除全部礼装" : "清除礼装"}
            onClick={(e) => {
              e.stopPropagation();
              onClear();
            }}
          >
            <Cross2Icon width={11} height={11} />
          </button>
        </>
      )}
      {primary && mlbRequired && (
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

export function GrandCraftEssenceOverlay({
  craftEssenceGroups,
  cardSrcGroups,
  mlbRequired,
  mlbIconSrc,
  grandBondCeMode,
  bondIconSrc,
  bondNpIconSrc,
  onSelect,
  onAdd,
  onManage,
  onClear,
}: GrandCraftEssenceOverlayProps) {
  const [failedCardSrcs, setFailedCardSrcs] = useState<Record<number, string>>({});
  const bondSrc = grandBondCeMode === "bond" ? bondIconSrc : bondNpIconSrc;
  const bondLabel = grandBondCeMode === "bond" ? "原始牵绊" : "冠位连接牵绊";
  return (
    <div className="grand-ce-overlay" aria-label="冠位礼装设置">
      {craftEssenceGroups.map((craftEssences, index) => {
        const craftEssence = craftEssences[0] ?? null;
        const cardSrc = cardSrcGroups[index]?.[0];
        const isStack = craftEssences.length > 1;
        const visibleStackSize = Math.min(craftEssences.length, 4);
        const showCardImage =
          craftEssence != null && cardSrc != null && failedCardSrcs[index] !== cardSrc;
        return (
          <div
            key={index}
            className={`grand-ce-slot${craftEssence ? " filled" : " empty"}${isStack ? " stacked" : ""}`}
            role="button"
            tabIndex={0}
            aria-label={
              craftEssence
                ? isStack
                  ? `冠位礼装 ${index + 1}：${craftEssence.name}等 ${craftEssences.length} 张`
                  : `冠位礼装 ${index + 1}：${craftEssence.name}`
                : `选择冠位礼装 ${index + 1}`
            }
            onClick={(event) => {
              event.stopPropagation();
              if (craftEssence && index !== 1) onManage(index);
              else onSelect(index);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                event.stopPropagation();
                if (craftEssence && index !== 1) onManage(index);
                else onSelect(index);
              }
            }}
          >
            {isStack ? (
              <div className="grand-ce-slot-stack" aria-hidden>
                {craftEssences.slice(0, 4).map((ce, stackIndex) => {
                  const src = cardSrcGroups[index]?.[stackIndex];
                  return (
                    <span
                      key={ce.id}
                      style={{
                        left: `${1 + (visibleStackSize - 1) * 5}px`,
                        right: "1px",
                        transform: `translateX(${-stackIndex * 5}px)`,
                        zIndex: 4 - stackIndex,
                      }}
                    >
                      {src ? <img src={src} alt="" /> : ce.name}
                    </span>
                  );
                })}
                <b>{craftEssences.length}</b>
              </div>
            ) : showCardImage ? (
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
              <>
                {index !== 1 && craftEssences.length < 10 && (
                  <button
                    type="button"
                    className="grand-ce-slot-add"
                    aria-label={`新增冠位礼装 ${index + 1}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      onAdd(index);
                    }}
                  >
                    <PlusIcon width={10} height={10} />
                  </button>
                )}
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
              </>
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
