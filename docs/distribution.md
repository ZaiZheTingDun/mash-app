# 发布与分发

mash 发布三个独立版本的 artifact：应用、`mash-cv` code package 与 `mash-cv` runtime package。三者都经由公开 CDN 发布到 Cloudflare R2，但发布频率和更新入口不同。

## R2 布局

当前默认值为 `R2_BUCKET=mash` 与 `R2_PREFIX=mash`。公开 URL 由 `RELEASE_BASE_URL` 与 object key 拼接而成。

```text
mash/
  releases/
    versions/
      v0.2.2/
        darwin-aarch64/
          mash.app.tar.gz
          mash.app.tar.gz.sig
        latest.json
    stable/
      latest.json

  downloads/
    latest/
      Mash_aarch64.dmg
    versions/
      Mash_0.5.3_aarch64.dmg

  runtime/
    mash-cv/
      code/
        mash-cv-code-v0.2.2.zip
      runtime/
        darwin-aarch64/
          mash-cv-runtime-darwin-aarch64-v0.2.2.zip
```

在 `s3://mash/mash/...` 中，第一个 `mash` 是 bucket 名，第二个 `mash` 是 object prefix。

## 版本来源

版本由 `versions.toml` 管理：

```toml
[app]
version = "0.2.2"

[mash_cv]
runtime = "0.2.2"
code = "0.2.2"
```

所有发布版本均使用 `x.x.x` 格式。tag 约定：

- App：`v0.2.2`
- CV code：`cv-code/0.2.2`
- CV runtime：`cv-runtime/darwin-aarch64/0.2.2`

## App 发布

应用通过 Tauri updater 分发；客户端仅读取 channel manifest：

```text
https://mash.xiaotongx.com/mash/releases/stable/latest.json
```

`latest.json` 指向不可变 artifact：

```text
mash/releases/versions/v0.2.2/darwin-aarch64/mash.app.tar.gz
```

发布流程：

```bash
scripts/bump-app-version.sh 0.2.2
git push --follow-tags
```

推送 `v*.*.*` tag 会触发 `.github/workflows/release-app.yml`，构建 macOS app 并将 updater artifact 上传到 R2。也支持本地发布：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-tauri-updater.sh
```

本地脚本要求当前 commit 恰好位于 `vX.Y.Z` tag 且 worktree 干净。它只构建 `app` bundle，不生成 DMG。

## DMG 安装包下载

DMG 安装包是直接供用户下载的产物，与 Tauri updater channel 分离，发布在 `downloads/`：

```text
mash/downloads/latest/Mash_aarch64.dmg
mash/downloads/versions/Mash_0.5.3_aarch64.dmg
```

公开 URL：

```text
https://mash.xiaotongx.com/mash/downloads/latest/Mash_aarch64.dmg
https://mash.xiaotongx.com/mash/downloads/versions/Mash_0.5.3_aarch64.dmg
```

`downloads/latest/` 是网站链接和手动安装使用的可变稳定下载路径。`downloads/versions/` 存放可回滚、可手动恢复的版本化 DMG。应先发布版本化 DMG，再覆盖 `downloads/latest/` 下的对应文件。发布脚本会为同一 architecture 仅保留最近三个版本化 DMG。

发布流程：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-dmg.sh
```

## CV Code 发布

CV code package 包含轻量 Python source，是 `sidecar/mash_cv/mash_cv/` 下识别逻辑变更的高频发布包。

发布流程：

```bash
scripts/bump-cv-code-version.sh 0.2.2
git tag cv-code/0.2.2
git push origin cv-code/0.2.2
```

推送 `cv-code/*` tag 会触发 `.github/workflows/release-cv-code.yml`，调用 `scripts/release-cv-code.sh`。同一脚本也可本地执行：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-code.sh 0.2.2
```

脚本构建：

```text
sidecar/mash_cv/dist/mash-cv-code-v0.2.2.zip
```

并上传至：

```text
mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip
```

workflow summary 会打印公开 URL 和 SHA-256。请用这些值更新当前平台的 `src-tauri/resources/runtime-manifest.json`，包括 `codeUrl`、`codeSha256` 与 `mashCvCodeVersion`。

## CV Runtime 发布

CV runtime package 包含 PyInstaller onedir build、native dependency、model 及 runtime layout。它体积较大，应低频发布。仅在以下变更时升级 runtime 版本：

- PyInstaller dependency
- Native dependency
- OCR model 或其他重量级 runtime resource
- Runtime launcher 或 archive layout

发布流程：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-runtime.sh 0.2.2
```

脚本将：

1. 更新 `versions.toml` 中的 `[mash_cv].runtime`；
2. 构建 `mash-cv-runtime-<platform>-v<version>.zip`；
3. 将 artifact 上传至 R2；
4. 计算 SHA-256；
5. 更新 `src-tauri/resources/runtime-manifest.json`；
6. 创建 commit；
7. 为 commit 打上 `cv-runtime/<platform>/<version>` tag。

上传前脚本会打印 artifact 名称、大小、S3 target 与公开 URL；上传后打印耗时和平均上传速度。

## Runtime Manifest

`src-tauri/resources/runtime-manifest.json` 会随 app 打包。应用启动时用它判断必须在本机安装哪些 CV code/runtime 版本。

```json
{
  "mashCvRuntimeVersion": "0.2.2",
  "mashCvCodeVersion": "0.2.2",
  "platforms": {
    "darwin-aarch64": {
      "runtimeUrl": "https://mash.xiaotongx.com/mash/runtime/mash-cv/runtime/darwin-aarch64/mash-cv-runtime-darwin-aarch64-v0.2.2.zip",
      "runtimeSha256": "...",
      "codeUrl": "https://mash.xiaotongx.com/mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip",
      "codeSha256": "..."
    }
  }
}
```

本地安装路径：

```text
app_data_dir()/runtime/mash-cv/runtime/<mashCvRuntimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<mashCvCodeVersion>/mash-cv-code/
```

本机缺少 manifest 所要求的版本时，app 会提示用户安装 CV package。更新 app 不代表 CV package 已安装；是否需要安装仍由 manifest 与本地安装状态共同决定。

## 上传说明

R2 上传使用 S3 API endpoint：

```text
https://<account-id>.r2.cloudflarestorage.com
```

该 endpoint 不同于公开 CDN domain。Cloudflare zone 级 WAF、Bot 与 Block AI Bots 设置通常不影响 S3 API endpoint。若出现 TCP connect timeout，问题位于本机到 R2 S3 API 的网络路径。常见处理方式：

- 让终端经过 proxy 或 VPN；
- 切换网络；
- 改用 GitHub Actions 上传。

连通性检查：

```bash
curl -I https://<account-id>.r2.cloudflarestorage.com
```

若该命令超时，发布脚本也会超时。

## 缓存策略

不可变 artifact 使用长缓存：

```text
Cache-Control: public, max-age=31536000, immutable
```

可变 channel manifest 使用短缓存或不缓存：

```text
Cache-Control: no-cache
```

App 发布时，先上传版本化 artifact，再上传版本化 `latest.json`，最后覆盖 `stable/latest.json`。否则客户端可能先看到新版本，却遇到仍在上传的 artifact。
