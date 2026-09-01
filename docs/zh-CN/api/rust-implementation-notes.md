# Rust API 实现说明

`openapi.yaml` 仍是先前后端捕获的兼容性基线。以下说明描述已评审的 Rust 行为，它们有意比生成的基线更明确。

## 运行时 OpenAPI

Rust API 现在从 Utoipa 注解生成其运行时 OpenAPI 3.1 契约：

| 路径 | 用途 |
|---|---|
| `GET /api/openapi.json` | 机器可读的生成契约 |
| `GET /api/docs` | 使用内置资产的离线可用 Swagger UI |

生成的契约目前覆盖全部 176 个 Rust 操作：进程 / 就绪健康、全部五个认证端点、八个比赛核心端点、十四个队伍与比赛名单端点、十六个题目目录与测试数据端点、十七个提交、重判、批量重判与导出端点、完整公告工作流、六个记分板与快照端点、七个打印端点、七个气球端点、七个 Clarification 端点、十三个 Resolver 端点、十八个颁奖、展示、主机脚本与证书端点、比赛 Judge 队列状态、四个工作人员账户与两个比赛管理员范围端点、一个审计日志查询、三个实时 SSE 端点，以及三个异步提交导出任务端点。它还暴露无需认证的 `/metrics` Prometheus 暴露端点，与 JSON 健康探针并列。契约将 `PB_SESSION` 会话 Cookie 以及 `XSRF-TOKEN` Cookie 与匹配的 `X-XSRF-TOKEN` 请求头记录为独立安全方案；变更需要所有适用方案。登录需要 CSRF Cookie 与请求头但不需要现有会话；登出与密码变更需要全部三者。契约测试验证唯一操作 ID、匹配的路径参数、认证安全组合、RFC3339 日期时间 schema 与提供的 JSON 端点。

此运行时契约有意与 `docs/api/openapi.yaml` 分离。YAML 文件是冻结的 Java 兼容性输入；已评审的 Rust 操作逐步进入生成契约，直到所有当前 Axum 路由都被表示。

## P2 提交相似度基础

新提交会持久化在移除注释与格式化空白（保留字符串与字符字面量）后生成的 SHA-256 `source_fingerprint`。比赛管理员可以查询 `GET /api/admin/contests/{contestId}/submission-similarity`，并带可选的 `problemId`、`language` 与 `minGroupSize` 筛选。结果按比赛、题目、语言与规范化指纹分组，只包含提交 / 队伍 ID 与计数。这个首个 P2 切片检测精确的规范化重复，而不改变判题或暴露源代码。同一迁移存储规范化五元组 shingle 的 64 位 SimHash；比赛管理员可以查询 `/api/admin/contests/{contestId}/submission-similarity/pairs`，带有限相似度阈值（50--100%），以复核跨队伍近似匹配。结果有上限并排除同队配对；近似匹配只是候选证据，绝不自动触发纪律处分。对迁移前的提交，一个带 CSRF 保护的回填端点每次请求最多处理 1,000 行，重新下载每个源码、验证其权威 SHA-256，并仅在验证后写入签名。

## P2 展示模板

Screen 与 live 展示配置选择四种已验证的视觉模板之一：`DEFAULT`、`CINEMATIC`、`MINIMAL` 或 `SPLIT`。现有配置迁移到 `DEFAULT`；两个操作员控制台都持久化选择，OBS live 视图应用所选布局与配置的强调色。未知模板标识符被 API 与数据库 check 约束拒绝，因此公共展示页面不能用于注入标记或 CSS。

## P2 OI/IOI 计分

比赛计分独立于 ICPC 投影配置。`ICPC` 保持解题 / 罚时排名；`OI` 与 `IOI` 使用整数毫分，每个题目选择最高分或最新完成的提交。每个比赛题目可以定义有序、不重叠的子任务及其测试索引，分数必须精确求和为题目最大值。判定结果持久化总分与每个子任务分数，记分板按总分排名。比赛级 `FULL`、`SCORE_ONLY` 与 `NONE` 反馈策略在比赛进行中编辑队伍面向的测试详情，而管理员保留完整审计视图。掩码一致应用于队伍提交列表与详情视图，因此受限策略无法通过列出提交而不是打开它们来绕过。

## P2 交互式与输出-only 判题

Judge 任务携带向后兼容的 `judgeMode` 字段。`OUTPUT_ONLY` 提交上传包含根级 `1.out`、`2.out` 等文件的有界 ZIP；Worker 验证归档而不编译或执行参赛者内容，并为每个预期用例计分。`INTERACTIVE` 题目引用存储在题目桶中、经 SHA-256 验证的 Linux ELF interactor。Worker 在同一无网络、只读根沙箱中运行 interactor 与参赛者，使用命名管道、墙钟超时、进程清理与单独的退出解释。题目管理员可以上传替换 interactor；旧对象进入持久清理队列，所有引用参与孤儿对账。

## 对象存储孤儿补偿

对象存储适配器现在支持分页桶枚举。清理模块每小时运行一次幂等孤儿扫描，只将应用拥有的前缀（`problems/`、`submissions/` 与 `prints/`）与权威数据库引用比较，然后将不匹配的键持久化为 `ORPHAN_SCAN` 清理任务。现有租约、重试与唯一 `(bucket, object_key)` 约束使扫描可以安全重复，并允许临时存储失败稍后由清理 runner 补偿。未知前缀绝不被触碰，新列出的对象受十五分钟宽限期保护，不支持列出的适配器保持安全的仅删除行为。

同一扫描还执行反向对账。完整桶列表中缺失的持久数据库引用被记录在 `object_storage_integrity_findings` 中；稍后成功的扫描在对象被恢复或其数据库引用被移除时将发现标记为已解决。未解决计数暴露在就绪 JSON、Prometheus 与管理员健康仪表板中。缺失引用绝不自动修复或删除，因为数据库没有足够信息安全地重建其内容。

## 工作人员账户

所有端点都要求 `SUPER_ADMIN` 用户类型与完成的强制密码变更：

| 方法 | 路径 | 行为 |
|---|---|---|
| `GET` | `/api/admin/staff-accounts` | 按用户名列出非队伍账户 |
| `POST` | `/api/admin/staff-accounts` | 创建启用的工作人员账户；`requirePasswordReset` 控制首次登录变更（默认 true） |
| `PATCH` | `/api/admin/staff-accounts/{userId}` | 更新显示名、工作人员类型或启用状态 |
| `POST` | `/api/admin/staff-accounts/{userId}/reset-password` | 替换密码，撤销所有目标会话；`requirePasswordReset` 控制下次登录变更（默认 true） |

分页接受从零开始的 `page` 与 1 至 100 的 `size`。唯一接受的排序表达式是 `username,asc`；排序固定而非插入 SQL。

重要的稳定错误：

| HTTP | 代码 | 含义 |
|---|---|---|
| 400 | `VALIDATION_FAILED` | 无效 body、分页、工作人员类型或密码 |
| 401 | `NOT_AUTHENTICATED` | 缺失或过期会话 |
| 403 | `PASSWORD_RESET_REQUIRED` | 操作者必须先更改自己的密码 |
| 403 | `FORBIDDEN` | 操作者不是超级管理员 |
| 404 | `STAFF_ACCOUNT_NOT_FOUND` | 目标不存在或是队伍账户 |
| 409 | `USERNAME_TAKEN` | 规范化用户名已存在 |
| 409 | `SELF_ACCESS_CHANGE_FORBIDDEN` | 操作者试图移除自己的超级管理员访问 |
| 409 | `LAST_SUPER_ADMIN` | 该变更会留下零个启用的超级管理员 |

密码哈希与原始会话 token 绝不出现在响应中。成功的变更向 `audit_logs` 写入 `STAFF_ACCOUNT_CREATED`、`STAFF_ACCOUNT_UPDATED` 或 `STAFF_PASSWORD_RESET`。

## 比赛管理范围

两个端点都要求 `SuperAdminContext`：

| 方法 | 路径 | 行为 |
|---|---|---|
| `GET` | `/api/admin/contest-managers` | 列出带 `CONTEST_MANAGE` 的账户与排序的比赛 ID |
| `PUT` | `/api/admin/contest-managers/{userId}/contests` | 原子替换管理员的比赛范围 |

输入 ID 必须为正，去重并排序，且限制为 1,000 项。每个引用的比赛必须存在且未软删除。校验、删除、批量插入与 `CONTEST_MANAGEMENT_SCOPE_UPDATED` 审计插入共享一个事务。校验失败使先前范围不变。

稳定的未找到错误为 `CONTEST_MANAGER_NOT_FOUND` 与 `CONTEST_NOT_FOUND`。

## 审计日志查询

`GET /api/admin/audit-logs` 要求 `SuperAdminContext`，并支持：

- 精确 `actorUserId`；
- 不区分大小写的子串 `action`；
- 不区分大小写的精确 `result`；
- 包含边界的 RFC3339 `from` 与 `to`；
- 1 至 100 的页大小。

固定排序为 `createdAt,desc`，并以 `id` 作为确定性并列决胜键。`action` 中的百分号、下划线与会读反斜杠字符在参数化 `LIKE` 查询之前被转义，因此用户输入不能成为意外通配符。倒置的时间范围返回 `VALIDATION_FAILED`。

## 比赛核心

本切片实现：

| 方法 | 路径 | 授权 |
|---|---|---|
| `GET` | `/api/contests` | 可选认证；响应按可见性限定 |
| `GET` | `/api/contests/{contestId}` | 可选认证；不可访问的比赛返回 404 |
| `POST` | `/api/contests` | 已完成密码的 `SUPER_ADMIN` |
| `PATCH` | `/api/contests/{contestId}` | `SUPER_ADMIN` 或分配的 `CONTEST_MANAGE` 账户 |
| `DELETE` | `/api/contests/{contestId}` | `SUPER_ADMIN` 或分配的 `CONTEST_MANAGE` 账户 |

读取可见性有意在单一数据库谓词中求值：

- 匿名用户看到未删除的 `PUBLIC` 比赛；
- 分配的队伍看到其比赛加公共比赛；
- 比赛管理员看到分配的比赛加公共比赛；
- 带运维权限的工作人员看到每个未删除的比赛；
- 只有超级管理员可以设置 `includeDeleted=true`。

分页限制为 500 行，因为现有权限管理屏幕请求该上限。排序字段采用白名单并翻译为固定 SQL 片段；值绝不插值。

创建接受无日程或全部 `startAt`、`freezeAt`、`endAt`，顺序为 `startAt <= freezeAt <= endAt`。Patch 请求可以组合存储日程更新单个日程字段，但比赛离开 `DRAFT` 或 `FROZEN_CONFIG` 后，日程变更以 `CONTEST_SCHEDULE_LOCKED` 拒绝。

活动比赛名称受 PostgreSQL 部分唯一索引保护。软删除的名称可以重用。更新递增 `version`；创建、更新与删除在同一事务中写入 `CONTEST_CREATED`、`CONTEST_UPDATED` 与 `CONTEST_DELETED`。

克隆已实现；归档比赛的恢复仍是未来切片。

## 比赛生命周期与延长

生命周期端点为 `POST /api/contests/{contestId}/transitions`。允许的边在纯 `project-balloon-domain` crate 中定义：

```text
DRAFT -> FROZEN_CONFIG -> RUNNING -> ENDED -> ARCHIVED
                              |
                              v
                            PAUSED
                              |
                              +-----> RUNNING
```

`RUNNING` 可以直接移到 `PAUSED` 或 `ENDED`。`FROZEN_CONFIG` 需要完整、有序的日程。服务在求值转换前锁定比赛行，因此并发相同请求恰好产生一个成功，其余请求收到 `CONTEST_TRANSITION_INVALID`。

`POST /api/contests/{contestId}/extensions` 接受 `expectedEndAt` 与 `newEndAt`。延长仅在 `RUNNING` 或 `PAUSED` 允许，要求存储的结束时间等于 `expectedEndAt`，并要求新值更晚。稳定冲突为：

- `CONTEST_EXTENSION_STATUS_INVALID`；
- `CONTEST_END_TIME_NOT_SET`；
- `CONTEST_EXTENSION_STALE`；
- `CONTEST_EXTENSION_NOT_LATER`。

成功的转换与延长在业务事务中递增 `version` 并写入审计行。延长还向 `realtime_outbox` 写入 PUBLIC 与 STAFF 的 `CONTEST_EXTENDED` 消息。

API 托管的分发器使用 `FOR UPDATE SKIP LOCKED` 认领可用行、递增尝试计数，并给每个 `PUBLISHING` 行租约。成功的本地扇出将行标记为 `PUBLISHED`；过期租约作为 `FAILED` 恢复并重新认领。因此投递至少一次，消费者按稳定事件 UUID 去重。

Rust API 保留现有浏览器路由：

- `GET /api/public/events/contests/{contestId}`；
- `GET /api/events/contests/{contestId}`；
- `GET /api/team/events/contests/{contestId}`。

帧保留版本 1 的 Java/TypeScript 形态。公共访问使用正常比赛可读性，工作人员访问要求已批准的运维权限加比赛可读性，队伍访问在比赛内解析认证队伍。当前扇出通道是进程本地的；生产实例可选地通过 Redis Pub/Sub 桥接其本地 Tokio 通道。线上信封保持与 Java 兼容：`{originInstanceId, teamId, event}`。源实例本地发布并忽略自己的 Redis 回声；对等实例验证并转发信封。Redis 发布必须成功，分发器才能将 Outbox 行标记为 `PUBLISHED`。

## 队伍账户与比赛名单

参赛者身份是从认证用户 ID 到队伍 ID 的显式一对一 `team_accounts` 关系。授权绝不依赖可变的用户或队伍显示名。

Rust API 在 `/api/teams` 与 `/api/contests/{contestId}/teams` 下提供队伍 CRUD、成员 CRUD、密码重置、原子批量导入与比赛名单分配。队伍响应包含其乐观 `version` 与账户元数据；更新可以发送 `expectedVersion` 以拒绝过期写入。

超级管理员可以管理每个队伍。只有包含该队伍的每个比赛都在其分配范围内时，比赛管理员才能管理该队伍；尚未分配给比赛的队伍仍仅限超级管理员。

批量导入接受 1--100 行并要求 `idempotencyKey`。PostgreSQL 事务 advisory locking 将同一键的重试串行化，整个批次要么提交要么回滚。提供的密码使用 Argon2 哈希，API 绝不返回。

生成的账户（批量导入、单个队伍创建、工作人员账户创建与每次密码重置）默认要求首次登录改密。每个此类请求接受可选的 `requirePasswordReset` 布尔值（默认 `true`）；仅当初始密码带外交付且接受其复用时设为 `false`。批量导入将其批次级 `requirePasswordReset` 应用于每一行，并忽略行级值。

比赛达到 `ENDED` 或 `ARCHIVED` 后名单变更被拒绝。每次变更在业务事务中记录审计条目。名单变更还入队 STAFF 事件与受影响队伍的私有 TEAM 事件。

## 题库核心

第一个 Rust 题库切片在 `/api/problems` 暴露仅超级管理员的 CRUD。它校验小写 kebab-case slug、有界正资源限制、语言标签与封闭的判题语言集（`c`、`cpp`、`java`、`go`、`rust` 与 `python`）。更新请求要求 `expectedVersion`，并发修改返回 `PROBLEM_VERSION_STALE`。

题目删除是软删除，存在任何比赛分配时以 `PROBLEM_ASSIGNED_TO_CONTEST` 拒绝。PostgreSQL 强制正限制与仅活动 slug 唯一性。创建、更新与删除在同一事务中写入 `PROBLEM_CREATED`、`PROBLEM_UPDATED` 与 `PROBLEM_DELETED` 审计事件。

队伍面向投影通过比赛题目读取提供；限定管理员可以通过其专用端点管理持久化描述、附件与不可变测试数据版本。

多语言描述在 `PUT /api/problems/{problemId}/statements/{langCode}` 处 upsert。API 保留有界 Markdown 源供编辑，并返回由 `pulldown-cmark` 渲染再经 `ammonia` 净化的 HTML。Script 元素、事件处理器属性与不安全链接目标不被信任，也不作为可执行标记返回。

比赛题目分配 CRUD 在 `/api/contests/{contestId}/problems` 下可用。超级管理员与分配的比赛管理员可以配置别名、显示顺序与气球颜色。变更锁定比赛行，且仅在其状态为 `DRAFT` 时接受；`FROZEN_CONFIG` 与之后每个状态返回 `CONTEST_PROBLEM_CONFIG_FROZEN`。数据库唯一约束是重复别名与显示位置的最终权威。

已认证的名单队伍使用同一比赛题目列表端点。队伍访问在 `RUNNING` 之前以及对其显式 `team_accounts` 身份未列名的比赛中，有意以 `CONTEST_NOT_FOUND` 隐藏。`RUNNING`、`PAUSED`、`ENDED` 与 `ARCHIVED` 比赛只暴露分配的题目别名、展示元数据、资源限制、允许语言与净化后的描述。可选 `lang` 查询选择首选描述；题目默认语言与确定性语言顺序提供回退。原始 Markdown、测试数据元数据、对象键与附件元数据不包含在此队伍投影中。名单队伍只能在比赛达到 `RUNNING` 后通过其不透明数据库 ID 下载特定附件；服务再次授权父题目，绝不返回存储键。

比赛管理员通过 `GET /api/problems?contestId={managedContestId}` 加载共享题目元数据。API 在返回有界目录前验证请求的非删除比赛已分配给操作者；省略 `contestId` 或指定外来比赛绝不扩大其访问。全局目录访问与题目创建仍仅限超级管理员。当使用该题目的每个活动比赛都在其分配范围内时，比赛管理员可以从比赛工作台编辑分配题目的别名、显示顺序、气球颜色、元数据、多语言描述、附件与不可变测试数据版本。外来分配或未分配题目隐藏为未找到，任何非 DRAFT 分配锁定元数据、描述、附件与测试数据。题目删除仍是仅限超级管理员的目录操作。

`PUT /api/contests/{contestId}/problems/reorder` 要求每个分配题目恰好一次，位置为 1 至 1000 的唯一值。该操作锁定比赛与其分配，拒绝不完整或外来 ID 集合，且仅在 `DRAFT` 中运行。PostgreSQL 将每比赛顺序唯一约束推迟到事务提交，允许安全的位置交换而不暴露或提交中间重复。被拒绝的不完整与冻结请求保持存储顺序不变。

## 对象存储与附件

对象存储在本地 API 配置中可选，启用后对文件操作是强制的。Rust 适配器使用一个共享 AWS S3 客户端，带显式端点、区域、静态部署凭据、path-style 桶寻址与请求超时。就绪检查配置的题目桶，而不在 HTTP 响应中暴露端点、桶、凭据或 SDK 错误。

题目附件键遵循 `problems/{problemId}/attachments/{sha256}/{uuid}-{filename}`。公共附件响应绝不包含桶或对象键。

`GET /api/problems/{problemId}/statements` 为限定题目编辑器返回每种存储语言的持久化 Markdown 与净化 HTML。它使用与描述变更相同的全分配管理规则。

`GET /api/problems/{problemId}/attachments` 为限定题目编辑器按创建时间与 ID 排序返回持久化附件历史。它使用与附件变更相同的全分配管理规则，绝不返回对象存储键。

`POST /api/problems/{problemId}/attachments` 接受恰好一个 `kind` 与一个 `file` multipart 字段。`kind` 为 `SAMPLE` 或 `SUPPLEMENT`；拒绝空文件与大于 20 MiB 的文件。文件名简化为安全 basename，媒体类型采用白名单。服务执行数据库预检、写入对象，然后锁定并重新验证题目，再插入其 SHA-256 元数据与审计行。失败的元数据事务触发对新写入对象的最佳努力删除。任何比赛超出 `DRAFT` 的分配在提交前拒绝该变更。

`GET /api/problems/{problemId}/attachments/{attachmentId}` 通过父题目授权访问。名单队伍只能从 `RUNNING`、`PAUSED`、`ENDED` 或 `ARCHIVED` 的已分配比赛读取附件；授权工作人员可以读取其限定分配。响应使用安全媒体类型、`Content-Disposition: attachment` 与 `X-Content-Type-Options: nosniff`。

`DELETE /api/problems/{problemId}/attachments/{attachmentId}` 使用与上传相同的全分配比赛管理员规则，并限制为仅被 `DRAFT` 比赛使用的题目。它先提交元数据删除与审计，再进行最佳努力对象清理，使失败的存储请求不会在数据库中留下对缺失对象的活引用。清理失败记录为孤儿候选。

20 MiB 附件上传路径在内存中有界，而 HTTP 附件下载使用有界 S3 流。失败清理记录在 `object_storage_cleanup_tasks`，并使用 `FOR UPDATE SKIP LOCKED`、过期租约、幂等 S3 删除与封顶指数退避重试。显式附件删除在元数据移除的同一事务中记录该任务。周期性双向对账在宽限期后将未引用的自有对象入队，并持久化缺失引用发现，直到引用对象重新出现或元数据被移除。

## 不可变测试数据

`POST /api/problems/{problemId}/testdata` 接受一个最大 256 MiB 的 ZIP 文件。它当前检查文件名、媒体类型与 ZIP 签名，然后计算 SHA-256 并写入带版本范围、UUID 后缀的对象。最终数据库事务重新锁定题目、重复全分配范围与生命周期检查，并且仅当先前版本仍为当前版本时推进当前指针。并发失败者或被拒绝的 freeze 竞争通过删除其唯一对象来补偿。

`problem_testdata_versions` 为每个成功版本保留对象键、哈希、字节数、用例数、上传者与创建时间。管理员 API 在 `GET /api/problems/{problemId}/testdata/versions` 列出此不可变历史，并在 `GET /api/problems/{problemId}/testdata/versions/{version}` 下载所选归档。浏览器响应标识当前版本但绝不暴露持久对象 URL 或键。每次下载针对不可变 SHA-256 元数据验证存储字节。`POST /api/problems/{problemId}/testdata/versions/{version}/activate` 在与上传相同的范围与生命周期锁下将兼容性指针移到现有版本。其 `expectedCurrentVersion` 字段防止过期浏览器替换并发的上传或激活。后续上传分配 `max(history.version) + 1`，因此激活旧版本不能重用不可变版本号。`GET /api/problems/{problemId}/testdata` 仍是授权的当前版本兼容下载；队伍不能调用这些管理端点。Vue 题目编辑器暴露完整历史、逐版本下载、当前标记与受保护的激活操作。

在触碰对象存储之前，API 在阻塞 worker 线程上解析 ZIP 中央目录并完整读取每个常规条目。它拒绝不安全或嵌套路径、重复名称、控制字符、加密、链接、特殊文件、不支持的压缩、不一致的展开大小、超过 10,000 个条目、超过 256 MiB 的条目、总展开超过 1 GiB，以及单条目压缩比超过 200。根级 `.in` 与 `.out` 文件必须形成与遗留 P0 Worker 兼容的精确非空配对集合；派生的用例数随不可变版本持久化。

Judge 任务构造可以请求内部权威引用。仅当 `problems` 兼容性指针与 `problem_testdata_versions` 中的相同版本、对象键与 SHA-256 完全匹配时返回，防止不一致或部分迁移的指针被分发。

测试数据下载使用有界 S3 流，并在消费响应时验证不可变 SHA-256。附件 HTTP 下载也使用有界 S3 流。失败的上传补偿删除进入持久对象清理队列。桶对账与真实 RustFS 集成测试覆盖存储边界。Worker 使用相同的根级用例策略执行有界解压。

## 提交创建边界

`POST /api/contests/{contestId}/submissions` 保留已评审的遗留 multipart 契约：一个 JSON `metadata` 字段与一个 `source` 文件。API 只接受 C、C++、Java 与 Python 文件扩展名，源码体为 1 字节至 64 KiB。队伍身份始终来自认证用户的显式 `team_accounts` 行；不接受客户端提供的队伍 ID。

预检与最终锁定验证要求未删除队伍、活动名单、已分配题目、当前时间在其日程内的 `RUNNING` 比赛、启用的语言，以及题目当前测试数据指针与其不可变版本行精确匹配。源码字节写入配置的源码桶中的 `submissions/{contestId}/{teamId}/{uuid}.{extension}`，并持久化 SHA-256。

最终 PostgreSQL 事务获取队伍 advisory lock，并强制前一分分钟最多接受 20 次提交。它原子地插入提交、初始判定 UUID、`submission_outbox` 中序列化的 `JudgeTask`、TEAM 范围状态事件与审计行。任何最终验证、限流或事务失败都触发对唯一源码对象的最佳努力删除。

## Judge Task RabbitMQ 分发

启用 RabbitMQ 分发时，Lapin 使用配置的 AMQP 或 AMQPS URL 连接，并声明已评审的持久 direct 拓扑：`judge.tasks`、`judge.retry`、`judge.dead`、可选的 `judge.rejudge` 与 `judge.results`，每个都带匹配的 exchange 与 routing key。任务队列将 Worker 拒绝死信到重试 exchange；重试队列等待 10 秒并死信回任务 exchange。

提交分发器使用 `FOR UPDATE SKIP LOCKED` 认领 PostgreSQL Outbox 行、将其改为 `PUBLISHING`，并分配实例 UUID 加过期租约。发布使用持久 JSON 消息，判定 UUID 同时作为 AMQP 消息 ID 与 `messageId` 请求头、强制路由与逐消息 Publisher Confirm。只有路由的 broker ACK 将行改为 `SENT`；该事务还将未变更的 `PENDING` 提交移到 `JUDGING` 并写入其 TEAM 事件。

失败变为带封顶指数退避的 `FAILED`。尝试次数封顶；API 崩溃后过期的 `PUBLISHING` 租约可重新认领。broker ACK 之后但数据库更新之前的崩溃可能再次发布同一判定 UUID，因此结果消费必须保持幂等。发布者重建断开的通道并重试一次。就绪检查主动连接、声明拓扑、报告 task/dead 队列深度，并包含 pending/failed 提交 Outbox 计数，而不暴露 AMQP URL。

API 以有界 prefetch 消费 `judge.results`。`JudgeResult` 具有封闭判定集、有界日志与指标、不可变消息 UUID、Worker 身份、时间戳与逐测试 runs。一个 PostgreSQL 事务锁定判定、验证其提交、写入最终判定与唯一 runs、更新提交并入队 TEAM 事件。仅在提交后 ACK 投递。相同消息 UUID 是幂等 ACK；不同消息不能覆盖已完成的判定。无效或冲突结果被拒绝到 `judge.dead`；瞬时数据库错误重新入队并强制消费者会话重连。

PostgreSQL 结果事务经过集成测试。实时 Docker 验证覆盖 Publisher Confirm、任务重试 TTL、结果 ACK、重复结果幂等、畸形结果死信、RustFS 往返与队列深度就绪。Broker 重启恢复由 Docker 故障注入测试覆盖。

## Judge Worker 获取与 C/C++ 执行

Rust Worker 执行 RabbitMQ 消费 / 结果确认 / ACK 边界、RustFS 源码与不可变测试数据获取、产物大小与 SHA-256 验证、哈希键本地测试数据缓存、安全根级用例解压与保证的每任务清理。C 与 C++ 通过 Bollard 使用固定运行时镜像，而不是主机进程执行。每次判定创建一个容器：编译与每个顺序用例运行使用该容器内的 Docker exec，然后强制移除容器。编译使用有界 1 GiB 配额，用例执行前 cgroup 减少到任务内存限制。规范输出保留在容器外；只有当前输入暂存到其工作目录。容器请求强制执行已评审的本地开发沙箱控制：无网络、只读根、非 root 用户、丢弃 capability、`no-new-privileges`、PID、CPU、内存、输出与墙钟限制。

被忽略的集成测试使用真实固定 C++ 镜像与完整的 RabbitMQ → RustFS → cache/hash → compile/run → 确认 JudgeResult 路径。真实锁定容器测试覆盖 C、C++、Java 21、Python 3.12、Go 1.24 与 Rust 1.88。Java 使用显式 2 倍时间乘数，Python 3 倍；Go 与 Rust 保持 1 倍。生产 rootless Podman/runsc 验证仍是后续工作。Docker cgroup 统计现在填充每次运行与聚合的峰值内存字段，cgroup CPU 纳秒驱动报告的运行时间与语言调整后的 CPU 限制。有界墙钟截止时间仍作为安全限制与短进程回退。Worker 现在发布带确认、带版本的 RabbitMQ 心跳，包含稳定进程实例 ID、容量、活动任务数、支持的语言、镜像版本标签与沙箱运行时。API 将其存储在 PostgreSQL 中，并在 `/api/health` 暴露在线 / 过期计数与当前容量。`JUDGE_TASK_PREFETCH` 是 Worker 执行容量：任务并发运行至该上限，优雅关闭排空已接受的工作而不消费新投递。自动化 Docker 故障测试在任务位于 handler 内部时重启 RabbitMQ。原始未确认投递被重新入队，Worker 重连，且恰好一个确认结果对稳定判定标识符保持可见。

第一个 ICPC 记分板 API 切片暴露 `GET /api/contests/{contestId}/scoreboard` 与范围保护的 `GET /api/admin/contests/{contestId}/scoreboard`。两者都接受 `groupName` 与 `participationType` 筛选。在配置的 freeze 间隔内，公共变体只使用 `freezeAt` 之前的提交重建单元；管理员变体读取实时 PostgreSQL 投影。排序为解题数降序、罚时升序、最后解题升序，然后队伍 ID。STAR 与 PRACTICE 行保持可见，但只有 OFFICIAL 行获得 `officialRank`。每题目 First Blood 是 OFFICIAL 与 STAR 队伍中最早可见的 AC，队伍 ID 作为确定性同时间决胜键；PRACTICE 队伍被排除。匹配的公共与管理 CSV 导出在相同路径加 `.csv` 后缀可用，并使用已筛选 / 冻结的响应，因此导出不能绕过记分板可见性规则。

比赛管理员可以通过 `POST /api/admin/contests/{contestId}/scoreboard/snapshots` 持久化不可变记分板，并通过 `GET /api/admin/contests/{contestId}/scoreboard/snapshots/latest` 检索最新匹配产物。请求选择 `PUBLIC` 或 `ADMIN` 以及与实时记分板相同的可选组与参赛筛选。PostgreSQL 在该精确选择器内分配单调递增版本、存储完整 JSON 载荷与其 SHA-256 摘要，并记录创建用户。数据库触发器拒绝更新与删除。重判从活动判定重建受影响的权威单元与行，而已创建的快照对 Resolver 与颁奖使用保持不变。

当 `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED=true` 时，实时记分板可选使用 Redis。PostgreSQL 在每个比赛上存储单调修订版本，数据库触发器为投影、名单、比赛题目、日程与队伍显示变更推进它。缓存键包含该修订版本、公共 / 管理员变体、freeze 阶段与规范化筛选。Redis 失败回退到 PostgreSQL；结果更新期间的故障不能在恢复后复活旧键。集成覆盖现在并发应用同一 Judge Result 并验证恰好一次应用加一次幂等重复、清空 Redis 并从 PostgreSQL 重建同一修订版本，以及暂停 Redis 容器验证配置的操作超时产生有界缓存未命中。同秒排名与 First Blood 并列使用队伍 ID 作为最终稳定键。

比赛管理员可以通过 `POST /api/admin/contests/{contestId}/submissions/{submissionId}/rejudge` 重判一个已完成提交。JSON body 要求 `expectedJudgementId`，使并发操作员操作乐观且确定。在一个事务中，API 取代旧判定、终态取消任何未发送的旧 Outbox 任务、创建新活动判定与 Judge 任务、将提交重置为 `PENDING`、重建受影响的记分板投影、发出 TEAM 与 STAFF 事件并记录审计条目。并发逃逸的旧任务仍可能被投递，但其结果作为已被取代而确认，不能覆盖新活动判定。拒绝归档比赛与非终态活动判定。

提交浏览可通过 `GET /api/contests/{contestId}/submissions` 与 `GET /api/contests/{contestId}/submissions/{submissionId}` 供认证队伍使用，另有匹配的 `/api/admin/contests/{contestId}/submissions` 管理员路径。列表分页，并支持队伍、题目、状态与语言筛选。队伍查询始终将显式 `team_accounts` 身份作为数据库谓词添加，因此提交 ID 不能枚举另一队伍的元数据或源码对象。详情在授权后从对象存储加载 UTF-8 源码，并包含带排序 runs 的活动与已被取代判定历史。编译日志与 stderr 尾部保持纯文本、移除控制字符，并在序列化前有界。

批量重判使用持久表 `batch_rejudge_tasks` 与 `batch_rejudge_items`，通过 `/api/admin/contests/{contestId}/rejudge-tasks` 下的比赛范围预览、创建、列表、详情、暂停与恢复端点。创建要求精确预览计数、确认文本 `REJUDGE {count}` 与幂等键；最多 10,000 个已完成活动判定可按题目、队伍、语言、判定或提交时间选择。后台 runner 使用 `FOR UPDATE SKIP LOCKED` 与 30 秒租约认领项。暂停停止新认领；恢复保留未完成项。每个创建的判定存储唯一批次项 ID，因此提交重判后但在更新进度前崩溃的 API 会恢复同一判定而不是调度另一个。任务计数器在记录每个结果的同一事务中从终态项行重新计算。

`GET /api/admin/contests/{contestId}/judge-queue/status` 为 Resolver 与比赛结束操作保留遗留排空状态响应。Rust 查询按比赛限定，并在单一快照中报告 `PENDING` 提交、`JUDGING` 提交、pending/leased Outbox 行、失败 Outbox 行与数据库检查时间。仅当全部四个计数为零时 `drained` 为 true。与遗留分发器不同，Rust 将 `PUBLISHING` 租约计入 `outboxPending`，因为未 Publisher Confirm 的任务不能安全视为排空。超级管理员与分配的比赛管理员可以读取状态；其他比赛 ID 保持不可枚举。比赛管理页面显示这些计数，并在单次重判后刷新。

比赛管理员可以从 `GET /api/admin/contests/{contestId}/exports/submissions.csv` 导出提交元数据与活动结果为 UTF-8 CSV，或从 `GET /api/admin/contests/{contestId}/exports/submission-sources.zip` 导出所有源码文件加清单。两个端点都复用比赛管理范围检查并记录审计条目。CSV 文本加引号并中和电子表格公式前缀。ZIP 路径仅从数字 ID 与受限题目别名生成；写入条目前验证存储源码大小与 SHA-256。同步 ZIP 兼容端点拒绝超过 10,000 个文件或 128 MiB。更大导出将使用后续异步导出任务与过期对象工作流，而不是在 API 内存中保留无界归档。CSV 序列化、清单生成与 ZIP 压缩在 Tokio 阻塞池上运行，因此大的兼容导出不会阻塞异步请求与实时 worker。

`submission_export_tasks` 迁移是下一步的持久基础。它记录比赛范围、请求者、导出类型、租约处理状态、重试时机、输出对象与过期时间。成功输出必须同时携带桶与键；处理租约与过期输出索引允许多个 API 实例安全共享生成与清理。Rust 服务现在可以创建与加载限定任务、使用 `FOR UPDATE SKIP LOCKED` 原子认领可用或过期租约工作、仅完成 worker 自己的租约、调度有界失败消息重试，并标记成功输出过期。导出类型使用稳定 `METADATA_CSV`/`SOURCES_ZIP` 线上名称。

启用对象存储时，API 启动一个导出 runner，一次认领一个任务、在异步 worker 线程之外生成其产物、上传到隔离的 `exports/contests/{contestId}/` 键下，并发布 24 小时过期。失败使用封顶指数退避。完成受租约所有者保护；如果 worker 在失去租约后上传，上传的对象会被删除或持久化到标准清理队列。比赛管理员使用 `POST /api/admin/contests/{contestId}/exports/tasks` 入队工作、`GET /api/admin/contests/{contestId}/exports/tasks/{taskId}` 轮询，并在成功后使用匹配的 `/download` 端点。创建要求会话加两个 CSRF 方案；读取复用比赛范围授权。下载以稳定 `EXPORT_TASK_NOT_READY` 与 `EXPORT_TASK_EXPIRED` 冲突拒绝未完成与过期任务。

每个 runner 迭代还用 `FOR UPDATE SKIP LOCKED` 认领最多 100 个过期成功任务。到 `EXPIRED` 的转换与 `EXPORT_EXPIRED` 对象清理任务的插入发生在一个 PostgreSQL 语句中，因此崩溃不会让过期产物脱离持久清理工作流。重复清理插入通过现有桶 / 键唯一约束无害。

任务下载路径使用对象存储流式 API。S3 `GetObject` body 通过有界 reader chunk 直接桥接到 Axum 响应 body，而不是收集到一个 `Bytes` 分配中。内存与测试适配器保留安全单 chunk 默认值，而生产 S3 路径保持内存使用独立于产物大小。后台产物生成到操作系统临时目录中的抗碰撞文件。S3 通过 `ByteStream::from_path` 上传这些文件，因此完整 ZIP 或 CSV 不会复制到上传 `Bytes` 缓冲区。runner 在成功与失败上传后都移除临时文件，生成失败对部分文件做最佳努力移除。同步兼容端点继续使用其现有有界内存响应。源码 ZIP 任务现在顺序处理条目：下载一个对象、验证大小 / 哈希、在阻塞池上压缩到临时 ZIP，然后在读取下一个对象前释放。保留的清单状态只包含生成路径与 SHA-256 值。因此生成内存由一个提交源码加 ZIP 库缓冲区界定，而不是比赛中每个源码的总和。

Clarification 保留已评审的遗留路由，用于提问、列出队伍自己的问题、工作人员列表 / 详情、回复与关闭。队伍身份完全来自 `team_accounts` 与比赛名单。问题仅在比赛为 `RUNNING` 或 `PAUSED` 时接受，使用 advisory 事务锁强制每队每五分钟一个问题，并针对比赛题目分配验证 `GENERAL`/`PROBLEM` 形态。工作人员访问需要 Judge 或 `CONTEST_MANAGE` 权限加比赛范围（或超级管理员）。每个变更原子写入审计与 STAFF 加仅收件人的 TEAM 事件。`PUBLIC` 描述回复是否可以转换为公告；它不向其他队伍暴露原始问题。

公告支持立即与定时发布、已发布行的乐观版本编辑、重新调度、取消、置顶 / 取消置顶、不可逆撤回、公共列表、工作人员历史与通过已评审遗留路径加 `PATCH /api/announcements/{announcementId}` 显式编辑的详情读取。写入要求比赛管理范围，并且除撤回外还要求开放的 `RUNNING`/`PAUSED` 比赛。日程必须在未来且不晚于比赛结束时间。一秒后台调度器使用 `FOR UPDATE SKIP LOCKED` 认领最多 100 个到期行，使发布在 API 实例间恰好一次；比赛不再开放时它取消而非发布。日程变更发出 STAFF 事件，而发布与撤回发出只含 ID 与状态的 PUBLIC 失效事件。私有比赛读取要求显式队伍名单或工作人员分配，未发布行绝不返回给队伍账户。转换已回答的 `PUBLIC` Clarification 会锁定源行、插入恰好一条已发布公告、链接两条记录、写入两条审计，并在一个 PostgreSQL 事务中发出 Clarification 加公共公告事件。Vue 管理员页面从比赛详情页暴露创建、历史、重新调度、取消、编辑、置顶与撤回。

打印请求保留遗留的创建、自己列表、操作员列表、PDF 下载、重试、取消与拒绝路由。队伍身份使用 `team_accounts` 加比赛名单，请求仅在 `RUNNING`/`PAUSED` 期间接受。输入为 UTF-8 纯文本，上限 20 KiB，拒绝控制字符，使用保守的 100 列 / 50 行 A4 估算且上限五页，并在 advisory 事务锁下限制为每十分钟一次、每队每比赛二十次。PDF 生成无 shell 调用 `cupsfilter`，使用固定通用 PDF PPD 且禁用 JCL；结果在被归档到对象存储之前必须是有界纯 `%PDF-` 文档。只有那时数据库任务才提交为 `QUEUED`。队伍 PDF / 列表读取按所有者限定，而队列操作要求 `PRINTING_MANAGE` 或超级管理员权限。审计与 STAFF 加收件人 TEAM 事件是事务性的。投递 runner 通过 `lp` 提交归档 PDF、持久化 CUPS 任务 ID、通过 `lpstat` 监控活动与完成队列、支持取消，并在可恢复数据库租约下将请求从 `PRINTING` 推进到 `COMPLETED`。真实打印机硬件仍是部署验收要求，而不是实现缺口。
