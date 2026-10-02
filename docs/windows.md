# Windows 构建与验证

首批支持 Windows x64。应用使用当前用户安装的 NSIS 安装包，并内嵌
WebView2 bootstrapper；缺少 WebView2 时，安装仍需联网下载运行时。

## 开发依赖

- Rust stable，使用 `x86_64-pc-windows-msvc` 工具链。
- Visual Studio C++ Build Tools 和 Windows SDK。
- Node.js、`package.json` 指定的 pnpm。
- Python 3.12、Poetry（构建或测试 CV 时需要）。

ADB 36.0.2 的 Windows 可执行文件和两份 DLL 已包含在
`src-tauri/resources/adb/`。`scripts/prepare-windows-adb.ps1` 可以从 Google
官方固定版本 ZIP 重建这些文件，并校验 ZIP 的 SHA-256。

## 本地构建

```powershell
pnpm install --frozen-lockfile
# 运行 Rust、发布脚本及前端检查，生成本地验证用的未签名安装包
./scripts/build-windows.ps1 -Unsigned
# 同时运行 CV 测试并构建 native runtime 与 code ZIP
./scripts/build-windows.ps1 -BuildCv -Unsigned
```

安装包位于 `src-tauri/target/release/bundle/nsis/`。
CV ZIP 位于 `sidecar/mash_cv/dist/`。也可以独立构建 CV：

```powershell
cd sidecar/mash_cv
poetry install --no-root
poetry run pytest
poetry run python build_sidecar.py
poetry run pytest tests/test_windows_runtime.py
```

`build_sidecar.py` 在原生 Windows Python 环境中调用 PyInstaller，不依赖
WSL、Git Bash、`zip` 或 `shasum`。代码 ZIP 使用固定时间戳和文件权限，
相同源文件在不同主机上生成相同校验值。runtime ZIP 仍保留 native 文件
权限和 macOS framework 符号链接；Bash 构建入口调用同一个 Python builder。

离线冻结运行时测试覆盖启动、中文路径模板加载及真实 OCR worker 启动。
ADB/scrcpy 的设备连接和实战自动化仍需设备验证。

## 运行时与发布

运行时的 OS 差异集中在两个边界：Rust 的 `src-tauri/src/platform/`
负责进程、文件系统、原生菜单和 artifact 命名；Python 的 `mash_cv/host/`
负责文件/管道 I/O 和子进程环境。共享命令、runner、CV 和流处理只调用统一
接口。新增平台优先扩展适配层，避免在共享代码中增加 `cfg` 或 `sys.platform`
判断。构建及发布脚本可以按目标平台选择产物；测试中的平台 fixture 和
`main.rs` 的 Windows 子系统声明是边界检查的明确例外。

`pnpm test:scripts` 检查 Rust 边界，pytest 的 `test_host_boundary.py` 检查
Python 边界。平台行为由符号链接/权限、可执行文件名、中文路径、子进程
环境和冻结运行时测试覆盖。

`runtime-manifest.json` 的 `windows-x86_64` 条目必须与实际 ZIP 完全匹配。
新增 Windows artifact 与 CV code 0.4.26 应先上传到清单中的 CDN 地址，
再分发引用它们的应用。发布前验证下载文件的 SHA-256；开发阶段可以在
资源管理中依次导入匹配清单的本地 runtime 与 code ZIP。

正式 updater 构建必须设置 `TAURI_SIGNING_PRIVATE_KEY`；`-Unsigned` 仅用于
本地构建验证，不生成 updater 签名。Windows Authenticode 签名与 Tauri
updater 签名是两个独立机制；如需 Authenticode，另外配置 Tauri Windows
签名设置与证书。

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = '私钥路径'
$env:R2_ENDPOINT = 'R2 endpoint'
$env:R2_BUCKET = 'mash'
$env:RELEASE_BASE_URL = 'https://mash.xiaotongx.com'
./scripts/release-tauri-updater.ps1
```

发布脚本要求干净工作区和准确的 `vX.Y.Z` tag。不同平台必须串行发布：
脚本合并同版本已有平台，拒绝覆盖更高版本的 channel，以及已登记的不同
签名 artifact。新版本不会继承上一版本的安装包。macOS 的 Bash updater
入口使用相同合并逻辑。CI 的 Windows workflow 只验证和保存构建产物，
不会向 CDN 发布。
