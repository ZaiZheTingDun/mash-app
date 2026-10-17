# 开发文档

安装和日常使用见[用户文档](https://docs.mash.xiaotongx.com/)，搭建环境和提交改动见[贡献指南](../CONTRIBUTING.md)。本目录记录架构、自动化行为和维护流程。

## 架构与开发

- [模块结构](module-structure.md)：代码目录、职责与扩展约定。
- [Windows 开发](windows.md)：依赖、本地构建和 MuMu 画面采集排查。
- [mash-cv](../sidecar/cv-code/README.md)：Python 环境、通信协议、测试与打包。
- [CV 资源](../src-tauri/resources/README.md)：区服配置、模板与运行时清单。
- [队伍成员身份](team-member-identity.md)：成员标识、持久化与换人解析。

## 自动化与识别

[公共约定](state-machines/README.md)说明生命周期、模板错误处理与识别失败诊断。

| 功能 | 参考 |
| --- | --- |
| 自动战斗 | [状态机](state-machines/battle-automation.md)、[冠位战策略](grand-battle-auto-strategy.md) |
| 从者强化 | [状态机与模板映射](state-machines/enhancement-automation.md) |
| 礼装强化 | [状态机与丸子策略](state-machines/craft-essence-enhancement-automation.md) |
| 友情点召唤 | [状态机](state-machines/friend-point-summon-automation.md) |
| 战斗识别 | [场景检测](battle-scene-detection.md)、[宝具就绪检测](np-readiness-detection.md) |
| 异常处理 | [Fallback 注册表](fallbacks.md) |

## 发布维护

- [发布指南](distribution.md)：组件版本、发布入口、顺序与恢复。
- [更新资源包](asset-bundle-update.md)：完整包、增量补丁与回滚。
- [安全政策](../SECURITY.md)：漏洞报告方式与范围。
