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
  roles: [],
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
