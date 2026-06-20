import { useState } from "react";
import { Text } from "@radix-ui/themes";
import { Cross2Icon, PlusIcon } from "@radix-ui/react-icons";
import type React from "react";
import type { CraftEssence } from "../types/craftEssence";
import type {
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceMlbRequired,
} from "../types/project";

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

/**
 * Plate pinned to the bottom of the servant portrait, sized to the
 * natural CE card aspect (`--ce-card-aspect` in CSS).
 */
export function CraftEssenceOverlay({
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

export function GrandCraftEssenceOverlay({
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
