# Windows 构建、验证与发布

首批支持 Windows x64。应用使用当前用户安装的 NSIS 安装包，并内嵌
WebView2 bootstrapper；缺少 WebView2 时，安装仍需联网下载运行时。

## 开发依赖

- Rust stable，使用 `x86_64-pc-windows-msvc` 工具链。
- Visual Studio C++ Build Tools 和 Windows SDK。
- Node.js、`package.json` 指定的 pnpm。
- Python 3.12、Poetry（构建或测试 CV 时需要）。
- 发布到 R2 还需要 AWS CLI、R2 凭据和 Tauri updater 私钥。

脚本按平台放在 `scripts/windows/` 与 `scripts/macos/`，共享 Node helpers
位于 `scripts/shared/`。Windows 使用 PowerShell 5.1 或 7；所有 Windows
入口均不依赖 WSL、Git Bash、`zip` 或 `shasum`。

Windows CI 及 `pnpm test:scripts` 的 Windows 子进程统一使用 PowerShell 7
（`pwsh`），本地运行这组测试时也需要将 `pwsh` 加入 PATH。发布脚本测试
保留含中文和空格的临时目录，验证脚本路径与工作目录能够正确传递。
测试通过 `pwsh -EncodedCommand` 传递命令，并在子进程内显式设置工作目录，
让含中文、空格和引号的路径按字面值传入脚本。fixture 使用 JS 递归复制，
避开 Node 22 的 Windows 中文目录复制问题。共享 Node CLI 和路径断言使用
原生真实路径解析，兼容 Windows 长短路径及目录别名。

| Windows 入口                | 用途                                                                           |
| --------------------------- | ------------------------------------------------------------------------------ |
| `release.ps1`               | 默认只读计划，`-Publish` 执行完整发布                                          |
| `bump-app-version.ps1`      | 更新 App 四处版本、commit 和 app tag                                           |
| `bump-cv-code-version.ps1`  | 更新 CV code 版本、commit 和 code tag                                          |
| `build-cv.ps1`              | 构建全部 CV ZIP，`-Mode code` / `runtime` 单独构建                             |
| `release-cv-code.ps1`       | 上传 code ZIP、验证 R2/CDN，打印 URL 与 SHA                                    |
| `release-cv-runtime.ps1`    | 上传 runtime、更新清单、commit 和平台 tag                                      |
| `build-windows.ps1`         | 检查并构建 NSIS，`-Unsigned` 用于本地验证                                      |
| `release-tauri-updater.ps1` | 构建签名 NSIS 并合并、发布 updater manifest                                    |
| `release-installer.ps1`     | 发布版本化和 latest 的直接安装包下载；`-Unsigned` 发布手动安装包，不改 updater |
| `prepare-windows-adb.ps1`   | 获取并校验固定版本 Windows ADB                                                 |
| `generate-icons.ps1`        | 使用 Tauri CLI 从源图片生成应用图标                                            |

上述路径均相对于 `scripts/windows/`。脚本内部定位仓库根目录，调用方
无需先 `cd`；下方示例从仓库根目录执行。

ADB 36.0.2 的 Windows 可执行文件和两份 DLL 已包含在
`src-tauri/resources/adb/`。`scripts/windows/prepare-windows-adb.ps1` 可以从 Google
官方固定版本 ZIP 重建这些文件，并校验 ZIP 的 SHA-256。

## 本地构建

```powershell
pnpm install --frozen-lockfile
# 运行 Rust、发布脚本及前端检查，生成本地验证用的未签名安装包
./scripts/windows/build-windows.ps1 -Unsigned
# 同时运行 CV 测试并构建 native runtime 与 code ZIP
./scripts/windows/build-windows.ps1 -BuildCv -Unsigned
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

也可从仓库根目录使用平台入口：

```powershell
./scripts/windows/build-cv.ps1
./scripts/windows/build-cv.ps1 -Mode code
./scripts/windows/build-cv.ps1 -Mode runtime
# 使用已装好依赖的 Python，无需 Poetry
./scripts/windows/build-cv.ps1 -Python C:/Python312/python.exe
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
$env:AWS_PROFILE = 'mash' # 可省略，届时使用 AWS CLI 默认凭据链
./scripts/windows/release-tauri-updater.ps1
```

如果发布变量由 mise 管理，可使用
`mise exec -- powershell -NoProfile -File scripts/windows/release.ps1 ...`。
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 按私钥的实际密码设置。
`R2_PREFIX` 默认 `mash`，Windows 的 `RELEASE_CHANNEL` 默认 `windows-stable`，updater 与编排的
`-Channel <name>` 可覆盖 channel。发布脚本不输出私钥或凭据内容。

完整发布包含测试、日志定稿、CV 上传与清单更新、App 版本、updater、可选
安装包下载和 Git 推送。Windows updater 使用独立的 `windows-stable` channel；macOS 保持 `stable`。
版本号均为示例，实际值必须满足发布检查：

```powershell
# 只读计划：允许工作区有未提交改动，不写文件、不提交、不上传
pnpm release:windows 0.16.0 -CvCode 0.4.27 -CvRuntime 0.3.0 -Installer
# 完整发布：先提交当前改动并在 CHANGELOG.md 填好“未发布”内容
pnpm release:windows 0.16.0 -CvCode 0.4.27 -CvRuntime 0.3.0 -Installer -Push -Publish
# 仅发布 App，前提是没有尚未发布的 CV code/runtime 改动
./scripts/windows/release.ps1 0.16.0 -Publish
```

`-CvCode` / `-CvRuntime` 只在需要发布对应 artifact 时传入；新版本检测到
相关改动但没有版本号会停止实际发布。`-SkipTests` 跳过编排前置测试，
updater 构建仍会运行 `build-windows.ps1` 的必要检查。
省略 `-Push` 会保留本地 commit/tag，省略 `-Installer` 只发布 updater。

也可分步发布：

```powershell
./scripts/windows/release-cv-runtime.ps1 0.3.0
./scripts/windows/bump-cv-code-version.ps1 0.4.27
./scripts/windows/release-cv-code.ps1 0.4.27
# 分步 code 发布后，将打印的 URL/SHA 和 code 版本写入 runtime-manifest.json 并提交
./scripts/windows/bump-app-version.ps1 0.16.0
./scripts/windows/release-tauri-updater.ps1
./scripts/windows/release-installer.ps1 -SkipBuild
# 没有 updater 私钥时，构建并发布未签名的手动安装包；不更新 stable/latest.json
./scripts/windows/release-installer.ps1 -Unsigned
```

Windows runtime 使用 `[mash_cv.runtime].windows-x86_64`（或对应架构）的独立版本，
发布时只更新清单中的该平台条目，不影响 macOS。完整新版本编排要求
runtime 版本相对目标平台递增；CV code 版本仍跨平台共用。
另一平台先发布 App 后，检出同一个 `vX.Y.Z` tag 再运行
`./scripts/windows/release.ps1 X.Y.Z -Installer -Publish`，即可补发 Windows；
此模式不更改已有日志、版本或 CV 清单，所以该 tag 的清单必须已包含可下载
的 Windows runtime/code artifact。

直接下载地址为 `mash/downloads/versions/Mash_<version>_x64-setup.exe` 和
`mash/downloads/latest/Mash_x64-setup.exe`，独立于 updater manifest。

发布脚本要求干净工作区和准确的 `vX.Y.Z` tag。不同平台必须串行发布：
脚本合并同版本已有平台，拒绝覆盖更高版本的 channel，以及已登记的不同
签名 artifact。新版本不会继承上一版本的安装包。macOS 的 Bash updater
入口使用相同合并逻辑。CI 已拆分为前端、Rust、CV 三个 workflow；普通检查
不会构建 App 安装包。需要 NSIS 时，手动运行 Rust tests 并勾选
`build_installer`，构建产物保存在 Actions，不会向 CDN 发布。

## CI 测试

| Workflow       | 平台           | 内容与触发范围                                                                                         |
| -------------- | -------------- | ------------------------------------------------------------------------------------------------------ |
| `frontend.yml` | Ubuntu         | 前端源码、配置或依赖变更：ESLint、TypeScript、Vitest、Vite 构建                                        |
| `rust.yml`     | Windows、macOS | Rust/Tauri 变更：Cargo 测试；发布脚本、Tauri 资源、前端依赖及发布 skill 变更：独立的脚本测试           |
| `cv.yml`       | Windows、macOS | CV 源码、测试、依赖、模板或构建输入变更：pytest；影响运行时/code ZIP 时额外执行 Windows 打包及集成测试 |
| `windows-release.yml` | Windows、Ubuntu | 手动发布：锁定 App tag，构建并签名一次 NSIS，安装/启动 smoke，复用候选包发布 direct download 与 Windows updater |
