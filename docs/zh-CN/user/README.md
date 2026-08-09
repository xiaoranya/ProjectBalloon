# 使用手册

面向不阅读后端源码的使用者的按角色操作指南：选手、比赛管理员与现场运营人员（大屏、直播、气球、打印、Resolver 与颁奖）。

每篇指南都假设 [`../ops/quickstart.md`](../ops/quickstart.md)（英文镜像：`docs/ops/quickstart.md`）中描述的部署已在运行。配置与环境变量参考见 [`../ops/configuration.md`](../ops/configuration.md)；常见故障检查见 [`../ops/troubleshooting.md`](../ops/troubleshooting.md)。

## 分区

- [`contestant/`](contestant/README.md)：注册/登录、提交代码、解读判定结果、榜单与封榜规则、Clarification 与打印。
- [`admin/`](admin/README.md)：比赛、队伍、题目、员工账号与权限、重判、备份与归档。
- [`onsite/`](onsite/README.md)：大屏、直播、气球、打印工作台、Resolver 仪式与颁奖展示。

## 语言

英文版位于 `docs/user/`，中文镜像位于 `docs/zh-CN/user/`，两者内容一一对应。代码块、命令、路由与权限码在两种语言中均保留英文。

## 本指南涉及的前端路由

| 区域 | 路由 |
| --- | --- |
| 选手 | `/contests`、`/contests/:contestId/{problems,submissions,clarifications,printing,scoreboard}`、`/profile`、`/change-password`、`/login`、`/register` |
| 管理员 | `/admin`（比赛、题目、队伍导入、公告、重判、竞赛、练习、员工账号、权限） |
| 现场 | `/judge`（Clarification 工作台）、`/printer`、`/balloon`、`/resolver`、`/awards`、`/screen/manage`、`/live/manage`，以及公开展示页 `/screen`、`/live*`、`/resolver/display/:runId`、`/awards/display` |
