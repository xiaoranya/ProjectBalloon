---
title: ProjectBalloon 文档
description: ProjectBalloon 的手册：安装部署平台、举办正式比赛，以及选手、管理员和现场工作人员使用 Web 界面的方法。
---

# ProjectBalloon 文档

ProjectBalloon 是面向线下 XCPC/ICPC 竞赛的平台。本文档是使用该平台的手册：包括安装部署、举办正式比赛，以及选手、管理员和现场工作人员如何使用 Web 界面。

## 从哪里开始

| 如果你是…… | 从这里开始 |
| --- | --- |
| 准备部署新环境 | [快速开始](ops/quickstart.md) |
| 使用平台的选手 | [选手指南](user/contestant/README.md) |
| 比赛管理员 | [管理员指南](user/admin/README.md) |
| 现场工作人员（大屏、气球、打印、Resolver、颁奖） | [现场运营指南](user/onsite/README.md) |
| 部署或运维服务器 | [安装](ops/install.md) 与 [运维](ops/ops.md) |
| 查询配置变量或端点 | [配置参考](ops/configuration.md) 与 [API 契约](../api/openapi.yaml) |

## 文档分区

- **使用手册** — `user/`：面向选手、比赛管理员和现场工作人员的分角色手册。这些页面假设 [快速开始](ops/quickstart.md) 描述的部署正在运行。
- **运维手册** — `ops/`：安装、配置、运维、故障排查、备份恢复与压测。
- **参考资料** — 环境变量、路由、权限和 HTTP 契约的精确取值。
- **开发与内部文档** — `architecture/`、`dev/`、`api/` 与 `requirements/`：系统设计、编码规范与需求追溯。这些是保留在仓库中的工程文档，不属于已发布的手册。

## 语言

英文为规范版本。每个页面在 `zh-CN/` 下有相同相对路径的对应中文镜像；代码块、命令、路由和权限码在两种语言中都保持英文。OpenAPI 契约（`api/openapi.yaml`）为机器生成，不做镜像。

## 撰写本文档

新增或修改文档时，请遵循[文档写作规范](STYLE.md)。贡献流程见 [`CONTRIBUTING.md`](../../CONTRIBUTING.md)。
