import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clarificationApi } from './clarifications';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const csrf = { headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' };
const clarification = {
  id: 9, contestId: 7, teamId: 3, teamName: 'Team Three', scope: 'GENERAL', problemId: null,
  problemAlias: null, question: 'Question', status: 'PENDING', reply: null, replyVisibility: null,
  askedByUserId: 4, repliedByUserId: null, repliedAt: null, convertedAnnouncementId: null,
  createdAt: '2026-07-20T08:00:00Z', updatedAt: '2026-07-20T08:00:00Z', version: 0,
};

describe('Rust clarification API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('asks with the exact GENERAL DTO and contest-scoped URL', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse(clarification, 201));

    await clarificationApi.ask(7, { scope: 'GENERAL', problemId: null, question: 'Question' });

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/contests/7/clarifications',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ scope: 'GENERAL', problemId: null, question: 'Question' }),
      }),
    ]);
  });

  it('uses direct arrays and lowercase Rust status filters', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse([clarification]))
      .mockResolvedValueOnce(jsonResponse([clarification]));

    await clarificationApi.listMine(7);
    await clarificationApi.listAll(7, 'ANSWERED');

    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/contests/7/clarifications/mine',
      '/api/contests/7/clarifications/all?status=answered',
    ]);
  });

  it('uses staff-only detail and exact reply DTO', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(clarification))
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse({ ...clarification, status: 'ANSWERED' }));

    await clarificationApi.get(9);
    await clarificationApi.reply(9, { reply: 'Answer', visibility: 'PUBLIC' });

    expect(vi.mocked(fetch).mock.calls[0][0]).toBe('/api/clarifications/9');
    expect(vi.mocked(fetch).mock.calls[2]).toEqual([
      '/api/clarifications/9/reply',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reply: 'Answer', visibility: 'PUBLIC' }),
      }),
    ]);
  });

  it('closes with no body and converts with nullable defaults', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(jsonResponse({ id: 12 }));

    await clarificationApi.close(9);
    await clarificationApi.convert(9, { title: null, body: null });

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/clarifications/9/close',
      expect.objectContaining({ method: 'POST', body: undefined }),
    ]);
    expect(vi.mocked(fetch).mock.calls[2]).toEqual([
      '/api/clarifications/9/convert',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ title: null, body: null }) }),
    ]);
  });
});
