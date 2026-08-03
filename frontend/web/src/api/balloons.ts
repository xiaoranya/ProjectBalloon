import { apiRequest } from './client';

export const balloonTaskStatuses = ['PENDING', 'CLAIMED', 'DELIVERED', 'CANCELLED'] as const;
export type BalloonTaskStatus = (typeof balloonTaskStatuses)[number];

export interface BalloonTask {
  id: number;
  contestId: number;
  teamId: number;
  problemId: number;
  submissionId: number;
  color: string;
  isFirstBlood: boolean;
  status: BalloonTaskStatus;
  seatNo: string | null;
  teamName: string;
  problemAlias: string;
  note: string | null;
  claimedByUserId: number | null;
  claimedAt: string | null;
  deliveredAt: string | null;
  cancelledAt: string | null;
  cancelledReason: string | null;
  createdAt: string;
  updatedAt: string;
  version: number;
  reopenedCount: number;
  priority?: number;
  deliveryZone?: string;
  dispatchAttempts?: number;
  lastDispatchedAt?: string | null;
}

export interface BalloonStats {
  total: number;
  pending: number;
  claimed: number;
  delivered: number;
  cancelled: number;
  firstBlood: number;
}

function taskPath(id: number, action: string) {
  return `/api/balloons/${id}/${action}`;
}

export const balloonApi = {
  list(contestId: number, status?: BalloonTaskStatus) {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return apiRequest<BalloonTask[]>(`/api/contests/${contestId}/balloons${query}`);
  },
  stats(contestId: number) {
    return apiRequest<BalloonStats>(`/api/contests/${contestId}/balloons/stats`);
  },
  dispatch(contestId: number, limit = 10, zone?: string) {
    const query = new URLSearchParams({ limit: String(limit) });
    if (zone) query.set('zone', zone);
    return apiRequest<BalloonTask[]>(`/api/contests/${contestId}/balloons/dispatch?${query}`, {
      method: 'POST',
    });
  },
  claim(id: number, expectedVersion: number) {
    return apiRequest<BalloonTask>(taskPath(id, 'claim'), {
      method: 'POST',
      body: { expectedVersion },
    });
  },
  deliver(id: number, expectedVersion: number) {
    return apiRequest<BalloonTask>(taskPath(id, 'deliver'), {
      method: 'POST',
      body: { expectedVersion },
    });
  },
  cancel(id: number, expectedVersion: number, reason: string) {
    return apiRequest<BalloonTask>(taskPath(id, 'cancel'), {
      method: 'POST',
      body: { expectedVersion, reason },
    });
  },
  reopen(id: number, expectedVersion: number) {
    return apiRequest<BalloonTask>(taskPath(id, 'reopen'), {
      method: 'POST',
      body: { expectedVersion },
    });
  },
  note(id: number, expectedVersion: number, note: string | null) {
    return apiRequest<BalloonTask>(taskPath(id, 'note'), {
      method: 'PATCH',
      body: { expectedVersion, note },
    });
  },
};
