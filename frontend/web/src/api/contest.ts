import { apiRequest } from './client';
import type {
  Contest,
  ContestProblem,
  PageResponse,
  Scoreboard,
  SubmissionDetail,
  SubmissionSummary,
  SubmitResult,
} from './types';

export const contestApi = {
  listContests(): Promise<PageResponse<Contest>> {
    return apiRequest('/api/contests?page=0&size=50&sort=startAt,desc');
  },

  getContest(contestId: number): Promise<Contest> {
    return apiRequest(`/api/contests/${contestId}`);
  },

  listProblems(contestId: number, lang?: string): Promise<ContestProblem[]> {
    const query = lang ? `?lang=${encodeURIComponent(lang)}` : '';
    return apiRequest(`/api/contests/${contestId}/problems${query}`);
  },

  submit(
    contestId: number,
    problemId: number,
    language: string,
    source: File,
  ): Promise<SubmitResult> {
    const form = new FormData();
    form.append('metadata', JSON.stringify({ problemId, language }));
    form.append('source', source, source.name);
    return apiRequest(`/api/contests/${contestId}/submissions`, {
      method: 'POST',
      body: form,
    });
  },

  listSubmissions(contestId: number, page = 0): Promise<PageResponse<SubmissionSummary>> {
    return apiRequest(
      `/api/contests/${contestId}/submissions?page=${page}&size=30&sort=submittedAt,desc`,
    );
  },

  getSubmission(contestId: number, submissionId: number): Promise<SubmissionDetail> {
    return apiRequest(`/api/contests/${contestId}/submissions/${submissionId}`);
  },

  getScoreboard(contestId: number): Promise<Scoreboard> {
    return apiRequest(`/api/contests/${contestId}/scoreboard`);
  },
};
