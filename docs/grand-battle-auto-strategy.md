# 冠位战自动策略

本文说明项目启用 advanced mode 并配置 `grandServants` 时使用的自动指令卡策略。

## 输入

- 每个已配置从者均携带由职阶定义的 `role`。Saber 使用 `main` / `deputy`；Lancer 使用 `single` / `aoe`。
- 旧版 Lancer `lancerRole` 会在项目规范化时迁移至 `role`，且不会回写。
- 用户在 advanced command editor 的主输出区域配置这些 role，而非在队伍阵容页面配置。
- 每个 Grand servant 存储：
  - `slotIndex`：项目队伍 slot，解析为当前前排从者 id。
  - `npCard`：`auto`、`buster`、`arts` 或 `quick`。
  - `priority`：`damage` 或 `np`。
- 已就绪的 Noble Phantasm 与已识别的指令卡会一并评分。
- 手写的 advanced `rules` 始终优先；本策略仅用于自动 advanced 流程。
- `grandClass` 选择自动规则集；缺失的旧版值默认 `saber`。
- 项目级 `grandCardStrategy.chainPriority` 列表可以重排 Saber 自动规则顺序；Berserker 使用固定顺序。
- 当 `grandCardStrategy.customRules` 非空时，先尝试用户规则，再尝试按职阶内置规则；内置 Saber/Berserker 规则始终保留为 fallback。

若 `npCard` 为 `auto`，runner 使用从者资源中的 `noblePhantasmCard` 值。若已配置的 Grand servant 当前不在前排，该 role 在本回合会被忽略。

## 策略模块

通用的 candidate builder、custom-rule matcher 与评分引擎位于 `runner/grand.rs`。职阶专属元数据与内置行为通过 `GrandClassStrategy` 接口，位于 `runner/grand/saber.rs`、`lancer.rs` 与 `berserker.rs`。同一个已注册策略还会提供项目规范化、启动校验、运行时 role 顺序、自动 Order Change 目标，以及 `get_grand_class_definitions` UI payload。

若要新增职阶，请添加 `GrandClass` enum 值、实现一个策略模块，并将其注册到 `grand_class_strategies`。React 会根据返回的 definition 渲染职阶选项、role slot、助战筛选、校验信息与设置，无需按职阶分支。

## 启动时的 Order Change

若一个场景没有生效的指令卡启动条件，runner 通常会直接在 Battle 画面执行第一个 control action 和 `startupActions`，然后仅在实际攻击时打开一次指令卡画面。这样避免仅为立刻返回而打开卡片页面。

当主 Grand servant 配置在后排 slot 时，advanced 场景可将 `grandAutoOrderChange` 设为启动条件。该设置按场景生效，不改变已配置的 control action 或 startup action。此时会禁用直接启动优化，因为 runner 仍需先进入一次指令卡画面，统计卡片归属并选择要换下的前排从者。

在该场景第一次进入指令卡画面时，runner 会：

1. 统计当前每个前排从者拥有的已识别指令卡数量。
2. 选择数量最多的前排从者；并列时选择最左侧从者。
3. 使用 Mystic Code `skill_3`，让该前排从者与后排主 Grand servant 执行 Order Change。
4. 重新读取指令卡画面，将启动视为已满足，然后执行场景配置的 startup action。

针对后排主 Grand servant 配置的 startup action，会在运行时按从者身份解析，再改写为该从者当前的前排位置。针对已被换出前排从者的 action 将被跳过，不会错误地作用于该位置的新成员。

若主 Grand servant 已在前排、无法定位，或无法解析换位目标，runner 会跳过自动换位而不中断战斗循环。

## 规则模型

Grand 自动选卡基于规则。每条规则恰有三个 slot，slot 顺序即点击顺序。候选组合必须满足 slot 约束及规则级约束：

- Owner：主 Grand、副 Grand、任意 Grand 或任意从者。
- Kind：指令卡、Noble Phantasm 或两者皆可。
- Color：精确 B/A/Q、任意颜色，或 Grand role 配置的 NP 颜色。
- `sameColor`：选出的三次攻击颜色相同。
- `colorSetBAQ`：三次攻击恰含一张 buster、一张 arts 和一张 quick，即「极致」chain。
- `include` / `exclude`：三卡组合中必须包含或不得包含的攻击。

多组组合匹配同一规则时，picker 依次偏好含更多目标 role 攻击、更多主／副 Grand 攻击、更多 NP，以及原始卡序更靠前的组合。规则 slot 允许多个有效攻击顺序时，尽可能将非 Grand 指令卡置于 Grand servant 指令卡之前。

## 用户自定义规则

用户 custom rule 与内置规则共用三 slot matcher。每条规则恰有三个 slot，每个 slot 可绑定到配置队伍中的具体从者 id，也可绑定任意已配置 Grand servant：

- Servant：拥有选定攻击的精确 `servantId`，或 `grandServant: true` 表示任意 Grand servant。
- Kind：任意攻击、仅指令卡或 Noble Phantasm。
- Color：任意颜色、buster、arts 或 quick。NP slot 不使用颜色作运行时匹配。

使用 `grandServant: true` 的 slot 还携带 Berserker 宽泛 slot 所用的隐藏 owner priority：主 Grand 攻击优先，其次是副 Grand 攻击。

UI 中的行顺序即匹配顺序。无效或不完整的 custom rule 会在运行时忽略，picker 将继续尝试下一条 custom rule 或内置 fallback rule。

## Saber 规则

Saber mode 保留用户可配置的 `grandCardStrategy.chainPriority` 顺序。每个 priority item 会展开为规则模板：

1. 含 NP 的主极致 brave chain：主指令卡、主指令卡、主 NP；规则级颜色集合必须为 B/A/Q。
2. 不含 NP 的主极致 brave chain：主 buster 指令卡、主 arts 指令卡、主 quick 指令卡；排除主 NP。
3. 主已就绪 NP：任意攻击、任意攻击、主 NP。
4. 含 NP 的副极致 brave chain：副指令卡、副指令卡、副 NP；规则级颜色集合必须为 B/A/Q。
5. 不含 NP 的副极致 brave chain：副 buster 指令卡、副 arts 指令卡、副 quick 指令卡；排除副 NP。
6. 主同色 chain：任意三次同色攻击，且至少包含一次主 Grand 攻击。
7. 副同色 chain：任意三次同色攻击，且至少包含一次副 Grand 攻击。
8. Fallback：任意三次攻击。

默认 Saber priority 顺序为：主极致 brave chain、主已就绪 NP、副极致 brave chain、主同色 chain、副同色 chain、最后 fallback。

## Berserker 规则

Berserker mode 使用固定顺序。

对每个 owner 为「任意从者」的内置 Berserker slot，匹配组合依次偏好主 Grand 攻击、副 Grand 攻击和非 Grand 攻击。这一偏好配置在规则 slot 上，而非硬编码于通用 matcher。

1. 主 Grand NP 同色 chain：匹配主 NP 颜色的指令卡、匹配主 NP 颜色的指令卡、主 NP。第二个指令卡 slot 可将就绪且 NP 颜色同样匹配主 NP 颜色的副 Grand NP 作为等同指令卡的攻击接受，并优先将该副 NP 放在第二次攻击。
2. 主 Grand NP：任意攻击、任意攻击、主 NP。第二个自由 slot 优先就绪的副 Grand NP；可用时点击顺序为普通／自由攻击、副 NP、主 NP。
3. 副 Grand NP 同色 chain：匹配副 NP 颜色的指令卡、匹配副 NP 颜色的指令卡、副 NP。
4. 主 Grand 其他同色 chain：任意三次同色攻击，包含主 Grand servant，但排除主 Grand NP。
5. 副 Grand 其他同色 chain：任意三次同色攻击，包含副 Grand servant，但排除副 Grand NP。
6. Grand 极致 chain：buster、arts、quick，且至少包含一次 Grand servant 攻击。
7. Fallback：任意三次攻击。

## Lancer 规则

Lancer mode 同样使用共享的 `GrandCardRule` matcher。固定规则顺序为双 NP 极致 chain、双 NP 同色 chain、双 NP fallback、单体 NP、AoE NP，最后为三张指令卡。双 NP 规则按 AoE NP、单体 NP、填充卡的顺序点击。指令卡 slot 采用可复用的主／副／其他、随后 Arts/Quick/Buster 的 candidate priority，从而保持旧版 Lancer 填充卡顺序。非 Grand NP 永不满足这些指令卡 slot。

## 非 Grand 行为

未配置 `grandServants` 时，runner 保留基于 `mainOutput`、输出类型和 NP 颜色的旧版 advanced 自动评分逻辑。
