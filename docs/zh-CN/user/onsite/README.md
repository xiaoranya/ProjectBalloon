---
title: 现场运营指南
description: 介绍现场工作人员如何使用大屏、直播、气球、打印工作台、Resolver 仪式与颁奖展示。
---

# 现场运营指南

本指南面向现场（ICPC/XCPC 风格）赛事的工作人员，覆盖赛事期间使用的工具：大屏、直播、气球、打印、Resolver 仪式与颁奖。各工作台需要拥有对应权限的员工账号；权限如何分配见[管理员指南](../admin/)。

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

### 直播节目包装（导播台、合成页与叠加层）

直播节目采用 OBS 图形包装层形态：导播台驱动合成节目页与透明叠加层，二者都作为 OBS 浏览器源使用，不引入任何外部流媒体服务。

| 界面 | 路由 | 访问方式 |
| --- | --- | --- |
| 导播台 | `/live/program/control` | 员工，需 `LIVE_MANAGE` |
| 合成节目页 | `/live/program` | 公开直播令牌 |
| 透明叠加层 | `/live/overlay?parts=ticker,popup,clock` | 公开直播令牌 |

推荐的 OBS 配置：

1. 将合成页 `/live/program` 作为**主浏览器源**（1920×1080、60 fps）。它渲染当前场景 —— 榜单、一血、气球、封榜倒计时、统计、Resolver、颁奖或标题卡 —— 以及公告条与时钟。
2. 将叠加层 `/live/overlay` 作为第二个**上层浏览器源**，背景透明。通过 `?parts=ticker,popup,clock` 只显示选中的元素；它不会重复舞台内容。
3. 将导播台 `/live/program/control` 保持在操作员屏幕上。场景切换、转场时长（100–5000 ms）、时钟、公告条与 RESOLVER run 都在导播台控制，并采用乐观并发（并发编辑返回 `LIVE_PROGRAM_VERSION_CONFLICT`，导播台会自动重载当前节目）。

导播台键盘快捷键：

| 按键 | 动作 |
| --- | --- |
| `1` | SCOREBOARD 场景 |
| `2` | FIRST_BLOOD 场景 |
| `3` | BALLOONS 场景 |
| `4` | FREEZE_COUNTDOWN 场景 |
| `5` | STATISTICS 场景 |
| `6` | RESOLVER 场景 |
| `7` | AWARDS 场景 |
| `8` | TITLE_CARD 场景 |
| `T` | 切换公告条 |
| `C` | 切换时钟 |

合成页通过 SSE 即时跟随节目变更，并在流中断时回退为 10 秒轮询。一血动画在合成页与叠加层弹窗层播放；两者都遵循 `prefers-reduced-motion`。

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

### 主持人脚本（`/awards/host-script`）

- 可为选定的比赛编辑并打印主持人提词稿；脚本按颁奖环节组织，包含提示词。
- 页面显示当前提示与下一项，并与展示状态保持同步。
- 保存使用乐观并发；若他人已修改脚本，请重新加载后重试。
- 为仪式主持人打印脚本。

## 活动检查清单

- 开始前：验证大屏、直播令牌、打印队列、气球颜色与 Resolver 访问权限。
- 进行中：监控 Clarification、气球、打印与判题队列。
- 结束后：排空判题队列、生成最终榜单与 Resolver 快照，然后进行 Resolver 与颁奖仪式。
- 任何失败都按[故障排查](../../ops/troubleshooting.md)记录事故日志。

## 另见

- [管理员指南](../admin/) — 权限与比赛配置。
- [故障排查](../../ops/troubleshooting.md) — 常见故障检查。
