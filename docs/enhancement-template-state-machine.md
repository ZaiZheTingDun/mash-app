# 强化自动化模板状态机

强化自动化使用独立于战斗流程的模板状态机。战斗流程仍使用现有 `cv.json` screen detection；本设计只复用 sidecar 的 `find_element`/`find_element_full` 模板匹配能力，不改变 IPC wire protocol。

## 识别模型

状态由三层组成：

- `screen`: `Main`、`Enhancement`、`ServantEnhancement`、`Ascension`
- `variant`: `MenuCollapsed`、`MenuOpen`、`Main`、`ServantSelect`、`MaterialSelect`
- `state`: `FilterDialogOpen`、`NotReady` 等覆盖在 variant 上的状态

每个 screen 先用唯一 anchor 模板确认，再用附加 probe 判断 variant/state。所有搜索区域来自临时 `rois.json` 里的 `paddedRoi`，当前已收束到 `src-tauri/resources/servers/jp/cv.json` 的 `EnhancementAutomation.elements` 中。

默认阈值：

- screen anchor: `0.85`
- variant/state/button probe: `0.80`

## 模板映射

Screen anchors:

- `Main`: `button_notification`
- `Enhancement`: `text_enhancement`
- `ServantEnhancement`: `text_enhancement_servant`
- `Ascension`: `screen_enhancement_ascension`

Variant/state probes:

- `Main / MenuOpen`: `button_enhancement`
- `ServantEnhancement / Main`: `text_enhancement_result`
- `ServantEnhancement / ServantSelect`: `text_enhancement_servant_select`
- `ServantEnhancement / MaterialSelect`: `text_enhancement_material`
- `ServantEnhancement / FilterDialogOpen`: `dialog_filter_setting`
- `Ascension / Main`: `text_ascension_main_variant`
- `Ascension / ServantSelect`: `text_enhancement_ascension_servant_select`
- `Ascension / NotReady`: `enhancement_ascension_not_ready`
- 最大显示数量确认: `button_scale_level_3`

Action button probes:

- `Main / MenuCollapsed` -> tap `button_menu`
- `Main / MenuOpen` -> tap `button_enhancement`
- `AscensionResult` fallback return -> tap `button_enhancement_ascension_to_servant` when present

## 跳转行为

- `Main / MenuCollapsed`: 点击 `button_menu`，进入 `MenuOpen`
- `Main / MenuOpen`: 点击 `button_enhancement`，进入 `Enhancement`
- `Enhancement / Main`: 点击从者强化入口坐标，进入 `ServantEnhancement`
- `ServantEnhancement / Main`: 读取等级 OCR；未满级进素材页，满级且有灵基再临入口则进入 `Ascension`
- `ServantEnhancement / ServantSelect`: 先确认 `button_scale_level_3`；未命中则点击密度切换按钮，最多点击 3 次直到命中；之后用头像模板匹配目标从者
- `ServantEnhancement / MaterialSelect`: 使用 OCR 确认当前列表只包含经验值素材；确认 `button_scale_level_3` 后拖选/点选 20 个素材
- `ServantEnhancement / FilterDialogOpen`: 确认 `dialog_filter_setting` 和 `text_filter_setting_type`，再用现有 OCR 找到经验值素材筛选项并点击
- `Ascension / Main`: 点击右下强化按钮并走现有二次确认
- `Ascension / NotReady`: 停止自动化并提示材料或状态不可执行，避免继续点击右下强化按钮

## 缺省与待补模板

这些能力仍保留 OCR 或兼容坐标兜底：

- 等级 `Lv. current/max` 读取
- 素材已选数量 `0/20` 读取
- 强化/灵基再临二次确认弹窗
- 资料更新弹窗
- 灵基再临结果页返回

需要补模板才能完全去 OCR/完全证明状态：

- 强化/灵基再临二次确认弹窗唯一模板
- 资料更新弹窗唯一模板
- 灵基再临结果页“返回从者强化”完整状态模板
- 等级数字或满级状态模板
- 素材已选数量模板
- 经验值筛选三态确认模板：当前 `text_filter_setting_type` 只是“種別”标签，不能证明中间经验值项激活且左右两项未激活

## 命名约定

代码内部使用 English enum 名称：`MaterialSelect`，不使用 `select_material`。文档中将“main main 变体”写作 `Main screen / Main variant`。本轮不重命名已有 PNG 文件，避免无关资源 churn；后续新增模板建议继续使用 `screen_*`、`text_*`、`button_*`、`dialog_*` 前缀。
