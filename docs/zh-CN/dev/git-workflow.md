# Git 工作流

本文档定义项目 Git 工作流。

## 分支

使用轻量级发布工作流。

| 分支 | 用途 |
|---|---|
| `main` | 仅可发布版本 |
| `feature/*` | 功能开发 |
| `fix/*` | 常规缺陷修复 |
| `release/*` 或 `chore/release-*` | 发布稳定化与二进制包验证 |
| `hotfix/*` | 基于 `main` 的紧急修复 |
| `deps/*`、`chore/*`、`refactor/*`、`docs/*` | 依赖更新、维护、重构与文档 |

功能分支示例：

- `feature/competition-mode-ip-login`
- `feature/permission-role-migration`
- `fix/sqlx-0.9-update`
- `deps/frontend-security-updates`
- `refactor/split-large-files`
- `docs/vitepress-site`

## 提交信息

使用 Conventional Commits。

格式：

```text
type(scope): summary
```

常见类型：

- `feat`
- `fix`
- `docs`
- `chore`
- `test`
- `refactor`
- `perf`
- `build`
- `ci`

常见作用域：

- `frontend`
- `backend`
- `judge`
- `scheduler`
- `scoreboard`
- `resolver`
- `printing`
- `balloon`
- `awards`
- `screen`
- `live`
- `deploy`
- `docs`
- `database`

示例：

```text
feat(backend): add team account import
feat(judge): add rabbitmq task consumer
fix(scoreboard): hide frozen submissions on public board
docs(deploy): add binary install procedure
build(release): generate binary package
```

## 发布流程

正常发布：

```text
feature/* 或 fix/* 分支
  -> 拉取请求评审后合并到 main
  -> 从 main 创建 release/X.Y.Z（或 chore/release-*）
  -> 验证迁移、测试、二进制包和可选兼容 Compose 配置
  -> 打标签 vX.Y.Z
```

正式比赛发布候选应包含：

- 构建好的前端。
- 构建好的 API 和 Judge Worker 二进制。
- 构建好的前端静态文件。
- 固定版本的 Judge Runtime 镜像 tar 文件（作为单独的 judge-images 归档发布）。
- 单独记录的外部服务前置条件。
- 校验和。
- 安装、备份和恢复文档。

## 标签

产品版本：

```text
v1.0.0
v1.0.1
v1.1.0
```

预发布版本使用 SemVer 后缀并作为 GitHub 预发布发布，例如 `v0.1.0-alpha.1`。

可选的竞赛包标签：

```text
contest-package-v1.0.0
```

可用于追溯的竞赛特定内部标签，例如：

```text
contest-2026-provincial-final
```

## 拉取请求检查清单

合并前：

- 与变更相关的测试通过。
- 数据库迁移可逆，或有记录在案的仅前向原因。alpha 阶段对单个 `migrations/0001_initial.sql` 的编辑属于破坏性变更，必须在变更说明中明确标注。
- 公共/直播/大屏 API 不暴露敏感数据。
- 部署或恢复变更更新运维文档。
- 考虑二进制部署影响。
- 新密钥仅以 `.env.example` 占位符表示。

## 受保护主线预期

`main` 应始终代表一个可打包或可审计的版本。除初始设置或经批准的紧急热修复外，避免直接向 `main` 提交。
