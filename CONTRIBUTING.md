# 参与贡献

欢迎修复 Bug、完善文档或提交新功能。较大的功能和架构调整请先开 Issue 讨论。

## 开发环境

- Node.js 22，pnpm 版本以 `package.json` 的 `packageManager` 为准。
- Rust stable。
- Windows：Visual Studio C++ Build Tools、Windows SDK、WebView2，详见 [Windows 开发](docs/windows.md)。
- macOS：Xcode Command Line Tools。
- 修改或测试图像识别组件时，需要 Python 3.11–3.14 和 Poetry；CI 使用 Python 3.12。

```bash
git clone https://github.com/ZaiZheTingDun/mash-app.git
cd mash-app
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` 仅启动前端，地址为 `http://localhost:1420`，不提供 Tauri 命令。

运行设备功能前，在应用中安装清单要求的 CV 运行时和资源。开发模式优先读取 `sidecar/cv-code/` 中的 Python 源码，仍需要已安装的原生运行时和 OCR 模型；仅运行离线测试无需连接设备。

## 代码约定

- Rust 管理业务状态、校验、默认值与持久化。React 通过 `invoke()` 读取状态和提交操作，本地 state 仅用于弹窗、输入草稿等临时交互。
- Rust 与 TypeScript 的 IPC 类型保持一致，JSON 字段使用 camelCase。
- 功能代码和测试放在所属模块附近，目录职责见[模块结构](docs/module-structure.md)。
- 修改自动化状态、页面路由或模板探针时，同步更新对应的[状态机文档](docs/README.md#自动化与识别)。
- 平台差异集中在 Rust 的 `src-tauri/src/platform/` 和 Python 的 `mash_cv/host/`，共享业务代码调用统一接口。

各层的详细约定见 [前端](src/AGENTS.md)、[Rust 后端](src-tauri/AGENTS.md)和 [Python sidecar](sidecar/cv-code/AGENTS.md)。

## 测试

根据改动运行对应检查：

| 改动范围 | 命令（从仓库根目录执行） |
| --- | --- |
| 前端 | `pnpm build` |
| Rust 后端 | `cargo test --manifest-path src-tauri/Cargo.toml` |
| 构建与发布脚本 | `pnpm test:scripts` |
| Python sidecar | 见下方命令 |

```bash
cd sidecar/cv-runtime
poetry install --no-root
poetry run pytest
```

`pnpm build` 依次运行 ESLint、TypeScript、Vitest 和 Vite 构建。开发时可用 `pnpm test` 或 `pnpm test:watch` 单独运行前端测试。Windows 上运行脚本测试还需要 PowerShell 7（`pwsh`）。

新行为和 Bug 修复应有回归测试。跨层协议或数据结构改动需运行前端、Rust 和 Python 三层测试。常规测试不需要 ADB 设备；纯文案、样式调整和已有测试覆盖的纯重命名可以不新增测试。只能通过设备验证的行为，请在 PR 中说明验证方式与限制；视觉调整无需连接实体设备。

## 构建

```bash
pnpm tauri build
```

Windows 本地未签名安装包使用：

```powershell
./scripts/windows/build-windows.ps1 -Unsigned
```

CV 业务代码随 App 打包，原生 runtime 独立发布，见 [mash-cv](sidecar/cv-code/README.md#build)。维护者发布流程见[发布指南](docs/distribution.md)。

## 提交 Pull Request

1. 围绕一个问题提交改动，说明原有问题和修改后的行为。
2. 运行相关检查，在 PR 中列出结果和未验证项；界面变更附截图。
3. 行为变化同步更新文档。不要提交凭据、个人配置或包含隐私的截图和日志。

安全漏洞请按[安全政策](SECURITY.md)私下报告。
