---
name: mash-release-notes
description: Use this skill when writing Mash user-facing update docs, release notes, changelog entries, version announcements, or 更新文档. It focuses on changes users can perceive and filters out implementation details, refactors, internal engineering work, and low-level technical notes.
metadata:
  short-description: Write Mash user-facing release notes
---

# Mash Release Notes

Use this skill to turn shipped changes, commits, PR notes, or rough bullet points into a concise Mash update document for end users.

## Core Rule

Only include user-perceptible changes: visible features, workflow changes, behavior changes, fixes users may notice, known limitations, compatibility notes, and upgrade impact.

Exclude implementation details unless they directly explain a user-visible impact. Do not mention internal architecture, refactors, dependency bumps, build tooling, code structure, tests, logging, serde fields, state machines, CV templates, or engineering chores by default.

## Output Format

Use this structure unless the user asks for a different one:

```text
Mash Version X.Y.Z - 简短主题
新增
[用户能感知的新功能或能力。]

改进
[用户能感知的体验、识别率、稳定性、速度、流程改进。]

修复
[用户能感知的问题修复。]

已知问题
[仍然存在、用户可能遇到的问题或限制。]
```

Omit empty sections. Keep section names exactly as Chinese labels: `新增`, `改进`, `修复`, `已知问题`.

## Writing Style

- Write in Chinese unless the user asks otherwise.
- Use short, direct bullet-like lines without Markdown bullets unless the user asks for Markdown.
- Keep wording product-facing and concrete.
- Prefer what changed and how it affects the user over how it was built.
- Use Mash feature names consistently with the project and existing docs.
- Avoid overpromising. Use cautious wording for partial rollout, unstable support, or behavior that depends on device/client state.
- If a user provides technical notes, translate them into user impact or drop them.

## Classification

Put items in `新增` when users gain a new capability, mode, setting, supported scenario, detection ability, or install/update path.

Put items in `改进` when an existing capability becomes more accurate, faster, smoother, clearer, more stable, or supports more cases without being a wholly new feature.

Put items in `修复` when a specific user-facing bug, crash, stuck state, incorrect behavior, or broken workflow has been corrected.

Put items in `已知问题` when the issue remains unfixed and users may need to understand the limitation.

## Filtering Examples

Include:

- 新增冠位戴冠战支持。
- 冠位支援从者检测并自动选择。
- 修复战斗后牵绊升级后无法主动点击跳过的问题。
- 软件第一次运行时点击开始按钮后需要较长时间等待连接到模拟器。

Rewrite technical notes:

- `runner.rs 增加 grand battle strategy state` -> `新增冠位戴冠战特化自动战斗策略。`
- `升级 runtime manifest downloader` -> `现在可以自动检测并下载新版本软件。`
- `cv template match resized for 1080p` -> `识别稳定性提升，适配更多模拟器分辨率。`

Exclude unless explicitly requested:

- 重构组件目录。
- 更新依赖版本。
- 增加测试覆盖。
- 调整 serde 字段名。
- 修改内部状态机实现。

## Quality Pass

Before finalizing, check:

1. Every line answers "用户能看到或感受到什么变化？"
2. No line exposes unnecessary implementation details.
3. Similar items are merged instead of repeated.
4. Known issues are honest but concise.
5. Empty sections are removed.
