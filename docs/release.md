# 发布与撤回

发布版本必须与 package.json、Cargo.toml 和 tauri.conf.json 一致。普通 CI 产物用于检查，正式 tag 由 Release 工作流生成草稿。

## 首次配置

Tauri updater 必须验证签名，不能关闭验证。公开密钥随源码分发；私钥与密码只保存在维护者安全存储和 GitHub Actions secrets，不进入仓库。维护者需要独立加密备份密钥，丢失后已有客户端无法信任后续更新。

仓库 secrets：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

本项目已生成发布密钥并配置上述 secrets。维护者本机的私钥与密码保存在 `%LOCALAPPDATA%\ClaudeCodeManagerPublisher`，目录权限限定为当前用户和 SYSTEM。公开密钥为 `src-tauri/resources/manager-updater.pub`。该本机目录不在仓库中，请将其另行加密备份。

Windows Authenticode 证书另外配置。当前 beta 明确标记未做 Authenticode 签名。Tauri `.sig` 只证明更新包由项目发布者签发，不会消除 Windows SmartScreen 提示。

安装包同时携带 `resources/THIRD_PARTY_NOTICES.txt`。更新依赖后运行 `node scripts/generate-notices.mjs` 重新汇总许可证；CI 的 `--check` 模式核对其与锁定依赖一致。未随包提供许可文件的上游依赖，其固定来源、版本和 SHA256 记录在 `licenses/overrides.json`。

## 发布步骤

1. 在功能分支更新版本、CHANGELOG 和实际验收记录，PR 检查通过后 squash 合并。
2. 从已通过 CI 的 main 创建并推送不可变的 `v<version>` tag。
3. Release 工作流再次检查、构建 NSIS、生成 Tauri 签名、`latest.json` 和 `SHA256SUMS`，上传到草稿。
4. 下载草稿附件，核对 SHA256、版本、签名、安装器行为和兼容性记录。预览版标注 prerelease。
5. 完成适用验收后发布草稿。只有非 prerelease 的已发布版本参与稳定通道更新。

Release 草稿中现有附件不会被自动覆盖。同 tag 重跑仅补充缺失文件；若重建文件与已上传内容不同，工作流停止，需调查后创建新版本，避免同版本对应多个安装包。

## 更新元数据

稳定地址为 `https://github.com/Yeyangchen/claude-code-manager/releases/latest/download/latest.json`。先完整上传所有附件，再发布草稿，让稳定入口只指向完整版本。元数据包含 Windows x64 的版本化 URL 和 Tauri 签名。

## 缺陷撤回

1. 将缺陷 Release 标记为 prerelease，使稳定 latest 恢复到之前验证过的稳定版本；核对 GitHub latest API 和元数据实际响应。
2. 保留原 tag 与附件供追踪，在发行说明和置顶 Issue 写清受影响版本、症状及恢复步骤。
3. 发布更高版本修复。撤回不会强制已安装客户端降级；必要时提供手动回退说明。
4. 用旧客户端重查更新，确认缺陷版本不再被推荐，记录 A35 演练。

## 外部验收

干净 Windows 10 / 11、WebView2 缺失、升级链路与杀毒扫描需要相应环境。结果记录到 `docs/compatibility.md`，不能以构建成功代替。

参考：[Tauri updater](https://v2.tauri.app/plugin/updater/)、[Windows installer](https://v2.tauri.app/distribute/windows-installer/)。
