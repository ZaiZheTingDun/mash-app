# 战斗场景检测

本文说明 `read_battle_scene` 如何识别攻击画面右上 HUD、金色 **BATTLE** 标签旁的 `m / n` 指示器，以及 runner 如何在每次场景切换时选择对应的已配置技能块。

- **Sidecar 入口**：`sidecar/mash_cv/mash_cv/cv.py` 中的 `_read_battle_scene`
- **Rust 客户端**：`src-tauri/src/screen.rs` 中的 `SidecarClient::read_battle_scene`
- **Runner 使用处**：`src-tauri/src/runner/engine.rs` 中 `tick_scene_state` 调用附近的 `Runner::handle_battle`
- **调试界面**：`debug_read_battle_scene` Tauri command 与 `DebugPage.tsx`

## 解决的问题

一个 quest 由一个或多个 Battle 组成。HUD 将当前 Battle 显示为 `m / n`（例如 `1/3`、`2/3`、`3/3`）。Runner 需要知道两件事：

1. **当前是哪个 Battle？** 用于选择匹配的已配置技能／目标／NP 块。
2. **Battle 是否刚切换？** 即使 CV 暂时无法读取该区域（NP 演出遮挡、攻击动画等），也要确保每次切换仅执行一次新块。

`(m, n)` 同时提供这两个信息：`m` 是 Battle id，而两个 tick 间 `m` 的变化就是切换信号。

这里刻意**不**使用 OCR sidecar 解析数字（`mash-cv` 的 RapidOCR 用于助战名读取）。该区域是固定位置、由手工位图字体渲染的五字符 widget；相较加载模型，使用 `digit_0`…`digit_9` 的模板匹配约快一个数量级，并能提供每个 glyph 的分数以过滤伪匹配。

## 流程

```text
frame ──► 裁剪 BATTLE_SCENE_REGION ──► matchTemplate(text_battle_label)
                                              │
                                              ▼  anchor 分数 ≥ 0.7
                       strip = 匹配到的标签框右侧像素
                                              │
                                              ▼
                  对 d in 0..=9：matchTemplate(digit_d) ≥ 0.8
                                              │
                                              ▼  贪心 x-NMS
                            score-margin 过滤（最佳分数 - 0.08）
                                              │
                                              ▼  至少保留 2 个
              在最大的 x 间隙处分割（必须大于 avg_w / 2）
                                              │
                                              ▼
              两侧分别 cohesion-trim（丢弃与簇间隔
                       大于 0.6 × avg_w 的数字）
                                              │
                                              ▼
                        ("".join(left), "".join(right)) → (m, n)
```

任一步都可带 `failReason` 短路返回 `(None, None)`；runner 将其视为「Battle 未变化」（见[状态机](#状态机)）。

## `cv.json` 集成

常规战斗模板 probe 位于 `src-tauri/resources/servers/<server>/cv.json` 的对应 screen 中：

- `Battle.variants.main.elements.attack_button`：供 `Runner::handle_battle`、`Runner::wait_for_attack_button` 和调试攻击按钮 probe 使用。
- `SupportSelect.variants.main.elements.support_scroll_start` / `.support_scroll_end`：用于判定助战列表位于顶部、底部或没有滚动条。
- `Battle.variants.main.elements.battle_scene_anchor`：将 `text_battle_label` 的搜索窗口暴露给调试模板 probe UI。

完整的 `m/n` 读取仍由 `SidecarClient::read_battle_scene` 完成，因为它在以 `text_battle_label` 定位后执行自定义数字匹配。

## 区域与 Anchor

自定义 `read_battle_scene` 流程使用以下 `BATTLE_SCENE_REGION`：

```rust
pub const BATTLE_SCENE_REGION: NormRect = NormRect {
    x: 0.587, y: 0.000, w: 0.160, h: 0.062,
};
```

该常量使用屏幕 0–1 比例，因而可在 JP/CN 的 1080p、1440p 与 2K stream 上复用。区域刻意比标签更宽，必须同时容纳标签及其右侧 `m / n` strip。

`templates.matchTemplate(roi, text_battle_label, TM_CCOEFF_NORMED)` 在区域内定位标签，返回左上角位置和分数。标签 glyph 因 server 而异：

| Server | 模板内容 |
|---|---|
| JP | `BATTLE`（金色拉丁文字） |
| CN | 游戏 UI 中的四个战斗场景中文字符 |

二者均位于 `src-tauri/resources/servers/<server>/templates/battle/text_battle_label.png`，启动时载入对应 server template bundle，因此 matcher 始终使用 `text_battle_label` 这一个 key。

```python
BATTLE_LABEL_THRESHOLD = 0.7
```

标签是深色渐变带上的高对比实色 glyph，真实匹配通常为 **0.99+**。`0.7` 是宽松但安全的阈值；`0.7` 至 `0.95` 常意味着标签被 NP 演出、攻击动画或 popup 部分遮挡，应直接放弃而非读取错误数字。anchor 缺失时返回 `failReason = "anchor_below_threshold"`。strip 起点为 `x_start = match_x + label.shape[1]`，即已匹配标签框右侧紧邻的像素列。

## 数字候选、去重与过滤

对于每个 `digit_d`（`d ∈ 0..=9`），在灰度 strip 上运行 `matchTemplate(strip, digit_d, TM_CCOEFF_NORMED)`，保留分数 `≥ BATTLE_DIGIT_THRESHOLD = 0.8` 的 `(x, y)`。

CN bundle 对 `0`、`1`、`4`–`9` 复用与 JP byte-identical 的模板；`digit_2.png` 和 `digit_3.png` 则是从真实 CN BATTLE strip 截取的原生模板。JP 模板与 CN 字体仅得到 `0.76`–`0.77`，低于 `0.80` 阈值。

单个渲染数字会触发多个略有重叠的候选，因此按分数从高到低执行仅基于 x 坐标的贪心 NMS：与已保留候选相距小于半个 glyph 宽度者丢弃。这既保留每个数字槽的一个检测，也不会合并 `2/3` 中相距约 1.5 个 glyph 宽度的两个数字。

随后执行 score-margin 过滤：

```python
BATTLE_DIGIT_SCORE_MARGIN = 0.08
score_floor = max(k[2] for k in kept) - BATTLE_DIGIT_SCORE_MARGIN
kept = [k for k in kept if k[2] >= score_floor]
```

同一 frame 内真实 `m/n` 数字的分数通常只差 1–2%。明显低于最佳值的候选几乎都是伪匹配，例如狭窄 `digit_1` 匹配到标签背景的竖缝，或相邻 UI 元素边缘进入 strip。`0.08` 足以吸收字体笔画宽度与次像素抗锯齿造成的真实噪声，也能在真实数字约为 `0.999` 时丢弃 `0.89` 的伪匹配。

## 分割与结果

过滤后按 x 排序，在最大间隙处分割：

```python
avg_w = mean(c.w for c in kept)
best_gap = max(kept[i+1].x - (kept[i].x + kept[i].w) for i in range(len(kept)-1))
```

若 `best_gap < avg_w * 0.5`，表示没有斜杠分隔符，保留的数字均紧密排版，可能是溢入 strip 的多位 HP 数字；返回 `failReason = "no_separator_gap"`。否则将最大间隙后的索引作为 `split_at`，得到 `left = kept[:split_at]` 与 `right = kept[split_at:]`。

每一侧随后进行 cohesion trim：

```python
cohesion_threshold = avg_w * 0.6  # BATTLE_DIGIT_COHESION_GAP_RATIO
```

`_trim_left(side)` 丢弃与右邻居间隙超过阈值的前导数字；`_trim_right(side)` 丢弃与左邻居间隙超过阈值的尾随数字。清理方向由外向内，因此斜杠附近数字始终保留，可清除恰好匹配到相邻 UI 文本的孤立数字。

成功结果：

```python
{"scene": int("".join(left)), "total": int("".join(right))}
```

任意短路路径返回：

```python
{"scene": None, "total": None}
# 调试 payload 还含 diagnostics.failReason
```

Rust 客户端通过 `Option::zip` 将结果映射为 `Option<(u32, u32)>`；任意一侧失败均折叠为 `None`，不会以不完整 pair 进入 runner。

## 失败模式（`failReason` enum）

| `failReason` | 原因 | 调用方行为 |
|---|---|---|
| `empty_region` | 裁剪结果为 0 像素。 | 配置错误。 |
| `missing_label_template` | bundle 未载入 `text_battle_label`。 | Bundle 错误。 |
| `region_smaller_than_label` | `BATTLE_SCENE_REGION` 小于标签尺寸。 | 配置错误。 |
| `anchor_below_threshold` | 标签匹配低于 `0.7`。 | 视为 Battle 未变化。 |
| `strip_too_narrow` | 标签右侧不足 5 px。 | 区域可能偏移。 |
| `no_digit_candidates` | 没有数字达到 `0.8`。 | strip 为空或运行时字体漂移。 |
| `fewer_than_two_digits` | NMS 与 score-margin 后仅剩一个。 | 通常是字体漂移或模板质量问题。 |
| `no_separator_gap` | 保留数字间没有斜杠间隔。 | strip 并非 `m/n`。 |
| `cohesion_trim_emptied_side` | cohesion trim 清空了一侧。 | 记录截图后提交 issue。 |
| `parse_error` | `int()` 失败。 | 记录 issue。 |

对 runner 而言，以上全部等价于 `Ok(None)`，状态机保持上一 Battle index。

## 状态机

Rust runner 每个 `handle_battle` tick 调用一次 `read_battle_scene`，并将结果传入纯 helper：

```rust
fn tick_scene_state(
    last_screen_scene: Option<u32>,
    current_scene_index: usize,
    executed_scene_index: Option<usize>,
    scene_m: Option<u32>,
) -> SceneTick { ... }
```

完整转换表由 `src-tauri/src/runner/tests.rs` 中的 `tick_scene_state_*` 固定；关键不变量如下：

1. **CV 读取失败绝不推进。** `scene_m == None` 时 index 和 `last_screen_scene` 保持不变。
2. **重复读取相同 `m` 不会重复触发。** 已执行 scene index `k` 后，只有画面上的 `m` 真正变化才会再次执行。
3. **第一次成功读取会将 index 对齐到 `m - 1`。** 中途进入 quest、画面已是 `2/3` 时，首次读取会将 index 定为 `1`，执行用户的第二块配置。首次 poll 失败后，首次成功读到 `m=2` 同样会立即对齐并重新触发。
4. **后续转换每次只前进一格。** 锚定后，每次观察到 `prev → curr`（且不同）仅使 index `+1`；真实 quest 不会发生跳关，因此不会信任任意 `curr` 来跳跃。
5. **首次读取失败仍执行默认块。** 若 CV 从一开始就不可用，仍会执行 scene index `0` 一次，避免自动化无限等待。

因此，CV 失败会表现为「Battle 未变化，使用下一个默认块」；runner 会保守地按顺序 fallback，而非基于误读触发错误块。相反，中途首次成功读取会直接对齐到正确块。

## CN 回归与校准

CN `2/3` screenshot（`tests/test_data/screenshots/battle_scene_cn.png`）曾出现：JP `digit_2` 得分 `0.760`、JP `digit_3` 得分 `0.772`，而标签竖缝上的 JP `digit_1` 得分 `0.890`。前两者低于阈值，导致 `fewer_than_two_digits`，画面已在 Battle 2 时 runner 却使用默认顺序。

将 `digit_2.png`、`digit_3.png` 重新从 CN HUD 字体裁剪后，真实 `2`/`3` 得分为 `0.998`/`0.999`。NMS 会保留三者；score-margin（`floor = 0.999 - 0.08 = 0.919`）会在分割前移除 `0.89` 伪匹配，从而正确解析 `2/3`。

## 常量与测试

```python
BATTLE_LABEL_THRESHOLD = 0.7            # Anchor 置信度
BATTLE_DIGIT_THRESHOLD = 0.8            # 单数字绝对下限
BATTLE_DIGIT_SCORE_MARGIN = 0.08        # 与 frame 最佳分数相差超过此值则丢弃
BATTLE_DIGIT_COHESION_GAP_RATIO = 0.6   # 数字内部 kerning 容差（× avg_w）
```

`sidecar/mash_cv/tests/test_cv.py::TestReadBattleScene` 覆盖 anchor 缺失、JP `1/3`、NP 遮挡、外缘孤立数字 trim，以及 CN `2/3` 回归。`src-tauri/src/runner/tests.rs::tick_scene_state_*` 覆盖消费读取结果的状态机。

```bash
cd sidecar/mash_cv
poetry run pytest tests/test_cv.py::TestReadBattleScene -v

cd ../..
cargo test --manifest-path src-tauri/Cargo.toml tick_scene_state
```

目前 CN bundle 只有原生 `digit_2.png`、`digit_3.png`；`digit_0`、`digit_1`、`digit_4`–`digit_9` 与 JP 模板仍 byte-identical，未必均能以 `0.80` 以上分数匹配 CN 字体。若截图中的数字明显存在却仍报告「scene unchanged」，应保存截图、用 `debug_read_battle_scene` 定位缺失数字的 `(x, y)` 和分数，以现有模板尺寸重新裁剪 CN 模板，并加入 `TestReadBattleScene` 回归 case。仅修改模板不需重建 sidecar runtime；模板由 Tauri app resource 在启动时传给已安装 runtime，但需要重建 app bundle 才能随 app 发布。不要降低 `BATTLE_DIGIT_THRESHOLD` 来掩盖字体不匹配；应重新裁剪受影响的模板。
