import type { ApiErrorBody } from './types';

interface CsrfResponse {
  headerName: string;
  parameterName: string;
  token: string;
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fieldErrors: ApiErrorBody['fieldErrors'];

  constructor(status: number, code: string, message: string, fieldErrors?: ApiErrorBody['fieldErrors']) {
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
  let body: ApiErrorBody = {};
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
  const code = body.code ?? body.error ?? `HTTP_${response.status}`;
  const message = body.message ?? body.detail ?? defaultMessage(response.status);
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
  return messages[status] ?? `请求失败（${status}）`;
}

export interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: BodyInit | object | null;
  responseType?: 'json' | 'text' | 'blob';
  acceptedStatuses?: number[];
  suppressUnauthorizedHandler?: boolean;
}

export async function apiRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { acceptedStatuses = [], responseType, suppressUnauthorizedHandler = false, ...fetchOptions } = options;
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
  SOURCE_TOO_LARGE: '源码文件不能超过 256 KiB',
  SOURCE_EXTENSION_MISMATCH: '源码扩展名与所选语言不匹配',
  INVALID_METADATA_JSON: '提交信息格式不正确',
  RATE_LIMIT_EXCEEDED: '操作过于频繁，请稍后重试',
  SUBMISSION_NOT_FOUND: '提交记录不存在或不可访问',
  CONTEST_NOT_FOUND: '比赛不存在或不可访问',
  CONTEST_REQUIRED: '比赛管理员导入队伍时必须选择已授权比赛',
  CONTEST_ADMIN_NOT_FOUND: '比赛管理员账号不存在',
  USERNAME_TAKEN: '用户名已被使用',
  STAFF_ACCOUNT_NOT_FOUND: '工作人员账号不存在',
  STAFF_TYPE_INVALID: '工作人员账号不能使用参赛队类型',
  DISPLAY_NAME_REQUIRED: '请输入显示名称',
  SELF_ACCESS_CHANGE_FORBIDDEN: '不能停用或移除自己的超级管理员权限',
  LAST_SUPER_ADMIN: '系统必须至少保留一个已启用的超级管理员',
  STAFF_ROLE_NOT_CONFIGURED: '该工作人员角色尚未完成系统配置',
  CURRENT_PASSWORD_INVALID: '当前密码不正确',
  PASSWORD_UNCHANGED: '新密码不能与当前密码相同',
  PASSWORD_RESET_REQUIRED: '请先修改初始密码',
  ACCOUNT_DISABLED: '账号已停用',
  ACCOUNT_ACCESS_CHANGED: '账号角色已变更，请重新登录',
  CONTEST_NAME_TAKEN: '比赛名称已存在',
  CONTEST_SCHEDULE_LOCKED: '比赛开始后不能修改赛程时间',
  CONTEST_ARCHIVE_BUSY: '比赛仍有运行中的任务，处理完毕后才能归档',
  CONTEST_ARCHIVED_READ_ONLY: '比赛已归档，数据只读',
  CONTEST_EXTENSION_STATUS_INVALID: '仅进行中或暂停状态的比赛可以延时',
  CONTEST_END_TIME_NOT_SET: '比赛尚未设置结束时间',
  CONTEST_EXTENSION_STALE: '比赛结束时间已被其他管理员修改，请刷新后重试',
  CONTEST_EXTENSION_NOT_LATER: '新的结束时间必须晚于当前结束时间',
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
  PRINTER_ROLE_REQUIRED: '此操作仅限打印员',
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
  AWARD_OPERATOR_REQUIRED: '此操作仅限奖项操作员',
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
  RESOLVER_OPERATOR_REQUIRED: '此操作仅限 Resolver 操作员',
  BATCH_REJUDGE_COUNT_CHANGED: '符合条件的提交集合已变化，请重新预览',
  IDEMPOTENCY_KEY_REUSED: '该幂等键已用于其他批量重判请求',
  BATCH_REJUDGE_NOT_FOUND: '批量重判任务不存在或不可访问',
};

export function getErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    return businessMessages[error.code] ?? businessMessages[error.message] ?? error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return '发生未知错误';
}
