# 战斗场景检测

`read_battle_scene` 读取 HUD 的 `m/n`，用于选择场次配置并避免重复执行技能。识别入口为 [cv.py](../sidecar/cv-code/mash_cv/cv.py) 的 `_read_battle_scene`，Rust 状态转换位于 [runner/state.rs](../src-tauri/src/runner/state.rs)。

## 模板识别

`BATTLE_SCENE_REGION` 使用归一化区域 `(0.587, 0, 0.160, 0.062)`。按当前区服加载 `battle/text_battle_label` 和 `digit/digit_0`–`digit_9`，并按实际帧尺寸缩放；旧的平铺 key 仍兼容。

1. 在区域内匹配标签，分数至少 `0.7`，取标签右侧作为数字 strip。
2. 对 0–9 匹配数字模板，保留分数至少 `0.8` 的候选。
3. 按得分执行横向 NMS，去掉重叠数字；再删除比最佳候选低超过 `0.08` 的结果。
4. 按 x 排序，在最大间隙处分成两侧；间隙须至少为平均字宽的 `0.5` 倍。
5. 从两侧外缘清理孤立数字，间隔门槛为平均字宽的 `0.6` 倍。若左侧多位读数导致场次越界，尝试去除前导误匹配。
6. 解析并确认 `1 ≤ scene ≤ total`，否则返回未识别。

成功返回 `{"scene": 2, "total": 3}`；普通未匹配返回两项 `null`。Rust 只接受完整数对，不用半个结果推进。

`Battle.variants.main.elements.battle_scene_anchor` 只暴露调试探针，完整数字解析仍走专用命令。

## 识别模式

“图像识别调试”控制单数字 ONNX 模型，“完整数字识别调试”控制 CNN-CTC；两者可分别配置：

| 模式 | 行为 |
| --- | --- |
| 关闭 | 使用当前基线，不运行对应模型分支 |
| 打开 | 使用模型结果，保留置信度与合法范围校验 |
| 影子模式 | 保留基线结果并比较模型；任何不一致，包括仅一方识别成功，都会报错并停止 |

单数字模型利用标签右侧区域识别数字并分割。CNN-CTC 在当前基线上读取完整数字区域，仍经过标签探针入口，避免非战斗区域误读。启用或影子模式要求启动时成功加载模型，不静默降级。选择与数值解释见 [battle_digit_policy.py](../sidecar/cv-code/mash_cv/battle_digit_policy.py)。

## 场次与回合

`tick_scene_state` 将 HUD 读数映射为配置下标：

| 观察 | 状态变化 |
| --- | --- |
| 首次成功读到 `m` | 对齐至 `m - 1`，支持中途启动 |
| 后续读数变化 | 下标仅增加 1，不按任意读数跳跃 |
| 重复读到相同 `m` | 不重复执行已完成配置 |
| 普通读取失败 | 保留下标和上次成功读数 |
| 启动后始终无有效读数 | 默认配置块 0 可执行一次，不无限等待 |

“未识别”不会自行推进下一场。普通模式的回合计数独立于场次：攻击结束后短暂等待可靠 HUD，确认仍为同一场再推进回合；等待耗尽时使用既有回合推进路径，进入新场则重置。完整执行关系见[战斗状态机](state-machines/battle-automation.md)。

## 诊断与校准

请求带 `debug: true` 时返回锚点分数、搜索区域、数字候选、保留结果、分割位置和 `failReason`；模型分支另附模型诊断。

| 原因 | 含义 |
| --- | --- |
| `empty_region` / `region_smaller_than_label` | 区域尺寸无效 |
| `anchor_below_threshold` | 标签不匹配或被遮挡 |
| `strip_too_narrow` | 标签右侧区域不足 |
| `no_digit_candidates` / `fewer_than_two_digits` | 数字模板未匹配或过滤后不足 |
| `no_separator_gap` | 无法找到分隔两数的间隙 |
| `cohesion_trim_emptied_side` / `parse_error` / `invalid_scene_range` | 清理、解析或范围校验失败 |
| `anchor_unavailable` | 单数字模型没有可用的锚定区域 |

以上为普通未识别。模板缺失 / 损坏是独立的[模板加载错误](state-machines/README.md#画面与模板)，不返回普通空读数；影子模式不一致也直接报错。

模板按区服校准。数字存在却无法识别时，保存原始帧，用 `debug_read_battle_scene` 检查候选位置和得分，重新裁剪对应区服模板并加回归。不要靠降低阈值掩盖字体差异。仅修改模板无需重建 CV runtime，但发布时需更新应用资源。

## 回归测试

模板识别覆盖 JP / CN、标签缺失、遮挡、孤立数字及非法场次；模型与切换策略另由数字 / 序列识别测试覆盖。

```bash
cd sidecar/cv-code
poetry -P ../cv-runtime run pytest tests/test_cv.py::TestReadBattleScene tests/test_battle_digit_policy.py tests/test_digit_classifier.py tests/test_sequence_classifier.py
cd ../..
cargo test --manifest-path src-tauri/Cargo.toml tick_scene_state
```
