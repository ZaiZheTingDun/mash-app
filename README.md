# Mash

基于 Tauri、Rust 和 React 的《Fate/Grand Order》自动化工具，通过 ADB 连接 Android 设备或模拟器，使用图像识别执行操作。

[下载安装](https://docs.mash.xiaotongx.com/start/install/) · [使用文档](https://docs.mash.xiaotongx.com/) · [开发文档](docs/README.md) · [参与贡献](CONTRIBUTING.md)

## 功能

- 自动战斗：按回合、战斗阶段和自定义条件释放技能、使用宝具、选择指令卡。
- 队伍与助战：管理从者和礼装配置，按条件筛选助战。
- 自动强化：从者强化、概念礼装强化与搓丸子。
- 友情点召唤：自动执行友情点抽取。

## 安装与使用

支持 Windows x64 和 macOS Apple Silicon。前往[安装指南](https://docs.mash.xiaotongx.com/start/install/)下载对应版本。

1. 启动 Android 设备或模拟器，开启 ADB 连接并运行 FGO。
2. 在 Mash 中连接设备，按提示安装 CV 运行时和资源。
3. 配置队伍与自动化任务，开始运行。

## 本地开发

需要 Node.js 22、Rust stable、`package.json` 指定的 pnpm，以及当前平台的 Tauri 构建依赖。环境配置与测试方式见[贡献指南](CONTRIBUTING.md)。

```bash
git clone https://github.com/ZaiZheTingDun/mash-app.git
cd mash-app
pnpm install --frozen-lockfile
pnpm tauri dev
```

仅预览前端可运行 `pnpm dev`；设备连接和自动化需要 Tauri 后端及已安装的 CV 运行时。

## 项目结构

```text
src/                  React 界面
src-tauri/            Rust 后端、自动化引擎与应用资源
sidecar/cv-code/      随 App 打包的 Python 图像识别与视频流处理
sidecar/cv-runtime/   原生依赖、OCR 模型、启动器与独立 runtime 构建
scripts/              构建与发布脚本
docs/                 开发与设计文档
```

Rust 管理应用状态与自动化决策，React 负责展示和交互；Python 子进程通过 scrcpy / PyAV 获取画面，使用 OpenCV 和 OCR 完成识别。详见[模块结构](docs/module-structure.md)。

## 贡献与反馈

欢迎提交 [Issue](https://github.com/ZaiZheTingDun/mash-app/issues) 和 Pull Request。提交问题时请附上 Mash 版本、操作系统、区服和复现步骤；较大的改动请先开 Issue 讨论。开发流程见[贡献指南](CONTRIBUTING.md)，安全漏洞请按[安全政策](SECURITY.md)私下报告。

## 许可证

原创源代码及文档采用 [MIT License](LICENSE)。第三方软件、模型和游戏资源的授权见 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES)。

Mash 是非官方项目，与 FGO 的开发商、发行商及相关权利方无关联。使用前请了解游戏服务条款及自动化带来的账号风险。
