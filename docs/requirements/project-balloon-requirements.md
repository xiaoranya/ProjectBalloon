# ProjectBalloon 产品需求与技术选型文档

## 1. 项目目标

ProjectBalloon 是一套支持内网部署的 XCPC/ICPC 竞赛平台，覆盖赛前准备、比赛进行、赛后滚榜、现场运营和数据归档等完整流程。

平台需要满足以下目标：

- 支持不少于 500 支队伍同时在线参赛。
- 支持每分钟 10,000 次请求量。
- 支持内网离线部署，不依赖公网服务。
- 支持 XCPC/ICPC 标准比赛流程。
- 支持封榜、Resolver 风格滚榜、打印、气球、颁奖、大屏和直播辅助。
- 支持自动判题、榜单实时更新、管理员后台和现场运维监控。
- 支持稳定、可恢复、可审计的正式比赛运行。

## 2. 使用场景

平台主要面向以下场景：

- 校内赛、省赛、区域赛、邀请赛等 XCPC 类赛事。
- 内网环境下的正式比赛。
- 需要现场大屏、直播、气球、打印、颁奖流程的线下赛事。
- 需要赛前导入题目和账号、赛中稳定判题、赛后导出结果和归档的比赛。

## 3. 核心规模指标

| 指标 | 目标 |
|---|---|
| 在线队伍数 | 至少 500 支 |
| 在线用户数 | 约 1,000-1,500 人 |
| 请求量 | 10,000 请求/分钟，约 167 RPS |
| 高峰提交量 | 按 100-300 次/分钟设计 |
| 判题并发 | 初始支持 30-60 个并发判题槽 |
| 部署环境 | 内网，离线可安装 |
| 赛制 | 第一版支持 XCPC/ICPC |

## 4. 总体架构

采用模块化单体业务后端加独立判题集群的架构。

```text
参赛者/管理员/大屏/直播端
  ↓
Nginx
  ↓
Web 前端 + API 后端
  ↓
PostgreSQL / Redis / RabbitMQ / RustFS
  ↓
Judge Scheduler
  ↓
Judge Worker 集群
  ↓
沙箱运行环境
```

确定技术选型：

| 模块 | 技术选型 |
|---|---|
| 前端 | Vue 3 + TypeScript + Vite + Element Plus |
| 后端 | Rust 2024 + Tokio + Axum |
| 数据库 | PostgreSQL 16 |
| 缓存 | Redis 7 |
| 消息队列 | RabbitMQ 3 |
| 对象存储 | RustFS |
| 判题沙箱 | rootless Podman + gVisor (`runsc`) + cgroups |
| 反向代理 | Nginx |
| 部署 | systemd 管理 API/Worker + Docker/Podman 判题沙箱 |
| 自动打印 | CUPS |
| 监控 | Prometheus + Grafana |
| 日志 | Loki + Promtail |
| 镜像分发 | Judge Runtime 离线镜像 tar 包 |
| 实时事件 | SSE |
| 构建与离线交付 | API/Worker 二进制 + 前端静态文件 + Judge Runtime 镜像 tar 包 + systemd/Nginx 配置 + 运维脚本 |

## 5. 内网部署要求

平台需要适配赛事现场内网环境。

部署原则：

- 不依赖公网 CDN。
- 不依赖公网对象存储。
- 不依赖公网容器镜像仓库。
- 不依赖公网包管理源。
- 不依赖第三方登录、短信、云监控等外部服务。
- 前端字体、图标、代码编辑器、MathJax/KaTeX、代码高亮资源全部本地化。
- Element Plus 组件库及其字体、图标资源随前端构建产物离线发布，不依赖任何 CDN。
- 提供 API/Worker 二进制、前端静态文件、Judge Runtime 镜像 tar 包、systemd unit、Nginx 配置和恢复脚本。
- PostgreSQL、Redis、RabbitMQ、RustFS、Nginx、CUPS 和观测组件由部署方自行提供和维护；发行包只提供应用二进制、静态文件及必要的配置模板，不负责创建或升级外部服务。
- API 和 Judge Worker 由 systemd 管理；Judge Worker 通过 Docker/Podman socket 启动隔离的判题容器。
- 服务安装和首轮配置通过发行包根目录的 `install.sh` 完成，运行状态通过 systemd 和健康接口检查。

标准部署形态：

| 机器 | 部署内容 |
|---|---|
| gateway-01 | Nginx |
| app-01 | 前端静态资源、后端 API |
| app-02 | 后端 API、Judge Scheduler |
| data-01 | PostgreSQL、Redis、RabbitMQ、RustFS |
| judge-01 ~ judge-N | Judge Worker |
| backup-01 | 数据备份、镜像备份、备用服务 |

正式比赛采用上述标准部署形态，判题服务与数据库部署在不同机器上。

二进制发行包目录结构：

```text
bin/
  project-balloon-api
  project-balloon-judge-worker
  bootstrap-admin

web/
systemd/
config/
nginx/
scripts/backup/
install.sh
PACKAGE-SHA256SUMS

docs/
  install.md
  ops.md
  disaster-recovery.md
```

判题 Runtime 镜像不随二进制发行包发布，单独提供
`project-balloon-<版本>-<平台>-judge-images.tar.gz` 归档，解压后通过
`install.sh --judge-images judge-images` 导入：

```text
judge-images/
  judge-runtime-*.tar
  SHA256SUMS
```

现场部署流程：

```text
拷贝二进制发行包和对应的 judge-images 归档到服务器
  ↓
部署 PostgreSQL、Redis、RabbitMQ、RustFS
  ↓
在 app/gateway 主机执行 install.sh --role api --no-start
  ↓
在 judge 主机部署 Docker/Podman，
  并执行 install.sh --role worker --no-start --judge-images ../judge-images
  ↓
填写 /etc/project-balloon/project-balloon.env
  ↓
再次执行对应 role 的 install.sh 启动服务
  ↓
执行 bootstrap-admin 创建首个管理员
  ↓
执行健康检查和备份验证
```

Judge Runtime 镜像名称必须使用固定版本，不使用 `latest` 标签，不依赖现场公网拉取镜像。基础服务的版本、升级和备份由部署方负责。

## 6. 功能范围

第一版平台定位为完整 XCPC 现场比赛系统，而不是单纯 Online Judge。

功能按优先级分阶段交付。P0 为第一版正式比赛必须具备的能力：

- 账号与队伍管理。
- 比赛管理。
- 题目管理。
- 提交与判题。
- ICPC/XCPC 实时榜单。
- 封榜。
- Resolver 风格滚榜。
- Clarification。
- 公告。
- CUPS 自动打印任意文本。
- 气球简单任务队列。
- 颁奖规则配置。
- 多大屏展示。
- 直播实时页面。
- 管理后台。
- 监控、日志、备份和恢复。

## 7. 账号与权限

### 7.1 用户类型

| 类型 | 说明 |
|---|---|
| 超级管理员 | 管理全局配置和所有比赛 |
| 工作人员 | 通过可组合权限执行赛事管理、答疑和现场工作 |
| 队伍 | 正式参赛用户 |
| 打星队 | 不同展示标识，但参与气球、滚榜、颁奖 |

工作人员权限直接分配到账号，可组合赛事管理、答疑、打印、气球、滚榜、颁奖、大屏和直播管理能力。赛事管理权限与可管理比赛范围分别控制“能做什么”和“能在哪些比赛做”。

### 7.2 权限要求

- 支持账号登录和密码重置。
- 支持批量导入队伍账号。
- 支持队伍学校、成员、座位号、分组、打星标识。
- 支持工作人员账号直接分配和组合权限。
- 支持登录日志和管理员操作审计。

## 8. 比赛管理

比赛管理功能包括：

- 创建比赛。
- 设置比赛名称、时间、赛制、可见性。
- 设置开始时间、结束时间、封榜时间。
- 支持比赛暂停和延长。
- 配置参赛队伍。
- 配置正式队、打星队和其他分组。
- 配置题目顺序和题目别名。
- 发布公告。
- 管理 clarification。
- 比赛结束后归档。

第一版赛制支持 ICPC/XCPC。

## 9. 题目管理

题目管理功能包括：

- 创建题目。
- 编辑题面。
- 使用 Markdown 作为题面编辑格式，PDF 作为题面附件格式。
- 管理样例输入输出。
- 上传题目附件。
- 上传测试数据。
- 配置时间限制和内存限制。
- 配置语言限制。
- 配置 Special Judge，优先级 P1。
- 配置题目气球颜色。
- 校验测试数据 hash。
- 赛前冻结题目配置。

测试数据存储在 RustFS 中，不直接存入数据库。

## 10. 提交系统

提交功能包括：

- 队伍选择题目和语言提交代码。
- P0 支持 C、C++、Java、Python；P1 支持 Go、Rust。
- 支持提交状态查询。
- 支持提交详情查看。
- 支持编译错误信息查看。
- 支持提交限流。
- 支持代码大小限制。
- 支持管理员重判单个或批量提交。
- 支持导出所有提交代码和结果。

提交状态包括：

```text
pending
judging
accepted
wrong_answer
time_limit_exceeded
memory_limit_exceeded
runtime_error
compile_error
output_limit_exceeded
system_error
cancelled
```

## 11. 判题系统

判题系统采用调度服务加 Worker 集群模式。

流程：

```text
用户提交
  ↓
写入 submissions
  ↓
发送判题任务到 RabbitMQ
  ↓
Judge Worker 拉取任务
  ↓
编译代码
  ↓
沙箱运行测试点
  ↓
写回结果
  ↓
触发榜单、气球、统计更新
```

判题系统要求：

- 支持多 Judge Worker。
- 支持 Worker 健康检查。
- 支持任务 ACK、失败重试和死信队列。
- 支持 CPU 时间限制。
- 支持内存限制。
- 支持进程数限制。
- 支持文件大小限制。
- 支持输出大小限制。
- 禁止运行时网络访问。
- 隔离文件系统。
- 清理临时目录。
- 记录编译日志和运行日志。
- 支持测试数据本地缓存。

判题 Worker 不应与主业务服务和数据库部署在同一资源池中。

## 12. 榜单系统

榜单系统第一版支持 ICPC/XCPC 规则。

功能包括：

- 实时榜单。
- 封榜。
- 管理员真实榜。
- 公开榜。
- First Blood。
- 错误提交罚时。
- 打星队展示。
- 分组榜单。
- 榜单导出。
- 榜单快照。

榜单不应每次请求从提交表全量计算。PostgreSQL 保存权威数据，Redis 保存实时榜单缓存。

封榜后：

- 公开榜隐藏封榜后的提交影响。
- 管理员可查看真实榜。
- Resolver 使用封榜快照和最终榜快照生成滚榜数据。

## 13. Resolver 风格滚榜

滚榜采用 ICPC Resolver 风格。

基本规则：

- 基于封榜时公开榜和最终真实榜生成滚榜数据。
- 从低排名队伍到高排名队伍逐队揭晓。
- 揭晓封榜后的 pending 提交。
- 根据揭晓结果动态更新排名。
- 打星队参与滚榜。

功能包括：

- 生成 Resolver 快照。
- 滚榜预览。
- 正式滚榜。
- 主持人控制台。
- 下一步、暂停、继续、回退。
- 自动播放。
- 滚榜大屏页面。
- 滚榜直播页面。
- 多分组滚榜，优先级 P1。
- 滚榜状态持久化。

滚榜数据必须快照化，避免正式滚榜时因重判或数据变更导致结果不一致。

规划数据：

```text
resolver_runs
resolver_snapshots
resolver_events
resolver_team_states
resolver_current_state
```

## 14. Clarification 与公告

Clarification 功能包括：

- 队伍向裁判提问。
- 问题可关联题目或 general。
- 裁判私有回复。
- 裁判公开回复。
- 可将回复转为公告。
- 队伍收到新回复提醒。
- 支持提问限流。

公告功能包括：

- 发布比赛公告。
- 置顶公告。
- 定时公告，优先级 P1。
- 撤回公告。
- 大屏公告展示。
- 直播公告条展示。

## 15. 自动打印模块

打印模块使用 CUPS 自动打印，内容为队伍粘贴的任意纯文本。

### 15.1 功能范围

- 队伍粘贴任意文本发起打印请求。
- 后端校验文本长度、页数和频率限制。
- 自动生成 PDF 打印文件。
- 通过 CUPS 投递到内网打印机。
- 打印工作台查看队列。
- 支持失败重试。
- 支持取消和拒绝。
- 支持手动下载兜底打印。
- 支持打印审计。

### 15.2 打印限制

| 限制项 | 默认值 |
|---|---|
| 单次页数 | 最多 5 页 |
| 文本大小 | 最多 20KB |
| 频率 | 每队每 10 分钟最多 1 次 |
| 全场次数 | 每队最多 20 次 |
| 内容类型 | 纯文本 |

### 15.3 打印状态

```text
requested
queued
printing
completed
failed
cancelled
rejected
```

### 15.4 打印审计

需要记录：

- 比赛 ID。
- 队伍 ID。
- 文本内容。
- 内容 hash。
- 页数。
- 打印机 ID。
- CUPS job ID。
- 请求时间。
- 完成时间。
- 失败原因。
- 操作员。
- 请求 IP。

## 16. 气球系统

气球系统 P0 实现简单任务队列，复杂调度归入 P2。

### 16.1 规则

- 封榜后不再生成气球。
- 打星队参与气球。
- 每队每题首次 AC 生成一次气球任务。
- 题目必须配置气球颜色。
- First Blood 任务需要特殊标记。

生成条件：

```text
提交时间 < 封榜时间
并且结果为 AC
并且是该队该题首次 AC
并且题目配置了气球颜色
```

### 16.2 功能

- 配置每题气球颜色。
- 自动生成气球任务。
- 显示队伍、题目、颜色、座位号。
- 标记 First Blood。
- 工作人员手动更新状态。
- 支持备注。
- 支持取消任务。
- 支持气球统计。
- 支持大屏展示。

### 16.3 状态

```text
pending
preparing
delivering
delivered
cancelled
```

## 17. 颁奖系统

颁奖系统支持按比例和固定数量生成获奖名单。

打星队默认参与颁奖，但每个奖项可单独配置是否包含打星队。

功能包括：

- 奖项配置。
- 按比例生成获奖名单。
- 按固定数量生成获奖名单。
- 按排名区间生成获奖名单。
- 按分组榜单生成获奖名单。
- First Blood 奖项，优先级 P1。
- 手动调整获奖名单。
- 冲突检测和重复提示。
- 冻结获奖名单。
- 导出 Excel/CSV。
- 导出证书数据。
- 颁奖大屏。
- 颁奖控制台，优先级 P1。

示例规则：

```text
冠军：第 1 名
金奖：前 10%
银奖：接下来 20%
铜奖：接下来 30%
最佳女队：女队榜第 1
首杀奖：每题 First Blood
```

## 18. 大屏系统

大屏系统需要支持多台大屏。

### 18.1 大屏页面

| 页面 | 说明 |
|---|---|
| 比赛总览 | 队伍数、提交数、AC 数、剩余时间 |
| 实时榜单 | 展示前 N 名或分页轮播 |
| First Blood | 首杀提示 |
| 气球统计 | 气球颜色、数量、待送情况 |
| 公告 | 重要公告展示 |
| 封榜倒计时 | 封榜前提示 |
| Resolver | 赛后滚榜 |
| 颁奖 | 展示奖项和获奖队伍 |
| 数据统计 | 提交趋势、通过率、语言分布 |

### 18.2 多屏控制

功能包括：

- 大屏实例注册。
- 大屏心跳。
- 控制台查看在线大屏。
- 远程切换页面。
- 多屏分组。
- 同步播放。
- 锁定到指定页面。
- 断线重连恢复。

规划数据：

```text
screen_instances
screen_groups
screen_playlists
screen_commands
screen_heartbeats
```

## 19. 直播辅助

直播实时展示公开赛事数据。

直播页面应适合 OBS 浏览器源采集。

功能包括：

- 实时榜单直播页面。
- Resolver 直播页面。
- 比赛总览直播页面。
- First Blood 弹窗。
- 气球统计页面。
- 颁奖页面。
- 底部公告条。
- 直播 token 鉴权。
- 隐藏账号、IP、提交代码、内部备注等敏感信息。

示例路径：

```text
/live/scoreboard
/live/resolver
/live/overview
/live/first-blood
/live/balloons
/live/awards
/live/ticker
```

直播和大屏共用 SSE 实时事件通道。

## 20. 管理后台

管理后台包括：

- 仪表盘。
- 队伍管理。
- 比赛管理。
- 题目管理。
- 提交管理。
- 判题管理。
- 榜单管理。
- Resolver 管理。
- Clarification 管理。
- 公告管理。
- 打印管理。
- 气球管理。
- 颁奖管理。
- 大屏管理。
- 直播管理。
- 权限管理。
- 审计日志。
- 数据导出。
- 系统健康检查。

## 21. 现场运维

现场运维能力包括：

- 健康检查页。
- 服务状态检查。
- PostgreSQL 状态。
- Redis 状态。
- RabbitMQ 队列积压。
- RustFS 状态。
- Judge Worker 在线数量。
- 判题队列长度。
- API QPS 和延迟。
- HTTP 5xx 错误率。
- 磁盘空间。
- CPU 和内存。
- 打印机状态。
- 大屏在线状态。

需要提供：

- 一键检查脚本。
- 一键备份脚本。
- 数据恢复流程。
- 服务重启脚本。
- Judge Runtime 离线镜像 tar 包。
- 配置备份。
- 压测脚本。

## 22. 安全要求

### 22.1 判题安全

- 禁止网络访问。
- 限制 CPU、内存、进程、文件大小和输出大小。
- 隔离文件系统。
- 不挂载 Docker socket。
- 不使用 privileged 容器。
- 每次判题清理临时目录。
- 测试数据只读挂载。

### 22.2 Web 安全

- 防 SQL 注入。
- 防 XSS。
- 防 CSRF。
- Markdown 内容过滤。
- 文件上传类型和大小限制。
- 登录限流。
- 提交限流。
- 打印限流。
- 管理员操作审计。

### 22.3 数据安全

- 赛前完整备份。
- 比赛中定时备份。
- 提交代码持久化保存。
- 测试数据 hash 校验。
- 对象存储备份。
- 关键操作留痕。

## 23. 数据存储规划

核心数据表规划：

```text
users
teams
team_members
contests
contest_teams
problems
contest_problems
submissions
judgements
runs
scoreboard_snapshots
clarifications
announcements
print_requests
balloon_tasks
balloon_colors
resolver_runs
resolver_snapshots
resolver_events
award_categories
award_rules
award_recipients
screen_instances
screen_groups
screen_commands
broadcast_tokens
audit_logs
```

文件类数据存储在 RustFS：

- 题目附件。
- 测试数据。
- 提交代码归档。
- 编译日志。
- 判题日志。
- 导出文件。
- 打印 PDF。

## 24. 优先级规划

### 24.1 P0 必须完成

- 队伍账号、登录、权限。
- 比赛创建、时间控制、封榜。
- 题目管理和测试数据管理。
- 代码提交。
- 判题队列和 Judge Worker。
- C/C++/Java/Python 支持。
- ICPC/XCPC 榜单。
- First Blood。
- Clarification。
- 公告。
- 管理后台。
- 榜单导出。
- 提交导出。
- Resolver 风格滚榜。
- 滚榜控制台。
- 滚榜大屏和直播页面。
- CUPS 自动打印任意文本。
- 打印工作台和失败重试。
- 气球颜色配置。
- 封榜前自动气球任务。
- 气球工作台。
- 打星队参与气球、滚榜、颁奖。
- 颁奖规则：比例和固定数量。
- 获奖名单导出。
- 实时大屏页面。
- 多大屏基础控制。
- 直播安全页面。
- 监控、健康检查、备份恢复。

### 24.2 P1 增强能力

- CUPS 打印机状态同步增强。
- 大屏播放列表。
- 多大屏分组同步。
- First Blood 动画。
- 颁奖控制台。
- 证书数据导出。
- 主持人脚本。
- Resolver 预演模式。
- 气球统计增强。
- Worker 测试数据本地缓存。
- 比赛克隆。

### 24.3 P2 扩展能力

- OI/IOI 赛制。
- 交互题。
- Output-only 题。
- 代码相似度检测。
- 高级直播包装。
- 自定义大屏模板。
- 多会场同步。
- 公开题库和训练系统。
- 多租户能力。

## 25. 赛前流程

赛前应支持以下流程：

```text
创建比赛
  ↓
导入队伍
  ↓
配置题目
  ↓
上传测试数据
  ↓
配置语言环境
  ↓
配置气球颜色
  ↓
配置打印机
  ↓
配置大屏和直播页面
  ↓
导入或生成账号
  ↓
试机赛
  ↓
压测
  ↓
赛前快照
  ↓
冻结配置
```

## 26. 比赛中流程

比赛中主要流程：

```text
队伍登录
  ↓
查看题面
  ↓
提交代码
  ↓
进入判题队列
  ↓
更新提交结果
  ↓
更新榜单
  ↓
封榜前 AC 生成气球任务
  ↓
队伍可发起文本打印
  ↓
裁判处理 clarification
  ↓
管理员监控系统状态
```

封榜后：

- 公开榜冻结显示。
- 管理员真实榜继续更新。
- 不再生成气球。
- 提交仍正常判题。

## 27. 比赛后流程

比赛结束后流程：

```text
停止提交
  ↓
等待判题队列清空
  ↓
必要时重判
  ↓
生成最终榜
  ↓
生成 Resolver 快照
  ↓
预览滚榜
  ↓
正式滚榜
  ↓
生成获奖名单
  ↓
颁奖大屏展示
  ↓
导出成绩、提交和日志
  ↓
归档比赛
```

## 28. 压测与演练

正式比赛前必须进行压测和故障演练。

压测内容：

- 1,500 用户登录。
- 500 队同时查看题面。
- 500 队同时刷新榜单。
- 100-300 次/分钟提交。
- Judge Worker 队列积压测试。
- Resolver 滚榜页面压力测试。
- 多大屏同时连接测试。
- 打印任务压力测试。

故障演练：

- API 服务重启。
- Judge Worker 掉线。
- RabbitMQ 重启。
- Redis 清空。
- PostgreSQL 备份恢复。
- RustFS 读取失败。
- 打印机离线。
- 大屏断线重连。

## 29. 版本边界

第一版产品目标：

```text
ProjectBalloon 支持内网部署，覆盖赛前配置、队伍管理、题目管理、提交判题、实时榜单、封榜、Resolver 滚榜、自动打印、气球配送、颁奖名单生成、多大屏展示和直播辅助。
```

第一版的重点不是功能花哨，而是：

- 正式比赛可用。
- 判题稳定。
- 榜单可信。
- 滚榜不出错。
- 打印和气球能服务现场。
- 大屏和直播展示安全。
- 内网部署可恢复。
- 数据可导出和归档。
