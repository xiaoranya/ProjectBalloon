# 记分板与 Resolver

本文档定义 P0 ICPC/XCPC 记分板与 Resolver 模型。

## 记分板原则

PostgreSQL 存储权威的比赛数据。Redis 存储实时记分板缓存，且必须可重建。

公共记分板绝不能每次请求都从完整提交表计算。更新应是增量的，并限定到受影响的比赛、队伍与题目。

## ICPC/XCPC 排名规则

P0 排名使用标准 ICPC 风格规则：

- 解出更多题目的队伍排名更高。
- 罚时更低的队伍排名更高。
- 罚时为已接受提交时间加上已解题目的错误提交罚时。
- 未解出题目的错误提交不计入最终罚时。
- First Blood 仅对正式非明星队伍按题目跟踪。
- 明星队伍会被展示，并可根据配置参与气球、Resolver 与颁奖。
- 支持分组记分板。

平分规则（第一版）：解题数降序、罚时升序、最近一次接受提交时间升序、队伍显示名升序（先不区分大小写，再区分大小写作为回退）。

## Freeze 行为

Freeze 之前：

- 公共记分板实时显示接受与拒绝的影响。
- 管理员记分板显示相同的权威视图。
- 为首次接受的提交按配置的颜色生成气球任务。

Freeze 之后：

- 公共记分板只包含 `submitted_at <= freeze_at` 的提交。
- 公共记分板隐藏 `submitted_at > freeze_at` 的提交。
- 管理员记分板继续显示真实状态。
- 提交仍正常判题。
- 不再生成气球任务。
- Resolver 使用 freeze 快照与最终快照。

比赛结束后，公共记分板保持冻结，不会自动揭示隐藏的提交。

## 第一版状态

第一版实现包含：

- 带显式平分规则的 ICPC/XCPC 排名。
- 带 freeze 语义的公共与管理员记分板变体。
- 分组记分板支持。
- 仅对正式非明星队伍按题目跟踪 First Blood。
- 带提交后失效的 Redis 缓存。
- 记分板快照持久化。

当前实现还包括：

- Resolver 控制、不可变快照、事件历史与当前状态恢复。
- 带暂停、继续、回退与完成控制的逐步 Resolver 揭示。
- 面向 Resolver、公共记分板、展示、公告以及工作人员 Clarification、气球与打印工作台的 SSE 失效事件。
- 用于跨 API 实例投递 SSE 的 Redis Pub/Sub 扇出，并带轮询回退。

推迟到未来版本：

- 超出权威状态刷新提示的富动画命令。
- 面向提交状态与私有 Clarification 回复的队伍级 SSE 通道。

## 数据模型

需求引用的核心表：

- `submissions`
- `judgements`
- `runs`
- `scoreboard_snapshots`
- `resolver_runs`
- `resolver_snapshots`
- `resolver_events`
- `resolver_team_states`
- `resolver_current_state`

`scoreboard_snapshots` 应存储足够的数据，以便在不读取可变提交历史的情况下重建公共或管理员记分板。

## 缓存策略

建议的 Redis 键：

```text
xcpc:scoreboard:v1:{contestId}:{postgresRevision}:{variant}:{phase}:{selectorHash}
contest:{contestId}:first-blood
```

缓存值应包含版本或生成时间戳，使客户端避免展示过期状态。

## Resolver 原则

Resolver 必须基于快照。

所需快照：

- Freeze 时的公共记分板。
- 最终真实记分板。
- 等待揭示的冻结提交。
- 每个揭示步骤的队伍 / 题目状态，或可重放的事件流。

Resolver 在正式仪式期间绝不能依赖实时提交行。生成快照后的重判或数据修正应要求显式重新生成 Resolver 快照。

## Resolver 流程

```text
比赛结束
  -> 等待 Judge 队列排空
  -> 可选重判与验证
  -> 生成最终记分板
  -> 生成 Resolver 快照
  -> 预览 Resolver run
  -> 为仪式冻结 Resolver run
  -> 操作正式 Resolver
  -> 持久化 Resolver 当前状态与事件
```

## Resolver 控制

P0 控制：

- 生成快照。
- 预览。
- 开始正式 run。
- 下一步。
- 暂停。
- 继续。
- 回退一步。
- 自动播放。
- 持久化并恢复当前状态。

公共 Resolver 页面包含 Screen 与 Live 变体。操作 API 需要相应的账户权限。

## 验证清单

正式 Resolver 之前：

- Judge 队列为空。
- 没有遗留的 `pending` 或 `judging` 提交（除非有意排除）。
- 最终管理员记分板已复核。
- Freeze 快照与最终快照已生成。
- Resolver 预览与预期的最终排名一致。
- 颁奖生成使用相同的最终排名来源。
