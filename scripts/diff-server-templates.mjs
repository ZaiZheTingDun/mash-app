#!/usr/bin/env node

import { readdir, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const defaultIgnoredServerDirs = new Set(["shared"]);

function parseArgs(argv) {
  const args = {
    check: false,
    extension: ".png",
    json: false,
    root: path.join(repoRoot, "src-tauri", "resources", "servers"),
    servers: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];

    if (arg === "--check") {
      args.check = true;
    } else if (arg === "--json") {
      args.json = true;
    } else if (arg === "--all-files") {
      args.extension = null;
    } else if (arg === "--extension") {
      args.extension = readValue(argv, index, arg);
      index += 1;
    } else if (arg === "--root") {
      args.root = path.resolve(readValue(argv, index, arg));
      index += 1;
    } else if (arg === "--servers") {
      args.servers = readValue(argv, index, arg)
        .split(",")
        .map((server) => server.trim())
        .filter(Boolean);
      index += 1;
    } else if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    } else if (arg === "--") {
      continue;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return args;
}

function readValue(argv, index, flag) {
  const value = argv[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function printHelp() {
  console.log(`Usage: node scripts/diff-server-templates.mjs [options]

Compare template files across server resource bundles.
The shared resource bundle is skipped by default; pass it explicitly via
--servers if you need to inspect it.

Options:
  --check              Exit with code 1 when any server is missing templates.
  --json               Print machine-readable JSON.
  --servers cn,jp      Compare only the listed server ids.
  --root <path>        Servers root. Defaults to src-tauri/resources/servers.
  --extension .png     File extension to compare. Defaults to .png.
  --all-files          Compare every file under templates/.
  -h, --help           Show this help.
`);
}

async function listServerIds(root) {
  const entries = await readdir(root, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isDirectory())
    .filter((entry) => !defaultIgnoredServerDirs.has(entry.name))
    .map((entry) => entry.name)
    .sort((left, right) => left.localeCompare(right));
}

async function collectFiles(root, extension) {
  const files = [];

  async function walk(dir) {
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch (error) {
      if (error.code === "ENOENT") {
        return;
      }
      throw error;
    }

    await Promise.all(
      entries.map(async (entry) => {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          await walk(fullPath);
          return;
        }
        if (!entry.isFile()) {
          return;
        }
        if (extension && path.extname(entry.name) !== extension) {
          return;
        }
        files.push(path.relative(root, fullPath).split(path.sep).join("/"));
      }),
    );
  }

  await walk(root);
  return files.sort((left, right) => left.localeCompare(right));
}

async function buildReport({ extension, root, servers }) {
  const selectedServers = servers ?? (await listServerIds(root));
  if (selectedServers.length < 2) {
    throw new Error("At least two server template folders are required for comparison.");
  }

  const templateLists = await Promise.all(
    selectedServers.map(async (server) => {
      const templatesRoot = path.join(root, server, "templates");
      const templateRootStat = await stat(templatesRoot).catch((error) => {
        if (error.code === "ENOENT") {
          return null;
        }
        throw error;
      });
      if (!templateRootStat?.isDirectory()) {
        throw new Error(`Missing templates directory for server "${server}": ${templatesRoot}`);
      }
      return [server, await collectFiles(templatesRoot, extension)];
    }),
  );

  const filesByServer = Object.fromEntries(templateLists);
  const allTemplates = [...new Set(templateLists.flatMap(([, files]) => files))].sort((left, right) =>
    left.localeCompare(right),
  );

  const missingByServer = Object.fromEntries(
    selectedServers.map((server) => {
      const serverFiles = new Set(filesByServer[server]);
      return [server, allTemplates.filter((template) => !serverFiles.has(template))];
    }),
  );

  const onlyIn = Object.fromEntries(
    selectedServers.map((server) => {
      const otherFiles = new Set(
        selectedServers
          .filter((otherServer) => otherServer !== server)
          .flatMap((otherServer) => filesByServer[otherServer]),
      );
      return [server, filesByServer[server].filter((template) => !otherFiles.has(template))];
    }),
  );

  return {
    allTemplateCount: allTemplates.length,
    extension,
    filesByServer,
    missingByServer,
    onlyIn,
    root,
    servers: selectedServers,
  };
}

function printTextReport(report) {
  console.log(`Template diff root: ${path.relative(repoRoot, report.root) || "."}`);
  console.log(`Servers: ${report.servers.join(", ")}`);
  console.log(
    `Compared files: ${report.extension ? `*${report.extension}` : "all files"} (${report.allTemplateCount} unique)`,
  );
  console.log("");

  for (const server of report.servers) {
    console.log(`${server}: ${report.filesByServer[server].length} templates`);
  }
  console.log("");

  let hasMissing = false;
  for (const server of report.servers) {
    const missing = report.missingByServer[server];
    if (missing.length === 0) {
      console.log(`${server}: no missing templates`);
      continue;
    }

    hasMissing = true;
    console.log(`${server}: missing ${missing.length} template(s)`);
    for (const template of missing) {
      console.log(`  - ${template}`);
    }
  }

  if (!hasMissing) {
    console.log("");
    console.log("All compared servers contain the same template file set.");
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const report = await buildReport(args);
  const hasMissing = Object.values(report.missingByServer).some((missing) => missing.length > 0);

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    printTextReport(report);
  }

  if (args.check && hasMissing) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
