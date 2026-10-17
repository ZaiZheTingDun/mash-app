# Mash 数字标注页面

这是独立的本地 Node 项目，不依赖 Python Web 服务。默认读取相邻
`digit_classifier/data/manifest.jsonl` 以及其中的原图和裁剪图。

```bash
pnpm start
```

然后打开 `http://127.0.0.1:8765`。

页面上方可切换“单个数字区域”和“完整数字区域（CTC）”。两个视图各自
按截图类型、服务器、数字来源和标注状态筛选。完整数字视图显示原图、整块
裁剪及旧单字标签拼出的候选；只有点击确认或输入正确整串数字并保存后，
该样本才会进入 CTC 训练。无数字或裁切错误可标为“无效”。

单字视图快捷键：

- `0`–`9`：标记数字
- `X`：标记 `invalid`
- `S` 或右方向键：跳过/下一个
- 左方向键：上一个
- `U`：撤销
- Backspace/Delete：清除当前标签

每次标注都会通过临时文件和原子替换更新清单。服务仅监听
`127.0.0.1`，不会把截图或标签发送到外部。
单字标签保存在 `manifest.jsonl`，完整数字标签保存在
`sequence-manifest.jsonl`。已有完整区域裁剪时，页面首次启动会生成后者；
以后运行 `digit-crop` 也会生成并保留已确认的完整数字标签。

可选启动参数：

```bash
node server.mjs --data-root ../digit_classifier/data --port 8765
```
