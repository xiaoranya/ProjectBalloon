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
export interface PracticeJudgement {
  id: string;
  verdict: string | null;
  totalTimeMs: number | null;
  peakMemoryKb: number | null;
  compileLog: string | null;
  workerId: string | null;
  startedAt: string | null;
  completedAt: string | null;
  createdAt: string;
  version: number;
  superseded: boolean;
  active: boolean;
  scoreMilli: number | null;
}
export interface PracticeSubmissionDetail extends PracticeSubmission {
  source: string;
  sourceSha256: string | null;
  judgements: PracticeJudgement[];
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
export interface Editorial { problemId: number; langCode: string; title: string; bodyHtml: string; bodyMarkdown?: string; unlockPolicy: string; unlocked: boolean; updatedAt: string; }
export interface VirtualSession { id:number;title:string;startAt:string;endAt:string;serverTime:string;status:'SCHEDULED'|'RUNNING'|'ENDED';totalProblems:number;solvedProblems:number; }
export interface VirtualProblem { problemId:number;slug:string;title:string;position:number;solved:boolean;attempts:number; }
export interface VirtualSessionDetail { session:VirtualSession;problems:VirtualProblem[]; }
export interface PracticeSettings { dailySubmissionLimit:number; concurrentJudgingLimit:number; sourceRetentionDays:number; updatedAt:string; }

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
  submit(problemId: number, language: string, source: string, trainingEnrollmentId?: number, virtualSessionId?: number) {
    const body = new FormData();
    body.append('metadata', JSON.stringify({ problemId, language, ...(trainingEnrollmentId ? { trainingEnrollmentId } : {}), ...(virtualSessionId ? { virtualSessionId } : {}) }));
    const extension = language === 'cpp' ? 'cpp' : language === 'java' ? 'java' : language === 'python' ? 'py' : language === 'output' ? 'zip' : 'c';
    body.append('source', new File([source], `Main.${extension}`, { type: language === 'output' ? 'application/zip' : 'text/plain' }));
    return apiRequest<{ submissionId: number; judgementId: string; status: string; submittedAt: string }>('/api/practice/submissions', { method: 'POST', body });
  },
  submissions() {
    return apiRequest<PageResponse<PracticeSubmission>>('/api/practice/submissions?page=0&size=100');
  },
  submission(id: number) {
    return apiRequest<PracticeSubmissionDetail>(`/api/practice/submissions/${id}`);
  },
  progress() {
    return apiRequest<PracticeProgress[]>('/api/practice/progress');
  },
  favorites() { return apiRequest<BankProblem[]>('/api/practice/favorites'); },
  favorite(problemId: number, favorite: boolean) { return apiRequest<{ problemId: number; favorite: boolean }>(`/api/practice/problems/${problemId}/favorite`, { method: 'PUT', body: { favorite } }); },
  editorial(problemId: number, lang = 'en') { return apiRequest<Editorial>(`/api/practice/problems/${problemId}/editorial?lang=${encodeURIComponent(lang)}`); },
  virtualSessions() { return apiRequest<VirtualSession[]>('/api/practice/virtual-sessions'); },
  virtualSession(id:number) { return apiRequest<VirtualSessionDetail>(`/api/practice/virtual-sessions/${id}`); },
  createVirtualSession(payload:{title:string;durationMinutes:number;problemIds:number[]}) { return apiRequest<VirtualSession>('/api/practice/virtual-sessions',{method:'POST',body:payload}); },
  practiceSettings() { return apiRequest<PracticeSettings>('/api/admin/practice/settings'); },
  updatePracticeSettings(payload:Omit<PracticeSettings,'updatedAt'>) { return apiRequest<PracticeSettings>('/api/admin/practice/settings',{method:'PUT',body:payload}); },
  adminEditorial(problemId:number, lang='en') { return apiRequest<Editorial>(`/api/admin/problems/${problemId}/editorials/${encodeURIComponent(lang)}`); },
  saveEditorial(problemId:number, lang:string, payload:{title:string;body:string;unlockPolicy:string;published:boolean}) { return apiRequest<Editorial>(`/api/admin/problems/${problemId}/editorials/${encodeURIComponent(lang)}`,{method:'PUT',body:payload}); },
};
