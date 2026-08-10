# 判题系统

判题系统分为 Scheduler 与 Worker 集群。它设计为支持每分钟 100 至 300 次提交，以及初始 30 至 60 个并发判题槽位。

## 职责

API 后端负责提交校验与持久化提交创建。Judge Scheduler 与 Worker 负责执行。

Scheduler 职责：

- 发布与协调 Judge 任务。
- 跟踪队列深度与 Worker 健康。
- 支持重判请求。
- 应用重试与死信策略。
- 避免让 Worker 或数据服务过载。

Worker 职责：

- 从 RabbitMQ 拉取任务。
- 获取源代码与测试数据。
- 编译源代码。
- 在沙箱中运行测试用例。
- 强制资源限制。
- 上传日志与产物。
- 报告最终判定。

## 任务生命周期

```text
pending 提交
  -> 发布 judge 任务
  -> worker 接收任务
  -> 提交标记为 judging
  -> 编译
  -> 运行测试用例
  -> 汇总结果
  -> 持久化判定与 runs
  -> 发布记分板 / 事件更新
```

## 队列设计

建议的 RabbitMQ 队列：

- `judge.tasks`：普通判题任务。
- `judge.retry`：延迟或重试任务。
- `judge.dead`：耗尽重试的任务或永久拒绝的结果。
- `judge.rejudge`：与普通队列分离时的显式重判任务。

Rust API 还声明 `judge.rejudge` 与 `judge.results`，以保留已评审的跨服务拓扑。提交 Outbox 行使用过期的 PostgreSQL 租约与稳定的判定 UUID 消息 ID；因此投递语义为至少一次。Rust 结果消费者在持久化判定、runs、提交状态与 TEAM 事件的同一事务中按不可变的结果消息 UUID 去重。它仅在提交后 ACK；无效 / 冲突消息进入死信，瞬时数据库错误重新入队。

专用死信消费者读取 `judge.dead`。每条消息（死信的 `JudgeTask` 或永久拒绝的 `JudgeResult`）携带其所属的判定与提交；消费者原子地将真正卡在 `judging`/`pending` 的提交标记为 `system_error`（并写入审计行与实时 TEAM 事件），然后确认消息。已经完成或已被取代的判定保持不动，因此恢复是幂等的。这保证死信任务绝不会让提交永远显示 "judging"。

Worker 必须在系统持久化最终结果或安全地将任务移入重试 / 死信路径之后 ACK。未处理的 Worker 进程退出不应静默丢失任务。

## 判定状态

提交状态：

- `pending`
- `judging`
- `accepted`
- `wrong_answer`
- `time_limit_exceeded`
- `memory_limit_exceeded`
- `runtime_error`
- `compile_error`
- `output_limit_exceeded`
- `system_error`
- `cancelled`

内部 run 记录应存储每个测试用例的状态、CPU 时间、墙钟时间、内存、退出码、信号与输出哈希或截断的输出引用。

## 沙箱要求

沙箱必须强制：

- 无网络访问。
- CPU 时间限制。
- 内存限制。
- 进程数限制。
- 文件大小限制。
- 输出大小限制。
- 隔离的文件系统。
- 只读测试数据挂载。
- 每次运行后清理临时目录。

**生产 / 比赛部署**：Judge 容器不得挂载 Docker socket，也不得以 privileged 容器运行。

**开发沙箱例外**：在本地开发与 CI 中，Worker 可以作为 sibling-docker 容器运行（挂载 `/var/run/docker.sock`），前提是 (a) Worker 进程以加入 `docker` 组的非 root 用户运行，(b) Worker 绝不从用户可控输入构造 shell 或 Docker CLI 命令字符串，只能通过 Rust `bollard` 客户端使用类型化的 Docker API 参数，并且 (c) 每个沙箱容器都以无网络、只读根文件系统、`no-new-privileges`、PID 限制、非 root UID/GID 以及自动清理的方式创建。威胁模型：用户提交的代码是不可信边界，且始终停留在沙箱容器内；Worker 代码属于可信基。

### 沙箱实现变体

生产选项：

- nsjail。
- bubblewrap。
- runsc (gVisor)。
- 非特权 Firecracker。

开发选项：

- sibling-docker，并满足上述开发沙箱例外约束。

## 语言支持

P0 语言：

- C
- C++
- Java
- Python

P1 语言：

- Go
- Rust

### 当前 Rust Worker 切片

Worker 现在以有界 prefetch 消费 `judge.tasks`，并校验 JSON 契约与 AMQP 消息 ID。它被动验证现有拓扑，因此启动时失败而非静默创建分歧队列。格式错误或永久无效的任务会在原始任务 ACK 之前 Publisher-Confirmed 到 `judge.dead`。瞬时存储 / 沙箱失败会拒绝到任务重试 / TTL 路径。完成的结果以持久模式发布到 `judge.results`，并且仅在 Publisher Confirm 之后 ACK 任务。结果消息 UUID 即判定 UUID，使 Worker 崩溃后的重放对 API 消费者保持幂等。

RustFS 源码与测试数据下载有大小限制并通过 SHA-256 验证。测试数据缓存名称包含题目、不可变版本与哈希；损坏的缓存条目会被丢弃。每个判定的工作目录是私有的，并在任何结果之后移除。

所有四种 P0 语言都通过 Bollard 针对固定的 `judge-runtime-c/cpp:12.2.0`、`judge-runtime-java:21` 与 `judge-runtime-python:3.12.13` 镜像进行编译或语法检查并执行。Java 获得 2 倍时间乘数，Python 3 倍；C/C++ 保持 1 倍。每个判定创建一个容器、编译一次，并通过 Docker exec 顺序执行用例后强制移除。编译阶段以 1 GiB 配额开始；成功编译后，容器 cgroup 收紧到题目内存限制。容器无网络、根文件系统只读、所有 capability 被丢弃、`no-new-privileges`、非 root 身份、PID/CPU/内存限制与有界输出。标准答案绝不挂载进参赛者容器：Worker 只将当前输入复制到工作目录，并与保留在 Worker 侧的答案比较输出。判定覆盖编译错误、答案错误、时间、内存、运行时与输出限制。标准比较规范化 CR/LF、每行尾部空格 / 制表符与尾部空行，但其余按字节精确比较。Worker 通过 RabbitMQ 发布带确认、带版本的心跳，包含进程实例、容量、活动任务数、支持的语言、运行时镜像版本与沙箱运行时。API 将这些心跳持久化到 PostgreSQL，并在最近一次报告后 15 秒内将 Worker 视为在线。`JUDGE_TASK_PREFETCH` 定义真实的并行执行容量；关闭时停止新投递并在关闭 RabbitMQ 连接前排空进行中的工作。

运行时镜像从 `deploy/judge/runtimes` 构建，并包含 GNU `time`。Worker 将每次运行的 user + system CPU 时间计入语言调整后的限制，并记录 GNU `time` 的峰值 RSS。指标记录由计时父进程在提交的进程退出后发出，从容器 stderr 末尾提取，并从参赛者可见日志中移除。三倍墙钟截止时间仍是死锁 / 睡眠的安全边界；Docker cgroup 采样仍作为容器在 GNU `time` 报告之前被杀死的回退。使用 `scripts/build-judge-runtimes.sh` 构建固定的本地标签。生产 `runsc` 验收仍是后续工作。

任务确认严格在确认结果发布之后。如果 RabbitMQ 在 Worker 接受任务之后但在该边界之前重启，持久的未确认投递会被重新入队。Worker 重连并在至少一次语义下重新评估它；稳定的判定 / 消息标识符让 API 结果事务折叠任何后续重复。

每种语言运行时都应定义编译命令、运行命令、文件扩展名、源码大小限制、需要的超时乘数，以及可在管理员诊断中显示的版本字符串。

## 测试数据处理

RustFS 是上传测试数据的真相来源。Worker 可以维护本地缓存。

缓存规则：

- 缓存键应包含题目 ID、数据版本与哈希。
- Worker 必须在判题前验证哈希。
- 题目数据变更后不得使用过期缓存条目。
- 正式比赛 freeze 应锁定测试数据版本。

## 故障处理

预期故障与所需行为：

| 故障 | 行为 |
|---|---|
| Worker 在任务期间退出 | RabbitMQ 重新投递或任务重试 |
| 编译工具缺失 | 标记 `system_error` 并发出健康告警 |
| 沙箱设置失败 | 标记 `system_error` 并发出健康告警 |
| RustFS 读取失败 | 瞬时失败重试；重复失败进入死信 |
| 结果写入失败 | ACK 前重试 |
| 测试数据哈希不匹配 | 停止判题并标记 `system_error` |

## 可观测性

Worker 与 Scheduler 应暴露：

- Worker 在线数。
- Judge 槽位容量与使用率。
- 队列深度。
- 任务延迟。
- 编译错误率。
- 系统错误率。
- 平均与 p95 判题时长。
- 各语言提交数。
- 本地测试数据缓存命中率。
