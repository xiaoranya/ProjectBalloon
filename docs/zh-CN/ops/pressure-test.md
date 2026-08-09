# 压测

本文档定义正式部署前必需的彩排压测。

## 目标

需求目标：

- 1,500 用户登录模拟。
- 500 支队伍并发查看题目描述。
- 500 支队伍并发刷新记分板。
- 每分钟 100 至 300 次提交。
- Judge 队列积压与排空测试。
- Resolver 页面压测。
- 多 Screen 连接测试。
- 打印任务压测。

## 测试环境

压测应在接近正式部署的硬件上运行。如果使用降级硬件，应清晰记录差异。

所需测试数据：

- 至少 500 支队伍。
- 题目数量贴近现实的比赛。
- C、C++、Java 与 Python 提交。
- 混合 accepted、wrong answer、compile error、runtime error 与 timeout 用例。
- 已配置的气球颜色。
- 已配置的打印机或打印机 mock。
- Screen 与 live 客户端。

## 自动化 k6 套件

仓库在 `scripts/pressure/k6` 提供三个 profile：

| Profile | 登录用户 | 并发读者 | 提交速率 | Screen 客户端 | 时长 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `smoke` | 1 | 1 | 启用时一次写入 | 1 | 10 秒 |
| `rehearsal` | 300 | 200 | 120/分钟 | 20 | 10 分钟 |
| `full` | 1,500 | 500 | 300/分钟 | 100 | 30 分钟 |

`rehearsal` 用于在降级硬件上调优。`full` 匹配需求目标。除非账户文件包含至少与登录目标一样多的唯一账户，非 smoke profile 拒绝启动。

在仓库外准备一个未跟踪的 JSON 文件：

```json
[
  {"username": "team-001", "password": "rehearsal-only-password"},
  {"username": "team-002", "password": "rehearsal-only-password"}
]
```

对本地环境运行只读 smoke 测试：

```bash
BASE_URL=http://127.0.0.1:8080 \
CONTEST_ID=1 \
ACCOUNTS_FILE=/secure/path/accounts.json \
scripts/pressure/run-k6.sh
```

运行授权的完整彩排，包括提交与打印请求：

```bash
BASE_URL=https://rehearsal.example.internal \
CONTEST_ID=42 \
PROFILE=full \
ENABLE_WRITES=true \
ACCOUNTS_FILE=/secure/path/accounts.json \
XCPC_PRESSURE_TARGET_ACK=I_UNDERSTAND_THIS_GENERATES_LOAD \
scripts/pressure/run-k6.sh
```

非本地确认提示特意冗长，以防止对未批准目标产生意外负载。绝不使用生产凭据。账户 fixture、提交源码、Cookie 与密码不会写入报告。默认报告为 `build/reports/k6/summary.json`。

可选控制：

- `P95_LATENCY_MS` 设置登录 / 读取 p95 阈值（默认 `1500`）。
- `MAX_FAILURE_RATE` 设置 HTTP 失败率阈值（默认 `0.01`）。
- `SUBMISSION_LANGUAGE` 与 `SUBMISSION_SOURCE` 选择生成的提交。设置 `SUBMISSION_LANGUAGE=mixed` 以轮换 C、C++、Java 与 Python 提交；省略 `SUBMISSION_SOURCE` 时，套件为每种语言提供有效源码。
- 诊断失败的 smoke 运行时，`ENABLE_SUBMISSIONS=false` 或 `ENABLE_PRINTS=false` 隔离一种写入负载。只要 `ENABLE_WRITES=true`，两者默认都启用。
- `REPORT_DIR` 选择摘要输出目录。
- `SAVE_K6_JSON=true` 还将原始 k6 指标流写入 `REPORT_DIR/metrics.json`；运行后压缩它，因为 rehearsal 与 full profile 可能产生大文件。
- `DURATION` 为有界的诊断运行覆盖所选 profile 的工作负载时长。官方彩排或 full 证据时省略它。
- `RESOLVER_RUN_ID` 启用 Rust 公共 Resolver 状态工作负载。没有准备好的已完成或活动彩排 run 时跳过。

runner 在本地安装 `k6` 二进制时使用它，否则使用固定的 `grafana/k6:0.57.0` Docker 镜像并采用主机网络。Rust API 将渲染的参赛者题目描述作为比赛范围题目列表的一部分返回，因此该请求即题目描述工作负载；仅超级管理员可用的全局题目端点绝不能由队伍账户使用。

运行 `rehearsal` 或 `full` 之前，确认比赛正在运行、每个账户都已分配、至少一个题目可见、judge worker 健康、打印机使用测试队列或 mock，且指标仪表板正在记录。套件会在任何掉队的 arrival-rate 迭代、检查率低于或等于 99%、HTTP 失败率高于或等于 1% 或 p95 阈值被突破时失败。

## 场景

登录测试：

```text
在一分钟内错开 1,500 个唯一账户登录
测量成功率、p95 延迟、5xx 率、数据库连接
```

非 smoke profile 将登录目标在一分钟内均匀分布。这模拟比赛登录浪潮，而不会把调度器同步的 BCrypt 尖峰变成延迟测量。其余工作负载在 30 秒稳定间隔后开始。提交与打印 arrival-rate executor 预先分配足够的 VU 用于认证与尾延迟，使运行时 VU 初始化不会造成虚假的掉队迭代失败。

题目描述测试：

```text
500 支队伍请求题目列表与描述
如适用，包含 RustFS 的附件读取
```

记分板测试：

```text
500 支队伍轮询或接收记分板更新
验证 Redis 缓存命中行为与 API 延迟
```

提交测试：

```text
每分钟生成 100 至 300 次提交
跟踪队列深度、等待时间、判题时长、system_error 率
```

对于 Judge 积压 / 排空测试，在启用写入前、提交窗口结束时以及每分钟直到深度回到起始值，记录队列深度。k6 请求摘要证明已接受的提交量；RabbitMQ 与 worker 指标证明没有任务丢失。

Resolver 测试：

```text
生成 resolver 快照
连接 screen 与 live 页面
重放官方揭示序列
测量 UI 更新延迟与服务器错误
```

打印测试：

```text
按配置的限制生成打印请求
验证限流、队列状态、失败重试与手动回退
```

## 成功标准

最低成功标准：

- 预期负载下 API 5xx 率保持接近零。
- 登录与普通页面 p95 延迟对比赛使用保持可接受。
- 记分板更新不是每次请求从完整提交历史计算。
- Judge 队列在提交突发后排空。
- 无丢失 Judge 任务。
- 无意外 `system_error` 突发。
- 公共 / live / screen 页面不暴露敏感字段。
- 备份与健康检查在负载期间或之后仍正常工作。

## 故障演练

彩排期间有意测试：

- API 重启。
- Judge worker 停止与重启。
- RabbitMQ 重启。
- Redis 重启或清空后缓存重建。
- 对测试数据做 PostgreSQL 备份与恢复。
- RustFS 临时读取失败。
- 打印机离线。
- Screen 断开与重连。

记录观察到的恢复时间与所需的任何手动命令。

不要在负载生成器中自动化故障演练。操作员应一次注入一个故障、标注仪表板时间戳、等待恢复，然后才继续。将 k6 JSON 摘要与指标截图、队列深度观察、硬件差异与事件时间线一起保存，作为彩排证据。
