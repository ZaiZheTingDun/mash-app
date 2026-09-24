# Mash 通用数字分类器训练工具

该子项目用于离线准备和训练 Mash 的游戏 UI 单数字分类器与共享数字序列
CNN-CTC。运行中的 `mash-cv` 不依赖 PyTorch；最终模型导出为 ONNX，由
OpenCV DNN 加载。

单字模型负责识别已经定位好的候选；序列模型直接读取完整数字区域并输出数字
字符串。页面定位和业务约束仍由各识别路径负责，例如 NP 百位只允许
`1`、`2`、`3`，场次结果仍需组成合法的 `当前/总数`。

## 数据目录

`data/` 和 `runs/` 仅保存在本地并已加入 Git 忽略：

```text
data/
  raw/                 按截图类型、服务器保存的原始完整截图
    battle/{cn,jp}/    战斗页面
  crops/               按截图类型保存的单字候选
  regions/             整行数字区域（供 CNN-CTC 使用）
  manifest.jsonl       标签和来源信息
  sequence-manifest.jsonl  完整数字区域的人工确认标签
runs/                  训练日志、检查点和临时导出
```

每个样本必须记录原始截图的 `parentId`。训练、验证、测试划分按
`parentId` 分组，避免同一帧的近似裁剪泄漏到不同数据集。

将完整战斗截图按服务器放到 `data/raw/battle/cn/` 或
`data/raw/battle/jp/` 后运行：

```bash
poetry run digit-crop
```

工具会同时提取己方 HP、敌方 HP、宝具、战斗场次、敌方数量和回合数，
并生成三类本地文件：`regions/` 保存整行区域，`crops/` 保存待标注单字，
`previews/` 在完整截图上绘制区域及候选框。裁剪命令可以重复运行；清单中
已有的标签、划分和备注会按稳定样本 ID 保留。原始截图不会被修改。
旧的 `data/raw/cn/`、`data/raw/jp/` 仍会兼容读取并视为 `battle`，但新增截图
统一使用 `raw/<截图类型>/<服务器>/`。清单会记录 `screenshotType`；以后新增
其他页面时，在统一配置里增加截图类型即可。

宝具数字不再按亮色连通块生成“干净单字”，而是按真实运行时完全相同的
百位、十位、个位固定框，每个宝具槽固定生成三张样本。空百位、相邻数字
残边和特效会保留在裁剪中，以便训练输入与运行输入一致。NP 运行识别、训练
裁剪和 Debug 画框共同使用
`mash_cv/assets/digit_classifier/screenshot-regions-v1.json`。这个文件按
`screenshotTypes → battle → sources` 组织，包含敌方 HP、己方 HP、NP、场次、
敌人数和回合数的所有裁切区域；调坐标只需修改这一处。NP 的 `regions` 用于
当前固定单字槽和运行判定，`sequenceRegions` 用于 CNN-CTC 整行输入，因此整行
区域可以校准而不会让空百位误吃到十位数字。
固定槽位样本 ID 不包含具体坐标，因此微调配置后重新裁剪仍会保留对应标签。
旧版已标注的 NP 连通块样本也会继续留在清单中，作为干净数字辅助样本。

本地开发版可在「设置 → 调试」开启“点击攻击前自动截图”。Runner 会在
真正点击攻击按钮前把最新 Battle 视频帧以无损 PNG 保存到
`app_data_dir()/debug/battle-before-attack/{cn|jp}/`；文件名带服务器、任务轮次、
Battle index 和 turn index，可作为宝具条与其他战斗数字的首批原始数据。

清单中的 `label` 可以是字符串 `0` 到 `9`、`invalid` 或 `null`。`invalid`
应覆盖空白、横线、百分号、汉字笔画、台词、特效和截断字形；不能只收集
正确数字，否则分类器面对干扰时会被迫输出某个数字。

示例：

```json
{"schemaVersion":1,"id":"battle-np_gauge-hundreds-...","image":"crops/battle/cn/frame/np_gauge-0-hundreds-....png","label":null,"source":"np_gauge","style":"battle_hud_fixed_slot","server":"cn","screenshotType":"battle","parentId":"sha256:...","resolution":[1920,1080],"sourceBboxPx":[346,986,24,30],"split":null,"notes":null}
```

## 环境与命令

```bash
cd sidecar/mash_cv/training/digit_classifier
poetry install

# 校验清单
poetry run digit-manifest data/manifest.jsonl validate

# 查看来源、标签和划分统计
poetry run digit-manifest data/manifest.jsonl stats

# 按原始截图分组，确定性划分数据集
poetry run digit-manifest data/manifest.jsonl split

# 按键标注：0-9 为数字，X 为 invalid，S 跳过，U 撤销，Q 退出
poetry run digit-label data/manifest.jsonl --dataset-root data

# 对所有未标注裁切生成单字模型候选和置信度
poetry run digit-suggest

# 训练、评估并导出 ONNX
poetry run digit-train

# 用人工确认的完整数字标签训练共享 CNN-CTC 序列模型
poetry run sequence-ctc-train

# 使用最新一次共享模型识别一个或多个完整数字裁切
poetry run sequence-ctc-predict path/to/image.png
```

标注工具每次确认后都会原子写回清单，意外退出不会丢失之前的标签。

可视化标注页面是相邻的独立 Node 项目：

```bash
cd ../digit-labeler
pnpm start
```

浏览器打开 `http://127.0.0.1:8765`。页面可切换单字和完整数字标注；左侧显示
原始截图和当前裁剪框，右侧放大裁剪图。完整数字区域会显示现有单字标签拼成的
建议值，需要对照完整裁剪图确认或修正后才能用于 CTC 训练。单字视图可以按截图类型、服务器、来源和标注状态筛选。数字键 `0`–`9` 直接标注，
`X` 标记 `invalid`，`S` 跳过，`U` 撤销，方向键浏览，Backspace 清除标签。
生成 `data/suggestions.json` 后，候选数字和置信度会显示在图片下方。按 Enter 或点
“确认候选”采纳，若不正确则按 `0`–`9` 或 `X` 直接重标。低置信度候选会明确提示，
仍由人工决定是否确认。建议每次重新裁剪或加入新截图后重新运行 `digit-suggest`。

## 模型产物

最终文件应放在：

```text
mash_cv/assets/digit_classifier/digit-classifier-v1.onnx
mash_cv/assets/digit_classifier/digit-classifier-v1.json
```

CNN-CTC 直接读取 `regions/battle/...` 下的完整数字区域。训练标签只从
`sequence-manifest.jsonl` 的人工确认结果读取。NP 固定槽位和生命值等连通块
单字标签只用来提供候选值；漏裁、错裁或错标时可直接在完整区域视图输入正确数字。
所有来源共享同一个 `0–9 + CTC blank` 模型，来源各自只负责业务解析。
产物名为 `digit-sequence-ctc-v1.onnx/json`。它必须先达到各来源独立测试集门槛，
再复制到运行资源并接入影子模式；训练命令不会覆盖当前应用使用的单字模型。

运行时通过「设置 → 调试 → 图像识别调试」选择是否使用这两个文件。
`打开`采用模型结果，`影子模式`同时运行新旧识别并在结果不一致时停止自动化，
`关闭`只运行现有模板识别。当前自动化决策接入战斗场次和攻击前宝具百位；
影子模式还会把当前回合数，以及三个宝具槽的百位、十位、个位写入运行日志，
用于核对模型但不参与决策。生命值和敌人数仍作为训练数据保留，等对应业务读取
接入后复用同一模型。

完整数字模型另由「设置 → 调试 → 完整数字识别调试」独立控制，使用
`digit-sequence-ctc-v1.onnx/json`。关闭时自动化沿用已有路径；影子模式在
Battle 页面比较整串结果，不一致或单侧未识别就停止；打开时仅采用达到
元数据置信度门槛的 CTC 输出。攻击前宝具条的完整数字区域只适用于 Battle，
不会在 Attack 选卡页运行。手动数字识别会显示 CTC 原始结果、置信度和是否通过
门槛，便于用新截图核对；HP、敌人数和回合数当前只用于训练或调试，
尚未替换对应自动化判断。模型文件缺失时无法启用影子或打开模式。

不要放进现有的 `mash_cv/models/`。构建脚本会从轻量 CV code 包中移除
`models/`，而 `assets/` 会随 code 包发布，数字模型更新不需要重发重量级
runtime。

元数据 JSON 应固定模型版本、类别顺序、输入尺寸、归一化方式、最低置信度、
最低 top-1/top-2 差值、训练数据清单哈希以及按来源拆分的评估结果。

训练会使用按原图分组的 train/validation/test 划分，避免同一帧泄漏。字符先按
亮色字形收紧边界，再保持比例居中到 `32×32`；训练集只加入轻微缩放和位移，
不旋转 HUD 字体。损失函数按类别频次的平方根倒数加权，缓解数字 `8` 等少数类
样本不足。最佳验证集宏平均召回率对应的权重会导出为 `.pt`、ONNX 和评估元数据。

## 推荐实施顺序

1. 先为现有 `digit/digit_` 使用点收集 CN 截图和 `invalid` 反例。
2. 在不影响自动化决策的影子模式中比较模型与模板结果。
3. 固定当前 `100% / 70% / 120%` 页面为回归样本。
4. CN 稳定后再加入 JP、`digit-type-crit`、`digit-type-3` 和 `digit_v2`。
5. 每个调用方继续保留自己的合法值约束和失败重试策略。
