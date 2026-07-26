const dateFormatter = new Intl.DateTimeFormat('zh-CN', {
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
});

export function formatDateTime(value: string | null | undefined): string {
  if (!value) return '—';
  return dateFormatter.format(new Date(value));
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KiB`;
}

export function contestStatusLabel(status: string): string {
  return {
    DRAFT: '草稿',
    FROZEN_CONFIG: '配置已锁定',
    RUNNING: '进行中',
    PAUSED: '已暂停',
    ENDED: '已结束',
    ARCHIVED: '已归档',
  }[status] ?? status;
}

export function languageLabel(language: string): string {
  return {
    c: 'C',
    cpp: 'C++',
    java: 'Java',
    python: 'Python',
  }[language] ?? language;
}

export function submissionStatusLabel(status: string): string {
  return {
    PENDING: '等待判题',
    JUDGING: '判题中',
    ACCEPTED: '答案正确',
    WRONG_ANSWER: '答案错误',
    COMPILE_ERROR: '编译错误',
    RUNTIME_ERROR: '运行时错误',
    TIME_LIMIT_EXCEEDED: '超出时间限制',
    MEMORY_LIMIT_EXCEEDED: '超出内存限制',
    OUTPUT_LIMIT_EXCEEDED: '输出超限',
    SYSTEM_ERROR: '系统错误',
    CANCELLED: '已取消',
  }[status] ?? status;
}

export function isFinalSubmissionStatus(status: string): boolean {
  return !['PENDING', 'JUDGING'].includes(status);
}

export function statusTagType(
  status: string,
): 'success' | 'danger' | 'warning' | 'info' | 'primary' {
  if (status === 'ACCEPTED') return 'success';
  if (['PENDING', 'JUDGING'].includes(status)) return 'warning';
  if (
    [
      'WRONG_ANSWER',
      'COMPILE_ERROR',
      'RUNTIME_ERROR',
      'TIME_LIMIT_EXCEEDED',
      'MEMORY_LIMIT_EXCEEDED',
      'OUTPUT_LIMIT_EXCEEDED',
      'SYSTEM_ERROR',
    ].includes(status)
  ) return 'danger';
  return 'info';
}
