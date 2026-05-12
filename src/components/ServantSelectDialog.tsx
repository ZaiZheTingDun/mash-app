import { useState, useMemo, useRef, useCallback, useEffect } from "react";
import {
  Dialog,
  Flex,
  Text,
  TextField,
  Box,
  Select,
} from "@radix-ui/themes";
import { invoke, convertFileSrc } from "../tauri";
import { MagnifyingGlassIcon } from "@radix-ui/react-icons";
import type { Servant } from "../types/servant";

interface ServantSelectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (servant: Servant) => void;
  servants: Servant[];
  /**
   * Servant ids to hide from the picker. Used by the team-builder to
   * prevent the same servant from being picked into two non-support
   * slots at once. Defaults to nothing-disabled, so callers (like the
   * support slot) that want to allow duplicates can simply omit it.
   */
  disabledIds?: number[];
}

const CLASS_COLORS: Record<string, string> = {
  Saber: "#edc637",
  Archer: "#6dc88e",
  Lancer: "#3d9be0",
  Rider: "#d46cce",
  Caster: "#5db3d1",
  Assassin: "#8b8b8b",
  Berserker: "#d34545",
  Ruler: "#d4a537",
  Avenger: "#6a4bad",
  "Moon Cancer": "#60c5d8",
  Foreigner: "#b0a0d0",
  Pretender: "#a0d070",
  Beast: "#c04040",
  Shielder: "#a0a0a0",
  Alterego: "#a050a0",
};

const ALL_CLASSES_VALUE = "__all_classes__";
const ALL_RARITIES_VALUE = "__all_rarities__";
const ROW_HEIGHT = 72;
const OVERSCAN = 4;
const VIEWPORT_H = 420;

function getClassColor(cls: string): string {
  if (CLASS_COLORS[cls]) return CLASS_COLORS[cls];
  for (const [key, color] of Object.entries(CLASS_COLORS)) {
    if (cls.includes(key)) return color;
  }
  return "var(--gray-9)";
}

function displayCnName(servant: Servant): string {
  return servant.name_cn_server?.trim() || servant.name_cn;
}

export function ServantSelectDialog({
  open,
  onOpenChange,
  onSelect,
  servants,
  disabledIds,
}: ServantSelectDialogProps) {
  const [search, setSearch] = useState("");
  const [classFilter, setClassFilter] = useState("");
  const [rarityFilter, setRarityFilter] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const [faceSrcByKey, setFaceSrcByKey] = useState<Record<string, string | null>>({});
  const scrollRef = useRef<HTMLDivElement>(null);

  const classOptions = useMemo(
    () => Array.from(new Set(servants.map((s) => s.class))).sort(),
    [servants]
  );
  const rarityOptions = useMemo(
    () => Array.from(new Set(servants.map((s) => s.rarity))).sort((a, b) => b - a),
    [servants]
  );

  // Hide already-picked servants entirely. Filtering (vs disabling) keeps
  // keyboard navigation simple — every visible item is selectable, so we
  // never have to skip-over disabled rows on ArrowUp/Down.
  const filtered = useMemo(() => {
    const blocked = new Set(disabledIds ?? []);
    const pool = blocked.size
      ? servants.filter((s) => !blocked.has(s.id))
      : servants;
    const filteredPool = pool.filter((s) => {
      if (classFilter && s.class !== classFilter) return false;
      if (rarityFilter && s.rarity !== Number(rarityFilter)) return false;
      return true;
    });
    if (!search.trim()) return filteredPool;
    const q = search.toLowerCase().trim();
    return filteredPool.filter(
      (s) =>
        displayCnName(s).toLowerCase().includes(q) ||
        s.name_cn.toLowerCase().includes(q) ||
        (s.name_cn_server ?? "").toLowerCase().includes(q) ||
        s.name_en.toLowerCase().includes(q) ||
        s.name_jp.includes(q) ||
        (s.name_other ?? "").toLowerCase().includes(q) ||
        (s.noblePhantasmName ?? "").toLowerCase().includes(q)
    );
  }, [servants, search, classFilter, rarityFilter, disabledIds]);

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
  const visible = useMemo(
    () => filtered.slice(startIndex, endIndex),
    [filtered, startIndex, endIndex]
  );

  const faceEntries = useMemo(
    () =>
      visible.map((s) => ({
        variantKey: s.variantKey,
        id: s.id,
        faceId: s.faceId ?? null,
      })),
    [visible]
  );

  useEffect(() => {
    const missing = faceEntries.filter((entry) => !(entry.variantKey in faceSrcByKey));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((entry) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: entry.id,
          faceId: entry.faceId,
        })
          .then((path) => [entry.variantKey, path ? convertFileSrc(path) : null] as const)
          .catch(() => [entry.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaceSrcByKey((prev) => {
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
    // `faceSrcByKey` is intentionally excluded; this effect should fetch
    // only when the visible id set changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [faceEntries]);

  const resetScroll = useCallback(() => {
    setScrollTop(0);
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, []);

  const handleSelect = useCallback(
    (servant: Servant) => {
      onSelect(servant);
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
        setClassFilter("");
        setRarityFilter("");
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
        <Dialog.Title size="4">选择从者</Dialog.Title>

        <TextField.Root
          placeholder="搜索从者名称..."
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

        <Flex gap="2" className="servant-filter-row">
          <Select.Root
            value={classFilter || ALL_CLASSES_VALUE}
            onValueChange={(value) => {
              setClassFilter(value === ALL_CLASSES_VALUE ? "" : value);
              setActiveIndex(0);
              resetScroll();
            }}
          >
            <Select.Trigger
              className="servant-filter-trigger"
              aria-label="职介筛选"
            />
            <Select.Content>
              <Select.Item value={ALL_CLASSES_VALUE}>全部职介</Select.Item>
              {classOptions.map((cls) => (
                <Select.Item key={cls} value={cls}>
                  {cls}
                </Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
          <Select.Root
            value={rarityFilter || ALL_RARITIES_VALUE}
            onValueChange={(value) => {
              setRarityFilter(value === ALL_RARITIES_VALUE ? "" : value);
              setActiveIndex(0);
              resetScroll();
            }}
          >
            <Select.Trigger
              className="servant-filter-trigger"
              aria-label="稀有度筛选"
            />
            <Select.Content>
              <Select.Item value={ALL_RARITIES_VALUE}>全部稀有度</Select.Item>
              {rarityOptions.map((rarity) => (
                <Select.Item key={rarity} value={String(rarity)}>
                  ★{rarity}
                </Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
        </Flex>

        <div
          className="servant-list-scroll"
          ref={scrollRef}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        >
          {filtered.length === 0 ? (
            <Flex align="center" justify="center" py="6">
              <Text size="2" color="gray">
                未找到匹配的从者
              </Text>
            </Flex>
          ) : (
            <div
              style={{
                height: filtered.length * ROW_HEIGHT,
                position: "relative",
              }}
              role="listbox"
              aria-label="从者列表"
            >
              {visible.map((servant, i) => {
                const index = startIndex + i;
                return (
                  <button
                    key={servant.variantKey}
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
                    onClick={() => handleSelect(servant)}
                    onMouseEnter={() => setActiveIndex(index)}
                  >
                    <div className="servant-option-content">
                      <div className="servant-face-frame">
                        {faceSrcByKey[servant.variantKey] ? (
                          <img src={faceSrcByKey[servant.variantKey] ?? ""} alt="" />
                        ) : (
                          <span>{servant.class.slice(0, 2)}</span>
                        )}
                      </div>
                      <Flex direction="column" align="start" gap="1" className="servant-option-text">
                        <Flex align="center" gap="2" wrap="wrap">
                          <Text size="2" weight="medium">
                            {displayCnName(servant)}
                          </Text>
                          <Box
                            className="servant-class-badge"
                            style={{ background: getClassColor(servant.class) }}
                          >
                            <Text size="1" weight="bold" style={{ color: "#fff" }}>
                              {servant.class.split(" ")[0]}
                            </Text>
                          </Box>
                          <Text size="1" className="servant-rarity">
                            {"★".repeat(servant.rarity)}
                          </Text>
                        </Flex>
                        <Text size="1" className="servant-np-name">
                          {servant.noblePhantasmName ?? "宝具未记录"}
                        </Text>
                      </Flex>
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
