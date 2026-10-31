# 抓包系统证书安装

环境配置需要已 root 的 Android 设备及已初始化的 Reqable。证书由 Rust 解析并计算哈希后，交给设备上的 shell 安装；不修改系统分区文件，不需要重启。挂载仅在本次设备启动期间有效，设备重启后需重新进行环境配置。

| 系统版本 | 证书安装方式 |
| --- | --- |
| Android 5–13（API 21–33） | 保留原有 `/system/etc/security/cacerts` 内容，加入 Reqable CA 后 bind mount，并检查 Reqable 进程可见的文件。 |
| Android 14+（API 34+） | 使用 `/apex/com.android.conscrypt/cacerts`；通过 `nsenter` 分别进入 Zygote、Zygote64 以及当前 Reqable、国服 FGO 主进程的挂载命名空间进行安装。 |

Android 14+ 的 `/apex` 挂载不会自动在不同命名空间间传播，所以仅从 ADB shell 挂载不足以生效。更新 Zygote 供后续启动的应用继承，已经运行的目标应用则单独处理。相同命名空间只挂载一次；每个命名空间保留各自原有的根证书，不用旧系统目录覆盖 APEX 根证书。

安装前检查命名空间访问权限、APEX 目录和源证书哈希。已包含同一 CA 的命名空间不重复挂载；其他 CA 占用相同 subject hash 时使用下一个空闲序号。安装后重新读取运行中的目标应用 PID 并比较证书内容，避免应用重启造成假成功。

失败时按相反顺序撤销本次已完成的挂载，再扫描并撤销安装期间新进程继承的同一目录挂载。若 PID/命名空间已经变化或无法撤销，明确提示重启设备后重试。一旦发生过挂载，无论成功还是失败都保留 backing 目录，避免并发 fork 产生的其他持有者读到已被删除的根证书；没有发生挂载时才清理临时目录。

Android 14+ 要求设备提供可用的 `nsenter`，并允许 root 进入目标挂载命名空间。缺少这些能力，或系统未提供预期的 APEX 证书库时明确失败。API 34+ 选择此安装方式不代表已验证所有后续系统版本、定制 ROM 或应用的 TLS 行为；文件可见性校验不能代替真实 HTTPS 抓包验证。

## 验证

运行 `cargo test --manifest-path src-tauri/Cargo.toml`。`packet_capture/certificate_script_tests.rs` 在临时目录内执行真实安装脚本，用命令替身模拟多个命名空间，覆盖双 Zygote、已启动/未启动游戏、共享命名空间、原证书保留、哈希冲突、重复/部分安装、挂载失败、回读失败、应用退出及回滚失败。

这些测试不使用 ADB、root 或真实挂载。设备上的 SELinux 策略、`nsenter` 实现、进程继承及真实 TLS 信任需要在已授权的 Android 14+ 设备上验证，本次尚未进行设备验证。现有忽略测试 `install_certificate_on_authorized_device` 可用于显式授权后的安装检查。

## 参考

- [AOSP Conscrypt：Android 14 的可更新根证书库](https://source.android.com/docs/core/ota/modular-system/conscrypt)
- [HTTP Toolkit：通过 nsenter 注入 Android 14 系统 CA 的原理](https://httptoolkit.com/blog/android-14-install-system-ca-certificate/)
