import { useEffect, useRef, useState } from "react";
import { Button, Dialog, Flex, Text, Spinner } from "@radix-ui/themes";
import { invoke, convertFileSrc } from "../../tauri";

export interface PortraitOption {
  id: number;
  path: string;
}

interface ServantPortraitOptions {
  options: PortraitOption[];
  selectedId: number | null;
}

interface PortraitSelectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  servantId: number;
  variantKey: string;
  servantName: string;
  onSaved: () => void;
}

export function PortraitSelectDialog({
  open,
  onOpenChange,
  servantId,
  variantKey,
  servantName,
  onSaved,
}: PortraitSelectDialogProps) {
  // null = not yet fetched (loading); array = fetch complete
  const [portraits, setPortraits] = useState<PortraitOption[] | null>(null);
  const [currentId, setCurrentId] = useState<number | null>(null);
  const [pendingId, setPendingId] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);

  const loading = portraits === null;

  // Drag-scroll state
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragStart = useRef<{ x: number; scrollLeft: number } | null>(null);

  // Fetch portrait options when the dialog is open. The component is mounted
  // fresh each time (parent conditionally renders it), so we fetch on mount.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ServantPortraitOptions>("list_servant_portraits", {
      servantId,
      variantKey,
    })
      .then((data) => {
        if (cancelled) return;
        const options = data.options.map((o) => ({
          id: o.id,
          path: convertFileSrc(o.path),
        }));
        setPortraits(options);
        setCurrentId(data.selectedId);
        setPendingId(data.selectedId);
      })
      .catch(() => {
        if (cancelled) return;
        setPortraits([]);
      });
    return () => {
      cancelled = true;
    };
  }, [open, servantId, variantKey]);

  const handleConfirm = () => {
    if (pendingId == null) {
      onOpenChange(false);
      return;
    }
    setSaving(true);
    invoke("save_servant_portrait_selection", {
      variantKey,
      portraitId: pendingId,
    })
      .then(() => {
        onSaved();
        onOpenChange(false);
      })
      .catch(() => {})
      .finally(() => setSaving(false));
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    if (!scrollRef.current) return;
    dragStart.current = { x: e.clientX, scrollLeft: scrollRef.current.scrollLeft };
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!dragStart.current || !scrollRef.current) return;
    const dx = e.clientX - dragStart.current.x;
    scrollRef.current.scrollLeft = dragStart.current.scrollLeft - dx;
  };

  const handleMouseUp = () => {
    dragStart.current = null;
  };

  const handleWheel = (e: React.WheelEvent) => {
    if (!scrollRef.current) return;
    const delta = e.shiftKey ? e.deltaY : e.deltaX || e.deltaY;
    scrollRef.current.scrollLeft += delta;
    e.preventDefault();
  };

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="550px" className="portrait-select-dialog">
        <Dialog.Title>立绘设置 — {servantName}</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          选择立绘后点击确认，对所有队伍生效。
        </Dialog.Description>

        <div className="portrait-select-scroll-area" style={{ marginTop: 16 }}>
          {loading ? (
            <Flex align="center" justify="center" style={{ height: 180 }}>
              <Spinner size="3" />
            </Flex>
          ) : portraits.length === 0 ? (
            <Flex align="center" justify="center" style={{ height: 180 }}>
              <Text size="2" color="gray">无可用立绘资源</Text>
            </Flex>
          ) : (
            <div
              ref={scrollRef}
              className="portrait-select-scroll"
              onMouseDown={handleMouseDown}
              onMouseMove={handleMouseMove}
              onMouseUp={handleMouseUp}
              onMouseLeave={handleMouseUp}
              onWheel={handleWheel}
            >
              {portraits.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={`portrait-select-card${pendingId === p.id ? " selected" : ""}`}
                  onClick={() => setPendingId(p.id)}
                  draggable={false}
                >
                  <img
                    src={p.path}
                    alt={`立绘 ${p.id}`}
                    className="portrait-select-img"
                    draggable={false}
                  />
                  {currentId === p.id && (
                    <span className="portrait-select-current-badge">当前</span>
                  )}
                </button>
              ))}
            </div>
          )}
        </div>

        <Flex gap="3" justify="end" mt="4">
          <Dialog.Close>
            <Button variant="soft" color="gray">
              取消
            </Button>
          </Dialog.Close>
          <Button
            disabled={saving || loading || portraits?.length === 0}
            onClick={handleConfirm}
          >
            {saving ? <Spinner size="1" /> : null}
            确认
          </Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
