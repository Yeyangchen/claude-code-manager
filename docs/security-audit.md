# 依赖审计与例外

审查时间：2026-11-17 UTC。`pnpm audit`、`cargo audit`、`cargo deny` 继续在 CI 中运行。下面两个传递依赖仍带有公告，不能将本项目描述为“依赖无已知漏洞”。

| 公告 | 当前依赖 | 上游修复 | 当前用途的可达性判断 |
| --- | --- | --- | --- |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | rsa 0.9.10，经 pgp 0.21.1 引入 | rsa >=0.10.0 | 漏洞涉及私钥运算的时间侧信道；本项目只用固定官方公钥验证公开 manifest，不执行 RSA 私钥签名或解密 |
| [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) | crypto-bigint 0.5.5，经 pgp 的椭圆曲线分支引入 | crypto-bigint >=0.7.6 | 影响秘密整数运算；固定官方公钥为 RSA 4096，当前验签不进入椭圆曲线分支，也不输入秘密整数 |

截至审查时，pgp 最新稳定版为 0.21.1，仍依赖上述旧接口。直接强制替换不同主版本的传递库不兼容。仅对这两个 ID 设置临时例外。

## 审查依据

`src-tauri/src/releases.rs` 仅使用 `SignedPublicKey`、`DetachedSignature` 验证发布清单。固定公钥指纹为 `31DDDE24DDFAB679F42D7BD2BAA929FF1A7ECACE`，资源中只有 RSA 4096 主公钥。调用路径为 `DetachedSignature::verify` → `PubKeyInner::verify` → RSA `VerifyingKey::verify_prehash`。这里处理公钥和公开签名，项目没有 OpenPGP 私钥、签名生成或解密功能。

此判断针对当前用途，不是对上游库的普遍免责。官方真实 manifest/signature fixture 与篡改回归测试用于验证当前路径。

以下任一变化必须重新审查：

- 更新 pgp、rsa、crypto-bigint 版本或算法 feature。
- 引入 PGP 私钥、签名生成、加解密、用户可配置的验签公钥。
- 官方发布密钥、算法或签名格式改变。
- 新公告指出验签、内存安全、整数解析或公钥数据也受影响。

`scripts/check-security-exceptions.mjs` 固定审查涉及的版本。Dependabot 更新时必须重新审查并更新或删除本文与例外，不能自动沿用结论。其他漏洞继续阻止 CI。

全平台 lockfile 的审计也可能显示 Linux GUI 传递依赖的维护告警。首发仅构建 Windows x64，扩展 Linux 前需要单独评估。
