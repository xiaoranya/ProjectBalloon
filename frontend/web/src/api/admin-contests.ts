import { apiRequest } from './client';
import type {
  Contest,
  ContestExtension,
  ContestProblem,
  ContestProblemAssignment,
  ContestStatus,
  ContestTeam,
  ContestVisibility,
  LifecycleTransition,
  PageResponse,
  Problem,
  RejudgeResult,
  SubmissionDetail,
  SubmissionSummary,
  SubmissionSimilarityGroup,
  SubmissionSimilarityPair,
  SubmissionSimilarityBackfillResult,
  Team,
} from './types';

export interface ContestPayload {
  name: string;
  visibility: ContestVisibility;
  startAt: string | null;
  freezeAt: string | null;
  endAt: string | null;
}
export interface ContestCloneResult {
  sourceContestId: number;
  contest: Contest;
  problemsCopied: number;
  teamsCopied: number;
}
export interface JudgeQueueStatus {
  contestId: number;
  drained: boolean;
  pendingSubmissions: number;
  judgingSubmissions: number;
  outboxPending: number;
  outboxFailed: number;
  checkedAt: string;
}
export interface ScoringPolicy {
  contestId: number;
  scoringMode: 'ICPC' | 'OI' | 'IOI';
  scoreAggregation: 'BEST' | 'LAST';
  feedbackPolicy: 'FULL' | 'SCORE_ONLY' | 'NONE';
}
export interface ContestProblemSubtask {
  id: number;
  subtaskKey: string;
  name: string;
  displayOrder: number;
  scoreMilli: number;
  testIndexes: number[];
}
export interface ContestProblemSubtasks {
  contestId: number;
  problemId: number;
  maxScoreMilli: number;
  subtasks: ContestProblemSubtask[];
}

export interface SubmissionFilters {
  page?: number;
  size?: number;
  teamId?: number;
  problemId?: number;
  status?: string;
  language?: string;
}

export interface SubmissionSimilarityFilters {
  problemId?: number;
  language?: string;
  minGroupSize?: number;
}

export interface SubmissionSimilarityPairFilters {
  problemId?: number;
  language?: string;
  minSimilarityPercent?: number;
}

function submissionQuery(filters: SubmissionFilters): string {
  const params = new URLSearchParams({
    page: String(filters.page ?? 0),
    size: String(filters.size ?? 30),
    sort: 'submittedAt,desc',
  });
  if (filters.teamId !== undefined) params.set('teamId', String(filters.teamId));
  if (filters.problemId !== undefined) params.set('problemId', String(filters.problemId));
  if (filters.status) params.set('status', filters.status);
  if (filters.language) params.set('language', filters.language);
  return params.toString();
}

export const adminContestApi = {
  listContests(page = 0, size = 25) {
    return apiRequest<PageResponse<Contest>>(
      `/api/contests?page=${page}&size=${size}&sort=updatedAt,desc`,
    );
  },
  getContest(contestId: number) {
    return apiRequest<Contest>(`/api/contests/${contestId}`);
  },
  getScoringPolicy(contestId: number) {
    return apiRequest<ScoringPolicy>(`/api/admin/contests/${contestId}/scoring-policy`);
  },
  updateScoringPolicy(contestId: number, payload: Omit<ScoringPolicy, 'contestId'>) {
    return apiRequest<ScoringPolicy>(`/api/admin/contests/${contestId}/scoring-policy`, {
      method: 'PUT',
      body: payload,
    });
  },
  getProblemSubtasks(contestId: number, problemId: number) {
    return apiRequest<ContestProblemSubtasks>(
      `/api/admin/contests/${contestId}/problems/${problemId}/subtasks`,
    );
  },
  replaceProblemSubtasks(
    contestId: number,
    problemId: number,
    payload: {
      maxScoreMilli: number;
      subtasks: Array<Omit<ContestProblemSubtask, 'id'>>;
    },
  ) {
    return apiRequest<ContestProblemSubtasks>(
      `/api/admin/contests/${contestId}/problems/${problemId}/subtasks`,
      {
        method: 'PUT',
        body: payload,
      },
    );
  },
  createContest(payload: ContestPayload) {
    return apiRequest<Contest>('/api/contests', { method: 'POST', body: payload });
  },
  cloneContest(sourceContestId: number, payload: ContestPayload & { copyTeams: boolean }) {
    return apiRequest<ContestCloneResult>(`/api/contests/${sourceContestId}/clones`, {
      method: 'POST',
      body: payload,
    });
  },
  updateContest(contestId: number, payload: Partial<ContestPayload>) {
    return apiRequest<Contest>(`/api/contests/${contestId}`, { method: 'PATCH', body: payload });
  },
  transitionContest(contestId: number, to: ContestStatus) {
    return apiRequest<LifecycleTransition>(`/api/contests/${contestId}/transitions`, {
      method: 'POST',
      body: { to },
    });
  },
  extendContest(contestId: number, expectedEndAt: string, newEndAt: string) {
    return apiRequest<ContestExtension>(`/api/contests/${contestId}/extensions`, {
      method: 'POST',
      body: { expectedEndAt, newEndAt },
    });
  },
  listTeams(page = 0, size = 500) {
    return apiRequest<PageResponse<Team>>(`/api/teams?page=${page}&size=${size}&sort=name,asc`);
  },
  listContestTeams(contestId: number) {
    return apiRequest<ContestTeam[]>(`/api/contests/${contestId}/teams`);
  },
  assignTeam(
    contestId: number,
    payload: Pick<ContestTeam, 'teamId' | 'participationType' | 'groupName'>,
  ) {
    return apiRequest<ContestTeam>(`/api/contests/${contestId}/teams`, {
      method: 'POST',
      body: payload,
    });
  },
  unassignTeam(contestId: number, teamId: number) {
    return apiRequest<void>(`/api/contests/${contestId}/teams/${teamId}`, { method: 'DELETE' });
  },
  listProblems(page = 0, size = 100, contestId?: number) {
    const boundedSize = Math.min(100, Math.max(1, Math.trunc(size)));
    const params = new URLSearchParams({ page: String(page), size: String(boundedSize) });
    if (contestId !== undefined) params.set('contestId', String(contestId));
    return apiRequest<PageResponse<Problem>>(`/api/problems?${params}`);
  },
  async listAllProblems(contestId: number) {
    const firstPage = await this.listProblems(0, 100, contestId);
    const remainingPages = await Promise.all(
      Array.from({ length: Math.max(0, firstPage.totalPages - 1) }, (_, index) =>
        this.listProblems(index + 1, 100, contestId),
      ),
    );
    return [firstPage, ...remainingPages].flatMap((page) => page.content);
  },
  listContestProblems(contestId: number) {
    return apiRequest<ContestProblem[]>(`/api/contests/${contestId}/problems`);
  },
  assignProblem(
    contestId: number,
    payload: Pick<ContestProblem, 'problemId' | 'alias' | 'displayOrder' | 'color'>,
  ) {
    return apiRequest<ContestProblemAssignment>(`/api/contests/${contestId}/problems`, {
      method: 'POST',
      body: payload,
    });
  },
  updateProblemAssignment(
    contestId: number,
    problemId: number,
    payload: Pick<ContestProblemAssignment, 'alias' | 'displayOrder' | 'color'>,
  ) {
    return apiRequest<ContestProblemAssignment>(
      `/api/contests/${contestId}/problems/${problemId}`,
      { method: 'PATCH', body: payload },
    );
  },
  reorderProblems(contestId: number, entries: Array<{ problemId: number; displayOrder: number }>) {
    return apiRequest<ContestProblemAssignment[]>(`/api/contests/${contestId}/problems/reorder`, {
      method: 'PUT',
      body: entries,
    });
  },
  unassignProblem(contestId: number, problemId: number) {
    return apiRequest<void>(`/api/contests/${contestId}/problems/${problemId}`, {
      method: 'DELETE',
    });
  },
  listSubmissions(contestId: number, filters: SubmissionFilters = {}) {
    return apiRequest<PageResponse<SubmissionSummary>>(
      `/api/admin/contests/${contestId}/submissions?${submissionQuery(filters)}`,
    );
  },
  listSubmissionSimilarity(contestId: number, filters: SubmissionSimilarityFilters = {}) {
    const params = new URLSearchParams();
    if (filters.problemId !== undefined) params.set('problemId', String(filters.problemId));
    if (filters.language) params.set('language', filters.language);
    if (filters.minGroupSize !== undefined)
      params.set('minGroupSize', String(filters.minGroupSize));
    const query = params.toString();
    return apiRequest<SubmissionSimilarityGroup[]>(
      '/api/admin/contests/' + contestId + '/submission-similarity' + (query ? '?' + query : ''),
    );
  },
  listSubmissionSimilarityPairs(contestId: number, filters: SubmissionSimilarityPairFilters = {}) {
    const params = new URLSearchParams();
    if (filters.problemId !== undefined) params.set('problemId', String(filters.problemId));
    if (filters.language) params.set('language', filters.language);
    if (filters.minSimilarityPercent !== undefined)
      params.set('minSimilarityPercent', String(filters.minSimilarityPercent));
    const query = params.toString();
    return apiRequest<SubmissionSimilarityPair[]>(
      '/api/admin/contests/' +
        contestId +
        '/submission-similarity/pairs' +
        (query ? '?' + query : ''),
    );
  },
  backfillSubmissionSimilarity(contestId: number) {
    return apiRequest<SubmissionSimilarityBackfillResult>(
      '/api/admin/contests/' + contestId + '/submission-similarity/backfill',
      { method: 'POST' },
    );
  },
  getSubmission(contestId: number, submissionId: number) {
    return apiRequest<SubmissionDetail>(
      `/api/admin/contests/${contestId}/submissions/${submissionId}`,
    );
  },
  getJudgeQueueStatus(contestId: number) {
    return apiRequest<JudgeQueueStatus>(`/api/admin/contests/${contestId}/judge-queue/status`);
  },
  rejudgeSubmission(contestId: number, submissionId: number, expectedJudgementId: string) {
    return apiRequest<RejudgeResult>(
      `/api/admin/contests/${contestId}/submissions/${submissionId}/rejudge`,
      { method: 'POST', body: { expectedJudgementId } },
    );
  },
  exportScoreboard(contestId: number) {
    return apiRequest<Blob>(`/api/admin/contests/${contestId}/scoreboard.csv`, {
      responseType: 'blob',
      headers: { Accept: 'text/csv' },
    });
  },
  exportSubmissions(contestId: number) {
    return apiRequest<Blob>(`/api/admin/contests/${contestId}/exports/submissions.csv`, {
      responseType: 'blob',
      headers: { Accept: 'text/csv' },
    });
  },
  exportSubmissionSources(contestId: number) {
    return apiRequest<Blob>(`/api/admin/contests/${contestId}/exports/submission-sources.zip`, {
      responseType: 'blob',
      headers: { Accept: 'application/zip' },
    });
  },
};
