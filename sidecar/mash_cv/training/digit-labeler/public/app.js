const state = { samples: [], sequences: [], mode: "digit", visible: [], cursor: 0,
  histories: { digit: [], sequence: [] }, busy: false };
const byId = (id) => document.getElementById(id);
const allItems = () => state.mode === "digit" ? state.samples : state.sequences;
const history = () => state.histories[state.mode];

function addOptions(select, values) {
  for (const value of values) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = value;
    select.appendChild(option);
  }
}

function applyFilters(keepId = null) {
  const status = byId("statusFilter").value;
  const screenshotType = byId("screenshotTypeFilter").value;
  const server = byId("serverFilter").value;
  const source = byId("sourceFilter").value;
  state.visible = allItems().filter((sample) =>
    (server === "all" || sample.server === server) &&
    (screenshotType === "all" || (sample.screenshotType ?? "battle") === screenshotType) &&
    (source === "all" || sample.source === source) &&
    (status === "all" || (status === "unlabeled" ? sample.label === null : sample.label !== null))
  );
  if (keepId) {
    const kept = state.visible.findIndex((sample) => sample.id === keepId);
    if (kept >= 0) state.cursor = kept;
  }
  state.cursor = Math.max(0, Math.min(state.cursor, Math.max(0, state.visible.length - 1)));
  render();
}

function current() { return state.visible[state.cursor] ?? null; }

function confirmSuggestion() {
  const sample = current();
  const label = state.mode === "digit" ? sample?.suggestion?.label : sample?.suggestedLabel;
  if (!sample || sample.label !== null || !label) return;
  saveLabel(label);
}

function render() {
  const sample = current();
  const items = allItems();
  const labeled = items.filter((item) => item.label !== null).length;
  byId("progress").textContent = `${state.mode === "digit" ? "单字" : "完整数字"}已标注 ${labeled} / ${items.length} · 当前筛选 ${state.visible.length}`;
  byId("digitMode").classList.toggle("active", state.mode === "digit");
  byId("sequenceMode").classList.toggle("active", state.mode === "sequence");
  byId("labelButtons").hidden = state.mode !== "digit";
  byId("sequenceForm").hidden = state.mode !== "sequence";
  byId("cropTitle").textContent = state.mode === "digit" ? "待标注字符" : "待标注完整数字区域";
  byId("shortcutHint").textContent = state.mode === "digit"
    ? "快捷键：Enter 确认模型候选，0–9 直接重标，X 标记无效，S 跳过，U 撤销，←/→ 浏览，Backspace 清除标签。"
    : "输入完整数字后按 Enter 保存；可确认单字拼接候选，或标记无效。S 跳过，U 撤销，←/→ 浏览。";
  if (!sample) {
    byId("contextImage").removeAttribute("src");
    byId("cropImage").removeAttribute("src");
    byId("sourceBox").style.display = "none";
    byId("position").textContent = "当前筛选没有样本";
    byId("sourceMeta").textContent = "可切换筛选条件继续查看";
    byId("labelMeta").textContent = "";
    byId("suggestionPanel").hidden = true;
    return;
  }
  byId("position").textContent = `${state.cursor + 1} / ${state.visible.length}`;
  byId("sourceMeta").textContent = `${sample.screenshotType ?? "battle"} · ${sample.server.toUpperCase()} · ${sample.source} #${sample.slot ?? "—"} · ${sample.resolution.join("×")}`;
  byId("labelMeta").textContent = `当前标签：${sample.label ?? "未标注"}`;
  if (state.mode === "sequence") byId("sequenceInput").value = sample.label === "invalid" ? "" : sample.label ?? "";
  const suggestion = sample.label === null
    ? state.mode === "digit" ? sample.suggestion : sample.suggestedLabel
    : null;
  const suggestionPanel = byId("suggestionPanel");
  suggestionPanel.hidden = !suggestion;
  if (suggestion) {
    byId("suggestionCaption").textContent = state.mode === "digit" ? "模型识别候选" : "旧单字标签拼接候选（请核对整张图）";
    byId("suggestionValue").textContent = (state.mode === "digit" ? suggestion.label : suggestion) === "invalid"
      ? "无效（没有数字）"
      : state.mode === "digit" ? suggestion.label : suggestion;
    if (state.mode === "digit") {
      const confidencePercent = Math.round(suggestion.confidence * 100);
      byId("suggestionConfidence").textContent = suggestion.confident
        ? `模型置信度 ${confidencePercent}%`
        : `置信度较低 ${confidencePercent}%，请仔细确认`;
    } else {
      byId("suggestionConfidence").textContent = "核对无误后点击确认；有遗漏时直接输入正确数字";
    }
  }
  byId("confirmSuggestionButton").disabled = !suggestion || state.busy;
  byId("confirmSuggestionButton").textContent = state.mode === "digit" ? "确认候选 Enter" : "确认这串数字";
  byId("cropImage").src = sample.cropUrl;
  byId("contextImage").src = sample.rawUrl ?? "";
  const [x, y, width, height] = sample.sourceBboxPx;
  const [imageWidth, imageHeight] = sample.resolution;
  const box = byId("sourceBox");
  box.style.display = sample.rawUrl ? "block" : "none";
  box.style.left = `${x / imageWidth * 100}%`;
  box.style.top = `${y / imageHeight * 100}%`;
  box.style.width = `${width / imageWidth * 100}%`;
  box.style.height = `${height / imageHeight * 100}%`;
  for (const button of document.querySelectorAll(".label-button")) {
    button.classList.toggle("active", button.dataset.label === sample.label);
  }
  byId("previousButton").disabled = state.cursor === 0 || state.busy;
  byId("nextButton").disabled = state.cursor >= state.visible.length - 1 || state.busy;
  byId("undoButton").disabled = history().length === 0 || state.busy;
  byId("saveSequenceButton").disabled = state.busy;
  byId("invalidSequenceButton").disabled = state.busy;
}

async function saveLabel(label) {
  const sample = current();
  if (!sample || state.busy || sample.label === label) return;
  if (state.mode === "sequence" && label !== null && label !== "invalid" && !/^[0-9]{1,12}$/.test(label)) {
    byId("message").textContent = "请输入 1–12 位数字";
    return;
  }
  const mode = state.mode;
  state.busy = true;
  byId("message").textContent = "正在保存…";
  render();
  try {
    const response = await fetch(mode === "digit" ? "/api/label" : "/api/sequence-label", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: sample.id, label }),
    });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || "保存失败");
    state.histories[mode].push({ id: sample.id, label: sample.label });
    allItems().find((item) => item.id === sample.id).label = payload.label;
    const oldCursor = state.cursor;
    applyFilters();
    state.cursor = Math.min(oldCursor, Math.max(0, state.visible.length - 1));
    byId("message").textContent = "已保存";
  } catch (error) {
    byId("message").textContent = String(error);
  } finally {
    state.busy = false;
    render();
  }
}

async function undo() {
  if (!history().length || state.busy) return;
  const previous = history().pop();
  const sample = allItems().find((item) => item.id === previous.id);
  if (!sample) return;
  state.busy = true;
  try {
    const response = await fetch(state.mode === "digit" ? "/api/label" : "/api/sequence-label", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: previous.id, label: previous.label }),
    });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || "撤销失败");
    sample.label = payload.label;
    applyFilters(previous.id);
    byId("message").textContent = "已撤销";
  } catch (error) {
    history().push(previous);
    byId("message").textContent = String(error);
  } finally {
    state.busy = false;
    render();
  }
}

function move(delta) {
  if (!state.visible.length || state.busy) return;
  state.cursor = Math.max(0, Math.min(state.cursor + delta, state.visible.length - 1));
  render();
}

async function boot() {
  const response = await fetch("/api/catalog");
  const catalog = await response.json();
  if (!response.ok) throw new Error(catalog.error || "无法读取标注清单");
  state.samples = catalog.samples;
  state.sequences = catalog.sequences ?? [];
  addOptions(byId("screenshotTypeFilter"), catalog.screenshotTypes);
  addOptions(byId("serverFilter"), catalog.servers);
  addOptions(byId("sourceFilter"), catalog.sources);
  for (const label of [..."0123456789", "invalid"]) {
    const button = document.createElement("button");
    button.className = `label-button${label === "invalid" ? " invalid" : ""}`;
    button.dataset.label = label;
    button.textContent = label === "invalid" ? "无效 X" : label;
    button.addEventListener("click", () => saveLabel(label));
    byId("labelButtons").appendChild(button);
  }
  for (const id of ["statusFilter", "screenshotTypeFilter", "serverFilter", "sourceFilter"]) {
    byId(id).addEventListener("change", () => { state.cursor = 0; applyFilters(); });
  }
  byId("digitMode").addEventListener("click", () => { state.mode = "digit"; state.cursor = 0; applyFilters(); });
  byId("sequenceMode").addEventListener("click", () => { state.mode = "sequence"; state.cursor = 0; applyFilters(); });
  byId("sequenceForm").addEventListener("submit", (event) => {
    event.preventDefault();
    saveLabel(byId("sequenceInput").value.trim());
  });
  byId("invalidSequenceButton").addEventListener("click", () => saveLabel("invalid"));
  byId("previousButton").addEventListener("click", () => move(-1));
  byId("nextButton").addEventListener("click", () => move(1));
  byId("skipButton").addEventListener("click", () => move(1));
  byId("undoButton").addEventListener("click", undo);
  byId("confirmSuggestionButton").addEventListener("click", confirmSuggestion);
  document.addEventListener("keydown", (event) => {
    if (event.target instanceof HTMLSelectElement || event.target instanceof HTMLInputElement) return;
    if (state.mode === "sequence") {
      if (event.key.toLowerCase() === "s" || event.key === "ArrowRight") move(1);
      else if (event.key === "ArrowLeft") move(-1);
      else if (event.key.toLowerCase() === "u") undo();
      else if (event.key === "Backspace" || event.key === "Delete") saveLabel(null);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      confirmSuggestion();
    } else if (/^[0-9]$/.test(event.key)) saveLabel(event.key);
    else if (event.key.toLowerCase() === "x") saveLabel("invalid");
    else if (event.key.toLowerCase() === "s" || event.key === "ArrowRight") move(1);
    else if (event.key === "ArrowLeft") move(-1);
    else if (event.key.toLowerCase() === "u") undo();
    else if (event.key === "Backspace" || event.key === "Delete") saveLabel(null);
  });
  applyFilters();
}

boot().catch((error) => { byId("message").textContent = String(error); });
