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
  - `assets.rs` + `assets/`：asset bundle manifest 与安装计划；`import.rs` 负责本地 ZIP 导入和共享目录安装，`status.rs` 负责安装状态与 self-check，`download.rs` 负责远端下载与校验。
  - `automation.rs` + `automation/`：共享 start/stop/status，以及 battle、servant enhancement、CE enhancement、friend-point summon 的 lifecycle command。
  - `catalog.rs` + `catalog/`：从者目录入口；`portraits.rs` 负责从者立绘/头像资源选择与偏好持久化，`craft_essences.rs` 负责 CE catalog 与卡面资源，`metadata.rs`、`skills.rs` 分别负责 metadata localization 和技能资料。
  - `debug.rs` + `debug/`：通用模板命令入口；`session.rs` 管理共享 sidecar，`capture.rs` 管理截图/视频流，`battle.rs`、`noble_phantasm.rs`、`support.rs`、`enhancement.rs` 按诊断领域分组。
  - `projects.rs` + `projects/`：项目 CRUD 与场景文件命令；`normalization.rs` 负责项目兼容迁移和约束归一化，`catalog.rs` 负责目录分组与排序，`ui_settings.rs` 负责应用 UI 偏好持久化，`config_transfer.rs` 负责配置 import/export。
  - `runtime.rs` + `runtime/resolution.rs`：CV runtime status/import/download 与 runtime resource resolution。
  - `settings.rs` + `settings/tests.rs`：app settings、startup migration、server selection、update-check settings 与对应测试。
- `runner/` 按 domain 拆分战斗自动化 state machine：
  - `engine.rs`、`state.rs`：tick routing 与权威流程状态。
  - `prebattle.rs`、`results.rs`、`ap_recovery.rs`：战前、结算和 AP 恢复流程。
  - `support.rs` + `support/`：助战筛选 facade；`class_filter.rs` 负责职阶策略，`class_filter_runtime.rs` 负责国服 EXTRA 弹窗交互，`craft_essence.rs` 负责礼装配置、区域计算和诊断，`craft_essence_runtime.rs` 负责礼装模板解析和设备校验，`requirements.rs` 负责等级、技能、星图门槛和诊断文本，`scrolling.rs` 负责滚动决策和诊断，`navigation_runtime.rs` 负责列表滚动、耗尽探针、刷新和旧版选择流程，`runtime.rs` 负责主选择编排。
  - `actions.rs` + `actions/helpers.rs`：技能/换人执行与纯坐标、分类 helper。
  - `attack.rs` + `attack/`：选卡入口；`critical.rs`、`noble_phantasm.rs`、`conditions.rs` 负责纯决策，`runtime.rs`、`selection_runtime.rs`、`advanced_runtime.rs` 负责设备交互流程。
  - `party.rs` + `party/`：阵容 identity 入口；`lineup.rs`、`resolution.rs`、`replay.rs`、`runtime.rs` 分别负责阵容变更、动作槽位解析、历史重放和运行时身份构建。
  - `grand.rs` + `grand/`：共享 Grand rule engine/strategy registry，以及按 class 划分的 strategy module。
  - `config.rs`、`coords.rs`、`runtime.rs`：runner 配置、归一化坐标和共享运行时 helper。
  - `tests.rs`：跨 runner 子模块的行为测试。
- `touch/` 存放底层 touch input construction。
- `screen.rs` 是 Python sidecar IPC facade；`screen/client.rs` 管理进程与请求生命周期，`screen/protocol.rs` 定义 JSON-line protocol，`screen/types.rs` 定义通过 `crate::screen` re-export 的 DTO，`screen/operations/` 按 battle、support、enhancement、template matching 拆分调用封装，`screen/tests.rs` 覆盖协议与 DTO。
- `enhancement_runner.rs` + `enhancement_runner/` 负责从者强化自动化及其 runtime helper。
- `craft_essence_enhancement_runner.rs` + `craft_essence_enhancement_runner/` 负责 CE 强化策略、材料选择、runtime 与测试。
- `friend_point_summon_runner.rs` 负责友情点召唤自动化。
- `models.rs` + `models/`：共享 serde DTO facade；`advanced.rs` 负责高级战斗条件、动作、规则和场景，`project.rs` 负责项目、队伍槽位、助战/Grand 配置、识别覆盖项和项目目录模型。
- `paths.rs` 存放 app-data/resource path resolution 与 migration helper。
- `server.rs` 存放 server enum 与 stream-resolution validation。
- 根 `tests.rs` 存放跨 module backend 测试；domain-specific 测试放在最近的 sibling `tests.rs` 或 local `#[cfg(test)]` module。
- `src-tauri/src/resources/` 存放通过 `include_str!` 编入 backend 的 catalog JSON；`src-tauri/resources/` 存放随应用分发的 runtime manifest、server CV 配置、模板、图片与 scrcpy server。两者不要混用。

## 新文件规则

- Feature 自有 React UI 放在 `src/features/<feature>/`，不要放到根 `src/components/`。
- 只有至少两个 feature 使用或抽象明显可复用时，才放入 `src/components/common/`。
- 测试放在最近的 sibling `__tests__/`。
- 新 Tauri command 放在 `src-tauri/src/commands/`；当某个 domain 已有同名子目录时，按现有职责放入该子模块，并由同名 `.rs` facade re-export/register。
- 自动化 state-machine 逻辑保留在 `runner/` 或 `enhancement_runner.rs`；状态或页面路由变化时同步更新 `docs/state-machines/`。
- 大文件拆分优先保留 facade：`foo.rs` 放共享类型、入口与 re-export，`foo/` 按业务职责拆分纯逻辑、runtime/device I/O 和测试。不要只按行数机械切文件。
