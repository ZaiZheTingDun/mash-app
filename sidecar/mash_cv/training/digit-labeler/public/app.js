const state = { samples: [], visible: [], cursor: 0, history: [], busy: false };
const byId = (id) => document.getElementById(id);

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
  const server = byId("serverFilter").value;
  const source = byId("sourceFilter").value;
  state.visible = state.samples.filter((sample) =>
    (server === "all" || sample.server === server) &&
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

function render() {
  const sample = current();
  const labeled = state.samples.filter((item) => item.label !== null).length;
  byId("progress").textContent = `已标注 ${labeled} / ${state.samples.length} · 当前筛选 ${state.visible.length}`;
  if (!sample) {
    byId("contextImage").removeAttribute("src");
    byId("cropImage").removeAttribute("src");
    byId("sourceBox").style.display = "none";
    byId("position").textContent = "当前筛选没有样本";
    byId("sourceMeta").textContent = "可切换筛选条件继续查看";
    byId("labelMeta").textContent = "";
    return;
  }
  byId("position").textContent = `${state.cursor + 1} / ${state.visible.length}`;
  byId("sourceMeta").textContent = `${sample.server.toUpperCase()} · ${sample.source} · ${sample.resolution.join("×")}`;
  byId("labelMeta").textContent = `当前标签：${sample.label ?? "未标注"}`;
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
  byId("undoButton").disabled = state.history.length === 0 || state.busy;
}

async function saveLabel(label) {
  const sample = current();
  if (!sample || state.busy || sample.label === label) return;
  state.busy = true;
  byId("message").textContent = "正在保存…";
  render();
  try {
    const response = await fetch("/api/label", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: sample.id, label }),
    });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || "保存失败");
    state.history.push({ id: sample.id, label: sample.label });
    state.samples.find((item) => item.id === sample.id).label = payload.label;
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
  if (!state.history.length || state.busy) return;
  const previous = state.history.pop();
  const sample = state.samples.find((item) => item.id === previous.id);
  if (!sample) return;
  state.busy = true;
  try {
    const response = await fetch("/api/label", {
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
    state.history.push(previous);
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
  for (const id of ["statusFilter", "serverFilter", "sourceFilter"]) {
    byId(id).addEventListener("change", () => { state.cursor = 0; applyFilters(); });
  }
  byId("previousButton").addEventListener("click", () => move(-1));
  byId("nextButton").addEventListener("click", () => move(1));
  byId("skipButton").addEventListener("click", () => move(1));
  byId("undoButton").addEventListener("click", undo);
  document.addEventListener("keydown", (event) => {
    if (event.target instanceof HTMLSelectElement) return;
    if (/^[0-9]$/.test(event.key)) saveLabel(event.key);
    else if (event.key.toLowerCase() === "x") saveLabel("invalid");
    else if (event.key.toLowerCase() === "s" || event.key === "ArrowRight") move(1);
    else if (event.key === "ArrowLeft") move(-1);
    else if (event.key.toLowerCase() === "u") undo();
    else if (event.key === "Backspace" || event.key === "Delete") saveLabel(null);
  });
  applyFilters();
}

boot().catch((error) => { byId("message").textContent = String(error); });
