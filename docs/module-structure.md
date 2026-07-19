# 模块结构

本文说明 backend 与 frontend 拆分后的当前源码布局。

## Frontend

Frontend 代码位于 `src/`。

- `main.tsx` 是 browser/Tauri entrypoint，并导入 `styles/index.css`。
- `App.tsx` 负责顶层编排：项目加载、页面路由、更新检查、自动化事件和跨 feature wiring。
- `components/common/` 存放多个 feature 共用的 UI primitive 与小型 helper：`BattleActorIcon`、`OptionCardRadioGroup`、`SectionHeading`、`ThresholdLevelPicker`、`battleActorLabels`。
- `features/team/` 负责队伍设置、助战要求、从者/CE 选择、队员 helper 与测试。
- `features/battle/` 负责 legacy battle-scene 编辑、指令编辑、battle page shell 与测试。
- `features/advanced/` 负责高级 battle-scene、Grand card strategy、backend-defined Grand class/role helper、Grand rule slot helper 与测试。
- `features/debug/` 负责 debug page、canvas、popout window、debug DTO/helper 与测试。
- `features/settings/` 负责设置页面、runtime/assets/self-check 控件与测试。
- `features/setup/`、`features/enhancement/`、`features/status/`、`features/projects/` 分别负责对应页面、component 与测试。
- `styles/` 包含全局样式入口和按 feature 划分的 CSS。CSS 按设计保持 global；不要引入 CSS Modules 或 styled-components。大型 feature 可像 `styles/team/` 一样使用带 `index.css` aggregator 的子目录。
- `types/` 存放必须与 Rust serde DTO 保持一致的 frontend interface。
- `test/` 存放共享 Vitest setup 与 render helper。

测试放在所属 feature 旁的 `__tests__/` 目录；共享 app-level 测试保留在 `src/__tests__/`。

## Backend

Backend 代码位于 `src-tauri/src/`。

- `main.rs` 是精简的 binary entrypoint。
- `lib.rs` 负责 Tauri builder：plugin registration、managed state、menu 与 command registration。
- `commands/` 存放 Tauri command 及相关 helper：
  - `adb.rs`：ADB status/reset/screenshot。
  - `assets.rs`：asset bundle status/import/download/self-check。
  - `automation.rs`：战斗/强化自动化 start/stop/status 与 sidecar startup。
  - `catalog.rs`：从者/CE catalog、asset lookup 与 metadata localization。
  - `debug.rs`：debug page command 与共享 debug sidecar state。
  - `projects.rs`：项目 CRUD 与配置 import/export。
  - `runtime.rs`：CV runtime status/import/download 与 runtime resource resolution。
  - `settings.rs`：app settings、startup migration、server selection 与 update-check settings。
- `runner/` 按 domain 拆分战斗自动化 state machine：config、coordinates、state、support、AP recovery、party mutation、attack selection、runtime helper、prebattle routing、result handling 与测试。`runner/grand.rs` 是共享 Grand rule engine/strategy registry；`runner/grand/` 每个 Grand class 一个 strategy module。
- `touch/` 存放底层 touch input construction。
- `screen.rs` 负责 Python sidecar client/IPC；`screen/types.rs` 定义通过 `crate::screen` re-export 的 screen/CV DTO。
- `enhancement_runner.rs` 负责强化自动化。
- `models.rs` 存放 command 与 frontend IPC 共用的 serde DTO。
- `paths.rs` 存放 app-data/resource path resolution 与 migration helper。
- `server.rs` 存放 server enum 与 stream-resolution validation。
- `tests.rs` 存放跨 module backend 测试；domain-specific 测试应尽量与 module 放在一起。
- `resources/` 存放 backend 使用的 embedded catalog JSON。

## 新文件规则

- Feature 自有 React UI 放在 `src/features/<feature>/`，不要放到根 `src/components/`。
- 只有至少两个 feature 使用或抽象明显可复用时，才放入 `src/components/common/`。
- 测试放在最近的 sibling `__tests__/`。
- 新 Tauri command 放在 `src-tauri/src/commands/`，纯 domain 内部逻辑除外。
- 自动化 state-machine 逻辑保留在 `runner/` 或 `enhancement_runner.rs`；状态或页面路由变化时同步更新 `docs/state-machines/`。
