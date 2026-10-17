import { createHash, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, readdir, rename, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const PROJECT_ROOT = fileURLToPath(new URL(".", import.meta.url));
const PUBLIC_ROOT = join(PROJECT_ROOT, "public");
const VALID_LABELS = new Set(["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "invalid"]);
const IMAGE_EXTENSIONS = new Set([".png", ".jpg", ".jpeg"]);
const MAX_BODY_BYTES = 16 * 1024;
const SEQUENCE_LABEL = /^(?:[0-9]{1,12}|invalid)$/u;

const CONTENT_TYPES = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".jpeg", "image/jpeg"],
  [".jpg", "image/jpeg"],
  [".png", "image/png"],
]);

export class DatasetStore {
  constructor(dataRoot) {
    this.dataRoot = resolve(dataRoot);
    this.manifestPath = join(this.dataRoot, "manifest.jsonl");
    this.sequenceManifestPath = join(this.dataRoot, "sequence-manifest.jsonl");
    this.samples = [];
    this.sequences = [];
    this.suggestions = {};
    this.indexById = new Map();
    this.rawByParent = new Map();
    this.writeQueue = Promise.resolve();
  }

  async initialize() {
    this.samples = parseManifest(await readFile(this.manifestPath, "utf8"));
    try {
      const suggestionPayload = JSON.parse(
        await readFile(join(this.dataRoot, "suggestions.json"), "utf8"),
      );
      if (suggestionPayload.schemaVersion === 1 && suggestionPayload.suggestions) {
        this.suggestions = suggestionPayload.suggestions;
      }
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    this.indexById = new Map(this.samples.map((sample, index) => [sample.id, index]));
    if (this.indexById.size !== this.samples.length) {
      throw new Error("manifest contains duplicate sample ids");
    }
    this.rawByParent = await indexRawImages(join(this.dataRoot, "raw"));
    try {
      this.sequences = parseManifest(await readFile(this.sequenceManifestPath, "utf8"));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      this.sequences = await this.bootstrapSequences();
      await writeManifestAtomic(this.sequenceManifestPath, this.sequences);
    }
    this.sequenceIndexById = new Map(this.sequences.map((sample, index) => [sample.id, index]));
    if (this.sequenceIndexById.size !== this.sequences.length) {
      throw new Error("sequence manifest contains duplicate sample ids");
    }
  }

  async bootstrapSequences() {
    const configPath = fileURLToPath(new URL("../../mash_cv/assets/digit_classifier/screenshot-regions-v1.json", import.meta.url));
    const config = JSON.parse(await readFile(configPath, "utf8"));
    const parents = new Map();
    for (const sample of this.samples) {
      if (!parents.has(sample.parentId) && sample.resolution) parents.set(sample.parentId, sample);
    }
    const sequences = [];
    for (const sample of parents.values()) {
      const screenshotType = sample.screenshotType ?? "battle";
      const definition = config.screenshotTypes?.[screenshotType];
      if (!definition) continue;
      const frame = sample.image.split("/").at(-2);
      const [width, height] = sample.resolution;
      for (const source of definition.sources) {
        for (const region of source.sequenceRegions ?? source.regions) {
          const image = `regions/${screenshotType}/${sample.server}/${frame}/${source.name}-${region.slot}.png`;
          try {
            await requireFile(safeChildPath(this.dataRoot, image));
          } catch (error) {
            if (error.code === "ENOENT") continue;
            throw error;
          }
          const x = Math.max(0, Math.min(width - 1, Math.round(region.x * width)));
          const y = Math.max(0, Math.min(height - 1, Math.round(region.y * height)));
          const right = Math.max(x + 1, Math.min(width, Math.round((region.x + region.w) * width)));
          const bottom = Math.max(y + 1, Math.min(height, Math.round((region.y + region.h) * height)));
          const digest = createHash("sha256")
            .update(`${sample.parentId}:${screenshotType}:${source.name}:${region.slot}:sequence-v1`)
            .digest("hex").slice(0, 16);
          sequences.push({
            schemaVersion: 1,
            id: `${screenshotType}-${source.name}-sequence-${digest}`,
            image, label: null, source: source.name, server: sample.server,
            parentId: sample.parentId, screenshotType, slot: region.slot,
            resolution: [width, height], sourceBboxPx: [x, y, right - x, bottom - y],
          });
        }
      }
    }
    return sequences;
  }

  catalog() {
    const samples = this.samples.map((sample) => ({
      ...sample,
      suggestion: this.suggestions[sample.id] ?? null,
      cropUrl: `/asset?path=${encodeURIComponent(sample.image)}`,
      rawUrl: this.rawByParent.has(sample.parentId)
        ? `/raw?parentId=${encodeURIComponent(sample.parentId)}`
        : null,
    }));
    const derivedLabels = sequenceSuggestions(this.samples);
    const sequences = this.sequences.map((sample) => ({
      ...sample,
      suggestedLabel: derivedLabels.get(sample.id) ?? null,
      cropUrl: `/asset?path=${encodeURIComponent(sample.image)}`,
      rawUrl: this.rawByParent.has(sample.parentId)
        ? `/raw?parentId=${encodeURIComponent(sample.parentId)}`
        : null,
    }));
    return {
      samples,
      sequences,
      servers: [...new Set(samples.map((sample) => sample.server))].sort(),
      sources: [...new Set(samples.map((sample) => sample.source))].sort(),
      screenshotTypes: [...new Set(samples.map((sample) => sample.screenshotType ?? "battle"))].sort(),
    };
  }

  async setLabel(id, label) {
    if (label !== null && !VALID_LABELS.has(label)) {
      throw new RequestError(400, "label must be 0-9, invalid, or null");
    }
    const index = this.indexById.get(id);
    if (index === undefined) {
      throw new RequestError(404, `unknown sample: ${id}`);
    }
    const previous = this.writeQueue;
    let release;
    this.writeQueue = new Promise((resolveQueue) => { release = resolveQueue; });
    await previous;
    try {
      const updated = { ...this.samples[index], label };
      this.samples[index] = updated;
      await writeManifestAtomic(this.manifestPath, this.samples);
      return updated;
    } finally {
      release();
    }
  }

  async setSequenceLabel(id, label) {
    if (label !== null && (typeof label !== "string" || !SEQUENCE_LABEL.test(label))) {
      throw new RequestError(400, "sequence label must be 1-12 digits, invalid, or null");
    }
    const index = this.sequenceIndexById.get(id);
    if (index === undefined) throw new RequestError(404, `unknown sequence: ${id}`);
    const previous = this.writeQueue;
    let release;
    this.writeQueue = new Promise((resolveQueue) => { release = resolveQueue; });
    await previous;
    try {
      const updated = { ...this.sequences[index], label };
      this.sequences[index] = updated;
      await writeManifestAtomic(this.sequenceManifestPath, this.sequences);
      return updated;
    } finally {
      release();
    }
  }

  async assetPath(relativePath) {
    if (typeof relativePath !== "string" || !relativePath) {
      throw new RequestError(400, "missing asset path");
    }
    const path = safeChildPath(this.dataRoot, relativePath);
    await requireFile(path);
    return path;
  }

  async rawPath(parentId) {
    const path = this.rawByParent.get(parentId);
    if (!path) throw new RequestError(404, "raw screenshot not found");
    await requireFile(path);
    return path;
  }
}

function sequenceSuggestions(samples) {
  const groups = new Map();
  for (const sample of samples) {
    const name = sample.image.split("/").at(-1);
    const match = name.match(/^(.+)-(\d+)-(hundreds|tens|ones|\d+)-[0-9a-f]+\.png$/u);
    if (!match || match[1] !== sample.source) continue;
    const [, source, slotText, position] = match;
    const fixed = ["hundreds", "tens", "ones"].includes(position);
    if (source === "np_gauge" && !fixed) continue;
    if (source !== "np_gauge" && fixed) continue;
    const slot = Number(slotText);
    const screenshotType = sample.screenshotType ?? "battle";
    const digest = createHash("sha256")
      .update(`${sample.parentId}:${screenshotType}:${source}:${slot}:sequence-v1`)
      .digest("hex").slice(0, 16);
    const id = `${screenshotType}-${source}-sequence-${digest}`;
    if (!groups.has(id)) groups.set(id, { fixed, positions: new Map() });
    groups.get(id).positions.set(position, sample.label);
  }
  const result = new Map();
  for (const [id, group] of groups) {
    let labels;
    if (group.fixed) {
      const order = ["hundreds", "tens", "ones"];
      if (!order.every((position) => group.positions.has(position))) continue;
      labels = order.map((position) => group.positions.get(position));
      if (labels.includes(null)) continue;
      while (labels[0] === "invalid") labels.shift();
      if (!labels.length || labels.includes("invalid")) continue;
    } else {
      labels = [...group.positions.entries()]
        .sort(([a], [b]) => Number(a) - Number(b))
        .map(([, label]) => label);
      if (labels.includes(null)) continue;
      labels = labels.filter((label) => label !== "invalid");
    }
    if (labels.length && labels.every((label) => /^[0-9]$/u.test(label))) {
      result.set(id, labels.join(""));
    }
  }
  return result;
}

export function createLabelServer(store, { publicRoot = PUBLIC_ROOT } = {}) {
  return createServer(async (request, response) => {
    try {
      const url = new URL(request.url ?? "/", "http://127.0.0.1");
      if (request.method === "GET" && url.pathname === "/api/catalog") {
        sendJson(response, 200, store.catalog());
        return;
      }
      if (request.method === "POST" && url.pathname === "/api/label") {
        const payload = await readJsonBody(request);
        if (typeof payload.id !== "string") {
          throw new RequestError(400, "id must be a string");
        }
        if (payload.label !== null && typeof payload.label !== "string") {
          throw new RequestError(400, "label must be a string or null");
        }
        sendJson(response, 200, await store.setLabel(payload.id, payload.label));
        return;
      }
      if (request.method === "POST" && url.pathname === "/api/sequence-label") {
        const payload = await readJsonBody(request);
        if (typeof payload.id !== "string") throw new RequestError(400, "id must be a string");
        sendJson(response, 200, await store.setSequenceLabel(payload.id, payload.label));
        return;
      }
      if (request.method === "GET" && url.pathname === "/asset") {
        await sendFile(response, await store.assetPath(url.searchParams.get("path")));
        return;
      }
      if (request.method === "GET" && url.pathname === "/raw") {
        await sendFile(response, await store.rawPath(url.searchParams.get("parentId")));
        return;
      }
      if (request.method === "GET") {
        const publicPath = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
        const path = safeChildPath(publicRoot, publicPath);
        await sendFile(response, path);
        return;
      }
      throw new RequestError(404, "not found");
    } catch (error) {
      if (error instanceof RequestError) {
        sendJson(response, error.status, { error: error.message });
        return;
      }
      if (error?.code === "ENOENT") {
        sendJson(response, 404, { error: "not found" });
        return;
      }
      console.error(error);
      sendJson(response, 500, { error: "internal server error" });
    }
  });
}

class RequestError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

export function parseManifest(text) {
  return text
    .split(/\r?\n/u)
    .filter((line) => line.trim())
    .map((line, index) => {
      try {
        const sample = JSON.parse(line);
        if (!sample || typeof sample !== "object" || typeof sample.id !== "string") {
          throw new Error("sample id is missing");
        }
        return sample;
      } catch (error) {
        throw new Error(`manifest line ${index + 1}: ${error.message}`);
      }
    });
}

async function writeManifestAtomic(path, samples) {
  await mkdir(resolve(path, ".."), { recursive: true });
  const temporaryPath = `${path}.${process.pid}.${randomUUID()}.tmp`;
  const body = `${samples.map((sample) => JSON.stringify(sample)).join("\n")}\n`;
  await writeFile(temporaryPath, body, "utf8");
  await rename(temporaryPath, path);
}

async function indexRawImages(rawRoot) {
  const indexed = new Map();
  await indexRawImagesBelow(rawRoot, indexed);
  return indexed;
}

async function indexRawImagesBelow(root, indexed) {
  let entries = [];
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch (error) {
    if (error.code === "ENOENT") return;
    throw error;
  }
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      await indexRawImagesBelow(path, indexed);
    } else if (entry.isFile() && IMAGE_EXTENSIONS.has(extname(entry.name).toLowerCase())) {
      const digest = createHash("sha256").update(await readFile(path)).digest("hex");
      indexed.set(`sha256:${digest}`, path);
    }
  }
}

export function safeChildPath(root, child) {
  if (typeof child !== "string" || !child || isAbsolute(child)) {
    throw new RequestError(400, "invalid path");
  }
  const rootPath = resolve(root);
  const path = resolve(rootPath, child);
  const pathRelativeToRoot = relative(rootPath, path);
  if (!pathRelativeToRoot || pathRelativeToRoot.startsWith("..") || isAbsolute(pathRelativeToRoot)) {
    throw new RequestError(400, "path escapes root");
  }
  return path;
}

async function requireFile(path) {
  if (!(await stat(path)).isFile()) throw new RequestError(404, "not a file");
}

async function readJsonBody(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) throw new RequestError(413, "request body is too large");
    chunks.push(chunk);
  }
  if (!size) throw new RequestError(400, "request body is empty");
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new RequestError(400, "request body is not valid JSON");
  }
}

async function sendFile(response, path) {
  await requireFile(path);
  const fileStat = await stat(path);
  response.writeHead(200, {
    "Cache-Control": "no-store",
    "Content-Length": fileStat.size,
    "Content-Type": CONTENT_TYPES.get(extname(path).toLowerCase()) ?? "application/octet-stream",
  });
  createReadStream(path).pipe(response);
}

function sendJson(response, status, payload) {
  const body = Buffer.from(JSON.stringify(payload), "utf8");
  response.writeHead(status, {
    "Cache-Control": "no-store",
    "Content-Length": body.length,
    "Content-Type": "application/json; charset=utf-8",
  });
  response.end(body);
}

function parseArguments(argv) {
  const options = {
    dataRoot: resolve(PROJECT_ROOT, "../digit_classifier/data"),
    host: "127.0.0.1",
    port: 8765,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = argv[index + 1];
    if (argument === "--data-root" && value) {
      options.dataRoot = resolve(value);
      index += 1;
    } else if (argument === "--host" && value) {
      options.host = value;
      index += 1;
    } else if (argument === "--port" && value) {
      options.port = Number.parseInt(value, 10);
      index += 1;
    } else {
      throw new Error(`unknown or incomplete argument: ${argument}`);
    }
  }
  if (!Number.isInteger(options.port) || options.port < 1 || options.port > 65535) {
    throw new Error("port must be an integer between 1 and 65535");
  }
  return options;
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArguments(argv);
  const store = new DatasetStore(options.dataRoot);
  await store.initialize();
  const server = createLabelServer(store);
  server.listen(options.port, options.host, () => {
    console.log(`digit labeler: http://${options.host}:${options.port}`);
  });
  return server;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
