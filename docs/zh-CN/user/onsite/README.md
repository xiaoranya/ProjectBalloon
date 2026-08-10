---
title: 现场运营指南
description: 介绍现场工作人员如何使用大屏、直播、气球、打印工作台、Resolver 仪式与颁奖展示。
---

# 现场运营指南

本指南面向现场（ICPC/XCPC 风格）赛事的工作人员，覆盖赛事期间使用的工具：大屏、直播、气球、打印、Resolver 仪式与颁奖。各工作台需要拥有对应权限的员工账号；权限如何分配见[管理员指南](../admin/README.md)。

## 员工工作台总览

| 工作台 | 路由 | 权限 |
| --- | --- | --- |
| Clarification | `/judge` | `CLARIFICATION_MANAGE` |
| 打印 | `/printer` | `PRINTING_MANAGE` |
| 气球 | `/balloon` | `BALLOON_MANAGE` |
| Resolver | `/resolver` | `RESOLVER_MANAGE` |
| 颁奖 | `/awards` | `AWARD_MANAGE` |
| 大屏 | `/screen/manage` | `SCREEN_MANAGE` |
| 直播 | `/live/manage` | `LIVE_MANAGE` |

## Clarification 工作台（`/judge`）

- 审查新 Clarification，私密回复、把问题转为公开公告或关闭问题。
- 公告会推送到选手的 Clarification 页面，并按配置显示在直播/大屏视图上。

## 大屏（`/screen`）

- `/screen/manage` 控制大屏；`/screen` 是公开展示客户端（也可用作 OBS 浏览器源）。
- 添加/移除大屏并分配内容。展示过期时刷新浏览器源或重新连接客户端。
- 动态展示无法快速恢复时，使用静态兜底页面。

## 直播（`/live`）

- `/live/manage` 配置直播视图；`/live` 是公开直播页面，包含多种视图：
  - `/live` — 榜单视图。
  - `/live/first-blood` — First Blood 信息流。
  - `/live/balloons` — 气球信息流。
  - `/live/freeze-countdown` — 封榜倒计时。
  - `/live/statistics` — 统计。
- 直播页面可能受令牌保护。正式比赛前请轮换排练时共享的令牌。

## 气球（`/balloon`）

- 气球任务会按题目配置的气球颜色，为首次通过的提交自动生成。
- 仅正式非明星队伍生成 First Blood 任务；明星队伍可按配置参与气球（以及 Resolver/颁奖）。
- 封榜期间不再生成新的气球任务。
- 工作台显示待处理任务并标记配送；请保持审计状态准确，避免漏掉任何队伍。

## 打印（`/printer`）

- 工作台列出 `QUEUED`/`PRINTING`/`DONE`/`FAILED` 状态的打印请求，并发送到 CUPS 队列。
- 选手侧限制：单次请求 20 KiB / 5 页；每队每 10 分钟 1 次；每队每场比赛 20 次。
- 打印机恢复后请重试失败任务；紧急请求可使用手动下载兜底。

## Resolver（`/resolver`）

Resolver 在闭幕仪式中逐步揭示封榜榜单。它基于快照：

1. 比赛结束后（或基于封榜状态）生成 Resolver 快照。
2. 预览 run。
3. 为仪式冻结 Resolver run。
4. 操作仪式：揭示、暂停、继续、回退与完成。
5. Resolver 当前状态与事件历史会被持久化；中断的仪式可从保存的状态恢复。

仪式期间 Resolver 绝不能依赖实时提交行。生成快照后的重判或数据修正需要显式重新生成 Resolver 快照。

## 颁奖（`/awards`）

- 从最终榜单生成获奖名单，并支持需要时的人工获奖者流程。
- 颁奖生成/冻结/导出与获奖者管理在 `/awards`；展示控制位于 `/awards/presentation`，公开展示页位于 `/awards/display`。
- 仪式前冻结获奖名单以保持稳定。

## 活动检查清单

- 开始前：验证大屏、直播令牌、打印队列、气球颜色与 Resolver 访问权限。
- 进行中：监控 Clarification、气球、打印与判题队列。
- 结束后：排空判题队列、生成最终榜单与 Resolver 快照，然后进行 Resolver 与颁奖仪式。
- 任何失败都按[故障排查](../../ops/troubleshooting.md)记录事故日志。

## 另见

- [管理员指南](../admin/README.md) — 权限与比赛配置。
- [故障排查](../../ops/troubleshooting.md) — 常见故障检查。
