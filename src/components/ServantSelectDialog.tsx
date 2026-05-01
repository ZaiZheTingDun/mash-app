import { useState, useMemo, useRef, useCallback, useEffect } from "react";
import {
  Dialog,
  Flex,
  Text,
  TextField,
  ScrollArea,
  Box,
} from "@radix-ui/themes";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
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

function getClassColor(cls: string): string {
  if (CLASS_COLORS[cls]) return CLASS_COLORS[cls];
  for (const [key, color] of Object.entries(CLASS_COLORS)) {
    if (cls.includes(key)) return color;
  }
  return "var(--gray-9)";
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
  const [faceSrcByKey, setFaceSrcByKey] = useState<Record<string, string | null>>({});
  const listRef = useRef<HTMLDivElement>(null);

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
        s.name_cn.toLowerCase().includes(q) ||
        s.name_en.toLowerCase().includes(q) ||
        s.name_jp.includes(q) ||
        (s.name_other ?? "").toLowerCase().includes(q) ||
        (s.noblePhantasmName ?? "").toLowerCase().includes(q)
    );
  }, [servants, search, classFilter, rarityFilter, disabledIds]);

  const faceEntries = useMemo(
    () =>
      filtered.map((s) => ({
        variantKey: s.variantKey,
        id: s.id,
        faceId: s.faceId ?? null,
      })),
    [filtered]
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

  // Keep the keyboard-highlighted row valid when the visible list shrinks
  // (e.g. opening the dialog from a different slot tightens `disabledIds`).
  useEffect(() => {
    setActiveIndex((prev) => {
      if (filtered.length === 0) return 0;
      return Math.min(prev, filtered.length - 1);
    });
  }, [filtered.length]);

  const handleSelect = useCallback(
    (servant: Servant) => {
      onSelect(servant);
      onOpenChange(false);
      setSearch("");
      setActiveIndex(0);
    },
    [onSelect, onOpenChange]
  );

  const handleOpenChange = useCallback(
    (nextOpen: boolean) => {
      onOpenChange(nextOpen);
      if (!nextOpen) {
        setSearch("");
        setClassFilter("");
        setRarityFilter("");
        setActiveIndex(0);
      }
    },
    [onOpenChange]
  );

  const scrollActiveIntoView = useCallback(
    (index: number) => {
      const list = listRef.current;
      if (!list) return;
      const items = list.querySelectorAll<HTMLElement>('[role="option"]');
      items[index]?.scrollIntoView({ block: "nearest" });
    },
    []
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (filtered.length === 0) return;

      switch (e.key) {
        case "ArrowDown": {
          e.preventDefault();
          const next = Math.min(activeIndex + 1, filtered.length - 1);
          setActiveIndex(next);
          scrollActiveIntoView(next);
          break;
        }
        case "ArrowUp": {
          e.preventDefault();
          const prev = Math.max(activeIndex - 1, 0);
          setActiveIndex(prev);
          scrollActiveIntoView(prev);
          break;
        }
        case "Enter": {
          e.preventDefault();
          if (filtered[activeIndex]) {
            handleSelect(filtered[activeIndex]);
          }
          break;
        }
      }
    },
    [filtered, activeIndex, handleSelect, scrollActiveIntoView]
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
          }}
          onKeyDown={handleKeyDown}
          className="servant-search"
        >
          <TextField.Slot>
            <MagnifyingGlassIcon />
          </TextField.Slot>
        </TextField.Root>

        <Flex gap="2" className="servant-filter-row">
          <select
            className="servant-filter-select"
            value={classFilter}
            onChange={(e) => {
              setClassFilter(e.target.value);
              setActiveIndex(0);
            }}
          >
            <option value="">全部职介</option>
            {classOptions.map((cls) => (
              <option key={cls} value={cls}>
                {cls}
              </option>
            ))}
          </select>
          <select
            className="servant-filter-select"
            value={rarityFilter}
            onChange={(e) => {
              setRarityFilter(e.target.value);
              setActiveIndex(0);
            }}
          >
            <option value="">全部稀有度</option>
            {rarityOptions.map((rarity) => (
              <option key={rarity} value={rarity}>
                ★{rarity}
              </option>
            ))}
          </select>
        </Flex>

        <ScrollArea className="servant-list-scroll">
          <Flex
            ref={listRef}
            direction="column"
            gap="1"
            p="1"
            role="listbox"
            aria-label="从者列表"
          >
            {filtered.length === 0 ? (
              <Flex align="center" justify="center" py="6">
                <Text size="2" color="gray">
                  未找到匹配的从者
                </Text>
              </Flex>
            ) : (
              filtered.map((servant, index) => (
                <button
                  key={servant.variantKey}
                  role="option"
                  aria-selected={index === activeIndex}
                  className={`servant-option ${index === activeIndex ? "focused" : ""}`}
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
                          {servant.name_cn}
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
              ))
            )}
          </Flex>
        </ScrollArea>
      </Dialog.Content>
    </Dialog.Root>
  );
}
