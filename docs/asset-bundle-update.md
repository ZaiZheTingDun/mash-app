# 更新资源包

资源包是独立于 App 和 `mash-cv` 的 CDN 发布物。App 内的
`src-tauri/resources/assets-manifest.json` 只声明该版本的 App 最多应安装到哪个资源版本；实际包、补丁链和校验值由公开的远端 manifest 决定。

当前公开入口：

```text
https://mash.xiaotongx.com/mash/assets/latest.json
```

## 资源包格式

远端 `latest.json` 指向一个不可变的 `manifests/manifest-vN.json`。该 manifest 包含：

- `base`：首次安装或强制重装所需的完整包；
- `patches`：`from` 到 `to` 的顺序补丁；
- 每个下载文件的 SHA-256 与字节大小；
- 可选的 `state`：构建时的资源状态快照。

压缩包可以将资源直接置于根目录，或放在 `assets/` 根目录下。当前受 App 管理的顶层目录为：

```text
servants/
ces/
icons/
skills/
mystic-codes/
```

基础包会替换其中存在的目录；补丁包只合并文件，不会删除未出现在补丁内的旧文件。每个包只需包含本次需要更新的受管目录。资源包根目录的 `assets-version.json` 用于人工导入时记录版本；通过 CDN 安装时，App 在成功完成计划后写入版本记录。

## 发布新的资源版本

资源构建与上传不在本仓库中执行。使用资源发布端生成 base 或 patch 压缩包后，按以下顺序发布：

1. 确定下一个递增版本 `N`，并保留从每个仍受支持版本到 `N` 的连续补丁链；无法形成链时，客户端会回退下载 base。
2. 构建压缩包。若新增顶层资源目录，先在本仓库的 `ASSET_DIRS` 中登记它，并补充安装、替换清理和导入的 Rust 测试，再生成包含该目录的包。
3. 计算每个包的 SHA-256 与精确字节大小，写入新的不可变 `manifests/manifest-vN.json`。不要修改已发布 manifest 或已发布压缩包。
4. 上传所有 base／patch 文件和新的版本化 manifest，逐个下载校验 SHA-256 与大小。
5. 最后覆盖 `latest.json`，使其指向 `manifest-vN.json`。该文件应使用 `Cache-Control: no-cache`；版本化 manifest 和压缩包应使用长期 immutable 缓存。

`latest.json` 必须最后更新，避免客户端先看到新版本但下载到尚未上传完整的文件。

## 更新 App 内目标版本

远端发布完成并验证后，更新本仓库：

1. 读取 `latest.json`，确认 `latest` 为目标版本，且它所指向的 manifest 可下载。
2. 检查从当前 App 版本到目标版本的补丁链连续，尤其确认新增或删除资源文件的 patch 已包含预期目录。
3. 将 `src-tauri/resources/assets-manifest.json` 的 `assetsVersion` 改为目标版本。
4. 同步 `src-tauri/src/tests.rs` 中 `assets_app_manifest_parses_target_version_and_latest_url` 的断言。
5. 运行：

   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml
   ```

6. 发布包含此 manifest 的 App 版本。旧 App 会继续将安装目标限制在其内置版本；新 App 才会提示用户更新到新资源包版本。

例如，资源包 v8 已发布 `patches/v7-to-v8.zip`，因此将 App 的 `assetsVersion` 从 7 提升到 8 后，已安装 v7 的用户只会下载该补丁；首次安装或选择“重新下载”时则下载 base 加完整补丁链。

## 发布后检查

- `latest.json` 的 `latest`、`latestBase` 和 `manifest` 与新版本一致。
- 新 manifest 中每个 base／patch 文件可下载，且 SHA-256、大小匹配。
- 从当前稳定版资源版本开始可计算出补丁计划；必要时在 App 中使用“重新下载”验证 base 安装路径。
- 安装结束后的 `assets-version.json` 为目标版本，且 `servants/` 与 `ces/` 均有文件；这是 App 判断资源包已安装的必要条件。
- 若引入了新的受管顶层目录，确认基础重装会替换它、补丁会合并它，并且遗留的 `<目录>.replaced-*` 会被清理。

## 回滚

不要覆盖或删除已经发布的版本化文件。若 vN 有问题，上传修复后的新版本 `vN+1` 及补丁，再将 `latest.json` 指向它。若必须暂时停止向新客户端提供 vN，可将 `latest.json` 指回已验证的旧 manifest，并在下一次 App 发布前把 `assetsVersion` 保持在该可用版本。
