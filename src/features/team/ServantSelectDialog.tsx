import { useState, useMemo, useRef, useCallback, useEffect } from "react";
import {
  Dialog,
  Flex,
  Text,
  TextField,
  Box,
} from "@radix-ui/themes";
import { invoke, convertFileSrc } from "../../tauri";
import { MagnifyingGlassIcon } from "@radix-ui/react-icons";
import type { Servant } from "../../types/servant";
import classAllIcon from "../../../src-tauri/resources/images/class/silver_all.png";
import classSaberIcon from "../../../src-tauri/resources/images/class/silver_saber.png";
import classArcherIcon from "../../../src-tauri/resources/images/class/silver_archer.png";
import classLancerIcon from "../../../src-tauri/resources/images/class/silver_lancer.png";
import classRiderIcon from "../../../src-tauri/resources/images/class/silver_rider.png";
import classCasterIcon from "../../../src-tauri/resources/images/class/silver_caster.png";
import classAssassinIcon from "../../../src-tauri/resources/images/class/silver_assassin.png";
import classBerserkerIcon from "../../../src-tauri/resources/images/class/silver_berserker.png";
import classShielderIcon from "../../../src-tauri/resources/images/class/silver_shielder.png";
import classRulerIcon from "../../../src-tauri/resources/images/class/silver_ruler.png";
import classAvengerIcon from "../../../src-tauri/resources/images/class/silver_avenger.png";
import classMoonCancerIcon from "../../../src-tauri/resources/images/class/silver_mooncell.png";
import classAlteregoIcon from "../../../src-tauri/resources/images/class/silver_alterego.png";
import classForeignerIcon from "../../../src-tauri/resources/images/class/silver_foreigner.png";
import classPretenderIcon from "../../../src-tauri/resources/images/class/silver_pretender.png";
import classBeastIcon from "../../../src-tauri/resources/images/class/silver_beast.png";
import classAllSelectedIcon from "../../../src-tauri/resources/images/class/gold_all.png";
import classSaberSelectedIcon from "../../../src-tauri/resources/images/class/gold_saber.png";
import classArcherSelectedIcon from "../../../src-tauri/resources/images/class/gold_archer.png";
import classLancerSelectedIcon from "../../../src-tauri/resources/images/class/gold_lancer.png";
import classRiderSelectedIcon from "../../../src-tauri/resources/images/class/gold_rider.png";
import classCasterSelectedIcon from "../../../src-tauri/resources/images/class/gold_caster.png";
import classAssassinSelectedIcon from "../../../src-tauri/resources/images/class/gold_assassin.png";
import classBerserkerSelectedIcon from "../../../src-tauri/resources/images/class/gold_berserker.png";
import classShielderSelectedIcon from "../../../src-tauri/resources/images/class/gold_shielder.png";
import classRulerSelectedIcon from "../../../src-tauri/resources/images/class/gold_ruler.png";
import classAvengerSelectedIcon from "../../../src-tauri/resources/images/class/gold_avenger.png";
import classMoonCancerSelectedIcon from "../../../src-tauri/resources/images/class/gold_mooncell.png";
import classAlteregoSelectedIcon from "../../../src-tauri/resources/images/class/gold_alterego.png";
import classForeignerSelectedIcon from "../../../src-tauri/resources/images/class/gold_forigner.png";
import classPretenderSelectedIcon from "../../../src-tauri/resources/images/class/gold_prentender.png";
import classBeastSelectedIcon from "../../../src-tauri/resources/images/class/gold_beast.png";

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
  defaultClassFilter?: string;
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

const RARITY_FILTER_OPTIONS = [null, 1, 2, 3, 4, 5] as const;
const ROW_HEIGHT = 72;
const OVERSCAN = 4;
const VIEWPORT_H = 420;

interface ClassFilterOption {
  value: string;
  label: string;
  icon: string;
  selectedIcon: string;
  servantClasses: string[];
}

const CLASS_FILTER_OPTIONS: ClassFilterOption[] = [
  {
    value: "",
    label: "全部职阶",
    icon: classAllIcon,
    selectedIcon: classAllSelectedIcon,
    servantClasses: [],
  },
  {
    value: "Saber",
    label: "剑阶",
    icon: classSaberIcon,
    selectedIcon: classSaberSelectedIcon,
    servantClasses: ["Saber"],
  },
  {
    value: "Archer",
    label: "弓阶",
    icon: classArcherIcon,
    selectedIcon: classArcherSelectedIcon,
    servantClasses: ["Archer"],
  },
  {
    value: "Lancer",
    label: "枪阶",
    icon: classLancerIcon,
    selectedIcon: classLancerSelectedIcon,
    servantClasses: ["Lancer"],
  },
  {
    value: "Rider",
    label: "骑阶",
    icon: classRiderIcon,
    selectedIcon: classRiderSelectedIcon,
    servantClasses: ["Rider"],
  },
  {
    value: "Caster",
    label: "术阶",
    icon: classCasterIcon,
    selectedIcon: classCasterSelectedIcon,
    servantClasses: ["Caster"],
  },
  {
    value: "Assassin",
    label: "杀阶",
    icon: classAssassinIcon,
    selectedIcon: classAssassinSelectedIcon,
    servantClasses: ["Assassin"],
  },
  {
    value: "Berserker",
    label: "狂阶",
    icon: classBerserkerIcon,
    selectedIcon: classBerserkerSelectedIcon,
    servantClasses: ["Berserker"],
  },
  {
    value: "Shielder",
    label: "盾阶",
    icon: classShielderIcon,
    selectedIcon: classShielderSelectedIcon,
    servantClasses: ["Shielder"],
  },
  {
    value: "Ruler",
    label: "裁阶",
    icon: classRulerIcon,
    selectedIcon: classRulerSelectedIcon,
    servantClasses: ["Ruler"],
  },
  {
    value: "Avenger",
    label: "仇阶",
    icon: classAvengerIcon,
    selectedIcon: classAvengerSelectedIcon,
    servantClasses: ["Avenger"],
  },
  {
    value: "Moon Cancer",
    label: "月癌",
    icon: classMoonCancerIcon,
    selectedIcon: classMoonCancerSelectedIcon,
    servantClasses: ["Moon Cancer"],
  },
  {
    value: "Alterego",
    label: "他人格",
    icon: classAlteregoIcon,
    selectedIcon: classAlteregoSelectedIcon,
    servantClasses: ["Alterego"],
  },
  {
    value: "Foreigner",
    label: "降临者",
    icon: classForeignerIcon,
    selectedIcon: classForeignerSelectedIcon,
    servantClasses: ["Foreigner"],
  },
  {
    value: "Pretender",
    label: "伪装者",
    icon: classPretenderIcon,
    selectedIcon: classPretenderSelectedIcon,
    servantClasses: ["Pretender"],
  },
  {
    value: "Beast",
    label: "兽阶",
    icon: classBeastIcon,
    selectedIcon: classBeastSelectedIcon,
    servantClasses: ["Beast", "BeastEresh"],
  },
];

function normalizeClassFilter(className: string | undefined): string {
  if (!className) return "";
  return (
    CLASS_FILTER_OPTIONS.find((option) =>
      option.servantClasses.includes(className)
    )?.value ?? ""
  );
}

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

function aliasMatchesSearch(servant: Servant, q: string): boolean {
  return (servant.overWriteServantNames ?? []).some((alias) => {
    const jp = alias.nameJp ?? "";
    const cn = alias.nameCn ?? "";
    return jp.toLowerCase().includes(q) || cn.toLowerCase().includes(q);
  });
}

export function ServantSelectDialog({
  open,
  onOpenChange,
  onSelect,
  servants,
  disabledIds,
  defaultClassFilter,
}: ServantSelectDialogProps) {
  const [search, setSearch] = useState("");
  const [classFilter, setClassFilter] = useState(
    normalizeClassFilter(defaultClassFilter)
  );
  const [rarityFilter, setRarityFilter] = useState<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const [faceSrcByKey, setFaceSrcByKey] = useState<Record<string, string | null>>({});
  const scrollRef = useRef<HTMLDivElement>(null);

  const classOptions = useMemo(
    () => [
      CLASS_FILTER_OPTIONS[0],
      ...CLASS_FILTER_OPTIONS.slice(1).filter((option) =>
        servants.some((servant) => option.servantClasses.includes(servant.class))
      ),
    ],
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
      const selectedClass = CLASS_FILTER_OPTIONS.find(
        (option) => option.value === classFilter
      );
      if (
        selectedClass &&
        selectedClass.value &&
        !selectedClass.servantClasses.includes(s.class)
      ) {
        return false;
      }
      if (rarityFilter !== null && s.rarity !== rarityFilter) return false;
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
        (s.noblePhantasmName ?? "").toLowerCase().includes(q) ||
        aliasMatchesSearch(s, q)
    );
  }, [servants, search, classFilter, rarityFilter, disabledIds]);

  useEffect(() => {
    if (!classFilter) return;
    if (!classOptions.some((option) => option.value === classFilter)) {
      setClassFilter("");
    }
  }, [classFilter, classOptions]);

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

  useEffect(() => {
    if (!open) return;
    setClassFilter(normalizeClassFilter(defaultClassFilter));
    setActiveIndex(0);
    resetScroll();
  }, [open, defaultClassFilter, resetScroll]);

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
      if (nextOpen) {
        setClassFilter(normalizeClassFilter(defaultClassFilter));
        setActiveIndex(0);
        resetScroll();
        return;
      }
      if (!nextOpen) {
        setSearch("");
        setClassFilter("");
        setRarityFilter(null);
        setActiveIndex(0);
        resetScroll();
      }
    },
    [defaultClassFilter, onOpenChange, resetScroll]
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
          <div
            className="servant-class-filter-grid"
            role="group"
            aria-label="职阶筛选"
          >
            {classOptions.map((option) => {
              const selected = classFilter === option.value;
              return (
                <button
                  key={option.value || "all"}
                  type="button"
                  className="servant-class-filter-button"
                  aria-label={option.label}
                  aria-pressed={selected}
                  title={option.label}
                  onClick={() => {
                    setClassFilter(option.value);
                    setActiveIndex(0);
                    resetScroll();
                  }}
                >
                  <img
                    src={selected ? option.selectedIcon : option.icon}
                    alt=""
                  />
                </button>
              );
            })}
          </div>
          <div
            className="servant-rarity-filter-grid"
            role="group"
            aria-label="稀有度筛选"
          >
            {RARITY_FILTER_OPTIONS.map((rarity) => {
              const selected = rarityFilter === rarity;
              const label = rarity === null ? "全部稀有度" : `★${rarity}`;
              return (
                <button
                  key={rarity ?? "all"}
                  type="button"
                  className={`servant-rarity-filter-button${selected ? " selected" : ""}`}
                  aria-label={label}
                  aria-pressed={selected}
                  title={label}
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
