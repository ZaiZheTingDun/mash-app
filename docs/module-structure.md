# 模块结构

## 前端：`src/`

| 目录 / 文件 | 职责 |
| --- | --- |
| `main.tsx` | React 入口、主题和全局样式 |
| `App.tsx` | 项目加载、页面路由、更新检查与自动化事件 |
| `features/` | 按业务组织界面：队伍、战斗、高级策略、调试、设置、强化等 |
| `components/common/` | 多个功能共用的 UI 组件和展示辅助函数 |
| `styles/` | 全局 CSS，按功能拆分；较大模块可用子目录和 `index.css` 聚合 |
| `types/` | 与 Rust serde DTO 对应的 TypeScript 类型 |
| `test/` | Vitest 配置和共享渲染辅助函数 |

功能测试放在相邻的 `__tests__/`，应用级测试放在 `src/__tests__/`。业务状态、校验和持久化由后端负责；前端只处理展示与临时交互状态。

## 后端：`src-tauri/src/`

| 目录 / 文件 | 职责 |
| --- | --- |
| `main.rs` / `lib.rs` | 程序入口、Tauri 插件、共享状态、菜单与命令注册 |
| `commands/` | 设备、资源、自动化、目录、调试、项目、运行时和设置命令 |
| `runner/` | 战斗状态机及技能、选卡、阵容、助战、冠位策略 |
| `enhancement_runner/` | 从者强化 |
| `craft_essence_enhancement_runner/` | 礼装强化策略与材料选择 |
| `friend_point_summon_runner.rs` | 友情点召唤 |
| `screen/` | Python 子进程、JSON-line 协议、CV 类型和识别操作 |
| `models/` | 项目、队伍、高级战斗规则等共享数据结构 |
| `adb.rs` / `touch/` | ADB 通信和底层输入操作 |
| `platform/` | 操作系统适配 |
| `paths.rs` / `server.rs` | 路径与迁移、区服与分辨率校验 |
| `resources/` | 通过 `include_str!` 编入程序的目录数据 |

`src-tauri/resources/` 则是随应用打包的 CV 配置、模板、图片、运行时清单和 scrcpy server，详见[资源说明](../src-tauri/resources/README.md)。

## 图像识别：`sidecar/cv-code/`

Python 子进程负责 OpenCV / OCR 识别和 scrcpy 视频流处理，通过标准输入输出与 Rust 通信。业务代码、测试和训练工具位于 `cv-code/`；依赖锁文件、OCR 模型和原生构建器位于 `sidecar/cv-runtime/`。环境、测试和打包见 [mash-cv](../sidecar/cv-code/README.md)。

## 扩展约定

- React 功能代码放入 `features/<feature>/`；至少两个功能共用或明显可复用的组件再放入 `components/common/`。沿用全局 CSS。
- Tauri 命令放入 `commands/` 对应领域；自动化流程放入对应 runner，状态和页面路由变化同步更新[状态机文档](README.md#自动化与识别)。
- 大模块使用 `foo.rs` 保留入口、共享类型与 re-export，`foo/` 按职责拆分纯逻辑、设备 I/O 和测试。
- Rust 测试放在最近的同级 `tests.rs` 或本地 `#[cfg(test)]` 模块；跨模块测试放在所属领域的汇总测试模块。
- Rust 与 Python 的平台差异分别收敛到 `platform/` 和 `mash_cv/host/`；构建脚本按平台放入 `scripts/{macos,windows}/`，共享工具放入 `scripts/shared/`。
