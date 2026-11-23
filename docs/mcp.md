# Mash MCP 服务

在 Mash 的「设置 → 应用 → MCP 服务」打开「本地 MCP 服务」开关，再点击「复制 Codex 配置」。将配置粘贴到本机 Codex 的 `config.toml`，重载客户端后就可以使用 `mash` 工具。其他 MCP 客户端可以分别复制连接地址和访问令牌。

Mash 首次启动 MCP 服务时保存端口和访问令牌，之后重新启动服务或重开 Mash 都可复用同一份 Codex 配置。旧版临时配置需更新一次。配置中包含令牌，请作为凭据保存。

服务与 Mash 共用 Rust 后端，仅监听本机回环地址。若已保存的端口被占用，释放该端口后即可继续使用原地址。查询和编队保存可以在没有模拟器时使用。

要运行项目，在 Mash 开启「允许运行项目」。服务运行中也可切换开关，权限即时生效。每次启动沿用 Mash 当前的 ADB 设备选择规则。将游戏停在目标关卡的助战选择、编队确认或战斗画面。

客户端始终可以发现战斗工具。Mash 在收到启动请求时检查当前权限，关闭权限后会拒绝新的战斗启动请求。最近一次 MCP 战斗仍可通过 `runId` 查询，当前运行中的 MCP 战斗仍可停止。关闭服务后，已启动的战斗仍可在 Mash 停止。

## 工具

| 工具 | 用途 |
| --- | --- |
| `search_catalog` | 通过名称或 ID 分页查询从者、概念礼装和御主礼装。区服用于选择显示语言，查询结果不代表账号持有或该服已实装。 |
| `get_servant_skills` | 通过 `servantId` 和 `variantKey` 查询目标要求与技能选项。 |
| `list_projects`、`get_project` | 查看已保存的编队、助战和当前模式的分波指令。 |
| `validate_formation`、`create_formation` | 校验草稿、预览生成的指令，或者另存为新普通编队。 |
| `start_battle_run` | 按 `projectId` 运行已保存的普通或冠位战斗项目，返回 `runId`。沿用项目的重复次数、AP 回复（包括已配置的圣晶石）和停止条件。
| `get_battle_run` | 用 `runId` 查询最近一次 MCP 战斗的状态、结果、进度或编队检查报告。桌面启动其他战斗后仍可查询。结果保留到下一次 MCP 战斗启动或退出 Mash。 |
| `stop_battle_run` | 用 `runId` 请求停止当前 MCP 战斗。用 `get_battle_run` 确认结束状态。 |

## 草稿结构

先通过目录工具查出实际的 ID 和形态。槽位与技能编号从 1 开始，技能选项索引从 0 开始。下面的 `waves` 含一面一回合，先选择敌人，再优先使用 1 号位从者的宝具。

```json
{
  "name": "普通战斗编队",
  "servants": [{ "slot": 1, "servantId": 16 }],
  "support": { "slot": 3 },
  "waves": [
    [
      {
        "actions": [{ "type": "enemyTarget", "slot": 1 }],
        "attackPriority": [{ "slot": 1, "card": "np" }]
      }
    ]
  ]
}
```

可用行动为从者技能 `servantSkill`、御主礼装技能 `mysticCodeSkill` 与敌方选定 `enemyTarget`。缺少可选技能资料时，预览会列出无法确认的项目。编队检查暂停时，请在 Mash 查看报告并处理。

使用换人技能时，在草稿中设置对应的 `mysticCodeId`，并在 `mysticCodeSkill` 内传 `orderChange`。`frontSlot` 为前排 1–3 号位，`backSlot` 为后排 4–6 号位。技能编号可通过 `search_catalog` 查询。

```json
{"type":"mysticCodeSkill","skill":3,"orderChange":{"frontSlot":1,"backSlot":4}}
```

这个指令交换 1 号与 4 号位。后续技能和出卡会按已编排的换人及退场规则更新槽位，包括后续回合和波次。在这个例子里，后续指定 `slot: 1` 会引用刚换上来的从者。助战可以放在 1–6 号位，换人按槽位识别。用助战施放从者技能或把助战设为技能目标时，请填写 `support.servantId`。
