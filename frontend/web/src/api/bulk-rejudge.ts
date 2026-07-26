import { apiRequest } from './client';

export type BatchRejudgeTaskStatus = 'PENDING' | 'RUNNING' | 'PAUSED' | 'COMPLETED' | 'CANCELLED';
export type BatchRejudgeItemStatus = 'PENDING' | 'PROCESSING' | 'SUCCEEDED' | 'FAILED' | 'CANCELLED';
export type BatchRejudgeVerdict =
  | 'ACCEPTED'
  | 'WRONG_ANSWER'
  | 'COMPILE_ERROR'
  | 'RUNTIME_ERROR'
  | 'TIME_LIMIT_EXCEEDED'
  | 'MEMORY_LIMIT_EXCEEDED'
  | 'OUTPUT_LIMIT_EXCEEDED'
  | 'SYSTEM_ERROR'
  | 'CANCELLED';

export interface BatchRejudgeFilter {
  problemId: number | null;
  teamId: number | null;
  language: string | null;
  verdict: BatchRejudgeVerdict | null;
  submittedFrom: string | null;
  submittedTo: string | null;
}

export interface BatchRejudgePreview {
  matchedSubmissions: number;
}

export interface BatchRejudgeCreateRequest {
  filter: BatchRejudgeFilter;
  expectedCount: number;
  confirmationText: string;
  idempotencyKey: string;
}

export interface BatchRejudgeItem {
  id: number;
  submissionId: number;
  status: BatchRejudgeItemStatus;
  oldJudgementId: string | null;
  newJudgementId: string | null;
  errorMessage: string | null;
  attempts: number;
  processedAt: string | null;
}

export interface BatchRejudgeTask {
  id: number;
  contestId: number;
  status: BatchRejudgeTaskStatus;
  totalItems: number;
  processedItems: number;
  succeededItems: number;
  failedItems: number;
  cancelRequested: boolean;
  createdByUserId: number;
  startedAt: string | null;
  completedAt: string | null;
  createdAt: string;
  updatedAt: string;
  items: BatchRejudgeItem[];
  itemsTruncated: boolean;
}

function taskPath(contestId: number, suffix = '') {
  return `/api/admin/contests/${contestId}/rejudge-tasks${suffix}`;
}

export const bulkRejudgeApi = {
  preview(contestId: number, filter: BatchRejudgeFilter) {
    return apiRequest<BatchRejudgePreview>(taskPath(contestId, '/preview'), {
      method: 'POST',
      body: filter,
    });
  },

  create(contestId: number, payload: BatchRejudgeCreateRequest) {
    return apiRequest<BatchRejudgeTask>(taskPath(contestId), {
      method: 'POST',
      body: payload,
    });
  },

  list(contestId: number) {
    return apiRequest<BatchRejudgeTask[]>(taskPath(contestId));
  },

  get(contestId: number, taskId: number) {
    return apiRequest<BatchRejudgeTask>(taskPath(contestId, `/${taskId}`));
  },

  pause(contestId: number, taskId: number) {
    return apiRequest<BatchRejudgeTask>(taskPath(contestId, `/${taskId}/pause`), {
      method: 'POST',
    });
  },

  resume(contestId: number, taskId: number) {
    return apiRequest<BatchRejudgeTask>(taskPath(contestId, `/${taskId}/resume`), {
      method: 'POST',
    });
  },
};
