# 宝具就绪检测

Sidecar 提供宝具卡、底部条亮度及数字槽的原始测量，Rust 根据配置选择就绪规则。识别位于 [cv.py](../sidecar/cv-code/mash_cv/cv.py)，采样位于 [attack/selection_runtime.rs](../src-tauri/src/runner/attack/selection_runtime.rs)，完整性与聚合规则位于 [attack/noble_phantasm.rs](../src-tauri/src/runner/attack/noble_phantasm.rs)。

## 三种模式

| 模式 | 采样时机 | 就绪条件 |
| --- | --- | --- |
| `Card`：宝具指令卡 | Attack 页面 | 使用上方宝具卡的 `cardReady` |
| `Gauge`：选卡时宝具条 | Attack 页面 | 端帽亮度 `npGlowScore ≥ 0.5` |
| `GaugeBeforeAttack`：选卡前宝具条 | Battle 页面、点击攻击前 | 百位数字槽可见，即至少 100% |

攻击前结果缓存在 `BattleState.pre_attack_nps`，进入 Attack 后复用。当前出卡配置无宝具时跳过采样；从者台词可能遮挡攻击前读数，运行时会提示关闭台词。

## 采样与重试

两种宝具条模式均采样至少 1 秒、取得至少 3 次结果，并按槽位聚合：

- 选卡时：有效端帽亮度的中位数达到 `0.5` 即就绪。
- 选卡前：有效百位可见性严格多数为真才就绪，票数相同视为未就绪。

完整读取要求三个槽位都有对应测量；端帽分数或百位可见性缺失属于未知，不当作未就绪。结果不完整时等待 `ACTION_DELAY` 后重试，用户取消可退出。

Attack 页面先等待全部五张普通卡都有 suit / icon 信号，再读取宝具，避开切页动画。宝具卡模式要求三个槽位的 `cardReady` 都可用，缺失时同样重试。

## 亮度与数字信号

`npGlowScore` 是底部宝具条右端小区域的灰度均值，归一化至 `[0, 1]`。区域和偏移以 `cv.py` 的 `DEFAULT_NP_GAUGE_DIGIT_REGIONS`、`NP_GAUGE_GLOW_*` 常量为准。

数字按固定槽右对齐。百位检测先检查白色数字主体的高度与位置，再匹配通用数字模板，排除宝具标签笔画、底线、百分号和字幕残片。选卡前模式只需确认百位，不要求完整读取百分比。

“图像识别调试”可用单数字模型替换百位检测；“完整数字识别调试”可读取 CNN-CTC 数字，接受置信度合格且范围为 0–300 的结果。影子模式比较结果，不一致即停止，规则见[识别模式](battle-scene-detection.md#识别模式)。CTC 仅参与 Battle 的攻击前采样，Attack 页面仍走原宝具卡或亮度路径。

## 响应字段

| 字段 | 用途 |
| --- | --- |
| `slot` / `cardRegion` | 槽位编号与宝具点击区域 |
| `ready` / `readySource` | 当前读数的就绪状态与来源，Rust 按模式和聚合结果更新 |
| `cardReady` | 宝具卡模式的判据 |
| `npGlowScore` / `npGlowReady` / `npGlowRegion` | 端帽测量、阈值判断与区域 |
| `gaugeHundredsVisible` / `gaugeDigitCount` / `gaugeRegion` | 百位存在性、数字数量与区域 |
| `edgeFrac` / `stdBgr` / `edgeThreshold` | 上方宝具卡的纹理诊断，不作为宝具条模式的备用规则 |

不要用亮度、数字和卡面信号互相兜底；三种模式各自使用明确判据。

## 调试与回归

`debug_read_noble_phantasm_gauges_live` 对实时亮度采样一秒，无需落盘截图。测试覆盖未满 / 已满、超过 100%、变暗、标签和字幕遮挡、百位误匹配，以及 Rust 聚合与模式选择。

```bash
cd sidecar/cv-code
poetry -P ../cv-runtime run pytest tests/test_cv.py -k NoblePhantasms
cd ../..
cargo test --manifest-path src-tauri/Cargo.toml np_gauge
```
