# Claude Code Manager

[![CI](https://github.com/Yeyangchen/claude-code-manager/actions/workflows/ci.yml/badge.svg)](https://github.com/Yeyangchen/claude-code-manager/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-557568.svg)](LICENSE)

在 Windows 上查看 Claude Code 的安装来源、管理版本，并追踪更新、PATH 与策略更改。

**非官方社区项目。与 Anthropic 无隶属、赞助或背书关系。** Claude、Claude Code 是其权利人的商标。本项目采用原创图标，不分发 Claude Code 二进制文件，不绕过官方登录、订阅或组织策略。

当前版本为 **0.1.0-beta.1**。发布与验收状态见 [实际支持和验收记录](docs/compatibility.md)。未经验证的场景不会被标记为稳定支持。

## 下载

从 [GitHub Releases](https://github.com/Yeyangchen/claude-code-manager/releases) 下载 Windows x64 安装包。开发构建可在 [Actions](https://github.com/Yeyangchen/claude-code-manager/actions) 中下载。

目标系统为 Windows 10 1809 及以后版本、Windows 11，使用当前用户安装模式。安装包在需要时下载 Microsoft WebView2。ARM64、macOS、Linux 暂不作为首发支持平台。

预览版暂未使用 Windows Authenticode 代码签名证书，Windows 可能显示“未知发布者”或 SmartScreen 提示。Tauri 更新签名与 Windows 代码签名是两套独立机制。请先核对来源与 SHA256，不要为运行应用关闭系统安全功能。

```powershell
Get-FileHash -Algorithm SHA256 '.\Claude Code Manager_0.1.0-beta.1_x64-setup.exe'
```

将结果与同一 Release 的 `SHA256SUMS` 对照。不同构建的文件名以发布页为准。

## 使用

1. 打开概览，查看所有安装实例、当前终端命中路径和前置条件。
2. 选择实例及目标版本，检查计划中的路径、命令、配置影响与风险提示。
3. 确认计划后执行，在后台任务中查看实际阶段、日志和结果。
4. 环境页面提供 PATH 修复、策略变更记录及撤销。撤销前会检查外部修改，存在冲突时保留当前状态。

| 来源 | 管理能力 | 边界 |
| --- | --- | --- |
| Native | 安装、更新、指定版本重装、回退、卸载 | 使用官方发布文件；签名、摘要和安装结果逐项验证 |
| npm | 检测、升级、卸载 | 绑定检测时的 npm 可执行文件与 prefix；无法确认归属时只读 |
| WinGet | 检测、升级、卸载 | 需要管理员权限的实例提供人工命令，不静默提权 |
| IDE / Desktop 私有副本 | 路径解释 | 不接管宿主应用的内部版本 |

策略页可选择官方自动更新、管理器手动更新或版本保持。多个实例可能共享同一 Claude 配置，确认页会说明影响范围。组织策略优先；`DISABLE_UPDATES` 等限制不会被绕过。

## 数据与隐私

默认无遥测。管理器只保存自身设置、操作状态、脱敏日志、变更字段和 PATH 备份。代理密码保存在 Windows Credential Manager。诊断导出前可预览内容。

管理器不会删除 `.claude`、`.claude.json`、会话记录或项目文件。卸载管理器前可在应用中还原其写入的 PATH 与策略字段。数据保留及具体网络来源见 [支持说明](docs/compatibility.md)。

## 开发

技术栈为 Tauri 2、Rust、React、TypeScript、Vite。工具版本、完整构建命令及 Windows C++ 前置要求见 [开发指南](docs/development.md)。

```powershell
pnpm install --frozen-lockfile
pnpm tauri:dev
```

`pnpm dev` 只启动明确标记的浏览器界面预览；真实检测与执行通过桌面应用的 Rust 后端完成。

## 项目文档

- [用户修订的完整方案](PROJECT_PLAN.md)
- [架构与 IPC](docs/architecture.md)
- [验收记录](docs/compatibility.md)
- [发布与撤回](docs/release.md)
- [变更记录](CHANGELOG.md)
- [贡献指南](CONTRIBUTING.md) · [安全报告](SECURITY.md)

设计参考了 [Wangnov/Codex-App-Manager](https://github.com/Wangnov/Codex-App-Manager) 的任务管理与恢复思路。Claude Code CLI 的安装来源、版本获取和配置处理独立实现。

MIT © 2026 Yeyangchen
