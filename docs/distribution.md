# 发布指南

供维护者发布应用与 CV 组件。开发构建见[贡献指南](../CONTRIBUTING.md)，图片资源的打包和回滚见[资源更新流程](asset-bundle-update.md)。

## 组件版本

| 改动 | 更新位置 |
| --- | --- |
| 应用发布 | `versions.toml` 的 `[app].version` |
| Python 识别代码 | `[mash_cv].code` 及运行时清单中的 code 版本、URL、SHA-256 |
| 原生依赖、OCR 模型、启动器或包结构 | `[mash_cv.runtime].<platform>` 及该平台的 runtime 清单 |
| 图片资源 | 先发布远端资源包，再更新 `assets-manifest.json` 的目标版本 |

CV code 跨平台共用，runtime 按平台独立版本化；仅修改 UI 或 Rust 时不升级 CV。版本与清单由发布入口更新，格式见 [CV 资源](../src-tauri/resources/README.md#runtime-manifestjson)。

Tag 分别为 `vX.Y.Z`、`cv-code/X.Y.Z` 和 `cv-runtime/<platform>/X.Y.Z`。

## 发布环境

应用和 runtime 在对应原生平台构建，需开发依赖、AWS CLI、R2 凭据和 Tauri updater 签名私钥。

| 环境变量 | 用途 |
| --- | --- |
| `R2_ENDPOINT` / `R2_BUCKET` | R2 S3 API 地址与 bucket |
| `RELEASE_BASE_URL` | 公开下载域名 |
| `TAURI_SIGNING_PRIVATE_KEY` | updater 签名私钥或路径 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码，按需设置 |
| `AWS_PROFILE` | 凭据配置；Windows 默认使用 AWS 凭据链，macOS 默认 `mash` |
| `R2_PREFIX` / `RELEASE_CHANNEL` | 可选对象前缀与更新通道 |

对象前缀默认 `mash`；macOS 通道默认 `stable`，Windows 默认 `windows-stable`。凭据放在本机环境或 CI secrets。Windows Authenticode 与 updater 签名独立；未签名安装包仅供手动安装。

## 发布入口

### GitHub Actions：预检查与联合发布

应用 CI 发布分为两个手动 workflow：

1. 在默认分支运行 [Release preflight](../.github/workflows/release-preflight.yml)，填写 `source_ref` 和目标 `app_version`。若实际 CV 包源码变化，另填递增的 `cv_code_version`；未变化时留空。
2. 查看 Actions Summary 中的发布范围、源码 SHA、CV 基准及阻塞原因。预检查只读 Git 对象，不修改版本、创建 tag 或上传发布物；`release-plan` artifact 保存 `release-plan.json`，保留 30 天。
3. 只有成功的预检查可用于发布。在默认分支运行 [App release](../.github/workflows/app-release.yml)，填写预检查的 **run ID**。该操作执行正式发布；workflow 不接受任意上传的计划，也不重新选择最新 main。
4. 发布成功后合并自动创建的 `codex/app-release-<version>` 版本同步 PR，使默认分支保存实际发布的版本、CV 清单和日志。若 main 已发生冲突，sync job 会失败，已完成的发布仍然有效；需手动同步该发布提交的元数据。

预检查以 App 当前 `runtime-manifest.json` 引用的 `cv-code/<version>` tag 为 CV 基准，比较实际被打包的源码；不取最新 CV tag。测试变更会记录，但不会单独要求 CV 升版；App 内的模板随 App 发布。模型、原生依赖、启动器和 runtime 构建脚本变化会阻塞发布，要求先独立发布并应用对应平台的 runtime。历史 runtime 没有对应 tag 时，以该平台版本在 `versions.toml` 中的引入提交为基准；缺少基准时阻塞。

执行时，共享 CV ZIP 只构建一次；无变化时下载并校验 App 已引用的 ZIP。随后写入准确的 CV URL/SHA-256、应用版本和日志，生成以所选源码为父提交的固定发布提交。这个提交在构建阶段仅通过 Git bundle 传递，不提前推送 tag。Windows x64 与 macOS Apple Silicon 使用同一份 CV ZIP 和固定 runtime，运行源码测试、冻结 runtime 的协议/OCR 检查、Rust 安装集成测试和应用构建。macOS Intel 暂不参与该矩阵。

所有平台通过后，候选会保存源码 bundle、依赖记录、平台测试记录、文件大小与 SHA-256；updater 签名使用各平台已配置的公钥通过 minisign 验证。publish job 复用这些候选文件，条件上传 R2 并下载校验，再检查公开 CDN 的完整文件哈希；随后创建源码 tag、验证 GitHub Release 附件，最后更新 `stable` 和 `windows-stable` 两个通道及直接下载链接。两个通道都包含本批次的平台条目。

配置如下（R2 bucket 默认 `mash`，对象前缀默认 `mash`）：

| 位置 | 配置 |
| --- | --- |
| Repository variables | 可选 `RELEASE_BASE_URL`、`R2_PREFIX`，预检查与执行必须一致 |
| `app-release-build` environment variables | `R2_ENDPOINT`，可选 `R2_BUCKET` |
| `app-release-build` environment secrets | 只读 R2 的 `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY`；`TAURI_SIGNING_PRIVATE_KEY_WINDOWS` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD_WINDOWS`；`TAURI_SIGNING_PRIVATE_KEY_MACOS` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD_MACOS` |
| `app-release` environment variables | `R2_ENDPOINT`，可选 `R2_BUCKET`，与构建 environment 指向相同存储 |
| `app-release` environment secrets | 可读写 R2 的 `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` |

Windows 与 macOS 当前公钥不同，需分别提供匹配的 updater 私钥。签名私钥只传给对应平台的打包步骤；R2 写凭据只传给 publish job。可按维护策略为 `app-release` environment 配置审批；构建 environment 的审批也会影响候选准备和测试。仓库还需允许 Actions 创建 PR。

重试时优先选择 **Re-run failed jobs**，复用已验证的候选。已有同路径文件只有哈希相同才允许复用；tag 不移动，通道拒绝降级，并通过 ETag 条件写入防止覆盖并发发布。两个通道、GitHub 和直接下载链接之间不是原子事务，中断后可能部分入口已推进；重试会完成剩余步骤。不要同时运行本地发布脚本与 CI 发布，也不要在同一版本下重新构建不同的二进制。

### 本地发布

先提交工作区改动，在 `CHANGELOG.md` 的“未发布”或目标版本标题下准备更新说明。以下为命令用法，版本参数均替换为目标递增版本。

macOS：

```text
scripts/macos/release.sh <app-version>
scripts/macos/release.sh <app-version> --dmg --push --publish
```

Windows：

```text
pnpm release:windows <app-version>
pnpm release:windows <app-version> -Installer -Push -Publish
```

第一条只输出计划，第二条发布 updater、直接安装包并推送 Git refs。需要升级 CV 时，分别加 `--cv-code <version>` / `--cv-runtime <version>`，或 Windows 的 `-CvCode <version>` / `-CvRuntime <version>`。脚本检测到未发布的 CV 改动而未提供版本号时会停止。

CV code 也可单独通过 [CV code release workflow](../.github/workflows/cv-code-release.yml) 发布：在默认分支手动运行，填写 `source_ref` 和 `code_version`，默认仅构建候选，勾选 `publish` 才上传并创建清单 PR。候选会使用已发布的 Windows / macOS runtime 验证，PR 合并后再发布引用它的应用。

发布时，同一份已验证的 `mash-cv-code-vX.Y.Z.zip` 和 `release.json` 会上传到 R2 与 `cv-code/X.Y.Z` 对应的 GitHub Release。GitHub 发布使用 `softprops/action-gh-release`：缺少附件时失败，重试时跳过已有同名附件，不覆盖。CV Release 不设为 Latest，应用清单继续使用 R2 下载地址。

该 workflow 的 `cv-code-release` environment 需配置审批、`R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` secrets，以及 `R2_ENDPOINT` / `R2_BUCKET` variables；下载域名和对象前缀可选配置。

兼容性测试和发布 job 均使用该 environment：候选测试需对象读取权限，发布还需写入权限；配置了人工审批时，候选测试也须获批。runtime 下载和上传后的 SHA-256 校验通过 R2 S3 API 完成；workflow 不验证公网 CDN 下载。runtime object key 由清单 URL 移除 `RELEASE_BASE_URL` 前缀得到，URL 必须属于该前缀；清单仍保存供应用使用的公开 URL。

## 发布顺序

1. 运行检查、定稿日志，发布有变化的 CV 组件并更新 `runtime-manifest.json`。
2. 确认清单引用的组件已可下载，再更新应用版本并创建 tag。
3. 上传版本化应用产物与签名，验证后更新 updater 通道及直接安装包链接。
4. 全部验证通过后推送 commit 和 tag。

完整入口负责上述顺序。若手动上传 CV code，仍须把输出的版本、URL 和 SHA-256 写回清单并提交；仅推送 code tag 不会上传。

不同平台串行发布。同一应用版本补发另一平台时，检出相同 tag，确保其清单已包含目标平台可用的 CV 组件。updater 合并同版本的平台条目，不覆盖更高版本通道或不同内容的已登记产物。

## 验证与恢复

- 从公开下载地址验证产物可用、SHA-256 与清单一致；updater 产物还须有匹配签名。
- 检查应用版本、Git tag、更新通道和目标平台条目一致；直接安装包与 updater 分别确认。
- 失败后先检查已上传产物和本地 commit / tag，再决定从哪一步重试；code workflow 可复用同 SHA-256 的对象。
- 不覆盖已发布的版本化文件或重打已有 tag。应用与 CV 问题通过递增版本发布修复，必要时以已验证代码为基础重发；脚本拒绝通道降级，不提供自动降级回滚。
- 资源包可暂时把远端 latest 指回已验证 manifest，具体限制见[资源回滚](asset-bundle-update.md#回滚)。
