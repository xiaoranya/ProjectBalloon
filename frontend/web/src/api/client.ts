import type { ApiErrorBody } from './types';
import { currentLocale, translate } from '../i18n';

interface CsrfResponse {
  headerName: string;
  parameterName: string;
  token: string;
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fieldErrors: ApiErrorBody['fieldErrors'];

  constructor(
    status: number,
    code: string,
    message: string,
    fieldErrors?: ApiErrorBody['fieldErrors'],
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.fieldErrors = fieldErrors;
  }
}

let csrf: CsrfResponse | null = null;
let unauthorizedHandler: (() => void) | null = null;

export function setUnauthorizedHandler(handler: () => void) {
  unauthorizedHandler = handler;
}

export function clearCsrfToken() {
  csrf = null;
}

async function getCsrfToken(): Promise<CsrfResponse> {
  if (csrf) {
    return csrf;
  }
  const response = await fetch('/api/auth/csrf', {
    credentials: 'same-origin',
    headers: { Accept: 'application/json' },
  });
  if (!response.ok) {
    if (response.status === 401) {
      unauthorizedHandler?.();
    }
    throw await createApiError(response);
  }
  csrf = (await response.json()) as CsrfResponse;
  return csrf;
}

async function createApiError(response: Response): Promise<ApiError> {
  const contentType = response.headers.get('content-type') ?? '';
  let body: ApiErrorBody;
  if (contentType.includes('json')) {
    try {
      body = (await response.json()) as ApiErrorBody;
    } catch {
      body = {};
    }
  } else {
    const text = await response.text();
    body = { message: text };
  }
  const code = body.code ?? `HTTP_${response.status}`;
  const message = body.message ?? defaultMessage(response.status);
  return new ApiError(response.status, code, message, body.fieldErrors);
}

function defaultMessage(status: number): string {
  const messages: Record<number, string> = {
    400: '请求内容不正确',
    401: '用户名或密码错误，或登录已失效',
    403: '没有权限执行此操作',
    404: '请求的内容不存在',
    409: '当前状态不允许执行此操作',
    429: '操作过于频繁，请稍后重试',
    500: '服务器处理请求时发生错误',
  };
  return translate(messages[status] ?? '请求失败（{status}）', { status });
}

export interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: BodyInit | object | null;
  responseType?: 'json' | 'text' | 'blob';
  acceptedStatuses?: number[];
  suppressUnauthorizedHandler?: boolean;
}

export async function apiRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const {
    acceptedStatuses = [],
    responseType,
    suppressUnauthorizedHandler = false,
    ...fetchOptions
  } = options;
  const method = (fetchOptions.method ?? 'GET').toUpperCase();
  const headers = new Headers(fetchOptions.headers);
  if (!headers.has('Accept')) {
    headers.set(
      'Accept',
      responseType === 'text'
        ? 'text/plain'
        : responseType === 'blob'
          ? 'application/octet-stream'
          : 'application/json',
    );
  }

  let body = fetchOptions.body as BodyInit | null | undefined;
  if (body && !(body instanceof FormData) && !(body instanceof Blob) && typeof body !== 'string') {
    headers.set('Content-Type', 'application/json');
    body = JSON.stringify(body);
  }

  if (!['GET', 'HEAD', 'OPTIONS'].includes(method)) {
    const csrfToken = await getCsrfToken();
    headers.set(csrfToken.headerName, csrfToken.token);
  }

  const response = await fetch(path, {
    ...fetchOptions,
    method,
    body,
    headers,
    credentials: 'same-origin',
  });

  if (!response.ok && !acceptedStatuses.includes(response.status)) {
    if (response.status === 401 && !suppressUnauthorizedHandler) {
      unauthorizedHandler?.();
    }
    throw await createApiError(response);
  }

  if (response.status === 204) {
    return undefined as T;
  }
  if (responseType === 'text') {
    return (await response.text()) as T;
  }
  if (responseType === 'blob') {
    return (await response.blob()) as T;
  }
  return (await response.json()) as T;
}

const businessMessages: Record<string, string> = {
  CONTEST_NOT_RUNNING: '比赛当前不接受提交',
  TEAM_NOT_FOUND: '当前账号没有关联参赛队',
  TEAM_NOT_IN_CONTEST: '当前队伍未加入这场比赛',
  PROBLEM_NOT_FOUND: '题目不存在或不可访问',
  PROBLEM_NOT_IN_CONTEST: '该题目不属于当前比赛',
  LANGUAGE_NOT_ALLOWED: '该题目不允许使用所选语言',
  PROBLEM_LANGUAGES_INVALID: '题目语言配置异常，请联系管理员',
  SOURCE_REQUIRED: '请选择源码文件',
  SOURCE_TOO_LARGE: '源码文件不能超过 64 KiB',
  SOURCE_EXTENSION_MISMATCH: '源码扩展名与所选语言不匹配',
  INVALID_METADATA_JSON: '提交信息格式不正确',
  RATE_LIMIT_EXCEEDED: '操作过于频繁，请稍后重试',
  SUBMISSION_NOT_FOUND: '提交记录不存在或不可访问',
  CONTEST_NOT_FOUND: '比赛不存在或不可访问',
  CONTEST_REQUIRED: '赛事管理员导入队伍时必须选择已授权比赛',
  CONTEST_MANAGER_NOT_FOUND: '赛事管理员账号不存在',
  USERNAME_TAKEN: '用户名已被使用',
  STAFF_ACCOUNT_NOT_FOUND: '工作人员账号不存在',
  STAFF_TYPE_INVALID: '工作人员账号不能使用参赛队类型',
  DISPLAY_NAME_REQUIRED: '请输入显示名称',
  SELF_ACCESS_CHANGE_FORBIDDEN: '不能停用或移除自己的超级管理员权限',
  LAST_SUPER_ADMIN: '系统必须至少保留一个已启用的超级管理员',
  CURRENT_PASSWORD_INVALID: '当前密码不正确',
  PASSWORD_UNCHANGED: '新密码不能与当前密码相同',
  PASSWORD_RESET_REQUIRED: '请先修改初始密码',
  ACCOUNT_DISABLED: '账号已停用',
  ACCOUNT_ACCESS_CHANGED: '账号权限已变更，请重新登录',
  CONTEST_NAME_TAKEN: '比赛名称已存在',
  CONTEST_SCHEDULE_LOCKED: '比赛开始后不能修改赛程时间',
  CONTEST_ARCHIVE_BUSY: '比赛仍有运行中的任务，处理完毕后才能归档',
  CONTEST_ARCHIVED_READ_ONLY: '比赛已归档，数据只读',
  CONTEST_EXTENSION_STATUS_INVALID: '仅进行中或暂停状态的比赛可以延时',
  CONTEST_END_TIME_NOT_SET: '比赛尚未设置结束时间',
  CONTEST_EXTENSION_STALE: '比赛结束时间已被其他管理员修改，请刷新后重试',
  CONTEST_EXTENSION_NOT_LATER: '新的结束时间必须晚于当前结束时间',
  CONTEST_UPDATE_STALE: '比赛已被其他管理员修改，请刷新后重试',
  INVALID_CONTEST_TRANSITION: '当前比赛状态不允许执行该操作',
  CONTEST_TRANSITION_INVALID: '当前比赛状态不允许执行该操作',
  JUDGEMENT_VERSION_STALE: '该提交的有效判定已被其他管理员更新，请刷新后重试',
  TEAM_ALREADY_ASSIGNED: '该队伍已分配到本场比赛',
  TEAM_IMPORT_SIZE_INVALID: '每个后台批次必须包含 1 至 100 支队伍',
  CONTEST_ROSTER_CLOSED: '比赛已结束或归档，不能再导入队伍',
  TEAM_NAME_TAKEN: '队伍名称已被使用',
  TEAM_IMPORT_DUPLICATE_NAME: '当前后台批次包含重复队伍名称',
  TEAM_IMPORT_DUPLICATE_USERNAME: '当前后台批次包含重复用户名',
  TEAM_IMPORT_IDEMPOTENCY_CONFLICT: '该导入标识已用于另一批数据，请重新导入',
  TEAM_IMPORT_IN_PROGRESS: '该批导入仍在处理中，请稍后重试',
  TEAM_NAME_REQUIRED: '队伍名称不能为空',
  TEAM_USERNAME_INVALID: '用户名需为 3 至 64 位字母、数字、点、下划线或连字符',
  TEAM_PASSWORD_INVALID: '初始密码长度需为 8 至 128 位',
  TEAM_IMPORT_CONTEST_CLOSED: '已结束或归档的比赛不能导入队伍',
  DUPLICATE_TEAM_NAME: '队伍名称重复',
  DUPLICATE_USERNAME: '用户名重复',
  PROBLEM_ALREADY_ASSIGNED: '该题目已分配到本场比赛',
  PROBLEM_SLUG_REQUIRED: '请输入题目标识',
  PROBLEM_SLUG_INVALID: '题目标识仅允许小写字母、数字和连字符',
  PROBLEM_TITLE_REQUIRED: '请输入题目标题',
  PROBLEM_SLUG_TAKEN: '题目标识已被使用',
  PROBLEM_LIMIT_INVALID: '题目资源限制超出允许范围',
  PROBLEM_ASSIGNED_TO_CONTEST: '题目已分配到比赛，请先从比赛中移除',
  PROBLEM_VERSION_STALE: '题目已被其他管理员修改，请刷新后重试',
  INVALID_LANG_CODE: '题面语言代码格式不正确',
  STATEMENT_BODY_REQUIRED: '请输入题面内容',
  INVALID_ATTACHMENT_KIND: '附件类型不正确',
  ATTACHMENT_TOO_LARGE: '附件不能超过 20 MiB',
  ATTACHMENT_NOT_FOUND: '附件不存在或已被删除',
  TESTDATA_TOO_LARGE: '测试数据不能超过 256 MiB',
  INVALID_TESTDATA_FILE: '测试数据必须是有效的 ZIP 文件',
  TESTDATA_NOT_FOUND: '尚未上传测试数据',
  TESTDATA_VERSION_STALE: '测试数据已被其他管理员更新，请重试',
  TESTDATA_VERSION_NOT_FOUND: '测试数据历史版本不存在',
  TESTDATA_INTEGRITY_MISMATCH: '测试数据文件完整性校验失败，请联系管理员',
  TESTDATA_REFERENCE_INCONSISTENT: '测试数据引用不一致，请联系管理员',
  OBJECT_STORAGE_UNAVAILABLE: '对象存储暂不可用，请稍后重试',
  CONTEST_PROBLEM_CONFIG_FROZEN: '题目所属比赛已不在草稿状态，不能修改文件',
  ANNOUNCEMENT_NOT_FOUND: '公告不存在或不可访问',
  ANNOUNCEMENT_TITLE_EMPTY: '请输入公告标题',
  ANNOUNCEMENT_BODY_EMPTY: '请输入公告内容',
  ANNOUNCEMENT_CONTEST_NOT_OPEN: '仅进行中或暂停状态的比赛可以发布公告',
  ANNOUNCEMENT_ALREADY_WITHDRAWN: '该公告已撤回',
  ANNOUNCEMENT_NOT_PUBLISHED: '只有已发布公告可以执行该操作',
  ANNOUNCEMENT_NOT_SCHEDULED: '该公告不是待发布状态',
  ANNOUNCEMENT_VERSION_STALE: '公告已被其他管理员修改，请刷新后重试',
  ANNOUNCEMENT_SCHEDULE_REQUIRED: '请选择计划发布时间',
  ANNOUNCEMENT_SCHEDULE_NOT_FUTURE: '定时发布时间必须晚于当前时间',
  ANNOUNCEMENT_SCHEDULE_AFTER_CONTEST: '定时发布时间不能晚于比赛结束时间',
  CLARIFICATION_CONTEST_NOT_OPEN: '仅进行中或暂停状态的比赛可以提问',
  CLARIFICATION_RATE_LIMITED: '每队每 5 分钟最多提问一次，请稍后再试',
  CLARIFICATION_NOT_FOUND: '答疑不存在或不可访问',
  CLARIFICATION_CLOSED: '该答疑已关闭，不能继续操作',
  CLARIFICATION_ALREADY_CONVERTED: '该答疑已转为公告',
  CLARIFICATION_REPLY_NOT_PUBLIC: '只有已公开回复的答疑可以转为公告',
  CLARIFICATION_NOT_ANSWERED: '该答疑尚无可用于公告的回复',
  TEAM_ACCOUNT_REQUIRED: '此操作仅限参赛队账号',
  PRINTING_CONTENT_EMPTY: '请输入需要打印的纯文本内容',
  PRINTING_CONTENT_TOO_LARGE: '打印内容不能超过 20 KiB',
  PRINTING_TOO_MANY_PAGES: '打印内容不能超过 5 页',
  PRINTING_RATE_LIMITED: '每队每 10 分钟最多发起 1 次打印',
  PRINTING_QUOTA_EXCEEDED: '本队本场比赛的 20 次打印额度已用完',
  PRINTING_CONTEST_NOT_OPEN: '仅进行中或暂停状态的比赛可以发起打印',
  PRINTING_NOT_RETRYABLE: '只有排队中或失败的任务可以重试',
  PRINTING_NOT_CANCELLABLE: '该任务当前不能取消',
  PRINTING_NOT_REJECTABLE: '该任务当前不能拒绝',
  PRINTING_REJECT_REASON_EMPTY: '请输入拒绝原因',
  PRINTING_PDF_NOT_READY: '打印 PDF 尚未生成',
  PRINTING_DELIVERY_IN_PROGRESS: '打印任务正在投递，请稍后重试',
  PRINTING_STATE_CHANGED: '打印任务状态已变化，请刷新后重试',
  PRINTING_PERMISSION_REQUIRED: '当前账号没有打印管理权限',
  PDF_RENDERER_UNAVAILABLE: 'PDF 生成服务暂不可用，请稍后重试',
  PDF_RENDERER_TIMEOUT: 'PDF 生成超时，请稍后重试',
  PDF_RENDER_FAILED: 'PDF 生成失败，请稍后重试',
  PRINT_REQUEST_NOT_FOUND: '打印任务不存在或不可访问',
  BALLOON_TASK_NOT_FOUND: '气球任务不存在或不可访问',
  BALLOON_INVALID_STATUS: '气球任务状态不正确',
  BALLOON_NOT_CLAIMABLE: '只有待领取任务可以领取',
  BALLOON_ILLEGAL_TRANSITION: '当前气球任务状态不允许此操作',
  BALLOON_NOT_REOPENABLE: '只有已取消任务可以重新打开',
  BALLOON_VERSION_STALE: '气球任务已被其他工作人员更新，请刷新后重试',
  BALLOON_CLAIMED_BY_OTHER: '该气球任务由其他工作人员领取，不能确认送达',
  BALLOON_STATE_CHANGED: '气球任务状态已变化，请刷新后重试',
  AWARD_CATEGORY_CODE_TAKEN: '当前比赛中已存在相同奖项代码',
  AWARD_CATEGORY_NOT_FOUND: '奖项类别不存在或不可访问',
  AWARD_RECIPIENT_NOT_FOUND: '获奖记录不存在或不可访问',
  AWARD_RECIPIENT_EXISTS: '该队伍已在此奖项名单中',
  AWARD_FROZEN: '最终名单已锁定，请先解锁后再修改',
  AWARD_CATEGORY_REQUIRED: '请先配置至少一个奖项类别',
  AWARD_ALREADY_FROZEN: '最终名单已经锁定',
  AWARD_CERTIFICATE_EXPORT_NOT_FROZEN: '锁定最终名单后才能导出证书数据',
  AWARD_HOST_SCRIPT_NOT_READY: '锁定最终名单后才能生成主持人脚本',
  AWARD_HOST_SCRIPT_CATEGORY_INVALID: '主持人脚本包含无效的奖项类别',
  AWARD_HOST_SCRIPT_CATEGORY_DUPLICATE: '主持人脚本中的奖项类别重复',
  AWARD_HOST_SCRIPT_VERSION_CONFLICT: '主持人脚本已被其他操作员修改，请刷新后重试',
  AWARD_SET_NOT_FOUND: '尚未生成奖项名单',
  AWARD_RESOLVER_NOT_FINAL: '奖项名单需要已完成的正式 Resolver 运行',
  AWARD_SET_FROZEN: '奖项名单已锁定，不能执行此操作',
  AWARD_SET_NOT_MUTABLE: '需要先生成并保持奖项名单为草稿状态',
  AWARD_VERSION_STALE: '奖项名单已被其他操作员更新，请刷新后重试',
  AWARD_CATEGORY_VERSION_STALE: '奖项类别已被其他操作员更新，请刷新后重试',
  AWARD_CATEGORY_CONFLICT: '奖项代码或显示顺序已被占用',
  AWARD_RECIPIENT_GENERATED: '自动生成的获奖记录需通过重新生成名单来调整',
  AWARD_PERMISSION_REQUIRED: '当前账号没有颁奖管理权限',
  PRESENTATION_NOT_PUBLISHED: '展示页面尚未发布',
  PRESENTATION_ACCENT_INVALID: '强调色必须是六位十六进制颜色',
  PRESENTATION_ROW_LIMIT_INVALID: '榜单队伍数量必须在 5 到 30 之间',
  PRESENTATION_INTERVAL_INVALID: '公告切换间隔必须在 5 到 60 秒之间',
  REJUDGE_RESOLVER_ACTIVE: '该比赛正在进行正式 Resolver，不能重判',
  RESOLVER_RUN_NOT_FOUND: 'Resolver 运行不存在或不可访问',
  RESOLVER_SOURCE_SNAPSHOT_NOT_FOUND: '缺少 Resolver 所需的封榜快照或最终榜快照',
  RESOLVER_CONTEST_NOT_FINAL: '正式 Resolver 只能为已结束或已归档比赛创建',
  RESOLVER_OFFICIAL_EXISTS: '该比赛已经存在正式 Resolver 运行',
  RESOLVER_VERSION_STALE: 'Resolver 已被其他操作员更新，请刷新后重试',
  RESOLVER_STATE_CHANGED: '当前状态不能执行该 Resolver 操作，请刷新后重试',
  RESOLVER_AUTO_PLAY_INVALID: '自动播放只能在运行中且仍有待揭晓步骤时启用',
  RESOLVER_PERMISSION_REQUIRED: '当前账号没有 Resolver 管理权限',
  BALLOON_PERMISSION_REQUIRED: '当前账号没有气球管理权限',
  SCREEN_PERMISSION_REQUIRED: '当前账号没有大屏管理权限',
  LIVE_PERMISSION_REQUIRED: '当前账号没有直播管理权限',
  PRESENTATION_PERMISSION_REQUIRED: '当前账号没有展示管理权限',
  BATCH_REJUDGE_COUNT_CHANGED: '符合条件的提交集合已变化，请重新预览',
  IDEMPOTENCY_KEY_REUSED: '该幂等键已用于其他批量重判请求',
  BATCH_REJUDGE_NOT_FOUND: '批量重判任务不存在或不可访问',
  BALLOON_POLICY_ADMIN_REQUIRED: '此操作需要赛事管理员权限',
  BATCH_REJUDGE_ACTOR_DISABLED: '任务创建者账号已停用',
  BROADCAST_TOKEN_INVALID: '广播令牌无效或已过期',
  BROADCAST_TOKEN_NOT_FOUND: '广播令牌不存在',
  CONTEST_ARCHIVED: '已归档比赛的提交不能重判',
  CONTEST_HAS_ACTIVE_TEAMS: '比赛已分配队伍，删除前请先移除队伍',
  CONTEST_PROBLEM_ALIAS_TAKEN: '该题目别名在本场比赛中已被使用',
  CONTEST_PROBLEM_HAS_SUBMISSIONS: '该题目已有提交记录，不能从比赛中移除',
  CONTEST_PROBLEM_NOT_FOUND: '该题目未分配到当前比赛',
  CONTEST_PROBLEM_ORDER_TAKEN: '该显示顺序在本场比赛中已被使用',
  CONTEST_PROBLEM_REORDER_SET_MISMATCH: '排序请求必须包含且仅包含每道已分配题目一次',
  CONTEST_SCHEDULE_REQUIRED: '比赛尚未配置赛程时间',
  CONTEST_SCORING_CONFIG_FROZEN: '评分配置仅在草稿状态下可以修改',
  CONTEST_TEAM_ALREADY_ASSIGNED: '该队伍已分配到本场比赛',
  CONTEST_TEAM_NOT_FOUND: '该队伍未加入当前比赛',
  CSRF_INVALID: '安全令牌缺失或无效，请刷新页面后重试',
  DAILY_FEATURE_DISABLED: '竞赛模式下该功能已停用',
  EDITORIAL_LOCKED: '题解尚未满足解锁条件',
  EDITORIAL_NOT_FOUND: '题解不存在或不可访问',
  ENROLLMENT_NOT_FOUND: '报名记录不存在或不可访问',
  EXPORT_TASK_EXPIRED: '导出任务已过期，请重新发起导出',
  EXPORT_TASK_NOT_FOUND: '导出任务不存在或不可访问',
  INTERNAL_ERROR: '服务器处理请求时发生错误',
  INVALID_CREDENTIALS: '用户名或密码错误',
  JUDGEMENT_NOT_FINAL: '只有已完成的判定才能重判',
  NOT_AUTHENTICATED: '登录状态已失效，请重新登录',
  NO_ACTIVE_CONTEST: '当前没有进行中的比赛',
  PAIRING_CODE_INVALID: '配对码无效或已过期',
  PRACTICE_CONCURRENCY_LIMIT: '待评测的练习提交过多，请稍后再试',
  PRACTICE_DAILY_QUOTA_EXCEEDED: '今日练习提交额度已用完',
  PRACTICE_SUBMISSION_NOT_ALLOWED: '该题目未公开或没有可用的测试数据，不能提交',
  PRACTICE_SUBMISSION_RATE_LIMITED: '练习提交过于频繁，请稍后再试',
  PROBLEM_CONFIG_FROZEN: '题目已被冻结中或已开始的比赛使用，不能修改配置',
  SCREEN_GROUP_NAME_TAKEN: '大屏分组名称已存在',
  SCREEN_GROUP_NOT_FOUND: '大屏分组不存在或不可访问',
  SCREEN_GROUP_NOT_PAUSED: '该大屏分组不在暂停状态',
  SCREEN_GROUP_NOT_PLAYING: '该大屏分组不在播放状态',
  SCREEN_GROUP_VERSION_CONFLICT: '大屏分组已被其他人修改，请刷新后重试',
  SCREEN_INSTANCE_ALREADY_GROUPED: '该大屏已属于其他分组',
  SCREEN_INSTANCE_NOT_FOUND: '大屏不存在或不可访问',
  SCREEN_PLAYLIST_IN_USE: '播放列表正在被大屏使用，不能删除',
  SCREEN_PLAYLIST_NAME_TAKEN: '播放列表名称已存在',
  SCREEN_PLAYLIST_VERSION_CONFLICT: '播放列表已被其他人修改，请刷新后重试',
  SCREEN_PRESENTATION_NOT_PUBLISHED: '展示内容尚未发布',
  SCREEN_REGISTRATION_RATE_LIMITED: '大屏注册过于频繁，请稍后再试',
  SCREEN_TOKEN_INVALID: '大屏令牌无效或已过期',
  SCREEN_VERSION_REQUIRED: '请求缺少乐观锁版本号，请刷新后重试',
  STATEMENT_NOT_FOUND: '题面不存在或不可访问',
  SUBMISSION_NOT_ALLOWED: '当前比赛、名单、题目或测试数据状态不允许提交',
  SUBMISSION_RATE_LIMITED: '提交过于频繁，请稍后再试',
  SUBMISSION_SOURCE_UNAVAILABLE: '该提交缺少通过完整性校验的源码，无法重判',
  TEAM_ACCOUNT_NOT_FOUND: '参赛队账号不存在',
  TEAM_IN_USE: '队伍仍分配在比赛中，删除前请先移除',
  TEAM_MEMBER_NOT_FOUND: '队伍成员不存在',
  TEAM_VERSION_CONFLICT: '队伍信息已被其他请求修改，请刷新后重试',
  TEAM_WORKSTATION_ALREADY_BOUND: '该队伍在本场比赛中已绑定工位',
  TESTDATA_VERSION_EXHAUSTED: '测试数据版本号已达上限',
  TRAINING_ENROLLMENT_INVALID: '当前训练报名不包含该题目',
  TRAINING_SET_NOT_FOUND: '训练集不存在或不可访问',
  VALIDATION_FAILED: '请求内容未通过校验',
  VIRTUAL_PROBLEM_NOT_PUBLIC: '虚拟赛题目必须为公开题目',
  VIRTUAL_SESSION_NOT_ACTIVE: '虚拟赛未进行中或不包含该题目',
  VIRTUAL_SESSION_NOT_FOUND: '虚拟赛不存在或不可访问',
  WORKSTATION_ALREADY_BOUND: '该工位在本场比赛中已被绑定',
  WORKSTATION_BINDING_NOT_FOUND: '工位绑定记录不存在',
  WORKSTATION_IDENTITY_TAKEN: '该 IP 地址或座位号已被登记',
  WORKSTATION_NOT_BOUND: '当前 IP 地址未登记到进行中的比赛',
  WORKSTATION_OR_TEAM_NOT_FOUND: '工位或参赛队不存在或未启用',
  WORKSTATION_SESSION_RESTRICTED: '该操作需要登录账号后执行',
  WORKSTATION_UPDATE_STALE: '工位信息已被其他人修改，请刷新后重试',
};

/**
 * Translations keyed by the exact server message. Used for business codes whose
 * server-side message varies by context (e.g. FORBIDDEN, SOURCE_EXPORT_TOO_LARGE)
 * so every variant keeps its specificity while still displaying Chinese.
 */
const serverMessageTranslations: Record<string, string> = {
  'A frozen award set is required': '需要先生成并锁定奖项名单',
  'A frozen award set with categories is required': '需要先生成并锁定含奖项类别的奖项名单',
  'Batch rejudge task cannot be paused in its current state': '批量重判任务当前状态不能暂停',
  'Batch rejudge task cannot be resumed in its current state': '批量重判任务当前状态不能恢复',
  'Competition management is disabled': '竞赛管理模式已停用，不能执行竞赛管理操作',
  'Competition workstation login is disabled': '竞赛工位登录已停用',
  'Contest schedules must not overlap in competition mode': '竞赛模式下比赛时间不能重叠',
  'More than one contest is active': '竞赛模式下同时只能有一场进行中的比赛',
  'Export task has no output object': '导出任务尚未生成输出文件',
  'Export task is not ready for download': '导出任务尚未完成，暂时不能下载',
  'Contest managers must provide a contest scope': '赛事管理员必须指定比赛范围',
  'Insufficient permissions': '权限不足',
  'Only contest managers may list manageable contests': '只有赛事管理员可以查看可管理的比赛',
  'Only super administrators may include deleted contests': '只有超级管理员可以查看已删除的比赛',
  'Only super administrators may include deleted teams': '只有超级管理员可以查看已删除的队伍',
  'Only team administrators may list teams': '只有队伍管理员可以查看队伍列表',
  'Custom template not found': '自定义展示模板不存在',
  'Template not found': '展示模板不存在',
  'Snapshot was not found': '榜单快照不存在',
  'Source snapshot was not found': 'Resolver 所需的来源快照不存在',
  'Screen playlist was not found': '播放列表不存在或不可访问',
  'Screen playlist was not found or empty': '播放列表不存在或为空',
  'Async source export is limited to 2 GiB': '异步源码导出最大支持 2 GiB',
  'Source export size overflowed': '源码导出大小超出限制',
  'Synchronous metadata export is limited to 10,000 submissions; use the async export task for larger contests':
    '同步元数据导出最多 10,000 条提交；更大规模请使用异步导出任务',
  'Synchronous source export is limited to 10,000 files and 128 MiB':
    '同步源码导出最多 10,000 个文件且不超过 128 MiB',
  'Synchronous source export is limited to 128 MiB': '同步源码导出最大支持 128 MiB',
  'A stored submission source failed integrity verification': '存储的提交源码未通过完整性校验',
  'Stored practice source failed integrity verification': '存储的练习提交源码未通过完整性校验',
  'Stored submission source failed integrity verification': '存储的提交源码未通过完整性校验',
  'Stored practice source is not UTF-8': '存储的练习提交源码不是有效的 UTF-8 文本',
  'Stored submission source is not valid UTF-8': '存储的提交源码不是有效的 UTF-8 文本',
  'A stored submission source does not match its recorded size': '存储的提交源码与记录大小不一致',
  'Stored practice source does not match its recorded size': '存储的练习提交源码与记录大小不一致',
  'Stored practice source has an unsupported recorded size': '练习提交源码的记录大小不受支持',
  'Stored submission source does not match its recorded size': '存储的提交源码与记录大小不一致',
  'Stored submission source has an unsupported recorded size': '提交源码的记录大小不受支持',
};

export function getErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (currentLocale() === 'en') return error.message;
    return (
      businessMessages[error.code] ??
      businessMessages[error.message] ??
      serverMessageTranslations[error.message] ??
      error.message
    );
  }
  if (error instanceof Error) {
    return error.message;
  }
  return translate('发生未知错误');
}
