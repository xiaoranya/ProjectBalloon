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
};
