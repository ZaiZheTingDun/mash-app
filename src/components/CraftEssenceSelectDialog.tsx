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
import type { CraftEssence } from "../types/craftEssence";
import type { SupportGrandBondCeMode } from "../types/project";

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
const NARROW_HINT_THRESHOLD = 200;

function craftEssenceSearchText(ce: CraftEssence) {
  return [ce.name, ...(ce.nameAliases ?? []), ce.nameLink ?? ""]
    .join(" ")
    .toLowerCase();
}

/**
 * Picker for craft essences. Modeled after `ServantSelectDialog` but
 * simpler: Rust normalizes the bundled `craft_essences.json` to
 * collectionNo-as-id + Chinese name (+ optional wiki link), so the row
 * is text-only and we don't need a `disabledIds` prop — duplicate CEs
 * across slots are valid (e.g. a party can run several copies of the
 * same MLB CE).
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
  const [activeIndex, setActiveIndex] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    if (!search.trim()) return craftEssences;
    const q = search.toLowerCase().trim();
    return craftEssences.filter((ce) => craftEssenceSearchText(ce).includes(q));
  }, [craftEssences, search]);

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

  const showNarrowHint = filtered.length > NARROW_HINT_THRESHOLD;

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

        {showNarrowHint && (
          <Text size="1" color="gray" mt="2">
            共 {filtered.length} 项，输入名称以精确查找
          </Text>
        )}

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
                    <Flex align="center" gap="3">
                      <Text size="1" color="gray" style={{ minWidth: "3em" }}>
                        #{ce.id}
                      </Text>
                      <Text size="2" weight="medium">
                        {ce.name}
                      </Text>
                    </Flex>
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
