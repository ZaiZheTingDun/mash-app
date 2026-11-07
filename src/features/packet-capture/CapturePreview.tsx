import { Button, Checkbox, DataList, Dialog, Flex, Select, Tabs, TextField } from "@radix-ui/themes";
import { ChevronLeftIcon, ChevronRightIcon, MagnifyingGlassIcon } from "@radix-ui/react-icons";
import { useState } from "react";
import type { CapturePreview as Preview, CapturePreviewField, CapturePreviewItem, CapturePreviewQuery, CapturePreviewTab } from "../../types/packetCapture";
import { convertFileSrc } from "../../tauri";
import { CaptureInventorySkeleton, CaptureItemIcon } from "./CaptureInventory";
import { useCapturePreview } from "./useCapturePreview";

const tabs: { value: CapturePreviewTab; label: string }[] = [
  { value: "overview", label: "概览" }, { value: "servants", label: "从者" },
  { value: "craftEssences", label: "礼装" }, { value: "items", label: "物品" },
];
const initialQuery: CapturePreviewQuery = {
  tab: "overview", search: "", classId: null, rarity: null, storage: "all", sort: "number",
  itemCategory: null, limitBroken: false, showZero: false, page: 0,
};
const count = (value: number | null) => value === null ? "—" : value.toLocaleString("zh-CN");

function Fields({ fields }: { fields: CapturePreviewField[] }) {
  return <DataList.Root size="2" className="capture-fields">
    {fields.map(field => <DataList.Item key={field.label}><DataList.Label>{field.label}</DataList.Label><DataList.Value>{field.value ?? "—"}</DataList.Value></DataList.Item>)}
  </DataList.Root>;
}

function Items({ items, compact = false }: { items: CapturePreviewItem[]; compact?: boolean }) {
  return <dl className={`capture-inventory capture-preview-items${compact ? " is-compact" : ""}`}>
    {items.map(item => <div className="capture-inventory-item" key={item.id} title={`${item.name} · ID ${item.itemId}`}>
      <CaptureItemIcon itemId={item.itemId} icon={item.icon} />
      <div><dt>{item.name}</dt><dd>{count(item.quantity)}</dd>{item.note && <small>{item.note}</small>}</div>
    </div>)}
  </dl>;
}

function Portrait({ icon }: { icon: string | null }) {
  return icon ? <img className="capture-entity-face" src={convertFileSrc(icon)} alt="" loading="lazy" /> : <span className="capture-entity-placeholder" aria-hidden="true">◇</span>;
}

function Overview({ data, navigate }: { data: Preview; navigate: (tab: CapturePreviewTab) => void }) {
  return <div className="capture-overview">
    <Fields fields={data.profile} />
    <div className="capture-preview-subheading">主要货币</div>
    {data.currencies.length ? <Items items={data.currencies} /> : <p className="capture-preview-muted">此记录未提供货币数据</p>}
    <div className="capture-preview-subheading">体力恢复道具</div>
    {data.fruits.length ? <Items items={data.fruits} compact /> : <p className="capture-preview-muted">此记录未提供苹果数据</p>}
    <div className="capture-preview-subheading">持有摘要</div>
    <div className="capture-summary">
      <Button variant="ghost" color="gray" onClick={() => navigate("servants")}><span>从者</span><strong>{count(data.summary.servantTypes)} 种 / {count(data.summary.servantInstances)} 份</strong></Button>
      <Button variant="ghost" color="gray" onClick={() => navigate("craftEssences")}><span>概念礼装</span><strong>{count(data.summary.ceTypes)} 种 / {count(data.summary.ceInstances)} 张</strong></Button>
      <div><span>保管室</span><strong>{count(data.summary.storageInstances)} 份</strong></div>
      <div><span>礼物箱</span><strong>{count(data.summary.presents)} 条</strong></div>
    </div>
    {data.summary.unclassifiedInstances !== null && data.summary.unclassifiedInstances > 0 && <Button className="capture-unclassified-link" variant="ghost" color="gray" size="1" onClick={() => navigate("items")}>另有 {count(data.summary.unclassifiedInstances)} 份未分类实例，可在物品中查看</Button>}
  </div>;
}

export function CapturePreview({ recordId, loading, recordError, waiting }: { recordId: string | null; loading: boolean; recordError: boolean; waiting: boolean }) {
  const [query, setQuery] = useState<CapturePreviewQuery>(initialQuery);
  const preview = useCapturePreview(recordId, query);
  const data = preview.data;
  const selectionKey = JSON.stringify({ recordId, query });
  const [selection, setSelection] = useState<{ key: string; servant?: string; ce?: number } | null>(null);
  const servant = selection?.key === selectionKey ? data?.servants.find(v => v.instanceId === selection.servant) : undefined;
  const ce = selection?.key === selectionKey ? data?.craftEssences.find(v => v.svtId === selection.ce) : undefined;
  const selected = servant ?? ce;
  const change = (patch: Partial<CapturePreviewQuery>) => setQuery(current => ({ ...current, ...patch, page: 0 }));
  const navigate = (tab: CapturePreviewTab) => setQuery({ ...initialQuery, tab });
  const busy = loading || preview.loading;
  return <Tabs.Root className="capture-preview-browser" value={query.tab} onValueChange={tab => navigate(tab as CapturePreviewTab)}>
    <Tabs.List aria-label="账户数据分类">{tabs.map(tab => <Tabs.Trigger key={tab.value} value={tab.value} aria-label={tab.label}>{tab.label}</Tabs.Trigger>)}</Tabs.List>
    {recordId && query.tab !== "overview" && <div className="capture-preview-filters">
      <TextField.Root aria-label="搜索账户数据" placeholder={query.tab === "items" ? "搜索名称或 ID" : "搜索名称或图鉴编号"} value={query.search} onChange={event => change({ search: event.target.value })}>
        <TextField.Slot><MagnifyingGlassIcon /></TextField.Slot>
      </TextField.Root>
      {query.tab === "servants" && <Select.Root value={query.classId?.toString() ?? "all"} onValueChange={value => change({ classId: value === "all" ? null : Number(value) })}>
        <Select.Trigger aria-label="筛选职阶" placeholder="全部职阶" /><Select.Content><Select.Item value="all">全部职阶</Select.Item>{preview.classes.map(v => <Select.Item key={v.id} value={v.id.toString()}>{v.name}</Select.Item>)}</Select.Content>
      </Select.Root>}
      {query.tab !== "items" && <>
        <Select.Root value={query.rarity?.toString() ?? "all"} onValueChange={value => change({ rarity: value === "all" ? null : Number(value) })}>
          <Select.Trigger aria-label="筛选稀有度" /><Select.Content><Select.Item value="all">全部稀有度</Select.Item>{[0,1,2,3,4,5].map(v => <Select.Item key={v} value={v.toString()}>{v} 星</Select.Item>)}</Select.Content>
        </Select.Root>
        <Select.Root value={query.storage} onValueChange={value => change({ storage: value as CapturePreviewQuery["storage"] })}>
          <Select.Trigger aria-label="筛选仓库" /><Select.Content><Select.Item value="all">全部仓库</Select.Item><Select.Item value="inventory">普通仓库</Select.Item><Select.Item value="storage">保管室</Select.Item></Select.Content>
        </Select.Root>
      </>}
      {query.tab === "items" && <Select.Root value={query.itemCategory ?? "all"} onValueChange={value => change({ itemCategory: value === "all" ? null : value })}>
        <Select.Trigger aria-label="筛选物品类别" /><Select.Content><Select.Item value="all">全部物品</Select.Item><Select.Item value="consumable">恢复道具</Select.Item><Select.Item value="event">活动物品</Select.Item><Select.Item value="other">其他／未识别</Select.Item></Select.Content>
      </Select.Root>}
      <Select.Root value={query.sort} onValueChange={value => change({ sort: value as CapturePreviewQuery["sort"] })}>
        <Select.Trigger aria-label="排列方式" /><Select.Content><Select.Item value="number">按编号</Select.Item>{query.tab !== "items" && <Select.Item value="level">等级优先</Select.Item>}{query.tab === "servants" ? <Select.Item value="bond">羁绊优先</Select.Item> : <Select.Item value="quantity">数量优先</Select.Item>}</Select.Content>
      </Select.Root>
      {query.tab === "craftEssences" && <label className="capture-checkbox"><Checkbox checked={query.limitBroken} onCheckedChange={value => change({ limitBroken: value === true })} />仅满破</label>}
      {query.tab === "items" && <label className="capture-checkbox"><Checkbox checked={query.showZero} onCheckedChange={value => change({ showZero: value === true })} />显示零数量</label>}
    </div>}
    <Tabs.Content key={selectionKey} className="capture-preview-content" value={query.tab} aria-busy={busy}>
      {busy ? <CaptureInventorySkeleton /> : recordError ? <div className="capture-preview-empty"><span>数据加载失败</span><small>点击左侧记录重试</small></div> : preview.error ? <div className="capture-preview-empty" role="alert"><span>{preview.error}</span><Button variant="soft" size="1" onClick={preview.retry}>重新解析</Button></div> : !data ? <div className="capture-preview-empty">{waiting ? "正在等待游戏数据" : "尚无账户物品"}</div> : query.tab === "overview" ? <Overview data={data} navigate={navigate} /> : <>
        <div className="capture-result-count">共 {count(data.total)} {query.tab === "servants" ? "份从者" : query.tab === "craftEssences" ? "种礼装" : "种物品"}</div>
        {data.total === 0 ? <div className="capture-preview-empty">没有符合条件的数据</div> : query.tab === "servants" ? <div className="capture-servant-list">
          <div className="capture-servant-columns" aria-hidden="true"><span>从者</span><span>等级</span><span>宝具</span><span>技能</span><span>羁绊</span></div>
          {data.servants.map(v => <button type="button" className="capture-servant-row" key={v.instanceId} aria-label={`查看${v.name}，实例 ${v.instanceId}`} onClick={() => setSelection({ key: selectionKey, servant: v.instanceId })}>
            <span className="capture-entity-identity"><Portrait icon={v.icon} /><span><strong>{v.name}</strong><small>{v.className ?? "—"} · {v.rarity ?? "—"} 星{v.inStorage ? " · 保管室" : ""}</small></span></span>
            <span>{v.level ?? "—"}</span><span>{v.npLevel ?? "—"}</span><span>{v.skills.map(n => n ?? "—").join(" / ")}</span><span>{v.bond ?? "—"}</span>
          </button>)}
        </div> : query.tab === "craftEssences" ? <div className="capture-ce-list">{data.craftEssences.map(v => <button type="button" className="capture-ce-row" key={v.svtId} aria-label={`查看${v.name}，${v.quantity}张`} onClick={() => setSelection({ key: selectionKey, ce: v.svtId })}>
          <Portrait icon={v.icon} /><span className="capture-ce-text"><strong>{v.name}</strong><small>{v.rarity ?? "—"} 星 · {v.summary}</small></span><span className="capture-ce-quantity">×{v.quantity}</span><ChevronRightIcon />
        </button>)}</div> : <Items items={data.items} compact />}
        {data.pageCount > 1 && <Flex className="capture-pagination" align="center" justify="end" gap="3"><Button variant="ghost" size="1" aria-label="上一页" disabled={data.page === 0} onClick={() => setQuery(current => ({ ...current, page: data.page - 1 }))}><ChevronLeftIcon /></Button><span>{data.page + 1} / {data.pageCount}</span><Button variant="ghost" size="1" aria-label="下一页" disabled={data.page + 1 >= data.pageCount} onClick={() => setQuery(current => ({ ...current, page: data.page + 1 }))}><ChevronRightIcon /></Button></Flex>}
      </>}
    </Tabs.Content>
    {selected && <Dialog.Root open onOpenChange={open => { if (!open) setSelection(null); }}><Dialog.Content maxWidth="620px" className="capture-entity-dialog">
      <Dialog.Title>{selected.name}</Dialog.Title><Dialog.Description size="2" color="gray">{servant ? `实例 ${servant.instanceId} · ${servant.inStorage ? "保管室" : "普通仓库"}` : `图鉴编号 ${ce?.number} · ${ce?.quantity} 张`}</Dialog.Description>
      {servant && <Fields fields={[
        { label: "等级 / 宝具", value: `${servant.level ?? "—"} / ${servant.npLevel ?? "—"}` },
        { label: "持有技能", value: servant.skills.map(v => v ?? "—").join(" / ") },
        { label: "羁绊等级（同种从者共享）", value: servant.bond?.toString() ?? null },
        ...servant.details,
      ]} />}
      {ce && <div className="capture-ce-instances">{ce.instances.map(v => <div key={v.instanceId}><span>Lv.{v.level ?? "—"} · {v.limitBroken === true ? "满破" : `突破 ${v.limitCount ?? "—"}`}</span><span>{v.inStorage ? "保管室" : "普通仓库"} · {v.locked === null ? "锁定 —" : v.locked ? "已锁定" : "未锁定"}</span><small>实例 {v.instanceId}</small></div>)}</div>}
      <Flex justify="end" mt="5"><Dialog.Close><Button variant="soft">关闭</Button></Dialog.Close></Flex>
    </Dialog.Content></Dialog.Root>}
  </Tabs.Root>;
}
