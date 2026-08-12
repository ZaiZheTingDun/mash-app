import { useState, useMemo, useRef, useCallback } from "react";
import {
  Checkbox,
  Dialog,
  Flex,
  Select,
  Text,
  TextField,
} from "@radix-ui/themes";
import { MagnifyingGlassIcon } from "@radix-ui/react-icons";
import type {
  CraftEssence,
  CraftEssenceCategory,
} from "../../types/craftEssence";
import type { SupportGrandBondCeMode } from "../../types/project";
import { useCeCards } from "./contentGridAssets";

interface CraftEssenceSelectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (ce: CraftEssence) => void;
  craftEssences: CraftEssence[];
  mlbRequired?: boolean;
  onMlbRequiredChange?: (required: boolean) => void;
  grandBondCeMode?: SupportGrandBondCeMode;
  onGrandBondCeModeChange?: (mode: SupportGrandBondCeMode) => void;
}

const ROW_HEIGHT = 40;
const OVERSCAN = 4;
const VIEWPORT_H = 420;
const RARITY_FILTER_OPTIONS = [null, 5, 4, 3, 2, 1] as const;

const CATEGORY_FILTER_OPTIONS: {
  value: CraftEssenceCategory | null;
  label: string;
}[] = [
  { value: null, label: "全部类型" },
  { value: "normal", label: "常规" },
  { value: "bond", label: "牵绊礼装" },
  { value: "manaExchange", label: "魔力棱镜/进阶关卡礼装" },
  { value: "event", label: "活动礼装" },
  { value: "eventReward", label: "活动报酬礼装" },
];

function craftEssenceSearchText(ce: CraftEssence) {
  return [ce.name, ...(ce.nameAliases ?? []), ce.nameLink ?? ""]
    .join(" ")
    .toLowerCase();
}

/**
 * Picker for craft essences. Modeled after `ServantSelectDialog` but
 * simpler: Rust normalizes the bundled `craft_essences.json` to
 * collectionNo-as-id + Chinese name (+ optional wiki link), so we don't
 * need a `disabledIds` prop — duplicate CEs across slots are valid (e.g.
 * a party can run several copies of the same MLB CE).
 *
 * The CE catalog has ~2600 entries, so we render the list with a tiny
 * fixed-row-height windowed renderer instead of mounting every option.
 * Only the rows visible in the 420px viewport (+ overscan) are mounted,
 * keeping open/scroll snappy.
 */
export function CraftEssenceSelectDialog({
  open,
  onOpenChange,
  onSelect,
  craftEssences,
  mlbRequired,
  onMlbRequiredChange,
  grandBondCeMode,
  onGrandBondCeModeChange,
}: CraftEssenceSelectDialogProps) {
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] =
    useState<CraftEssenceCategory | null>(null);
  const [rarityFilter, setRarityFilter] = useState<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const categoryAndRarityFiltered = craftEssences.filter((ce) => {
      if (categoryFilter !== null && ce.category !== categoryFilter) return false;
      if (rarityFilter !== null && ce.rarity !== rarityFilter) return false;
      return true;
    });
    if (!search.trim()) return categoryAndRarityFiltered;
    const q = search.toLowerCase().trim();
    return categoryAndRarityFiltered.filter((ce) =>
      craftEssenceSearchText(ce).includes(q)
    );
  }, [craftEssences, search, categoryFilter, rarityFilter]);

  // Clamp at read-time rather than via a setState-in-effect (which the
  // lint rule rejects). The stored `activeIndex` may briefly exceed the
  // current `filtered.length` when the search shrinks the list; we just
  // never render past the end.
  const safeActiveIndex =
    filtered.length === 0 ? 0 : Math.min(activeIndex, filtered.length - 1);

  const startIndex = Math.max(
    0,
    Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN
  );
  const endIndex = Math.min(
    filtered.length,
    Math.ceil((scrollTop + VIEWPORT_H) / ROW_HEIGHT) + OVERSCAN
  );
  const visible = filtered.slice(startIndex, endIndex);
  const ceCardSrcById = useCeCards(visible.map((ce) => ce.id));

  const resetScroll = useCallback(() => {
    setScrollTop(0);
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, []);

  const handleSelect = useCallback(
    (ce: CraftEssence) => {
      onSelect(ce);
      onOpenChange(false);
      setSearch("");
      setCategoryFilter(null);
      setRarityFilter(null);
      setActiveIndex(0);
      resetScroll();
    },
    [onSelect, onOpenChange, resetScroll]
  );

  const handleOpenChange = useCallback(
    (nextOpen: boolean) => {
      onOpenChange(nextOpen);
      if (!nextOpen) {
        setSearch("");
        setCategoryFilter(null);
        setRarityFilter(null);
        setActiveIndex(0);
        resetScroll();
      }
    },
    [onOpenChange, resetScroll]
  );

  const ensureVisible = useCallback((index: number) => {
    const el = scrollRef.current;
    if (!el) return;
    const top = index * ROW_HEIGHT;
    const bottom = top + ROW_HEIGHT;
    if (top < el.scrollTop) {
      el.scrollTop = top;
    } else if (bottom > el.scrollTop + el.clientHeight) {
      el.scrollTop = bottom - el.clientHeight;
    }
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (filtered.length === 0) return;

      switch (e.key) {
        case "ArrowDown": {
          e.preventDefault();
          const next = Math.min(safeActiveIndex + 1, filtered.length - 1);
          setActiveIndex(next);
          ensureVisible(next);
          break;
        }
        case "ArrowUp": {
          e.preventDefault();
          const prev = Math.max(safeActiveIndex - 1, 0);
          setActiveIndex(prev);
          ensureVisible(prev);
          break;
        }
        case "Enter": {
          e.preventDefault();
          if (filtered[safeActiveIndex]) {
            handleSelect(filtered[safeActiveIndex]);
          }
          break;
        }
      }
    },
    [filtered, safeActiveIndex, handleSelect, ensureVisible]
  );

  return (
    <Dialog.Root open={open} onOpenChange={handleOpenChange}>
      <Dialog.Content maxWidth="560px" className="servant-dialog">
        <Dialog.Title size="4">选择礼装</Dialog.Title>

        <TextField.Root
          placeholder="搜索礼装名称..."
          size="2"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setActiveIndex(0);
            resetScroll();
          }}
          onKeyDown={handleKeyDown}
          className="servant-search"
        >
          <TextField.Slot>
            <MagnifyingGlassIcon />
          </TextField.Slot>
        </TextField.Root>

        <Flex gap="2" className="ce-filter-row">
          <div
            className="ce-category-filter-grid"
            role="group"
            aria-label="礼装类型筛选"
          >
            {CATEGORY_FILTER_OPTIONS.map((option) => {
              const selected = categoryFilter === option.value;
              return (
                <button
                  key={option.value ?? "all"}
                  type="button"
                  className={`ce-filter-button${selected ? " selected" : ""}`}
                  aria-label={option.label}
                  aria-pressed={selected}
                  onClick={() => {
                    setCategoryFilter(option.value);
                    setActiveIndex(0);
                    resetScroll();
                  }}
                >
                  {option.label}
                </button>
              );
            })}
          </div>
          <div
            className="ce-rarity-filter-grid"
            role="group"
            aria-label="礼装星级筛选"
          >
            {RARITY_FILTER_OPTIONS.map((rarity) => {
              const selected = rarityFilter === rarity;
              const label = rarity === null ? "全部星级" : `★${rarity}`;
              return (
                <button
                  key={rarity ?? "all"}
                  type="button"
                  className={`ce-filter-button${selected ? " selected" : ""}`}
                  aria-label={label}
                  aria-pressed={selected}
                  onClick={() => {
                    setRarityFilter(rarity);
                    setActiveIndex(0);
                    resetScroll();
                  }}
                >
                  {rarity === null ? "全部" : `★${rarity}`}
                </button>
              );
            })}
          </div>
        </Flex>

        {onMlbRequiredChange && (
          <Flex gap="3" align="center" wrap="wrap" mt="3">
            <label className="ce-select-option">
              <Checkbox
                checked={mlbRequired ?? true}
                onCheckedChange={(checked) => onMlbRequiredChange(checked === true)}
              />
              <Text size="2">满破礼装</Text>
            </label>
            {onGrandBondCeModeChange && (
              <Flex gap="2" align="center">
                <Text size="2" color="gray">
                  牵绊形态
                </Text>
                <Select.Root
                  value={grandBondCeMode ?? "any"}
                  onValueChange={(value) =>
                    onGrandBondCeModeChange(value as SupportGrandBondCeMode)
                  }
                >
                  <Select.Trigger aria-label="冠位牵绊礼装形态" />
                  <Select.Content>
                    <Select.Item value="any">任意</Select.Item>
                    <Select.Item value="bond">原始牵绊</Select.Item>
                    <Select.Item value="bondNp">冠位连接牵绊</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Flex>
            )}
          </Flex>
        )}

        <div
          className="ce-list-scroll"
          ref={scrollRef}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        >
          {filtered.length === 0 ? (
            <Flex align="center" justify="center" py="6">
              <Text size="2" color="gray">
                未找到匹配的礼装
              </Text>
            </Flex>
          ) : (
            <div
              style={{
                height: filtered.length * ROW_HEIGHT,
                position: "relative",
              }}
              role="listbox"
              aria-label="礼装列表"
            >
              {visible.map((ce, i) => {
                const index = startIndex + i;
                return (
                  <button
                    key={ce.id}
                    role="option"
                    aria-selected={index === safeActiveIndex}
                    className={`servant-option ${index === safeActiveIndex ? "focused" : ""}`}
                    style={{
                      position: "absolute",
                      top: index * ROW_HEIGHT,
                      left: 0,
                      right: 0,
                      height: ROW_HEIGHT,
                    }}
                    onClick={() => handleSelect(ce)}
                    onMouseEnter={() => setActiveIndex(index)}
                  >
                    <div className="ce-option-content">
                      <Flex align="center" gap="3" className="ce-option-text">
                      <Text size="1" color="gray" style={{ minWidth: "3em" }}>
                        #{ce.id}
                      </Text>
                      <Text size="2" weight="medium">
                        {ce.name}
                      </Text>
                      </Flex>
                      {ceCardSrcById[ce.id] && (
                        <img
                          className="ce-card-thumbnail"
                          src={ceCardSrcById[ce.id] ?? undefined}
                          alt=""
                        />
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
}
