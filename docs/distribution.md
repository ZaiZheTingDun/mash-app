# Distribution

mash 的发布物分成三层：app 本体、`mash-cv` code 包、`mash-cv` runtime 包。三层都发布到 Cloudflare R2 后面的 CDN，但更新节奏和入口不同。

## R2 Layout

当前默认 `R2_BUCKET=mash`，`R2_PREFIX=mash`。公开 URL 由 `RELEASE_BASE_URL` 加 object key 拼出。

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

  runtime/
    mash-cv/
      code/
        mash-cv-code-v0.2.2.zip
      runtime/
        darwin-aarch64/
          mash-cv-runtime-darwin-aarch64-v0.2.2.zip
```

`s3://mash/mash/...` 里第一个 `mash` 是 bucket，第二个 `mash` 是 object prefix。

## Version Sources

版本号集中在 `versions.toml`：

```toml
[app]
version = "0.2.2"

[mash_cv]
runtime = "0.2.2"
code = "0.2.2"
```

版本格式统一使用 `x.x.x`。对应 tag 规则：

- app: `v0.2.2`
- CV code: `cv-code/0.2.2`
- CV runtime: `cv-runtime/darwin-aarch64/0.2.2`

## App Release

app 本体走 Tauri updater。客户端只读取 channel manifest：

```text
https://mash.xiaotongx.com/mash/releases/stable/latest.json
```

`latest.json` 指向 immutable artifact：

```text
mash/releases/versions/v0.2.2/darwin-aarch64/mash.app.tar.gz
```

发布方式：

```bash
scripts/bump-app-version.sh 0.2.2
git push --follow-tags
```

推送 `v*.*.*` tag 后，`.github/workflows/release-app.yml` 会在 GitHub Actions 里构建并上传到 R2。也可以本地运行：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-tauri-updater.sh
```

本地脚本要求当前 commit 正好在 `vX.Y.Z` tag 上，且工作区干净。脚本只构建 `app` bundle，不生成 DMG。

## CV Code Release

CV code 包只包含轻量 Python 源码，适合高频发布。改动范围通常是 `sidecar/mash_cv/mash_cv/` 下的识别逻辑。

发布方式：

```bash
git tag cv-code/0.2.2
git push origin cv-code/0.2.2
```

推送 `cv-code/*` tag 后，`.github/workflows/release-cv-code.yml` 会构建：

```text
sidecar/mash_cv/dist/mash-cv-code-v0.2.2.zip
```

并上传到：

```text
mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip
```

workflow 会在 summary 输出 URL 和 SHA-256。拿到结果后，更新 `src-tauri/resources/runtime-manifest.json` 当前平台的 `codeUrl` 和 `codeSha256`，并同步 `mashCvCodeVersion`。

## CV Runtime Release

CV runtime 包包含 PyInstaller onedir、native dependency、模型和运行时布局，体积大、发布低频。只有以下变化需要 bump runtime：

- PyInstaller 依赖变化
- native dependency 变化
- OCR 模型或重型资源变化
- runtime launcher 或 archive layout 变化

发布方式：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-runtime.sh 0.2.2
```

脚本会：

1. 更新 `versions.toml` 的 `[mash_cv].runtime`
2. 构建 `mash-cv-runtime-<platform>-v<version>.zip`
3. 上传到 R2
4. 计算 SHA-256
5. 更新 `src-tauri/resources/runtime-manifest.json`
6. commit
7. 打 `cv-runtime/<platform>/<version>` tag

上传前会打印文件名、大小、S3 target 和公开 URL。上传完成后会打印平均速度。

## Runtime Manifest

`src-tauri/resources/runtime-manifest.json` 随 app bundle 一起发布。app 启动后根据 manifest 判断本机是否已经安装所需 CV code/runtime。

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

用户本机安装路径：

```text
app_data_dir()/runtime/mash-cv/runtime/<mashCvRuntimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<mashCvCodeVersion>/mash-cv-code/
```

如果本机缺少 manifest 要求的版本，app 会提示安装 CV 包。app 本体更新不会自动代表 CV 包已经更新；CV 包是否需要安装由 manifest 和本机安装状态决定。

## Upload Notes

R2 上传使用 S3 API endpoint：

```text
https://<account-id>.r2.cloudflarestorage.com
```

这个 endpoint 和公开 CDN 域名不同。Cloudflare zone 里的 WAF、Bot、Block AI Bots 通常不影响 S3 API endpoint。若出现 TCP connect timeout，问题在本机网络到 R2 S3 API 的连接路径，常见处理方式是：

- 让终端显式走代理或 VPN
- 换网络
- 改用 GitHub Actions 上传

验证连通性：

```bash
curl -I https://<account-id>.r2.cloudflarestorage.com
```

如果这里已经 timeout，发布脚本也会 timeout。

## Cache Policy

Immutable artifacts 使用长缓存：

```text
Cache-Control: public, max-age=31536000, immutable
```

Mutable channel manifest 使用短缓存或不缓存：

```text
Cache-Control: no-cache
```

发布 app 时必须先上传 versioned artifact，再上传 versioned `latest.json`，最后覆盖 `stable/latest.json`。否则客户端可能看到新版本，但 artifact 还没上传完成。
