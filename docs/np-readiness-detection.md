# NP 就绪检测

本文说明 `find_noble_phantasms` 如何判断前排各从者的 Noble Phantasm 是否就绪。

- **Sidecar 入口**：`sidecar/mash_cv/mash_cv/cv.py` 中的 `_find_noble_phantasms`
- **Rust 客户端**：`src-tauri/src/screen.rs` 中的 `SidecarClient::find_noble_phantasms`
- **Runner 使用处**：`src-tauri/src/runner/attack.rs` 中的 `Runner::read_attack_state`
- **调试界面**：`src/features/debug/DebugPage.tsx` 中的 `readNpGauges` / `动态检测端帽`

## 信号

“宝具条识别（选卡时）”的就绪状态由每个底部 NP 槽位右端附近的亮色端帽判断。检测器先从三个底部 NP 槽位百分比区域开始，每个前排从者各一个：

```python
DEFAULT_NP_GAUGE_DIGIT_REGIONS = (
    {"x": 0.182, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.429, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.678, "y": 0.913, "w": 0.0297, "h": 0.0278},
)
```

实际的就绪 probe 相对每个 gauge ROI 有以下偏移：

```python
NP_GAUGE_GLOW_OFFSET_X = 0.05329166666666668
NP_GAUGE_GLOW_OFFSET_Y = 0.026814814814814736
NP_GAUGE_GLOW_W = 0.00625
NP_GAUGE_GLOW_H = 0.011111111111111112
NP_GAUGE_GLOW_READY_THRESHOLD = 0.5
```

`npGlowScore` 是端帽 ROI 灰度均值归一化至 `0.0..1.0` 的结果。选卡时模式的最终规则为：

- `npGlowScore >= 0.5`：已就绪
- `npGlowScore < 0.5`：未就绪
- 缺少端帽分数：读取不完整／未知，runner 会重试

判断就绪不需要精确 NP 百分比；sidecar 仍会读取数字布局以供调试。FGO 的槽位显示具有以下特征：

- 数字在固定 gauge ROI 内右对齐；
- 小于 100 的值只占用十位和个位数字槽；
- 大于等于 100 的值还会占用百位数字槽。

每个 gauge ROI 会分为三个相对数字槽区域：

```python
DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS = (
    {"x": -2.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
    {"x": 17.0 / 57.0, "y": 0.0, "w": 23.0 / 57.0, "h": 1.0},
    {"x": 38.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
)
```

Sidecar 会检测每个固定数字槽是否含有可信的白色数字主体。十位和个位只检查是否存在；百位额外进行一道保护：形状检查后还必须匹配现有通用 `digit_` 模板，确认是可信的百位数字。此举会排除附近 `宝具` 标签的竖笔画——该笔画原本会像一个狭窄的 `1`。无需 NP gauge 专用模板。

检测通过以下条件滤除底部 gauge 线、HP 条、百分号残片及字幕文字：组件必须较高、起始于数字槽的上半部分，且跨越足够的数字高度。

数字结果通过 `gaugeDigitCount` 和 `gaugeHundredsVisible` 暴露。

“宝具条识别（选卡前）”使用 `gaugeHundredsVisible` 作为就绪条件：固定百位槽出现有效数字即表示宝具条至少为 100%。该模式仍采样约一秒，并以有效样本的多数结果决定每个槽位，避免单帧字幕或特效造成误判。它不要求完整 OCR 出具体百分比，也不要求十位和个位都可见。

上方 NP 卡片槽位矩形仍保留在响应中，既作为点击区域，也作为旧版调试测量（`edgeFrac`、`stdBgr`、`edgeThreshold`、`cardReady`）：

```python
DEFAULT_NP_CARD_SLOTS = (
    {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
)
```

## 重试行为

Runner 将缺少端帽分数视为不完整读取，返回：

```python
{
    "ready": False,
    "readySource": "unknown",
    "npGlowScore": None,
}
```

`Runner::read_attack_state` 会将任意 `None` 的 `npGlowScore` 视为不完整读取，等待 `ACTION_DELAY` 后再次调用 `find_noble_phantasms`。它只会在全部五个指令卡槽均出现 suit/icon 信号后启动 gauge 循环，从而让指令卡按钮切换动画离开底部 gauge 后再读取 NP；取消操作仍会退出该循环。

仅供调试的实时命令 `debug_read_noble_phantasm_gauges_live` 会采样一秒，并返回每个槽位所见的最高 `npGlowScore`，以捕获呼吸灯的峰值，无需写入截图文件。

## 响应结构

```python
{
    "slots": [
        {
            "slot": 0,
            "cardRegion": {"x": ..., "y": ..., "w": ..., "h": ...},
            "ready": True,
            "readySource": "glow",
            "npGlowScore": 0.61,
            "npGlowReady": True,
            "gaugeHundredsVisible": True,
            "npGlowRegion": {"x": ..., "y": ..., "w": ..., "h": ...},
            "gaugeDigitCount": 3,
            "gaugeRegion": {"x": 0.182, "y": 0.913, "w": 0.0297, "h": 0.0278},
            "edgeFrac": 0.12,
            "stdBgr": 68.0,
            "edgeThreshold": 0.07,
            "cardReady": True,
        },
    ],
    "edgeThreshold": 0.07,
}
```

`edge*` 与 `cardReady` 字段是供调试和后续配置使用的旧版上方卡片检测器输出，不参与当前的就绪判断。

## 测试

`tests/test_cv.py::TestFindNoblePhantasms` 覆盖 CN fixture 上的端帽检测路径：

- `battle_np_gauge_cn_50_40_70.png`：`50 / 40 / 70`
- `battle_np_gauge_cn_100_obscured_90.png`：`100 / obscured / 90`
- `battle_np_gauge_cn_100_60_190.png`：`100 / 60 / 190`
- `battle_np_gauge_cn_100_100_200.png`：`100 / 100 / 200`
- `battle_np_gauge_cn_dimmed_100_100_200.png`：变暗的指令卡画面，`100 / 100 / 200`
- `battle_np_gauge_cn_120_60_90.jpg`：指令卡画面，`120 / 60 / 90`
- `battle_np_gauge_cn_120_60_90_label_occluded.jpg`：指令卡画面，`120 / 60 / 90`，覆盖 `宝具` 标签，并验证百位槽的误匹配排除

同一测试类还以合成数据覆盖两个槽位级场景：

- 损坏的百位数字主体仍会被计为存在；
- 空百位槽中的底部 gauge 线不会被计为存在。

运行方式：

```bash
cd sidecar/mash_cv
poetry run pytest tests/test_cv.py -k NoblePhantasms
```
