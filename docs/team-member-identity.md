# 队伍成员身份

队伍配置必须区分从者图鉴条目与项目中的具体成员实例。同一项目可以同时包含自有从者和助战从者，且两者拥有相同的 `servantId`；因此不能仅以 `servantId` 作为修改数据或在运行时解析 action 的安全键。

## 身份字段

| 字段 | 含义 | 稳定性与用途 |
| --- | --- | --- |
| `memberId` | 一个队伍成员实例的稳定身份，通常是 `ProjectSlot.id`，如 `slot-2`。 | 配置引用、更新、删除、排序、Order Change，以及阵容移动后解析 action 的主键。 |
| `servantId` | Atlas Academy/FGO 的从者图鉴 id。 | 用于选择从者资源和识别模板；它是描述性元数据与兼容 fallback，不是唯一的队伍成员键。 |
| `isSupport` | 该成员是否为借用的助战。 | 可区分具有相同 `servantId` 的自有和助战实例，并控制助战特有行为；单独使用并不唯一。 |
| `slotIndex` | 当前或配置中的、从 0 开始的项目位置。 | 阵容构建和旧版配置使用的位置元数据；拖拽或 Order Change 后可能变化，不能替代 `memberId`。 |

规范的持久化成员引用为：

```json
{
  "memberId": "slot-2",
  "servantId": 309,
  "isSupport": true
}
```

某些 action 结构会按角色为相同字段添加前缀，例如 `servantMemberId`、`targetMemberId`、`frontMemberId` 和 `backMemberId`；语义一致。

## 项目存储

`Project.slots` 管理六个稳定的 slot id 及其排列。普通 slot 将所选从者存放在 `ProjectSlot.servantId`。助战 slot 是特殊情况：

- `ProjectSlot.id` 仍是助战成员的 `memberId`。
- `ProjectSlot.type` 为 `support`。
- 选中的助战从者存放于 `Project.supportServantId`，而非 `ProjectSlot.servantId`。
- `RunConfig.supportMemberId` 与 `RunConfig.supportSlotIndex` 将助战成员的身份和位置传入 runner。

因此，读取项目成员的代码必须对助战 slot 从 `Project.supportServantId` 推导 `servantId`，对普通 slot 则从 `ProjectSlot.servantId` 推导。

普通与高级指令的后端读取、保存、运行加载和导出共用 `commands/projects/member_references.rs`：已有 `memberId` 时，从当前项目成员刷新从者与助战元数据，覆盖技能来源、目标、换人、出卡、条件和策略。更换同一槽位的从者后，旧指令仍附着于该成员；旧的描述性字段不能覆盖当前编队。未携带成员 ID 的历史指令继续使用既有 fallback，不仅凭图鉴号猜测成员。

空白出卡项（`card: null`）的 `memberId`、`servantId` 必须同时清空，`isSupport` 重置为 `false`。后端会清理历史存档中的残留引用。有效指令引用不存在的成员时，拒绝保存、运行加载或导出，并报告所在面、回合及指令位置。

导入分享码时，有效指令的元数据必须与分享码中的编队一致，冲突仍然报错。成员引用一致不代表更换后的技能效果、技能形态或宝具卡色适用于原策略，调整从者后仍需检查这些执行选项。

## 解析规则

Runner 将配置的 action 解析到当前阵容时，按以下顺序处理：

1. 匹配 `memberId`。
2. 对没有可用 `memberId` 的旧数据，匹配 (`servantId`, `isSupport`) 组合。
3. 对更早的位置型配置，回退至已存储的 `servant_1`…`servant_6` 选择，并把原始成员映射到其当前所在位置。

此顺序可确保项目重排或 Order Change 后，action 仍附着于同一成员。若被引用成员已离场，常规 fallback 与可用性规则将决定跳过该 action，或是否可使用原始位置。

指令卡识别有所不同：计算机视觉只能报告识别到的 `servantId` 及头像是否属于助战。需要对应的已配置成员实例时，runner 会将该组合与当前 `PartyMemberRuntime` 阵容结合。

## 工程规则

- 绝不能仅按 `servantId` 更新、删除、交换或重排队伍成员。
- 新引用应同时持久化 `memberId`、`servantId` 和 `isSupport`。
- 将 `memberId` 视为权威字段；其余字段用于校验、展示、识别和旧数据 fallback 元数据。
- slot 移动时保持 `memberId` 稳定；不要在拖拽时重新生成。
- 规范化旧数据时，先解析有效的既有 `memberId`，再使用 `slotIndex`，最后才以 (`servantId`, `isSupport`) 作为恢复路径。
- 涉及重复从者的测试应包含一个自有实例和一个相同 `servantId` 的助战实例。
