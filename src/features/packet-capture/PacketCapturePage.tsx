import { Button, Flex, IconButton, Skeleton } from "@radix-ui/themes";
import { CopyIcon, DownloadIcon, TrashIcon } from "@radix-ui/react-icons";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { TaskActionButton } from "../../components/common/TaskActionButton";
import type { CaptureHistoryEntry } from "../../types/packetCapture";
import { CaptureInventory, CaptureInventorySkeleton } from "./CaptureInventory";
import { usePacketCapture } from "./usePacketCapture";

function dateLabel(timestamp: number) {
  return new Date(timestamp).toLocaleDateString("sv-SE").replace(/-/g, ".");
}
function timeLabel(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString("zh-CN", { hour12: false });
}

function CaptureEmpty({ title, detail, loading = false }: { title: string; detail?: string; loading?: boolean }) {
  return <div className={`capture-empty${loading ? " is-loading" : ""}`}>
    <span className="capture-empty-diamonds" aria-hidden="true"><i /><i /></span>
    <span>{title}</span>{detail && <small>{detail}</small>}
  </div>;
}

export function PacketCapturePage({ onBack }: { onBack: () => void }) {
  const capture = usePacketCapture();
  const history = capture.workspace?.history;
  const selected = capture.selected;
  const selectedEntry = history?.records.find(entry => entry.id === capture.selectedId);
  const response = selected?.response;
  const loading = capture.initialLoading || capture.recordLoading;
  // Date grouping is presentation only; ordering and selection come from Rust.
  const groups = new Map<string, CaptureHistoryEntry[]>();
  for (const entry of history?.records ?? []) {
    const day = dateLabel(entry.capturedAt);
    groups.set(day, [...groups.get(day) ?? [], entry]);
  }
  return <main className="main-content-inner workspace-page packet-capture-page" aria-label="抓包">
    <PageHeader title="数据抓包" english="PACKET CAPTURE" onBack={onBack} backDisabled={capture.busy}>{null}</PageHeader>
    <div className="capture-body">
      <aside className="capture-history" aria-label="抓包历史记录">
        <header className="capture-history-header">
          <div><small>HISTORY</small><h2>历史记录</h2></div>
          <Button variant="ghost" color="gray" size="1" disabled={capture.busy || !history?.records.length} onClick={() => void capture.remove(null)}><TrashIcon />清空</Button>
        </header>
        <div className="capture-history-scroll">
          {capture.initialLoading && <div className="capture-history-skeleton" role="status" aria-label="正在加载抓包历史">
            <Skeleton width="90px" height="12px" />
            {Array.from({ length: 4 }, (_, index) => <Skeleton key={index} width="100%" height="40px" />)}
          </div>}
          {Array.from(groups, ([day, entries]) => <section className="capture-history-day" key={day} aria-label={day}>
            <h3>{day}</h3>
            {entries.map(entry => <div className={`capture-history-row${capture.selectedId === entry.id ? " is-selected" : ""}`} key={entry.id}>
              <button type="button" className="capture-history-select" disabled={capture.busy} aria-pressed={capture.selectedId === entry.id} aria-label={`查看 ${day} ${timeLabel(entry.capturedAt)}`} onClick={() => void capture.select(entry.id)}>
                <i className="capture-diamond" aria-hidden="true" /><time dateTime={new Date(entry.capturedAt).toISOString()}>{timeLabel(entry.capturedAt)}</time>
                {history?.records[0]?.id === entry.id && <small>最新</small>}
              </button>
              <IconButton variant="ghost" color="gray" size="1" aria-label={`删除 ${day} ${timeLabel(entry.capturedAt)}`} disabled={capture.busy} onClick={() => void capture.remove(entry.id)}><TrashIcon /></IconButton>
            </div>)}
          </section>)}
          {!capture.initialLoading && !history?.records.length && <CaptureEmpty title="暂无历史记录" detail="完成抓包后自动保存" />}
        </div>
      </aside>
      <div className="capture-detail">
        <section aria-label="抓包结果" aria-busy={loading}>
          <header className="capture-section-header">
            <SectionHeading rail stacked english="RESPONSE">抓包结果</SectionHeading>
            {selectedEntry && <time>{dateLabel(selectedEntry.capturedAt)} {timeLabel(selectedEntry.capturedAt)}</time>}
          </header>
          <div className="capture-response-row">
            {loading ? <Skeleton className="capture-response-skeleton" height="14px" /> : <input aria-label="抓包原始响应" readOnly value={response?.body ?? ""} placeholder={capture.capturing ? "正在等待游戏响应…" : "等待抓包结果…"} />}
            <Button variant="ghost" className="capture-copy" size="2" aria-label="复制抓包结果" disabled={capture.busy || loading || !response} onClick={() => void capture.copy()}><CopyIcon />{capture.copied ? "已复制" : "复制"}</Button>
            <Button variant="ghost" className="capture-copy" size="2" aria-label="导出抓包结果" disabled={capture.busy || loading || !response} onClick={() => void capture.exportResponse()}><DownloadIcon />{capture.operation === "export" ? "导出中…" : capture.exported ? "已导出" : "导出"}</Button>
          </div>
        </section>
        <section className="capture-preview" aria-label="数据预览" aria-busy={loading}>
          <header className="capture-section-header">
            <SectionHeading rail stacked english="INVENTORY">数据预览</SectionHeading><span>账户物品</span>
          </header>
          {loading ? <CaptureInventorySkeleton /> : capture.recordError ? <CaptureEmpty title="数据加载失败" detail="点击左侧记录重试" /> : response?.items.length ? <CaptureInventory items={response.items} /> : <CaptureEmpty
            loading={capture.capturing && !response}
            title={response ? "暂无可预览的常用物品" : capture.capturing ? "正在等待游戏数据" : "尚无账户物品"}
            detail={response ? undefined : capture.capturing ? "取得响应后，将自动保存并展示结果" : "点击下方开始，抓包完成后在此预览"}
          />}
        </section>
      </div>
    </div>
    <Flex className="page-footer capture-footer" gap="5">
      <div className="capture-status" role={capture.error ? "alert" : "status"} data-error={!!capture.error}>
        {(capture.error || capture.message) && <><i className="capture-diamond" aria-hidden="true" /><span>{capture.error || capture.message}</span></>}
      </div>
      <Button variant="ghost" color="gray" className="capture-setup" aria-label="环境配置" title="设备需已安装 Reqable 并具备 root 权限；设备重启后需重新配置" disabled={capture.busy || capture.canStop} onClick={() => void capture.setup()}><span><small>SETUP</small><span>环境配置</span></span></Button>
      <TaskActionButton label={capture.canStop ? "停止" : "开始"} running={capture.canStop} disabled={capture.operation === "stop" || (capture.busy && !capture.canStop)} onClick={() => void (capture.canStop ? capture.stop() : capture.start())} />
    </Flex>
  </main>;
}
