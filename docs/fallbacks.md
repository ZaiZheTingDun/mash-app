# Fallback 注册表

本文记录仅为兼容不确定截图、旧数据或缺失模板而存在的 fallback 路径。当 primary path 被证明稳定后，应同时删除此处记录与代码中的 fallback。

## 助战选择

| 范围 | Primary path | Fallback path | 移除条件 |
| --- | --- | --- | --- |
| 助战行 anchor | 在右侧助战确认列中匹配 `button_support_form_confirm.png`。 | 通过矩形轮廓识别同一按钮。 | Grand 与普通助战的真实截图在目标分辨率和 server variant 下均能稳定匹配按钮模板。 |
| Grand 助战 CE 区域 | 根据可见确认按钮顶部推导三个 CE 搜索框。 | 只有右侧 panel anchor 时，仅为 skill OCR/debug 从 panel 顶部推导 CE 区域；runner 的 Grand CE 筛选仍要求确认按钮可见。 | 确认按钮模板能覆盖所有可选行。 |
| Grand 助战 CE 筛选 | 按顺序只匹配已配置的 CE 位置。 | 跳过未配置位置；三个位置都为空时禁用 CE 筛选。 | 除非 UI 将来要求三个位置全部配置，否则保留。 |
| Grand CE 模板解析 | 使用配置的 CE 模板路径。 | 缺少模板时该位置不可检查，并在 debug 中记录 missing-template diagnostics。 | 所有可选 CE 都有 bundled template，或 UI 禁止选择缺少模板的 CE。 |
| CE 装饰图标 | 启用时，在 CE 相对区域内匹配 `icon_mlb_mark`、`icon_grand_bond_ce` 或 `icon_grand_bond_ce_np`。 | 禁用时跳过；缺少必需模板时该图标检查失败，不接受该行。 | 图标区域和模板在所有助战列表 variant 中稳定。 |
| 普通助战 CE 筛选 | 仅在 Grand mode 关闭时使用 `supportCraftEssenceId`。 | Grand mode 隐藏并忽略普通单 CE 设置，但不删除配置。 | 用户仍可能关闭后重新开启 Grand mode 时保留。 |
| 助战 skill slots | 从确认按钮 anchor 投影固定 slot offset，并用 append-only slot 的 saturation 区分 owned/append panel。 | saturation 不明确时采用 owned slot layout。 | 在足够多的普通、append、Grand 行上验证按钮 anchor 与 saturation probe。 |
| 助战 OCR 行 | 使用 name、NP、skill、CE 条件进行完整行匹配。 | OCR fragment 无法组成完整行时生成 name-only 行。 | 目标 server/分辨率上的 OCR 行分组足够可靠。 |
| CN 从者名 | 优先使用 `name_cn_server` 匹配 CN 显示名。 | 缺少 server-specific name 时回退到 `name_cn`；按 id 索引避免 alias 串到其他从者。 | 数据源仍有不完整的 `name_cn_server` 字段时保留。 |

## 战斗自动化

| 范围 | Primary path | Fallback path | 移除条件 |
| --- | --- | --- | --- |
| 高级启动条件 | 等待已配置的 command-card 条件匹配。 | 尚未启动时，每回合执行一个 control action，并继续按优先级攻击但不使用 NP。 | 用户可通过明确的等待/跳过回合控制替代默认行为。 |
| Command-card owner 识别 | 等待并重试，直到所有可见指令卡都识别出从者。 | 重试耗尽后记录未识别卡牌，不把它们视为条件匹配。 | Card owner CV 足够稳定，可删除重试耗尽处理。 |
| 高级自动攻击 | 根据 main attacker 与 output type 生成三张卡。 | 策略无法补满三张时，从剩余卡牌中由左到右补齐；没有高级规则匹配时执行 non-blocking attack。 | 新策略层为所有 partial-card 情况定义明确行为。 |
| NP 颜色 | 有配置时使用手动 NP color。 | color 未知时将 NP 作为高优先级攻击卡，但不假设 color chain。 | Servant/NP metadata 能可靠提供 card color。 |

## 强化自动化

| 范围 | Primary path | Fallback path | 移除条件 |
| --- | --- | --- | --- |
| ADB 屏幕尺寸 | 使用 scrcpy 报告的 frame dimensions。 | scrcpy 尺寸异常或缺失时查询 ADB device dimensions。 | 所有支持设备上的 scrcpy stream metadata 都可靠。 |
| 强化 dialog | 识别并点击明确的 dialog/result button。 | 已知的 action 后结果页面保留 coordinate fallback。 | Template probe 覆盖所有 result/dialog variant。 |
