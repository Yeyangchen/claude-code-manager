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

## 验证环境

Rust 测试对临时目录、配置快照、PATH 字符串和模拟操作做回归验证。真实安装、卸载和注册表写入在一次性虚拟机或隔离账户中执行，并记录到兼容性表。

## Git

`main` 用于可追踪的主线，开发使用短期功能分支。每次提交检查暂存区。远端为 `Yeyangchen/claude-code-manager`，PR 合并采用 squash。
