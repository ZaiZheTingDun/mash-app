# 概念礼装强化自动化画面关系

本流程由独立的 `craft_essence_enhancement_runner` 驱动，当前仅支持国服。它不依赖从者强化 runner；两者只共享 ADB、视频流、识图和运行互斥基础设施。

```mermaid
stateDiagram-v2
    [*] --> Main
    Main --> Finished: 已选择目标礼装
    Main --> CraftEssenceSelect: 未选择，点击目标区域
    CraftEssenceSelect --> FilterDialog: 首次设置筛选
    FilterDialog --> CraftEssenceSelect: 保存筛选
    CraftEssenceSelect --> OrderDialog: 首次设置排序
    OrderDialog --> CraftEssenceSelect: 保存排序
    CraftEssenceSelect --> CraftEssenceSelect: 确认最大密度与降序
    CraftEssenceSelect --> Main: 选择第一张礼装
    Main --> Finished: 验证目标已选择
    MaterialSelect --> Error: MVP 暂不处理素材选择
```

## 画面分类

- 主页面必须同时命中 `icon_enhancement_result` 和 `element_enhancement_ce_stripe`。
- 主页面内，`element_enhancement_new` 表示未选择目标；缺失时表示已选择。`button_enhancement_ready` 只记录就绪状态。
- 选择列表必须命中 `button_enhancement_ce_select_ce_mark`。任一 `button_enhancement_ce_clean_all_select*` 存在时为素材选择，否则为目标礼装选择。
- 筛选和排序弹窗是目标礼装选择页的状态，分别由 `dialog_enhancement_ce_filter` 和 `dialog_enhancement_ce_order` 识别。

## 首次选择设置

- 列表密度必须命中礼装专用 `button_scale_level_3`，最多循环点击三次。
- 筛选先恢复初始设置，再确保五星、四星、三星为 off，二星、一星为 on。开关在指定 ROI 内按颜色亮度识别：平均亮度不高于 145 为蓝色 off，不低于 180 为白色 on，中间值视为不明确并停止。
- 排序设置为等级顺序并开启智能排序；返回列表后确保降序模板可见。
- 使用 `item_ce_bar_bronze` 作为通用物品网格锚点，按行列顺序点击第一格。

所有新增 1920 基准模板在 `cv.json` 中声明 `templateReferenceWidth: 1920`。每次点击在物理坐标层加入 ±6px 抖动。
