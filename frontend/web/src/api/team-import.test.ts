import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearCsrfToken } from './client';
import { teamImportApi } from './team-import';

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });
}

const row = {
  name: 'Alpha', school: 'University', seatNo: 'A-01', groupName: 'Regional', star: false,
  username: 'alpha', initialPassword: 'ChangeMe123!',
};

describe('teamImportApi', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('sends the exact Rust batch fields and idempotency key', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ batchId: 'batch-1', totalRequested: 1, created: [] }));

    await teamImportApi.importTeams({
      teams: [row], contestId: 42, participationType: 'OFFICIAL', idempotencyKey: 'import-part-1',
    });

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/teams/batch',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ teams: [row], contestId: 42, participationType: 'OFFICIAL', idempotencyKey: 'import-part-1' }),
      }),
    ]);
  });

  it('rejects more than 100 rows before making a request', () => {
    expect(() => teamImportApi.importTeams({
      teams: Array.from({ length: 101 }, () => row), contestId: 42, participationType: 'OFFICIAL', idempotencyKey: 'key',
    })).toThrow('1–100');
    expect(fetch).not.toHaveBeenCalled();
  });

  it('validates a nonblank idempotency key of at most 128 characters', () => {
    expect(() => teamImportApi.importTeams({ teams: [row], contestId: null, participationType: null, idempotencyKey: ' ' })).toThrow('幂等键');
    expect(() => teamImportApi.importTeams({ teams: [row], contestId: null, participationType: null, idempotencyKey: 'x'.repeat(129) })).toThrow('幂等键');
  });

  it('posts member fields to the separate Rust member endpoint', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ id: 3, teamId: 2, name: 'Alice' }));

    await teamImportApi.addMember(2, { name: 'Alice', email: 'a@example.com', phone: null, roleName: '队员' });

    expect(vi.mocked(fetch).mock.calls[1][0]).toBe('/api/teams/2/members');
    expect(vi.mocked(fetch).mock.calls[1][1]).toEqual(expect.objectContaining({
      body: JSON.stringify({ name: 'Alice', email: 'a@example.com', phone: null, roleName: '队员' }),
    }));
  });
});
