# 本地开发

项目使用 Node.js 24.14.0、pnpm 11.7.0 和 Rust 1.100.0，版本记录在仓库配置中。

Windows 构建需要 Microsoft C++ Build Tools 的 MSVC x64/x86 工具和 Windows SDK，以及 WebView2。安装说明见 [Tauri 官方前置要求](https://v2.tauri.app/start/prerequisites/)。

```powershell
pnpm install --frozen-lockfile
pnpm tauri:dev
```

浏览器界面预览使用 `pnpm dev`。开发预览页明确标记为模拟环境，桌面应用通过 Tauri IPC 检测真实环境。

## 检查与构建

```powershell
pnpm check
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
pnpm tauri:build --bundles nsis
```

NSIS 输出位于 `src-tauri/target/release/bundle/nsis/`。源码和安装包版本必须一致，`node scripts/check-project.mjs` 验证项目版本和基础发布元数据。

正式发布整理到 `artifacts/release/` 后使用 `claude-code-manager_<version>_x64-setup.exe` 及对应 `.sig`；以此规范名称生成更新地址和校验清单。原始 NSIS 输出文件名可能带空格，不能直接作为最终发布附件名。

## 验证环境

Rust 测试对临时目录、配置快照、PATH 字符串和模拟操作做回归验证。真实安装、卸载和注册表写入在一次性虚拟机或隔离账户中执行，并记录到兼容性表。

`Isolated Windows acceptance` 是手动触发的 GitHub Actions 工作流，使用一次性的 GitHub hosted Windows runner。它运行真实 PATH 写入/撤销/外部冲突烟测，以及官方 Native 精确版本安装与重装。相关 Rust 测试默认 `#[ignore]`，带环境断言，不能在开发者账户中启用。

CI 的 `windows-x64-preview` 附件包含 NSIS 安装包和可直接启动的主程序。预览附件没有 Release updater 签名；签名发布附件由 Release 工作流单独生成。

## Git

`main` 用于可追踪的主线，开发使用短期功能分支。每次提交检查暂存区。远端为 `Yeyangchen/claude-code-manager`，PR 合并采用 squash。
