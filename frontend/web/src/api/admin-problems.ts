import { apiRequest } from './client';
import type {
  JudgeLanguage,
  PageResponse,
  Problem,
  ProblemAttachment,
  ProblemAttachmentKind,
  ProblemStatement,
  ProblemTestdata,
  ProblemTestdataVersion,
} from './types';

export interface ProblemPayload {
  slug: string;
  title: string;
  timeLimitMs: number;
  memoryLimitMb: number;
  outputLimitKb: number;
  languages: JudgeLanguage[];
  defaultLangCode: string;
  judgeMode?: 'STANDARD' | 'INTERACTIVE' | 'OUTPUT_ONLY';
  interactorObjectKey?: string | null;
  interactorSha256?: string | null;
}

export interface UpdateProblemPayload extends Partial<ProblemPayload> {
  expectedVersion: number;
}

const MAX_PAGE_SIZE = 100;

function boundedPageSize(size: number): number {
  if (!Number.isFinite(size)) return 50;
  return Math.min(MAX_PAGE_SIZE, Math.max(1, Math.trunc(size)));
}

async function mutateThenRefresh<T>(problemId: number, mutation: Promise<T>) {
  const result = await mutation;
  try {
    const problem = await adminProblemApi.getProblem(problemId);
    return { result, problem, refreshFailed: false as const };
  } catch {
    // The mutation is already committed. Preserve that success so callers do not retry
    // uploads or deletes, and require a manual refresh before another mutation.
    return { result, problem: null, refreshFailed: true as const };
  }
}

export const adminProblemApi = {
  listProblems(page = 0, size = 50) {
    const params = new URLSearchParams({
      page: String(Math.max(0, Math.trunc(page))),
      size: String(boundedPageSize(size)),
    });
    return apiRequest<PageResponse<Problem>>(`/api/problems?${params.toString()}`);
  },
  getProblem(problemId: number) {
    return apiRequest<Problem>(`/api/problems/${problemId}`);
  },
  createProblem(payload: ProblemPayload) {
    return apiRequest<Problem>('/api/problems', { method: 'POST', body: payload });
  },
  updateProblem(problemId: number, payload: UpdateProblemPayload) {
    return apiRequest<Problem>(`/api/problems/${problemId}`, { method: 'PATCH', body: payload });
  },
  deleteProblem(problemId: number) {
    return apiRequest<void>(`/api/problems/${problemId}`, { method: 'DELETE' });
  },
  upsertStatement(problemId: number, langCode: string, body: string) {
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemStatement>(
        `/api/problems/${problemId}/statements/${encodeURIComponent(langCode)}`,
        { method: 'PUT', body: { body } },
      ),
    );
  },
  listStatements(problemId: number) {
    return apiRequest<ProblemStatement[]>(`/api/problems/${problemId}/statements`);
  },
  deleteStatement(problemId: number, langCode: string) {
    return apiRequest<void>(
      `/api/problems/${problemId}/statements/${encodeURIComponent(langCode)}`,
      { method: 'DELETE' },
    );
  },
  listAttachments(problemId: number) {
    return apiRequest<ProblemAttachment[]>(`/api/problems/${problemId}/attachments`);
  },
  uploadAttachment(problemId: number, kind: ProblemAttachmentKind, file: File) {
    const body = new FormData();
    body.append('kind', kind);
    body.append('file', file);
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemAttachment>(`/api/problems/${problemId}/attachments`, {
        method: 'POST',
        body,
      }),
    );
  },
  deleteAttachment(problemId: number, attachmentId: number) {
    return mutateThenRefresh(
      problemId,
      apiRequest<void>(`/api/problems/${problemId}/attachments/${attachmentId}`, {
        method: 'DELETE',
      }),
    );
  },
  downloadAttachment(problemId: number, attachmentId: number) {
    return apiRequest<Blob>(`/api/problems/${problemId}/attachments/${attachmentId}`, {
      responseType: 'blob',
    });
  },
  uploadTestdata(problemId: number, file: File) {
    const body = new FormData();
    body.append('file', file);
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemTestdata>(`/api/problems/${problemId}/testdata`, {
        method: 'POST',
        body,
      }),
    );
  },
  uploadInteractor(problemId: number, file: File) {
    const body = new FormData();
    body.append('file', file);
    return apiRequest<Problem>(`/api/problems/${problemId}/interactor`, { method: 'POST', body });
  },
  downloadTestdata(problemId: number) {
    return apiRequest<Blob>(`/api/problems/${problemId}/testdata`, { responseType: 'blob' });
  },
  listTestdataVersions(problemId: number) {
    return apiRequest<ProblemTestdataVersion[]>(`/api/problems/${problemId}/testdata/versions`);
  },
  downloadTestdataVersion(problemId: number, version: number) {
    return apiRequest<Blob>(`/api/problems/${problemId}/testdata/versions/${version}`, {
      responseType: 'blob',
    });
  },
  activateTestdataVersion(problemId: number, version: number, expectedCurrentVersion: number) {
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemTestdataVersion>(
        `/api/problems/${problemId}/testdata/versions/${version}/activate`,
        { method: 'POST', body: { expectedCurrentVersion } },
      ),
    );
  },
};
