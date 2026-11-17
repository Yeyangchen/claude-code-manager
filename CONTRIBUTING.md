# 参与开发

感谢参与 Claude Code Manager。开发前阅读 [实施方案](PROJECT_PLAN.md) 和 [开发说明](docs/development.md)。

## 工作流程

1. 从最新 `main` 建立 `feat/`、`fix/` 或 `docs/` 分支。
2. 保持变更聚焦，提交格式使用 Conventional Commits。
3. 为安装目标选择、PATH、配置合并、进程执行和恢复行为添加对应回归测试。
4. 提交前运行 `pnpm check`、`pnpm lint`、`pnpm test`、`pnpm build`，以及 Rust 格式、Clippy、测试。
5. PR 描述用户可见结果、验证过程和相关 Issue，合并采用 squash。

用户配置与 PATH 的写操作在隔离环境测试。常规测试使用临时文件和构造数据。日志与问题报告中的凭据、用户路径和个人信息先脱敏。

## 依赖与来源

使用仓库固定的 pnpm 与 Rust 工具链。提交 lockfile。新增依赖说明用途与许可证。复用第三方源码或资产时保留原许可并更新归属记录。

## 支持范围

Windows x64 是当前主要目标。扩展安装来源时同时提供检测证据、操作能力、失败后核查及测试样例。每次改变 IPC 结构，同步更新 `docs/IPC_CONTRACT.md` 与前端类型。
