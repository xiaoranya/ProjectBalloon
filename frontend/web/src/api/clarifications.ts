import { apiRequest } from './client';

export type ClarificationScope = 'GENERAL' | 'PROBLEM';
export type ClarificationStatus = 'PENDING' | 'ANSWERED' | 'CLOSED';
export type ClarificationReplyVisibility = 'PRIVATE' | 'PUBLIC';

export type AskClarificationRequest =
  | { scope: 'GENERAL'; problemId: null; question: string }
  | { scope: 'PROBLEM'; problemId: number; question: string };

export interface ReplyClarificationRequest {
  reply: string;
  visibility: ClarificationReplyVisibility;
}

export interface ConvertClarificationRequest {
  title: string | null;
  body: string | null;
}

export interface Clarification {
  id: number;
  contestId: number;
  teamId: number;
  teamName: string | null;
  scope: ClarificationScope;
  problemId: number | null;
  problemAlias: string | null;
  question: string;
  status: ClarificationStatus;
  reply: string | null;
  replyVisibility: ClarificationReplyVisibility | null;
  askedByUserId: number;
  repliedByUserId: number | null;
  repliedAt: string | null;
  convertedAnnouncementId: number | null;
  createdAt: string;
  updatedAt: string;
  version: number;
}

export interface Announcement {
  id: number;
  contestId: number;
  title: string;
  body: string;
  pinned: boolean;
  status: string;
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

function contestPath(contestId: number, suffix: string) {
  return `/api/contests/${contestId}/clarifications${suffix}`;
}

export const clarificationApi = {
  ask(contestId: number, request: AskClarificationRequest) {
    return apiRequest<Clarification>(contestPath(contestId, ''), {
      method: 'POST',
      body: request,
    });
  },

  listMine(contestId: number) {
    return apiRequest<Clarification[]>(contestPath(contestId, '/mine'));
  },

  listAll(contestId: number, status?: ClarificationStatus) {
    const query = status ? `?status=${status.toLowerCase()}` : '';
    return apiRequest<Clarification[]>(contestPath(contestId, `/all${query}`));
  },

  get(id: number) {
    return apiRequest<Clarification>(`/api/clarifications/${id}`);
  },

  reply(id: number, request: ReplyClarificationRequest) {
    return apiRequest<Clarification>(`/api/clarifications/${id}/reply`, {
      method: 'POST',
      body: request,
    });
  },

  close(id: number) {
    return apiRequest<void>(`/api/clarifications/${id}/close`, { method: 'POST' });
  },

  convert(id: number, request: ConvertClarificationRequest) {
    return apiRequest<Announcement>(`/api/clarifications/${id}/convert`, {
      method: 'POST',
      body: request,
    });
  },
};
