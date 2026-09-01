import { DOWNLOAD_TIMEOUT_MS, apiRequest } from './client';
import type {
  JudgeLanguage,
  PageResponse,
  ProblemResponse,
  ProblemAttachmentResponse,
  ProblemAttachmentKind,
  ProblemStatementResponse,
  ProblemTestdataResponse,
  ProblemTestdataVersionResponse,
} from './types';

/** Options for large-file downloads: external cancellation and byte progress. */
export interface DownloadOptions {
  signal?: AbortSignal;
  onProgress?: (loaded: number, total: number) => void;
}

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

export interface ProblemPublication {
  visibility: 'PUBLIC' | 'PRIVATE';
  difficulty: number | null;
  tags: string[];
  publishedAt: string | null;
}

export interface PublicationPayload {
  visibility: 'PUBLIC' | 'PRIVATE';
  difficulty?: number | null;
  tags?: string[];
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
    return apiRequest<PageResponse<ProblemResponse>>(`/api/problems?${params.toString()}`);
  },
  getProblem(problemId: number) {
    return apiRequest<ProblemResponse>(`/api/problems/${problemId}`);
  },
  createProblem(payload: ProblemPayload) {
    return apiRequest<ProblemResponse>('/api/problems', { method: 'POST', body: payload });
  },
  updateProblem(problemId: number, payload: UpdateProblemPayload) {
    return apiRequest<ProblemResponse>(`/api/problems/${problemId}`, {
      method: 'PATCH',
      body: payload,
    });
  },
  deleteProblem(problemId: number) {
    return apiRequest<void>(`/api/problems/${problemId}`, { method: 'DELETE' });
  },
  getPublication(problemId: number) {
    return apiRequest<ProblemPublication>(`/api/admin/problems/${problemId}/publication`);
  },
  updatePublication(problemId: number, payload: PublicationPayload) {
    return apiRequest<ProblemPublication>(`/api/admin/problems/${problemId}/publication`, {
      method: 'PUT',
      body: payload,
    });
  },
  upsertStatement(problemId: number, langCode: string, body: string) {
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemStatementResponse>(
        `/api/problems/${problemId}/statements/${encodeURIComponent(langCode)}`,
        { method: 'PUT', body: { body } },
      ),
    );
  },
  listStatements(problemId: number) {
    return apiRequest<ProblemStatementResponse[]>(`/api/problems/${problemId}/statements`);
  },
  deleteStatement(problemId: number, langCode: string) {
    return apiRequest<void>(
      `/api/problems/${problemId}/statements/${encodeURIComponent(langCode)}`,
      { method: 'DELETE' },
    );
  },
  listAttachments(problemId: number) {
    return apiRequest<ProblemAttachmentResponse[]>(`/api/problems/${problemId}/attachments`);
  },
  uploadAttachment(problemId: number, kind: ProblemAttachmentKind, file: File) {
    const body = new FormData();
    body.append('kind', kind);
    body.append('file', file);
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemAttachmentResponse>(`/api/problems/${problemId}/attachments`, {
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
  downloadAttachment(problemId: number, attachmentId: number, options: DownloadOptions = {}) {
    return apiRequest<Blob>(`/api/problems/${problemId}/attachments/${attachmentId}`, {
      responseType: 'blob',
      // 附件上限 20 MiB，但慢速网络下 30s 仍可能不够；统一使用下载预算。
      timeoutMs: DOWNLOAD_TIMEOUT_MS,
      signal: options.signal,
      onProgress: options.onProgress,
    });
  },
  uploadTestdata(problemId: number, file: File) {
    const body = new FormData();
    body.append('file', file);
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemTestdataResponse>(`/api/problems/${problemId}/testdata`, {
        method: 'POST',
        body,
      }),
    );
  },
  uploadInteractor(problemId: number, file: File) {
    const body = new FormData();
    body.append('file', file);
    return apiRequest<ProblemResponse>(`/api/problems/${problemId}/interactor`, {
      method: 'POST',
      body,
    });
  },
  downloadTestdata(problemId: number, options: DownloadOptions = {}) {
    return apiRequest<Blob>(`/api/problems/${problemId}/testdata`, {
      responseType: 'blob',
      // 测试数据 ZIP 最大 256 MiB；使用长下载预算并对停滞的传输设限。
      timeoutMs: DOWNLOAD_TIMEOUT_MS,
      signal: options.signal,
      onProgress: options.onProgress,
    });
  },
  listTestdataVersions(problemId: number) {
    return apiRequest<ProblemTestdataVersionResponse[]>(
      `/api/problems/${problemId}/testdata/versions`,
    );
  },
  downloadTestdataVersion(problemId: number, version: number, options: DownloadOptions = {}) {
    return apiRequest<Blob>(`/api/problems/${problemId}/testdata/versions/${version}`, {
      responseType: 'blob',
      // 测试数据 ZIP 最大 256 MiB；使用长下载预算并对停滞的传输设限。
      timeoutMs: DOWNLOAD_TIMEOUT_MS,
      signal: options.signal,
      onProgress: options.onProgress,
    });
  },
  activateTestdataVersion(problemId: number, version: number, expectedCurrentVersion: number) {
    return mutateThenRefresh(
      problemId,
      apiRequest<ProblemTestdataVersionResponse>(
        `/api/problems/${problemId}/testdata/versions/${version}/activate`,
        { method: 'POST', body: { expectedCurrentVersion } },
      ),
    );
  },
};
