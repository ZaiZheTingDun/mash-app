# Windows 开发与构建

支持 Windows x64。NSIS 安装包按当前用户安装，内嵌 WebView2 bootstrapper；缺少 WebView2 时需要联网下载。

## 环境

- Rust stable，工具链 `x86_64-pc-windows-msvc`。
- Visual Studio C++ Build Tools 和 Windows SDK。
- Node.js 22、`package.json` 指定的 pnpm。
- PowerShell 5.1 或 7；运行 `pnpm test:scripts` 需要 PowerShell 7（`pwsh`）。
- 构建或测试 CV 时需要 Python 和 Poetry，CI 使用 Python 3.12。

Windows 脚本位于 `scripts/windows/`，不依赖 WSL 或 Git Bash。以下命令从仓库根目录执行。

## 开发与构建

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

生成本地未签名安装包：

```powershell
./scripts/windows/build-windows.ps1 -Unsigned
# 同时测试并构建 CV runtime 和内部 code 验证 ZIP（code 随 App 内置）
./scripts/windows/build-windows.ps1 -BuildCv -Unsigned
```

构建入口运行脚本测试、Rust 测试和前端检查。安装包输出到 `src-tauri/target/release/bundle/nsis/`，runtime ZIP 输出到 `sidecar/cv-runtime/dist/`，内部 code ZIP 输出到 `sidecar/cv-code/dist/`。

单独构建 CV：

```powershell
./scripts/windows/build-cv.ps1
./scripts/windows/build-cv.ps1 -Mode code
./scripts/windows/build-cv.ps1 -Mode runtime
# 使用已有依赖的 Python 环境
./scripts/windows/build-cv.ps1 -Python C:/Python312/python.exe
```

CV 使用原生 Windows Python 和`sidecar/cv-runtime/build_runtime.py` 打包，runtime 平台必须匹配构建主机。测试与协议见 [mash-cv](../sidecar/cv-code/README.md)。

## ADB 与安装

Windows ADB 及两份 DLL 位于 `src-tauri/resources/adb/`。需要重新获取时运行：

```powershell
./scripts/windows/prepare-windows-adb.ps1
```

脚本从固定版本的官方 ZIP 恢复文件并校验 SHA-256。NSIS 覆盖文件前会按完整路径停止安装目录内的 ADB，释放 DLL；无法停止时中止安装，不影响其他目录的 ADB。

## MuMu 应用保活与画面采集

开启应用保活后，MuMu 为游戏分配独立显示屏，默认显示屏 0 可能只有桌面。Mash 通过 MuMuManager 和设备启动 ID 匹配实例，再使用模拟器自带的 `external_renderer_ipc.dll` 按区服包名查询游戏显示屏。

视频流、截图和输入使用同一逻辑显示屏。自动化或调试无法找到目标游戏时会报错；设备选择页仍可预览桌面。每次启动任务重新查询显示屏，游戏重启后需重新启动任务。暂不支持 MuMu 应用分身。

需要配套 CV 代码支持；旧版无法确认非零显示屏时会提示更新。显示屏编号由设备动态返回，不应固定为某个值，也不能与 SurfaceFlinger 的物理显示屏编号混用。

### 分辨率排查

Mash 使用首张解码画面的像素尺寸校验分辨率。模拟器设置和 scrcpy 握手尺寸可能与实际游戏画面不同；启动日志中 `[mash-cv] stream` 的 `decoded` 是实际校验值。

如果设置为 1080p 或 2K 后仍提示 1280×720：

1. 重启 MuMu 实例和游戏，再重新启动 Mash 任务。
2. 临时关闭应用保活并重启，对比游戏独立显示屏的行为。
3. 反馈时附上完整报错、stream 日志、Mash / MuMu 版本和保活开关状态。

## 发布

命令用法、凭据和多平台发布顺序见[发布指南](distribution.md)。Windows 默认 updater channel 为 `windows-stable`，可用 `-Channel` 覆盖。正式 updater 必须设置 Tauri 签名私钥，`-Unsigned` 生成的包仅供手动安装。

另一平台先发布后，检出相同应用 tag 再补发 Windows；该 tag 的清单必须包含可用的 Windows CV 组件。不同平台串行发布，以保留同版本 updater 的已有平台条目。

## CI

| Workflow | 平台 | 检查 |
| --- | --- | --- |
| [Frontend](../.github/workflows/frontend.yml) | Ubuntu | ESLint、TypeScript、Vitest、Vite |
| [Rust](../.github/workflows/rust.yml) | Windows、macOS | Rust 与脚本测试 |
| [CV](../.github/workflows/cv.yml) | Windows、macOS | pytest；涉及打包输入时验证 ZIP 与冻结运行时 |

普通检查不发布安装包。需要 NSIS 产物时，手动运行 Rust workflow 并勾选 `build_installer`，从 Actions 下载构建结果。
