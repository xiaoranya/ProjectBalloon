import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { presentationApi } from './presentation';
vi.mock('./client', () => ({ apiRequest: vi.fn() }));
describe('presentation API', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());
  it('sends broadcast tokens in a header instead of the URL', async () => {
    await presentationApi.published(7, 'LIVE', 'secret'); await presentationApi.metrics(7, 'LIVE', 'secret');
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/public/presentations/7?mode=LIVE', { suppressUnauthorizedHandler: true, headers: { 'X-Broadcast-Token': 'secret' } });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/public/presentations/7/metrics?mode=LIVE', { suppressUnauthorizedHandler: true, headers: { 'X-Broadcast-Token': 'secret' } });
  });
  it('matches Java token management routes', async () => {
    await presentationApi.tokens(7); await presentationApi.revokeToken(7, 4);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/presentation-configs/7/live/tokens');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/presentation-configs/7/live/tokens/4', { method: 'DELETE' });
  });
});
