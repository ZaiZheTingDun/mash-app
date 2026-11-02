# Reqable API key 自动导入

抓包页面点击「开始」时，Reqable 3.3.9 及以上版本使用以下流程：

1. 读取当前 ADB 设备对应的已保存 key，通过 Reqable 状态接口验证；有效时直接复用。
2. 没有 key，或已保存的 key 认证失败时，建立临时 scrcpy 2.7 控制连接。页面提示用户在 Reqable 开启 API 服务、创建 API key 并点击「复制」。
3. 等待模拟器的剪贴板变化，忽略空文本、无关格式及重复的无效 key，并确认当前 Android 焦点窗口属于 Reqable。候选值只发送到当前设备经 ADB 转发的 Reqable 状态接口，用 Bearer 认证验证，不修改捕获状态。
4. 验证成功后关闭剪贴板连接，按设备保存 key，再继续原有抓包启动流程。启动、结果读取及停止请求均携带认证。

Reqable 3.3.5–3.3.8 保持原有无 key 流程。服务未开启、设备断开、存储失败等情况明确报错，不覆盖原有 key。等待最多 120 秒，等待期间可以点击「停止」。

前台检查先查询 `dumpsys window displays`，缺少焦点字段时回退到完整 `dumpsys window`；不使用部分模拟器不提供焦点字段的 `dumpsys window windows`。两次查询仍无法取得焦点信息时明确报错，不将缺失字段当作「Reqable 不在前台」而静默丢弃 key。页面分别显示复制内容格式无效、Reqable 不在前台、正在验证以及认证失败等进度，均不包含 key 内容。

## 连接与存储

- 控制连接单独使用随机 scrcpy 会话、远端临时 JAR 和动态 ADB 转发端口，不启用视频、音频或 CV sidecar，不受触摸控制模式影响。
- 仅在上述等待窗口接收复制事件。key 验证成功、取消、超时或异常退出时关闭连接、回收子进程并移除本次转发及临时 JAR。
- 临时连接的 `clipboard_autosync=true` 只将模拟器复制事件发送给本次连接，不与电脑剪贴板同步，不注入复制、剪切或粘贴按键。
- key 存储在应用数据目录下的 `packet-capture/api-keys.json`，按 ADB serial 区分，使用现有私有临时文件及原子替换机制保存。这是本地配置文件存储，不是系统钥匙串加密存储。
- key 不包含在 Tauri 返回值、页面进度、抓包历史或日志中；Authorization 头标记为 sensitive，HTTP 错误不回显远端响应正文。

## 验证范围

Rust 测试覆盖剪贴板消息分片/边界、UTF-8、读取上限、版本边界、按设备持久化、复用有效 key、候选值验证、取消/超时/断开、Bearer 请求头以及抓包会话认证隔离。前端测试覆盖等待提示、成功后的取消订阅、停止及迟到事件隔离。

已在当前模拟器只读验证 `windows` 查询缺失 `mCurrentFocus`、`displays` 查询可取得 Reqable 焦点信息。Reqable 3.3.9+ 的完整「创建 → 复制 → 自动导入」流程仍需重新实测；诊断过程未读取或输出实际 API key。

协议参考：[scrcpy 2.7](https://github.com/Genymobile/scrcpy/tree/v2.7/server/src/main/java/com/genymobile/scrcpy/control)、[Reqable 官方 API 客户端](https://github.com/reqable/reqable-mcp-server/blob/main/lib/api/client.dart)。
