import type { BalloonTaskStatus } from '../../api/balloons';

export function statusLabel(t: (key: string) => string, status: BalloonTaskStatus): string {
  return t(
    { PENDING: '待领取', CLAIMED: '配送中', DELIVERED: '已送达', CANCELLED: '已取消' }[status],
  );
}

export function statusType(status: BalloonTaskStatus): 'success' | 'warning' | 'info' | 'danger' {
  return status === 'DELIVERED' ? 'success' : status === 'CANCELLED' ? 'danger' : 'warning';
}
