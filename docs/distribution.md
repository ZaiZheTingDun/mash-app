# 发布指南

CV 识别代码随 App 内置；只有 Python 解释器、原生依赖和 OCR 模型组成的 runtime 独立下载。开发环境见[贡献指南](../CONTRIBUTING.md)，资源包发布见[资源更新流程](asset-bundle-update.md)。

## 版本与打包

- App 使用 `versions.toml` 的 `[app].version`，与 Tauri、Cargo 和 `vX.Y.Z` tag 一致。
- runtime 使用 `[mash_cv.runtime].<platform>`，各平台独立版本，对应 `cv-runtime/<platform>/X.Y.Z` tag。
- CV code 没有独立版本、远端 ZIP、下载状态或发布 tag。修改识别代码时发布新 App。

`src-tauri/build.rs` 将 `sidecar/cv-code/mash_cv/` 复制到生成的 `resources/mash-cv-code/mash_cv/`，排除 `models`、Python 缓存和字节码，保留轻量分类器资源。OCR 模型、依赖锁文件、启动器和原生构建器位于 `sidecar/cv-runtime/`。正式版仅从 App 资源目录加载代码；开发模式读取源码。旧版下载的 code 目录不再加载，也不会删除，以免影响仍使用它的旧 App。

`runtime-manifest.json` 仅保存每个平台的 runtime 版本、URL 和 SHA-256，格式见 [CV 资源](../src-tauri/resources/README.md#runtime-manifestjson)。

## GitHub Actions 发布

1. 在默认分支运行 [App release](../.github/workflows/app-release.yml)，填写已审查的 `source_ref` 和递增的 `app_version`。无需填写 CV code 版本或其他 workflow 的 run ID。
2. 流水线先固定源码 SHA、构建工具 SHA 和 runtime 清单，检查版本、tag、更新说明和 runtime 清单的版本、URL、SHA-256。检查不通过时直接失败，原因显示在 Summary；发布计划 artifact 保留 30 天。
3. 两个平台分别校验内置 CV code 与清单指定的 runtime。任一平台校验或构建失败，后续发布 job 不会执行，更新通道保持不变。
4. 发布成功后合并 `codex/app-release-<version>` 版本同步 PR；发生冲突时手动同步发布提交。发布已完成的产物不会因同步失败而撤回。

流水线每次从固定源码构建一个内部 code 验证 ZIP，版本名跟随目标 App；它只用于两个平台的兼容性验证和候选留存，不上传 R2，不创建独立 GitHub Release。Windows x64 和 macOS Apple Silicon 使用同一份 code 候选与各自固定的 runtime，运行源码测试、冻结 runtime 协议/OCR 检查、Rust 安装测试和 App 构建。

构建时将 code 打入 App。只有两个平台都通过后，才验证 updater 签名、条件上传 App 产物、检查公开 CDN 哈希、创建 App tag/GitHub Release，最后更新 `stable`、`windows-stable` 以及直接下载入口。两个通道包含本批次的平台条目。macOS Intel 暂不参与该发布矩阵。

## 发布入口

先提交工作区改动，在 `CHANGELOG.md` 的“未发布”或目标版本标题下准备更新说明。以下为命令用法，版本参数均替换为目标递增版本。

App release 不再根据 runtime 源码文件变更或历史 tag 判断是否必须重发 runtime。它下载清单固定的原生 runtime，验证 SHA-256，并在 Windows 和 macOS 上运行内置识别代码的协议与 OCR 冒烟检查；任一检查失败，发布终止。这些检查覆盖启动和基础 OCR 链路，不代表所有识别功能都经过验证。

仅目录移动或打包脚本整理不要求重发 runtime。确实需要新的原生依赖、OCR 模型或启动器时，在对应平台先发布并应用新 runtime，再重新运行 App release。例如：

```text
scripts/macos/release-cv-runtime.sh --platform darwin-aarch64 <runtime-version>
./scripts/windows/release-cv-runtime.ps1 <runtime-version> -Platform windows-x86_64
```

runtime 发布脚本构建、上传、校验并更新目标平台版本和清单，创建提交与 tag；将这些提交和 tag 同步到远端后再发布引用它的 App。不要覆盖已发布的 runtime ZIP。

## 本地 App 发布

本地需开发依赖、Poetry、AWS CLI、R2 凭据和匹配的 updater 私钥。macOS 默认使用 AWS profile `mash`；Windows 使用默认凭据链。对象前缀默认 `mash`，通道分别为 `stable` 和 `windows-stable`。Windows Authenticode 与 updater 签名独立。

```text
scripts/macos/release.sh <app-version>
scripts/macos/release.sh <app-version> --dmg --push --publish
pnpm release:windows <app-version>
pnpm release:windows <app-version> -Installer -Push -Publish
```

不带发布开关时仅打印计划。需要升级 runtime 时，加 `--cv-runtime <version>` 或 `-CvRuntime <version>`。先提交工作区，并在 `CHANGELOG.md` 中准备用户可见的更新说明。完整验证应包含前端、Rust 与 Python 三套测试。

## 重试与恢复

优先使用 **Re-run failed jobs**，复用固定版本和已验证候选，不重新构建不同内容的同版本二进制。已存在的版本化对象只有哈希相同才允许复用；tag 不移动，通道拒绝降级，指针通过 ETag 条件写入保护。

通道、GitHub 和直接下载入口之间不是原子事务；中断后可能部分推进，重试完成剩余步骤。不要同时运行本地发布脚本和 CI。发布完成需确认版本、tag、签名、CDN 内容和通道一致；本地测试不代表已通过真实 Windows 安装或设备验证。资源回滚见[资源流程](asset-bundle-update.md#回滚)。
