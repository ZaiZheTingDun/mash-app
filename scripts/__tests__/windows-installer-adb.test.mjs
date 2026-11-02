import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { pwshArgs, psLiteral } from "./helpers/pwsh.mjs";

const repo = fileURLToPath(new URL("../../", import.meta.url));
const hooks = path.join(repo, "src-tauri/windows/installer-hooks.nsh");
const cleanup = path.join(repo, "src-tauri/windows/stop-adb.ps1");
const windows = process.platform === "win32";
const powershell = path.join(process.env.SystemRoot ?? "C:/Windows", "System32/WindowsPowerShell/v1.0/powershell.exe");
const invoke = (file, args) => execFileSync(file, args, {
  cwd: repo, encoding: "utf8", windowsHide: true, timeout: 30_000, stdio: "pipe",
});

test("Windows installer enables the ADB cleanup hook", () => {
  const config = JSON.parse(fs.readFileSync(path.join(repo, "src-tauri/tauri.windows.conf.json"), "utf8"));
  assert.equal(path.resolve(repo, "src-tauri", config.bundle.windows.nsis.installerHooks), hooks);
  assert.match(fs.readFileSync(hooks, "utf8"), /!macro NSIS_HOOK_PREINSTALL/);
});

async function waitFor(predicate, description, timeoutMs = 10_000) {
  const deadline = performance.now() + timeoutMs;
  while (!predicate()) {
    assert.ok(performance.now() < deadline, `Timed out waiting for ${description}`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}

test("installer cleanup releases DLL locks only for the installed ADB", { skip: !windows }, async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash installer 中文 ' "));
  const children = [];
  t.after(async () => {
    for (const child of children) {
      if (child.pid !== undefined && child.exitCode === null && child.signalCode === null) child.kill();
    }
    await Promise.all(children.map(child => child.pid === undefined || child.exitCode !== null || child.signalCode !== null
      ? Promise.resolve() : new Promise(resolve => child.once("exit", resolve))));
    fs.rmSync(root, { recursive: true, force: true });
  });
  const executable = path.join(root, "fixture.exe");
  const source = `using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
class FakeAdb {
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  static extern IntPtr LoadLibrary(string path);
  static void Main(string[] args) {
    if (LoadLibrary(args[0]) == IntPtr.Zero || LoadLibrary(args[1]) == IntPtr.Zero) Environment.Exit(2);
    File.WriteAllText(args[2], "ready");
    Thread.Sleep(Timeout.Infinite);
  }
}`;
  // Google's bundled ADB and its DLLs are x86, including on x64 Windows.
  invoke(powershell, pwshArgs(`
$ProgressPreference = 'SilentlyContinue'
$options = New-Object System.CodeDom.Compiler.CompilerParameters
$options.CompilerOptions = '/platform:x86'
$options.GenerateExecutable = $true
$options.OutputAssembly = ${psLiteral(executable)}
$compiler = New-Object Microsoft.CSharp.CSharpCodeProvider
try {
  $result = $compiler.CompileAssemblyFromSource($options, ${psLiteral(source)})
  if ($result.Errors.HasErrors) { throw ($result.Errors | Out-String) }
} finally { $compiler.Dispose() }`, repo));
  async function startAdb(directory) {
    fs.mkdirSync(directory, { recursive: true });
    const exe = path.join(directory, "adb.exe");
    const dll = path.join(directory, "AdbWinApi.dll");
    const usbDll = path.join(directory, "AdbWinUsbApi.dll");
    const ready = path.join(directory, "ready");
    fs.rmSync(ready, { force: true });
    fs.copyFileSync(executable, exe);
    fs.copyFileSync(path.join(repo, "src-tauri/resources/adb/AdbWinApi.dll"), dll);
    fs.copyFileSync(path.join(repo, "src-tauri/resources/adb/AdbWinUsbApi.dll"), usbDll);
    const child = spawn(exe, [dll, usbDll, ready], { windowsHide: true, stdio: ["ignore", "ignore", "pipe"] });
    let spawnError;
    let stderr = "";
    child.once("error", error => { spawnError = error; });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", data => { stderr += data; });
    children.push(child);
    // A freshly compiled x86 .NET executable can take longer to start on a
    // busy Windows runner. Keep it alive until teardown so slow cleanup and
    // optional NSIS checks cannot make the foreign process exit on its own.
    await waitFor(() => {
      if (spawnError) throw spawnError;
      assert.equal(child.exitCode, null, `Fake ADB exited before loading DLLs: ${stderr}`);
      assert.equal(child.signalCode, null, "Fake ADB was terminated before loading DLLs");
      return fs.existsSync(ready);
    }, `fake ADB to load DLLs (${exe})`, 30_000);
    for (const file of [exe, dll, usbDll]) assert.throws(() => fs.writeFileSync(file, "locked"));
    return { child, exe, dll, usbDll };
  }
  const installDir = path.join(root, "Mash 安装 ' 目录");
  const owned = await startAdb(path.join(installDir, "resources/adb"));
  // Same executable name AND same directory prefix must not match another tool.
  const foreign = await startAdb(path.join(installDir, "resources/adb-other"));
  invoke(powershell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", cleanup, "-AdbPath", owned.exe.toUpperCase()]);
  await waitFor(() => owned.child.exitCode !== null || owned.child.signalCode !== null, "installed ADB to exit");
  for (const file of [owned.exe, owned.dll, owned.usbDll]) fs.writeFileSync(file, "replaced");
  for (const file of [foreign.exe, foreign.dll, foreign.usbDll]) assert.throws(() => fs.writeFileSync(file, "must remain locked"));
  // Fresh installs / already exited daemons are valid and leave foreign ADB alone.
  invoke(powershell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", cleanup, "-AdbPath", owned.exe]);
  assert.equal(foreign.child.exitCode, null);
  assert.equal(foreign.child.signalCode, null);

  const makensis = path.join(process.env.LOCALAPPDATA ?? "", "tauri/NSIS/makensis.exe");
  if (!fs.existsSync(makensis)) {
    t.diagnostic("NSIS cache unavailable; executable hook smoke check skipped");
    return;
  }
  // Compile the actual hook and run it in a tiny installer, without installing
  // Mash or accessing a device. This covers NSIS quoting and DLL replacement.
  const ownedAgain = await startAdb(path.join(installDir, "resources/adb"));
  const replacement = path.join(root, "replacement.dll");
  const installer = path.join(root, "installer.exe");
  const nsi = path.join(root, "fixture.nsi");
  fs.writeFileSync(replacement, "new DLL content");
  fs.writeFileSync(nsi, `\uFEFFUnicode true
Name "Mash ADB test"
OutFile "${installer}"
RequestExecutionLevel user
SilentInstall silent
InstallDir "${installDir}"
!define MAINBINARYNAME "mash-test"
!define PRODUCTNAME "Mash test"
!macro CheckIfAppIsRunning binary product
  ; No actual Mash process runs in this isolated fixture.
!macroend
!include "${hooks}"
Section
  !insertmacro NSIS_HOOK_PREINSTALL
  SetOutPath "$INSTDIR\\resources\\adb"
  File "/oname=AdbWinApi.dll" "${replacement}"
SectionEnd
`);
  invoke(makensis, ["/V2", nsi]);
  invoke(installer, ["/S"]);
  await waitFor(() => ownedAgain.child.exitCode !== null || ownedAgain.child.signalCode !== null, "installed ADB to exit after NSIS cleanup");
  assert.equal(fs.readFileSync(ownedAgain.dll, "utf8"), "new DLL content");
  assert.throws(() => fs.writeFileSync(foreign.dll, "must remain locked"));
});

test("cleanup failure is surfaced instead of reporting the DLL released", { skip: !windows }, () => {
  const script = `
function Get-Process {
  $fake = [pscustomobject]@{ Path = 'C:\\Mash\\resources\\adb\\adb.exe'; Id = 123; HasExited = $false }
  $fake | Add-Member ScriptMethod Kill { }
  $fake | Add-Member ScriptMethod WaitForExit { param($timeout) return $false }
  $fake | Add-Member ScriptMethod Dispose { }
  return $fake
}
& ${psLiteral(cleanup)} -AdbPath 'C:\\Mash\\resources\\adb\\adb.exe'
`;
  assert.throws(() => invoke(powershell, pwshArgs(script, repo)), /Timed out stopping bundled ADB/);
});
