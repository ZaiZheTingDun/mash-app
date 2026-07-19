# 强化自动化模板状态机

强化自动化使用独立于战斗流程的模板状态机。战斗流程仍使用既有的 `cv.json` 画面检测；此设计仅复用 sidecar 的 `find_element` / `find_element_full` 模板匹配能力，不改变 IPC wire protocol。

## 识别模型

状态分为三层：

- `screen`：`Main`、`Enhancement`、`ServantEnhancement`、`Ascension`
- `variant`：`Main`、`ServantSelect`、`MaterialSelect`
- `status`：`MenuOpen`、`FilterDialogOpen`、`NotReady`，以及由 variant 内 element probe 表示的其他状态

每个 screen 先由唯一的 anchor template 确认，再通过 variant detection 识别当前视图，最后由 variant 内的 element probe 判断 status。搜索区域原先来自临时 `rois.json` 的 `paddedRoi` 配置，现已归入 `src-tauri/resources/servers/jp/cv.json` 的 `screens.*.detect` 与 `screens.*.variants`。

默认阈值：

- Screen anchor：`0.85`
- Variant detection 与 element status/button probe：`0.80`

## 模板映射

Screen anchor：

- `Main`：`button_notification`
- `Enhancement`：`text_enhancement`
- `ServantEnhancement`：`text_enhancement_servant`
- `Ascension`：`screen_enhancement_ascension`

Variant detection 与 status element：

- `Main / Main / MenuOpen status`：`button_enhancement`
- `ServantEnhancement / Main`：`text_enhancement_result`
- `ServantEnhancement / ServantSelect`：`text_enhancement_servant_select`
- `ServantEnhancement / MaterialSelect`：`text_enhancement_material`
- `ServantEnhancement / MaterialSelect / FilterDialogOpen status`：`dialog_filter_setting`
- `Ascension / Main`：`text_ascension_main_variant`
- `Ascension / ServantSelect`：`text_enhancement_ascension_servant_select`
- `Ascension / Main / NotReady status`：`enhancement_ascension_not_ready`
- 最大显示密度确认：`button_scale_level_3`

Action 按钮 probe：

- `Main / Main / menu collapsed status` → 点击 `button_menu`
- `Main / Main / menu open status` → 点击 `button_enhancement`
- `AscensionResult` fallback 返回 → 存在时点击 `button_enhancement_ascension_to_servant`

## 状态转换

- `Main / Main / menu collapsed status`：点击 `button_menu`，进入 menu-open status。
- `Main / Main / menu open status`：点击 `button_enhancement`，进入 `Enhancement`。
- `Enhancement / Main`：点击从者强化入口坐标，进入 `ServantEnhancement`。
- `ServantEnhancement / Main`：通过 OCR 读取等级。未满级时进入素材选择；满级且灵基再临入口可用时进入 `Ascension`。
- `ServantEnhancement / ServantSelect`：先确认 `button_scale_level_3`。若不存在，最多点击 3 次显示密度切换按钮，直至出现；随后通过头像模板匹配目标从者。
- `ServantEnhancement / MaterialSelect`：通过 OCR 确认当前列表仅包含 EXP 素材。确认 `button_scale_level_3` 后，拖选或点选 20 个素材。
- `ServantEnhancement / MaterialSelect / FilterDialogOpen status`：确认 `dialog_filter_setting` 与 `text_filter_setting_type`，随后使用既有 OCR 流程寻找并点击 EXP 素材筛选项。
- `Ascension / Main`：点击右下角强化按钮，进入既有的二次确认流程。
- `Ascension / Main / NotReady status`：停止自动化并报告素材不足或状态不可执行，避免反复点击右下角强化按钮。

## Fallback 与缺失模板

以下能力仍保留 OCR 或兼容的坐标 fallback：

- 等级 `Lv. current/max` 读取
- 已选素材数量 `0/20` 读取
- 强化／灵基再临二次确认对话框
- 数据更新对话框
- 从灵基再临结果页返回

以下模板可用于移除 OCR 或完整确认状态：

- 强化／灵基再临二次确认对话框的唯一模板
- 数据更新对话框的唯一模板
- 灵基再临结果页完整返回至从者强化状态的模板
- 等级数字或满级状态模板
- 已选素材数量模板
- 三态 EXP 筛选确认模板：当前 `text_filter_setting_type` 仅是筛选类型标签，无法证明中间 EXP 选项已启用而左右选项未启用

## 命名

代码使用 `MaterialSelect` 等英文 enum 名称，不使用 `select_material` 一类命名。文档将嵌套的基础视图称为 `Main screen / Main variant`。现有 PNG 文件名不改名，以避免无关的资源变动；新模板仍应使用 `screen_*`、`text_*`、`button_*` 与 `dialog_*` 前缀。
