import type {
  BatchRejudgeItemStatus,
  BatchRejudgeTask,
  BatchRejudgeTaskStatus,
} from '../../api/bulk-rejudge';
import { useI18n } from '../../i18n';

export function progressPercentage(task: BatchRejudgeTask): number {
  if (task.totalItems === 0) return 0;
  return Math.min(100, Math.round((task.processedItems / task.totalItems) * 100));
}

export function progressStatus(task: BatchRejudgeTask): '' | 'success' | 'exception' | 'warning' {
  if (task.failedItems > 0) return 'exception';
  if (task.status === 'COMPLETED') return 'success';
  if (task.status === 'PAUSED') return 'warning';
  return '';
}

export function taskStatusType(
  status: BatchRejudgeTaskStatus,
): 'success' | 'warning' | 'danger' | 'info' | 'primary' {
  return {
    PENDING: 'info',
    RUNNING: 'primary',
    PAUSED: 'warning',
    COMPLETED: 'success',
    CANCELLED: 'danger',
  }[status] as 'success' | 'warning' | 'danger' | 'info' | 'primary';
}

export function taskStatusLabel(status: BatchRejudgeTaskStatus): string {
  const { t } = useI18n();
  return t(
    {
      PENDING: '等待执行',
      RUNNING: '执行中',
      PAUSED: '已暂停',
      COMPLETED: '已完成',
      CANCELLED: '已终止',
    }[status],
  );
}

export function itemStatusType(
  status: BatchRejudgeItemStatus,
): 'success' | 'warning' | 'danger' | 'info' | 'primary' {
  return {
    PENDING: 'info',
    PROCESSING: 'primary',
    SUCCEEDED: 'success',
    FAILED: 'danger',
    CANCELLED: 'warning',
  }[status] as 'success' | 'warning' | 'danger' | 'info' | 'primary';
}

export function itemStatusLabel(status: BatchRejudgeItemStatus): string {
  const { t } = useI18n();
  return t(
    {
      PENDING: '等待',
      PROCESSING: '处理中',
      SUCCEEDED: '成功',
      FAILED: '失败',
      CANCELLED: '已终止',
    }[status],
  );
}
