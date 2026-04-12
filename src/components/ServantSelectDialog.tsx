import { useState, useMemo, useRef, useCallback } from "react";
import {
  Dialog,
  Flex,
  Text,
  TextField,
  ScrollArea,
  Box,
} from "@radix-ui/themes";
import { MagnifyingGlassIcon } from "@radix-ui/react-icons";
import type { Servant } from "../types/servant";

interface ServantSelectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (servant: Servant) => void;
  servants: Servant[];
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
}: ServantSelectDialogProps) {
  const [search, setSearch] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    if (!search.trim()) return servants;
    const q = search.toLowerCase().trim();
    return servants.filter(
      (s) =>
        s.name_cn.toLowerCase().includes(q) ||
        s.name_en.toLowerCase().includes(q) ||
        s.name_jp.includes(q) ||
        (s.name_other ?? "").toLowerCase().includes(q)
    );
  }, [servants, search]);

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
                  key={servant.id}
                  role="option"
                  aria-selected={index === activeIndex}
                  className={`servant-option ${index === activeIndex ? "focused" : ""}`}
                  onClick={() => handleSelect(servant)}
                  onMouseEnter={() => setActiveIndex(index)}
                >
                  <Flex align="center" gap="3">
                    <Box
                      className="servant-class-badge"
                      style={{ background: getClassColor(servant.class) }}
                    >
                      <Text size="1" weight="bold" style={{ color: "#fff" }}>
                        {servant.class.split(" ")[0]}
                      </Text>
                    </Box>
                    <Flex direction="column" align="start" gap="0">
                      <Text size="2" weight="medium">
                        {servant.name_cn}
                      </Text>
                      <Text
                        size="1"
                        style={{ color: "#d4a537", letterSpacing: "1px" }}
                      >
                        {"★".repeat(servant.rarity)}
                      </Text>
                    </Flex>
                  </Flex>
                </button>
              ))
            )}
          </Flex>
        </ScrollArea>
      </Dialog.Content>
    </Dialog.Root>
  );
}
