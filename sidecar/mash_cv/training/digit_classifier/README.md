# Mash 通用数字分类器训练工具

该子项目用于离线准备和训练 Mash 的游戏 UI 单数字分类器。运行中的
`mash-cv` 不依赖 PyTorch；最终模型导出为 ONNX，由 OpenCV DNN 加载。

模型只负责识别已经定位好的单个数字候选。页面定位、数字分割和业务约束
仍由各识别路径负责。例如宝具条百位只允许 `1`、`2`、`3`，场次数字仍需
组成合法的 `当前/总数`。

## 数据目录

`data/` 和 `runs/` 仅保存在本地并已加入 Git 忽略：

```text
data/
  raw/                 原始完整截图
  crops/               从截图提取出的单字候选
  manifest.jsonl       标签和来源信息
runs/                  训练日志、检查点和临时导出
```

每个样本必须记录原始截图的 `parentId`。训练、验证、测试划分按
`parentId` 分组，避免同一帧的近似裁剪泄漏到不同数据集。

将完整战斗截图按服务器放到 `data/raw/cn/` 或 `data/raw/jp/` 后运行：

```bash
poetry run digit-crop
```

工具会同时提取己方 HP、敌方 HP、宝具、战斗场次、敌方数量和回合数，
并生成三类本地文件：`regions/` 保存整行区域，`crops/` 保存待标注单字，
`previews/` 在完整截图上绘制区域及候选框。裁剪命令可以重复运行；清单中
已有的标签、划分和备注会按稳定样本 ID 保留。原始截图不会被修改。

本地开发版可在「设置 → 调试」开启“点击攻击前自动截图”。Runner 会在
真正点击攻击按钮前把最新 Battle 视频帧以无损 PNG 保存到
`app_data_dir()/debug/battle-before-attack/{cn|jp}/`；文件名带服务器、任务轮次、
Battle index 和 turn index，可作为宝具条与其他战斗数字的首批原始数据。

清单中的 `label` 可以是字符串 `0` 到 `9`、`invalid` 或 `null`。`invalid`
应覆盖空白、横线、百分号、汉字笔画、台词、特效和截断字形；不能只收集
正确数字，否则分类器面对干扰时会被迫输出某个数字。

示例：

```json
{"schemaVersion":1,"id":"np-cn-frame-001-slot-1-hundreds","image":"crops/np-cn-frame-001-slot-1-hundreds.png","label":null,"source":"np_gauge","style":"digit","server":"cn","parentId":"sha256:...","resolution":[2560,1440],"sourceBboxPx":[1096,1315,28,40],"split":null,"notes":null}
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

# 训练、评估并导出 ONNX
poetry run digit-train
```

标注工具每次确认后都会原子写回清单，意外退出不会丢失之前的标签。

可视化标注页面是相邻的独立 Node 项目：

```bash
cd ../digit-labeler
pnpm start
```

浏览器打开 `http://127.0.0.1:8765`。页面左侧显示完整战斗截图和当前候选框，
右侧放大单字；可以按服务器、来源和标注状态筛选。数字键 `0`–`9` 直接标注，
`X` 标记 `invalid`，`S` 跳过，`U` 撤销，方向键浏览，Backspace 清除标签。

## 模型产物

最终文件应放在：

```text
mash_cv/assets/digit_classifier/digit-classifier-v1.onnx
mash_cv/assets/digit_classifier/digit-classifier-v1.json
```

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
