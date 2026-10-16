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
