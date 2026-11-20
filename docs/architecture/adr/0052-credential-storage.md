# ADR-0052：凭据存储

状态：

**第一步已接受并实现**（所有者，2026-10-07；Delta PR「fix: create credential files owner-only and judge Windows ACLs by SID」）。**第二步已提议，等待所有者决定。**（审计发现 DL-002。）

## 背景

以下为撰写本 ADR 时的代码状态（已在代码中核对，并非假设）。其中 Windows 写入权限的问题已由第一步修正，见上文状态。

凭据目前存放的位置：

1. **Rust 运行时（产品路径）。** `projects/delta/crates/delta-core/src/model_authority.rs` 把模型提供方的 API 密钥保存在 `<state>/model-authority.json` 的 `secrets` 映射中；`projects/delta/crates/delta-core/src/application.rs` 把连接器凭据保存在 `<state>/application.json` 中。两者都由 `durability::atomic_write_private` 写入：在 Unix 上以 `create_new` 方式创建模式为 `0600` 的临时文件，再重命名。**在 Windows 上，`open_temp_file` 会忽略"私有"标志，不设置任何 ACL。** 因此那里的保护取决于父目录继承的权限。Tauri 命令 `update_model_key`（`projects/delta/apps/desktop/src-tauri/src/runtime_ipc.rs`）走的就是这条 Rust 路径，所以 Rust 侧是主存储，而非遗留物。
2. **Python 保险库（`projects/delta/packages/credential_store.py`）。** 连接器代码（`projects/delta/integrations/connectors/email_tools.py`、`projects/delta/delta_extension_api/credentials.py`）用它保存邮件应用密码与 OAuth 令牌，位于 `<state>/secrets.json`。该文件先以 `write_text` 写入，之后才被限制权限：POSIX 上为 `chmod 0600`，Windows 上为 `icacls /inheritance:r /grant:r <user>:F`。父目录先于文件被限制，因此暴露窗口很小，但限制目录时的异常会被吞掉。Windows 检查 `_verify_windows_acl` 在 `icacls` 输出中查找英文字符串 `NT AUTHORITY\SYSTEM` 与 `BUILTIN\Administrators`；在本地化的 Windows 上，组名不同，因此检查结果可能是"已受保护"而实际并非如此（反之亦然）。失败的检查会记录为 `*.acl-unprotected` 标记文件，但界面中没有任何地方读取它。
3. **状态目录。** `DELTA_STATE_DIR`，否则 Windows 上为 `%APPDATA%\delta`，其他系统上为 `~/.config/delta`。在**便携版**中，启动器把 `DELTA_STATE_DIR` 设为 `<folder>\Data`，因此数据随文件夹一起移动（U 盘、同步或共享目录、另一台电脑或另一位用户的电脑）。

模块注释称，计划中的 Keychain/age 后端尚未实现。所有值都以明文 JSON 文本保存。

## 决策动因

- Windows 是目前唯一发布的目标（`x86_64-pc-windows-msvc`）；macOS 与 Linux 构建在 CI 中存在。
- 便携版是产品承诺：文件夹可以移动，应用不改动注册表。任何与操作系统绑定的加密（DPAPI、Keychain）都无法在其他机器或其他用户下解密。
- 单一权威：Rust 运行时拥有凭据（`core_control_plane.rs` 中的注释原文为「Rust ApplicationStore remains the sole product credential authority」）。Python 保险库不应发展出自己的方案。
- 无头运行与 CI 没有桌面密钥环。

## 方案

**A. 操作系统凭据存储**（Windows 凭据管理器 / DPAPI、macOS Keychain、Linux Secret Service），例如通过 Rust 的 `keyring` crate 实现；JSON 中只保留引用。
- 优点：标准保护；密钥不在文件夹中；无需维护文件 ACL。
- 缺点：不可移植（密钥留在本机，便携文件夹移动后会丢失密钥，用户需重新输入）；Linux 需要正在运行的 Secret Service（无头环境与 CI 中不可用）；签名二进制新增依赖及其审计面；Python 端必须通过 IPC 或 CLI 访问 Rust 存储，而不能直接读取文件。

**B. 由平台保护的主密钥（Windows 上为 DPAPI，macOS 上为 Keychain），密钥保存在加密文件中。**
- 优点：数据文件可以复制而不暴露密钥；只需保护一把密钥，而不是许多条目。
- 缺点：与 A 相同的可移植性损失（被包裹的主密钥无法在别处打开）；代码量更大（格式、版本管理、密钥轮换、恢复）；被损坏的包裹主密钥会使所有密钥不可读。

**C. 仅加固文件（即当前模型，正确地修复）。**
- 优点：对便携版友好；无新依赖；改动最小。
- 缺点：同一用户下运行的任何程序，以及任何拿到文件夹的人（备份、同步、丢失的 U 盘），都可以读取密钥。这是"防范其他本地用户"，而不是加密。

**D. 便携版可选的基于口令的加密（age / scrypt 风格），安装版使用 A。**
- 优点：兑现便携承诺（口令保存在用户的记忆中）；安装版获得操作系统保护。
- 缺点：需要构建并测试两种模式；界面中需要口令输入；丢失口令即丢失密钥（与文件夹丢失时今天的后果相同）。

## 建议

分两步进行，使紧迫的缺口先关闭，同时保留设计问题待定：

1. **第一步（不改变格式；建议现在批准）。** 让方案 C 在每条路径上都正确：
   - Rust 的 `atomic_write_private`：在 Windows 上，让临时文件在创建时就已受限（使用只授予当前用户 SID 的 `SECURITY_ATTRIBUTES` 描述符，或先限制目录的 ACL 再在其中创建文件），使文件永远不会以继承的权限出现。POSIX 保持 `create_new` + `0600`。
   - Python 的 `write_private_text` / `_persist`：在 POSIX 上以 `os.open(path, O_WRONLY|O_CREAT|O_EXCL, 0o600)` 创建；在 Windows 上先限制（新建的）父目录，再写入。用比较 **SID** 取代英文组名的子串检查：用户自己的 SID，以及 `S-1-5-18`、`S-1-5-32-544`，不得作为用户本人之外的授权出现。不要吞掉错误：在界面状态中暴露 `acl_unprotected`。
   - 一次性迁移：不移动任何东西；下次写入时，对现有文件重新应用限制。
   - 测试：新文件在 Windows 上没有继承的 ACE，在 POSIX 上模式为 `0600`；无法限制的文件会设置标记；未受保护的标记能够到达界面状态。
2. **第二步（需要所有者决定）。** 在考虑安装版用户与便携版用户的不同需求后，在 A、D 与"C 已足够"之间选择。我的倾向是：**便携版用 D，安装版用 A**，二者置于同一个 Rust trait（`CredentialBackend`）之后，并以文件作为回退，使无头运行与 CI 仍能工作。迁移随后导入旧 JSON，验证其可读回，并用只含引用的受限文件覆盖明文。在所有者就便携版的取舍作出决定之前，不建议开始这项工作，因为它改变了"便携"的含义。

## 后果

第一步：不改变数据格式；对便携版没有破坏性变更。它不把任何东西移出文件夹，因此被盗的文件夹仍会暴露密钥。在第二步之前，README 与文档应如实说明这一点。

第二步（如果选择）：便携语义改变（除非受口令保护，否则密钥不再随便携文件夹移动），新增依赖；迁移必须幂等，并且在新副本成功读回之前不得删除明文；没有可用密钥环时的失败路径必须是可见的警告，而不是静默回退。

## 需要所有者回答的问题

1. 是否现在批准第一步？
2. 对于第二步，"除非受口令保护，否则密钥不随便携文件夹移动"是否可以接受？如果不可以，则只剩 C 与 D。
3. Linux / macOS 的支持是否重要到需要把 Secret Service / Keychain 路径纳入第一个版本？

## 未验证事项

`icacls` 在本地化 Windows 上的行为是根据代码推断的，未经复现。通过 `SECURITY_ATTRIBUTES` 创建 Windows ACL 的做法尚未在此原型验证。
