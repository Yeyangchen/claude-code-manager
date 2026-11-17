首次 Windows x64 预览版，提供 Claude Code 安装检测、版本计划与后台任务、PATH/策略变更台账、诊断预览和网络设置。

**非官方社区工具，与 Anthropic 无隶属或背书关系。**

此版本为 beta，尚未完成全部真实安装、跨版本升级和 Windows 10 兼容性验收。请阅读仓库 `docs/compatibility.md` 的逐项记录。

Windows Authenticode 状态：**未签名**。Tauri updater 附件使用独立发布密钥签名；这不会消除 SmartScreen 提示。安装前请核对 `SHA256SUMS`，保留重要配置备份。

附件包括 Windows x64 NSIS 安装包、Tauri 签名、版本化更新元数据、来源提交记录和 SHA256SUMS。管理器不会自动删除 Claude 的用户配置、会话或项目文件。
