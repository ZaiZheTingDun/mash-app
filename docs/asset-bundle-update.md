# 更新资源包

资源包是独立于 App 和 `mash-cv` 的 CDN 发布物。App 内的
`src-tauri/resources/assets-manifest.json` 的 `assetsVersion` 声明该版本的 App 最多应在线安装到哪个资源版本；在线使用的包、补丁链和校验值由公开的远端 manifest 决定。本地文件导入则使用同一配置文件中的 `manualImport` 校验清单。

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
events/
catalogs/
```

基础包会替换其中存在的目录；补丁包只合并文件，不会删除未出现在补丁内的旧文件。每个包只需包含本次需要更新的受管目录。资源包根目录的 `assets-version.json` 用于人工导入时记录版本；通过 CDN 安装时，App 在成功完成计划后写入版本记录。

从 v19 起，六份汇总数据统一位于 `catalogs/`：`servants.json`、`servants_variants.json`、`servants_variants_cn.json`、`craft_essences.json`、`events.json` 和 `event_items.json`。App 只读取已安装的汇总数据，不内置旧包兼容副本；文件缺失或无法解析时会报错，需重新下载资源包。安装或更新资源后，后端目录及派生索引缓存自动失效。单个从者、礼装的详细 JSON 仍按需从原目录读取。

## 本地导入升级包

“从本地导入”可以选择一个完整素材包，或一次选择一个／多个 mash-data 升级包。完整包仍通过 `assets-version.json` 记录版本并替换包内包含的目录，不能与升级包混选；多选仅用于升级包。

手动导入先校验 ZIP 的 SHA-256，再核对包内元数据，文件名可以任意修改：

- `patch-manifest.json`：`fromVersion` 和 `toVersion` 为整数，目标版本必须高于起始版本；
- `deletes.json`：`{"files":["ces/123/old.png"]}`，列出相对资源根目录的待删除文件；空列表也必须保留该文件；
- 受管目录中的新增／修改文件会合并到现有目录，其余文件保留。仅删除文件的补丁及无文件变化的版本补丁也受支持。

导入升级包要求本地已有版本记录，且 `servants/`、`ces/` 均包含文件。单包必须从当前版本升级；多包的选择顺序不限，后端按起始版本排序，要求所有选中包恰好组成从当前版本出发、严格递增的连续链。重复、分叉、重叠、断链或起点不匹配会拒绝整批导入。可以分次升级，例如当前 v14、App 目标 v19 时，先导入 v14 → v15、v15 → v16 与 v16 → v17，成功后版本为 v17，仍会提示需要升级到 v19。

手动导入的全部校验配置位于 `src-tauri/resources/assets-manifest.json`：

- `manualImport.bases`：手动完整包的 `version`、`sha256`。目前仅登记 v18、v19 两个手动包，其他完整包不可手动导入。
- `manualImport.patches`：升级包的 `from`、`to`、`sha256`、`size`；`file` 保留发布路径用于追溯，不限制本地文件名。

校验清单随 App 提供，手动导入无需联网。未登记、SHA-256 不符、大小不符或包内版本信息与配置不符时拒绝整批导入。重新压缩即使资源内容相同也可能改变 ZIP 的 SHA-256，新的手动包或升级包必须先更新此配置。缺少 `manualImport` 时手动导入不接受任何文件；在线安装继续按远端 manifest 校验，不受这个清单限制。

后端将所选 ZIP 暂存并同时计算 SHA-256，后续只解压已验证的暂存副本，避免校验后原文件变化。写入前解压并校验全部升级包，删除清单只允许受管目录下的普通文件路径，禁止越界路径、目录删除及符号链接目标。应用文件与写入最终版本属于同一次导入；导入失败或取消会尝试恢复本次改动的文件与原版本，若恢复失败会提示重新安装完整包。暂存、哈希计算和解压阶段支持取消。

安装成功后，App 会重新检查资源就绪状态，并重新读取从者、礼装和御主礼装列表；汇总数据读取成功后会清除先前的加载错误，无需重启。若新包仍缺少 `catalogs/servants.json` 等必需文件，界面会继续显示实际读取错误。分段升级仍低于 App 目标版本时，资源管理页保持挂载并正常显示，可继续导入下一批升级包。

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
