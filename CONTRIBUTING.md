# Contributing to FluxDown

感谢你有意为 FluxDown 做贡献！/ Thank you for your interest in contributing to FluxDown!

## 贡献者许可协议（CLA）/ Contributor License Agreement

**所有 Pull Request 必须先签署 CLA 才能被合并。**
All pull requests require a signed CLA before they can be merged.

- CLA 全文 / Full text: <https://gist.github.com/zerx-lab/575456570c7b7360fedbc37dfd32485e>
- 首次提交 PR 时，CLA assistant 机器人会自动在 PR 下评论。按提示点击链接、用 GitHub 账号登录并签署即可，只需签署一次，之后所有 PR 自动通过。
  When you open your first PR, the CLA assistant bot will comment with a link. Sign in with your GitHub account and sign once — all future PRs are covered.
- 未签署 CLA 的 PR 会被 `license/cla` 状态检查阻止合并。
  PRs without a signed CLA are blocked from merging by the `license/cla` status check.

## 开发流程 / Development Workflow

1. Fork 本仓库并从 `main` 创建分支。
2. 遵循仓库根目录 `AGENTS.md` 中的代码风格与规范（Rust: `cargo fmt --check && cargo clippy -- -D warnings`；Dart: `flutter analyze`）。
3. 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)（Release Notes 由 git-cliff 自动生成）。
4. 提交 PR 前确保相关 crate 的测试通过（例如 `cargo test -p fluxdown_engine`）。

## 反馈问题 / Reporting Issues

请通过 [GitHub Issues](https://github.com/zerx-lab/FluxDown/issues) 或官网 [反馈页面](https://fluxdown.app/feedback) 提交问题。
