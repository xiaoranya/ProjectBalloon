import { apiRequest } from './client';
import type { PageResponse } from './types';

export interface BankProblem {
  id: number;
  slug: string;
  title: string;
  statement: string | null;
  difficulty: number | null;
  tags: string[];
  publishedAt: string;
}

export interface TrainingSet {
  id: number;
  slug: string;
  title: string;
  description: string;
  visibility: 'DRAFT' | 'PUBLIC' | 'ARCHIVED';
  itemCount: number;
}

export interface TrainingItem {
  problemId: number;
  slug: string;
  title: string;
  position: number;
  required: boolean;
  difficulty: number | null;
  tags: string[];
}

export interface TrainingSetDetail {
  setInfo: TrainingSet;
  items: TrainingItem[];
}

export interface TrainingEnrollment {
  id: number;
  setId: number;
  teamId: number;
  status: 'ACTIVE' | 'COMPLETED' | 'ABANDONED';
  startedAt: string;
  completedAt: string | null;
}

export interface PracticeSubmission {
  id: number;
  problemId: number;
  problemSlug: string;
  problemTitle: string;
  trainingEnrollmentId: number | null;
  language: string;
  sourceSizeBytes: number;
  status: string;
  submittedAt: string;
  judgedAt: string | null;
  activeJudgementId: string | null;
  verdict: string | null;
  totalTimeMs: number | null;
  peakMemoryKb: number | null;
  score: number | null;
}

export interface PracticeProgress {
  problemId: number;
  attempts: number;
  bestScore: number;
  solved: boolean;
  lastSubmissionId: number | null;
  solvedAt: string | null;
  updatedAt: string;
}

export const trainingApi = {
  problemBank(page = 0, size = 50, tag?: string, difficulty?: number) {
    const query = new URLSearchParams({ page: String(page), size: String(size) });
    if (tag) query.set('tag', tag);
    if (difficulty !== undefined) query.set('difficulty', String(difficulty));
    return apiRequest<PageResponse<BankProblem>>(`/api/public/problem-bank?${query}`);
  },
  problem(slug: string) {
    return apiRequest<BankProblem>(`/api/public/problem-bank/${encodeURIComponent(slug)}`);
  },
  sets() {
    return apiRequest<TrainingSet[]>('/api/training/sets');
  },
  set(id: number) {
    return apiRequest<TrainingSetDetail>(`/api/training/sets/${id}`);
  },
  enroll(id: number) {
    return apiRequest<TrainingEnrollment>(`/api/training/sets/${id}/enroll`, { method: 'POST' });
  },
  submit(problemId: number, language: string, source: string, trainingEnrollmentId?: number) {
    const body = new FormData();
    body.append('metadata', JSON.stringify({ problemId, language, ...(trainingEnrollmentId ? { trainingEnrollmentId } : {}) }));
    const extension = language === 'cpp' ? 'cpp' : language === 'java' ? 'java' : language === 'python' ? 'py' : language === 'output' ? 'zip' : 'c';
    body.append('source', new File([source], `Main.${extension}`, { type: language === 'output' ? 'application/zip' : 'text/plain' }));
    return apiRequest<{ submissionId: number; judgementId: string; status: string; submittedAt: string }>('/api/practice/submissions', { method: 'POST', body });
  },
  submissions() {
    return apiRequest<PageResponse<PracticeSubmission>>('/api/practice/submissions?page=0&size=100');
  },
  progress() {
    return apiRequest<PracticeProgress[]>('/api/practice/progress');
  },
};
