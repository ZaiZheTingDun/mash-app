# Mash 数字标注页面

这是独立的本地 Node 项目，不依赖 Python Web 服务。默认读取相邻
`digit_classifier/data/manifest.jsonl` 以及其中的原图和裁剪图。

```bash
pnpm start
```

然后打开 `http://127.0.0.1:8765`。

页面支持按服务器、数字来源和标注状态筛选。快捷键：

- `0`–`9`：标记数字
- `X`：标记 `invalid`
- `S` 或右方向键：跳过/下一个
- 左方向键：上一个
- `U`：撤销
- Backspace/Delete：清除当前标签

每次标注都会通过临时文件和原子替换更新清单。服务仅监听
`127.0.0.1`，不会把截图或标签发送到外部。

可选启动参数：

```bash
node server.mjs --data-root ../digit_classifier/data --port 8765
```
