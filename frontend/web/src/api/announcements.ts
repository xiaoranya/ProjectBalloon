import { apiRequest } from './client';

export type AnnouncementStatus = 'SCHEDULED' | 'PUBLISHED' | 'WITHDRAWN' | 'CANCELLED';

export interface Announcement {
  id: number;
  contestId: number;
  title: string;
  body: string;
  pinned: boolean;
  status: AnnouncementStatus;
  createdByUserId: number;
  publishedAt: string | null;
  scheduledAt: string | null;
  withdrawnAt: string | null;
  withdrawnByUserId: number | null;
  sourceClarificationId: number | null;
  cancelledAt: string | null;
  cancelledByUserId: number | null;
  createdAt: string;
  updatedAt: string;
  version: number;
}

export interface AnnouncementPayload {
  title: string;
  body: string;
  pinned: boolean;
  scheduledAt: string | null;
}

export const announcementApi = {
  list(contestId: number, includeWithdrawn = false) {
    return apiRequest<Announcement[]>(
      `/api/contests/${contestId}/announcements?includeWithdrawn=${includeWithdrawn}`,
    );
  },
  create(contestId: number, payload: AnnouncementPayload) {
    return apiRequest<Announcement>(`/api/contests/${contestId}/announcements`, {
      method: 'POST',
      body: payload,
    });
  },
  update(
    id: number,
    payload: { title: string; body: string; pinned: boolean; expectedVersion: number },
  ) {
    return apiRequest<Announcement>(`/api/announcements/${id}`, { method: 'PATCH', body: payload });
  },
  schedule(id: number, payload: AnnouncementPayload) {
    return apiRequest<Announcement>(`/api/announcements/${id}/schedule`, {
      method: 'POST',
      body: payload,
    });
  },
  cancel(id: number) {
    return apiRequest<Announcement>(`/api/announcements/${id}/cancel`, { method: 'POST' });
  },
  pin(id: number, pinned: boolean) {
    return apiRequest<Announcement>(`/api/announcements/${id}/pin`, {
      method: 'POST',
      body: { pinned },
    });
  },
  withdraw(id: number) {
    return apiRequest<void>(`/api/announcements/${id}/withdraw`, { method: 'POST' });
  },
};
