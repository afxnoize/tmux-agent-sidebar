---
name: version-release
description: Use when bumping version, creating a release tag, or pushing a version tag. Triggers on "version up", "release", "tag push", "bump version".
---

# Version Release

Cargo.toml のバージョンを更新し、git タグを作成・pushするワークフロー。

## Workflow

1. **現在のバージョン確認**: `grep '^version' Cargo.toml` で現在のバージョンを取得
2. **バージョン更新**: Cargo.toml の `version` フィールドを Edit ツールで更新（patch/minor/major に応じて）
3. **Cargo.lock 再生成**: `cargo check` を実行して Cargo.lock を更新
4. **フォーマット・テスト確認**: `cargo fmt --check && cargo clippy && cargo test` を実行
5. **コミット**: `Cargo.toml` と `Cargo.lock` をコミット（メッセージ例: `Bump version to X.Y.Z`）
6. **タグ作成・push**: `git tag vX.Y.Z && git push && git push origin vX.Y.Z`

## Quick Reference

| Bump type | Example       |
|-----------|---------------|
| patch     | 0.2.0 → 0.2.1 |
| minor     | 0.2.0 → 0.3.0 |
| major     | 0.2.0 → 1.0.0 |

## Notes

- タグは `v` プレフィックス付き（例: `v0.2.0`）
- タグ作成前に必ず CI チェック（fmt, clippy, test）を通すこと
- `Cargo.lock` も一緒にコミットすること
