import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSession } from './session';

const mocks = vi.hoisted(() => ({
  apiRequest: vi.fn(),
  clearCsrfToken: vi.fn(),
  setUnauthorizedHandler: vi.fn(),
}));

vi.mock('../api/client', () => ({
  apiRequest: mocks.apiRequest,
  clearCsrfToken: mocks.clearCsrfToken,
  setUnauthorizedHandler: mocks.setUnauthorizedHandler,
}));

const user = {
  id: 4,
  username: 'alice',
  displayName: 'Alice',
  userType: 'INDIVIDUAL' as const,
  permissions: [],
  passwordResetRequired: false,
};

describe('session state', () => {
  const session = useSession();

  beforeEach(() => {
    mocks.apiRequest.mockReset();
    mocks.clearCsrfToken.mockReset();
    session.clearSession();
  });

  it('stores the authenticated user returned by login and forwards caller input', async () => {
    mocks.apiRequest.mockResolvedValue(user);

    await session.login(' alice ', 'secret');

    expect(mocks.apiRequest).toHaveBeenCalledWith('/api/auth/login', {
      method: 'POST',
      body: { username: ' alice ', password: 'secret' },
    });
    expect(session.state.user).toEqual(user);
    expect(session.isAuthenticated.value).toBe(true);
    expect(session.isIndividual.value).toBe(true);
  });

  it('updates profile and clears state even when logout request fails', async () => {
    mocks.apiRequest.mockResolvedValueOnce({ ...user, displayName: 'New Name' });
    await session.updateProfile('New Name');
    expect(session.state.user?.displayName).toBe('New Name');
    expect(mocks.apiRequest).toHaveBeenCalledWith('/api/auth/profile', {
      method: 'PATCH',
      body: { displayName: 'New Name' },
    });

    mocks.apiRequest.mockRejectedValueOnce(new Error('network down'));
    await expect(session.logout()).rejects.toThrow('network down');
    expect(session.state.user).toBeNull();
    expect(session.isAuthenticated.value).toBe(false);
    expect(mocks.clearCsrfToken).toHaveBeenCalled();
  });

  it('routes unauthorized callbacks through clearSession', async () => {
    expect(mocks.setUnauthorizedHandler).toHaveBeenCalledWith(expect.any(Function));
    mocks.apiRequest.mockResolvedValue(user);
    await session.login('alice', 'secret');
    const handler = mocks.setUnauthorizedHandler.mock.calls[0][0] as () => void;
    handler();
    expect(session.state.user).toBeNull();
    expect(session.state.initialized).toBe(true);
  });
});

describe('boot initialization', () => {
  beforeEach(() => {
    mocks.apiRequest.mockReset();
    mocks.clearCsrfToken.mockReset();
  });

  it('re-probes on the next navigation after a failed boot and succeeds later', async () => {
    vi.resetModules();
    const { useSession: freshUseSession } = await import('./session');
    const fresh = freshUseSession();

    mocks.apiRequest.mockRejectedValueOnce(new Error('network down'));
    await fresh.initialize();
    expect(fresh.state.initialized).toBe(false);
    expect(fresh.state.user).toBeNull();

    mocks.apiRequest
      .mockResolvedValueOnce({ mode: 'competition', activeContest: null })
      .mockResolvedValueOnce(user);
    await fresh.initialize();
    expect(fresh.state.initialized).toBe(true);
    expect(fresh.state.user).toEqual(user);
    expect(fresh.state.deployment.mode).toBe('competition');
  });

  it('does not stack concurrent probes while a failed probe is retried', async () => {
    vi.resetModules();
    const { useSession: freshUseSession } = await import('./session');
    const fresh = freshUseSession();

    mocks.apiRequest.mockRejectedValue(new Error('network down'));
    await Promise.all([fresh.initialize(), fresh.initialize(), fresh.initialize()]);
    expect(mocks.apiRequest).toHaveBeenCalledTimes(1);
    expect(fresh.state.initialized).toBe(false);

    await fresh.initialize();
    expect(mocks.apiRequest).toHaveBeenCalledTimes(2);
  });
});
