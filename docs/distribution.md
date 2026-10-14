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
        windows-x86_64/
          Mash_0.2.2_x64-setup.exe
          Mash_0.2.2_x64-setup.exe.sig
        latest.json
    stable/
      latest.json

  downloads/
    latest/
      Mash_aarch64.dmg
      Mash_x64-setup.exe
    versions/
      Mash_0.5.3_aarch64.dmg
      Mash_0.2.2_x64-setup.exe

  runtime/
    mash-cv/
      code/
        mash-cv-code-v0.2.2.zip
      runtime/
        darwin-aarch64/
          mash-cv-runtime-darwin-aarch64-v0.2.2.zip
        windows-x86_64/
          mash-cv-runtime-windows-x86_64-v0.2.2.zip
```

在 `s3://mash/mash/...` 中，第一个 `mash` 是 bucket 名，第二个 `mash` 是 object prefix。

## 版本来源

版本由 `versions.toml` 管理：

```toml
[app]
version = "0.2.2"

[mash_cv]
code = "0.2.2"

[mash_cv.runtime]
darwin-aarch64 = "0.2.2"
windows-x86_64 = "0.1.0"
```

Runtime 按操作系统和 CPU 架构独立维护版本；发布某个平台只更新该平台的版本、URL 和 SHA-256。CV code 继续共用一个版本。现有 macOS runtime 保留历史版本号 `2026.05.08-runtime1`，后续新发布版本使用 `x.x.x` 格式。tag 约定：

- App：`v0.2.2`
- CV code：`cv-code/0.2.2`
- CV runtime：`cv-runtime/darwin-aarch64/0.2.2`

## 完整发布编排

平台脚本位于 `scripts/macos/` 与 `scripts/windows/`；跨平台 Node 工具位于
`scripts/shared/`。`pnpm templates:diff` 和 `pnpm test:scripts` 已引用新路径。
以下 Bash 示例用于 macOS，Windows 原生 PowerShell 完整流程见
[Windows 构建与发布](windows.md)。

`scripts/macos/release.sh` 串联测试、更新日志、CV artifact、runtime manifest、App
版本、updater、DMG 与 Git refs。默认仅输出只读计划；添加 `--publish` 后才会
提交、打 tag 或上传，添加 `--push` 后才会推送 Git refs。

```bash
# 检查发布计划
mise exec -- scripts/macos/release.sh 0.2.2

# 只发布 App updater
mise exec -- scripts/macos/release.sh 0.2.2 --publish

# 同时发布 CV code、DMG，并在全部验证通过后推送 Git refs
mise exec -- scripts/macos/release.sh 0.2.2 \
  --cv-code 0.2.2 \
  --dmg \
  --push \
  --publish
```

发布前工作区必须干净，`CHANGELOG.md` 中必须有 `## 未发布` 或目标版本标题。
脚本会检查自上次发布以来的 CV code/runtime 相关改动；检测到改动却未提供
对应版本号时会停止实际发布。CV runtime 只在显式传入 `--cv-runtime` 时发布。

Windows 对应入口：

```powershell
# 只读计划
./scripts/windows/release.ps1 0.16.0 -CvCode 0.4.27 -CvRuntime 0.3.0 -Installer
# 执行测试、日志、版本、CV 发布、updater、安装包下载与 Git 推送
./scripts/windows/release.ps1 0.16.0 -CvCode 0.4.27 -CvRuntime 0.3.0 -Installer -Push -Publish
```

Windows 的 `-Installer` 发布 NSIS `.exe`；macOS 的 `--dmg` 发布 DMG。
Windows 编排在新版本检测到 CV 改动时要求显式版本号；对同一 `vX.Y.Z` tag
补发 Windows 时不修改日志、版本或 CV 清单，直接合并同版本 updater 平台。

## App 发布

### Windows GitHub Actions 发布

Windows App 发布使用 `.github/workflows/windows-release.yml`，并且必须在 CV code
发布 PR 合并后、App 版本 tag 已创建时运行。workflow 只接受默认分支上的 workflow
版本，`source_ref` 应指向待发布的 `vX.Y.Z` tag。

workflow 分为四个阶段：

1. `plan` 检查 tag、Tauri/Cargo/`versions.toml` 版本一致性，并锁定 Windows
   manifest 中的 CV code/runtime URL 与 SHA-256；
2. `build` 在 Windows runner 上安装锁定依赖、准备 ADB、执行前端/脚本/Rust 测试，
   只构建一次签名 NSIS 安装包，并保存安装包、`.sig` 与 SHA-256 receipt；
3. `verify` 校验候选包在 job 之间没有被替换，确认它是 Windows PE/NSIS 文件且有
   updater signature，并在干净 runner 上静默安装、启动、退出和卸载；
4. `publish` 只复用候选包：先发布版本化 direct download，再发布 updater artifact
   和同版本 manifest，最后更新 `latest.json`。发布脚本要求 R2 object 与公开 CDN
   可访问；重复运行时不会重新构建安装包。

`windows-release` environment 需要 `R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、
`TAURI_SIGNING_PRIVATE_KEY`（以及可选的
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）secrets，以及 `R2_ENDPOINT`、`R2_BUCKET`、可选的
`RELEASE_BASE_URL`、`R2_PREFIX` variables。默认 `publish=false` 只跑候选构建和验证；
人工确认后再勾选 `publish`。`publish_installer=false` 可只更新 updater channel，
保留已有 direct download。

Windows x64 的原生构建和 PowerShell 发布入口见 [Windows 构建与验证](windows.md)。
跨平台 updater 必须串行发布，并合并同一版本的 `platforms` 条目。

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
scripts/macos/bump-app-version.sh 0.2.2
git push --follow-tags
```

应用 updater 使用本地发布入口：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/macos/release-tauri-updater.sh
```

本地脚本要求当前 commit 恰好位于 `vX.Y.Z` tag 且 worktree 干净。
macOS 构建 `app` bundle，不生成 DMG；Windows 构建 NSIS 安装包。
脚本保留同版本已发布的平台，并拒绝将 channel 降级到更旧版本。

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
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/macos/release-dmg.sh
```

## CV Code 发布

CV code package 包含轻量 Python source，是 `sidecar/mash_cv/mash_cv/` 下识别逻辑变更的高频发布包。

发布流程：

```bash
scripts/macos/bump-cv-code-version.sh 0.2.2
git push origin cv-code/0.2.2
```

版本更新脚本会创建 `cv-code/*` tag；当前仓库没有 CV code 上传 workflow，
请使用本地发布入口上传（仅推送 tag 不会上传）：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/macos/release-cv-code.sh 0.2.2
```

脚本构建：

```text
sidecar/mash_cv/dist/mash-cv-code-v0.2.2.zip
```

并上传至：

```text
mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip
```

发布脚本会打印公开 URL 和 SHA-256。请用这些值更新当前平台的 `src-tauri/resources/runtime-manifest.json`，包括 `codeUrl`、`codeSha256` 与 `mashCvCodeVersion`。完整发布编排会自动完成这一步。

Windows 使用 `scripts/windows/bump-cv-code-version.ps1 <version>` 和
`scripts/windows/release-cv-code.ps1 <version>`。code ZIP 内容由共享 Python
builder 生成，与平台无关；不要重复发布同版本但不同内容的 ZIP。

### GitHub Actions code 发布

正式发布使用 `.github/workflows/cv-code-release.yml` 的手动运行入口。workflow
默认只构建候选包；勾选 `publish` 后，必须在默认分支上运行，并按以下顺序执行：

发布 environment `cv-code-release` 需要配置 `R2_ACCESS_KEY_ID` 与
`R2_SECRET_ACCESS_KEY` secrets，以及 `R2_ENDPOINT`、`R2_BUCKET` 和可选的
`RELEASE_BASE_URL`、`R2_PREFIX` variables。为该 environment 配置人工审批，
使候选测试和真正上传分开。

1. 固定 `source_ref` 到 commit SHA，读取该 commit 中已发布的 Windows x64 与
   macOS Apple Silicon runtime 版本、URL 和 SHA-256；
2. 在 Linux 上构建一次 `mash-cv-code-v<version>.zip`，保存候选包和发布记录；
3. 在 Windows 与 macOS runner 上分别下载清单中的原生 runtime，使用同一份 code
   ZIP 做源码测试、冻结 runtime 兼容性测试和对应平台的 Rust 导入测试；
4. 使用候选包原文件上传 R2，使用条件写入保护不可变对象，再从公开 CDN 完整下载
   并验证 SHA-256；
5. 创建或验证 `cv-code/<version>` tag，并生成更新 `versions.toml` 与
   `runtime-manifest.json` 的 PR。

Code 版本是所有平台共用的字段，因此 PR 会同步更新 manifest 中每个平台的
`codeUrl` 与 `codeSha256`；runtime 字段和 runtime 文件不会被修改。发布失败后应
重新运行同一个候选版本，脚本会复用同 SHA-256 的已上传对象，并拒绝覆盖不同内容。
App 发布必须等这个 PR 合并后再创建新的 App tag。

## CV Runtime 发布

CV runtime package 包含 PyInstaller onedir build、native dependency、model 及 runtime layout。它体积较大，应低频发布。仅在以下变更时升级 runtime 版本：

- PyInstaller dependency
- Native dependency
- OCR model 或其他重量级 runtime resource
- Runtime launcher 或 archive layout

发布流程：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/macos/release-cv-runtime.sh 0.2.2
```

脚本将：

1. 更新 `versions.toml` 中的 `[mash_cv.runtime].<platform>`；
2. 构建 `mash-cv-runtime-<platform>-v<version>.zip`；
3. 将 artifact 上传至 R2；
4. 计算 SHA-256；
5. 更新 `src-tauri/resources/runtime-manifest.json`；
6. 创建 commit；
7. 为 commit 打上 `cv-runtime/<platform>/<version>` tag。

上传前脚本会打印 artifact 名称、大小、S3 target 与公开 URL；上传后打印耗时和平均上传速度。

Windows 原生 runtime 使用 `scripts/windows/release-cv-runtime.ps1 <version>`，
也执行构建、上传、远端验证、清单更新、commit 和平台 tag。
`-Platform` 必须匹配原生 MSVC host；不支持把本机 ZIP 标记成另一平台。

## Windows 安装包下载

`scripts/windows/release-installer.ps1` 从干净的 app tag 构建 NSIS 安装包，
先上传并验证版本文件，再覆盖稳定下载链接：

```text
mash/downloads/versions/Mash_0.16.0_x64-setup.exe
mash/downloads/latest/Mash_x64-setup.exe
```

`-SkipBuild` 复用已经构建的当前版本签名产物，完整编排在 updater 成功后使用
此选项。只选择当前版本和架构的 `.exe` 与 `.sig`，不会回退到旧安装包。
版本文件使用 immutable 长缓存，latest 使用 `no-cache`。Windows 下载入口保留
所有历史版本；macOS DMG 的保留数量策略保持不变。

默认发布签名安装包。没有 Tauri updater 私钥时，可以显式使用 `-Unsigned`
构建并发布可手动安装的 `.exe`；这不会生成签名、不会上传 updater artifact，
也不会修改 `stable/latest.json`：

```powershell
./scripts/windows/release-installer.ps1 -Unsigned
```

## Runtime Manifest

`src-tauri/resources/runtime-manifest.json` 会随 app 打包。应用启动时用它判断必须在本机安装哪些 CV code/runtime 版本。

```json
{
  "mashCvCodeVersion": "0.2.2",
  "platforms": {
    "darwin-aarch64": {
      "runtimeVersion": "0.2.2",
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
app_data_dir()/runtime/mash-cv/runtime/<platform.runtimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<mashCvCodeVersion>/mash-cv-code/
```

本机缺少 manifest 所要求的版本时，app 会提示用户安装 CV package。更新 app 不代表 CV package 已安装；是否需要安装仍由 manifest 与本地安装状态共同决定。

从全局 runtime 版本迁移后，macOS 使用现有产物的历史版本号
`2026.05.08-runtime1`。若旧安装记录被标为 `0.1.0`，应用会要求重新安装
清单所指向的现有 runtime 包；下载地址和 SHA-256 没有变化。

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
