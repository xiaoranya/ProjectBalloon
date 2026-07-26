import { apiRequest } from './client';

export const PRINT_CONTENT_MAX_BYTES = 20_480;
export const PRINT_PAGE_MAX = 5;
export const PRINT_REJECT_REASON_MAX_CHARS = 255;

export const printRequestStatuses = [
  'REQUESTED',
  'QUEUED',
  'PRINTING',
  'COMPLETED',
  'FAILED',
  'CANCELLED',
  'REJECTED',
] as const;

export type PrintRequestStatus = (typeof printRequestStatuses)[number];
export type PrintRequestAction = 'QUEUED' | 'PRINTING' | 'COMPLETED' | 'FAILED' | 'CANCELLED' | 'REJECTED';

export interface PrintRequestResponse {
  id: number;
  contestId: number;
  teamId: number;
  teamName: string | null;
  seatNo: string | null;
  contentHash: string;
  pageCount: number;
  status: PrintRequestStatus;
  printerId: string | null;
  cupsJobId: string | null;
  requestedByUserId: number;
  operatorUserId: number | null;
  completedAt: string | null;
  failedReason: string | null;
  createdAt: string;
  updatedAt: string;
  version: number;
}

export interface PrintContentValidation {
  content: string;
  bytes: number;
  pageCount: number;
  error: string | null;
}

export function normalizePrintContent(content: string): string {
  return content.replace(/\r\n?/g, '\n');
}

export function estimatePrintPages(content: string): number {
  const physicalLines = content.split('\n');
  const wrappedLines = physicalLines.reduce((total, line) => {
    let columns = 0;
    for (const character of line) columns += character === '\t' ? 4 : 1;
    return total + Math.max(1, Math.ceil(columns / 100));
  }, 0);
  return Math.ceil(wrappedLines / 50);
}

export function validatePrintContent(rawContent: string): PrintContentValidation {
  const content = normalizePrintContent(rawContent);
  const bytes = new TextEncoder().encode(content).byteLength;
  const pageCount = estimatePrintPages(content);
  let error: string | null = null;

  if (!content.trim()) error = '请输入需要打印的纯文本内容';
  else if ([...content].some((character) => character !== '\n' && character !== '\t' && /\p{Cc}/u.test(character))) {
    error = '打印内容不能包含换行和制表符以外的控制字符';
  } else if (bytes > PRINT_CONTENT_MAX_BYTES) error = '打印内容不能超过 20 KiB';
  else if (pageCount > PRINT_PAGE_MAX) error = '打印内容不能超过 5 页';

  return { content, bytes, pageCount, error };
}

export function normalizeRejectReason(reason: string): string {
  return reason.replace(/[\r\n]/g, ' ').trim();
}

export function rejectReasonLength(reason: string): number {
  return Array.from(normalizeRejectReason(reason)).length;
}

function contestPath(contestId: number, suffix = '') {
  return `/api/contests/${contestId}/print-requests${suffix}`;
}

export const printingApi = {
  create(contestId: number, content: string) {
    return apiRequest<PrintRequestResponse>(contestPath(contestId), {
      method: 'POST',
      body: { content },
    });
  },

  listMine(contestId: number) {
    return apiRequest<PrintRequestResponse[]>(contestPath(contestId, '/mine'));
  },

  listAll(contestId: number, status?: PrintRequestStatus) {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return apiRequest<PrintRequestResponse[]>(contestPath(contestId, `/all${query}`));
  },

  retry(id: number) {
    return apiRequest<PrintRequestResponse>(`/api/print-requests/${id}/retry`, { method: 'POST' });
  },

  cancel(id: number) {
    return apiRequest<PrintRequestResponse>(`/api/print-requests/${id}/cancel`, { method: 'POST' });
  },

  reject(id: number, reason: string) {
    return apiRequest<PrintRequestResponse>(`/api/print-requests/${id}/reject`, {
      method: 'POST',
      body: { reason },
    });
  },

  pdf(id: number) {
    return apiRequest<Blob>(`/api/print-requests/${id}/pdf`, { responseType: 'blob' });
  },
};
