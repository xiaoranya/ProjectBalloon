import { computed } from 'vue';
import type { RouteLocationNormalized } from 'vue-router';
import { describe, expect, it, vi } from 'vitest';
import { resolveRouteGuard, type RouterGuardSession } from './guards';
import type { CurrentUserResponse, DeploymentInfo, PermissionCode } from '../api/types';

function buildUser(overrides: Partial<CurrentUserResponse> = {}): CurrentUserResponse {
  return {
    username: 'staff1',
    displayName: 'Staff',
    userType: 'STAFF',
    permissions: [],
    passwordResetRequired: false,
    ...overrides,
  } as CurrentUserResponse;
}

function buildSession(
  overrides: { user?: CurrentUserResponse | null; deploymentMode?: string } = {},
): RouterGuardSession {
  const user = overrides.user === undefined ? buildUser() : overrides.user;
  const mode = overrides.deploymentMode ?? 'standard';
  return {
    initialize: vi.fn(async () => undefined),
    state: {
      deployment: { mode } as DeploymentInfo,
      user,
    },
    isAuthenticated: computed(() => user !== null),
    isTeam: computed(() => user?.userType === 'TEAM'),
    isStaff: computed(() => user !== null && !['TEAM', 'INDIVIDUAL'].includes(user.userType)),
    isSuperAdmin: computed(() => user?.userType === 'SUPER_ADMIN'),
    hasPermission: (permission: string) =>
      user?.userType === 'SUPER_ADMIN' ||
      user?.permissions.includes(permission as PermissionCode) === true,
  };
}

function buildRoute(
  meta: RouteLocationNormalized['meta'],
  overrides: Partial<Pick<RouteLocationNormalized, 'name' | 'fullPath'>> = {},
): RouteLocationNormalized {
  return {
    name: overrides.name ?? 'target',
    fullPath: overrides.fullPath ?? '/target',
    meta,
  } as RouteLocationNormalized;
}

describe('resolveRouteGuard', () => {
  it('awaits session initialization before deciding', async () => {
    const session = buildSession();
    await resolveRouteGuard(buildRoute({}), session);
    expect(session.initialize).toHaveBeenCalledOnce();
  });

  describe('deployment-mode redirects', () => {
    it('sends authenticated users to contests for daily-only pages in competition mode', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ dailyOnly: true }),
        buildSession({ deploymentMode: 'competition' }),
      );
      expect(result).toEqual({ name: 'contests' });
    });

    it('sends unauthenticated users to login for daily-only pages in competition mode', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ dailyOnly: true }),
        buildSession({ user: null, deploymentMode: 'competition' }),
      );
      expect(result).toEqual({ name: 'login' });
    });

    it('allows daily-only pages in standard mode', async () => {
      const result = await resolveRouteGuard(buildRoute({ dailyOnly: true }), buildSession());
      expect(result).toBe(true);
    });

    it('redirects competition-only pages to the admin home in standard mode', async () => {
      const result = await resolveRouteGuard(buildRoute({ competitionOnly: true }), buildSession());
      expect(result).toEqual({ name: 'admin-home' });
    });

    it('allows competition-only pages in competition mode', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ competitionOnly: true }),
        buildSession({ deploymentMode: 'competition' }),
      );
      expect(result).toBe(true);
    });
  });

  describe('authentication redirects', () => {
    it('redirects unauthenticated users to login and preserves the redirect target', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true }, { fullPath: '/contests/3' }),
        buildSession({ user: null }),
      );
      expect(result).toEqual({ name: 'login', query: { redirect: '/contests/3' } });
    });

    it('lands unauthenticated users on admin-login for staff routes', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresStaff: true }, { fullPath: '/balloon' }),
        buildSession({ user: null }),
      );
      expect(result).toEqual({ name: 'admin-login', query: { redirect: '/balloon' } });
    });

    it('treats permission-guarded routes as staff routes for the login landing', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiredPermission: 'AWARD_MANAGE' }),
        buildSession({ user: null }),
      );
      expect(result).toEqual({ name: 'admin-login', query: { redirect: '/target' } });
    });

    it('allows authenticated users through auth-required routes', async () => {
      const result = await resolveRouteGuard(buildRoute({ requiresAuth: true }), buildSession());
      expect(result).toBe(true);
    });
  });

  describe('forced password reset', () => {
    it('funnels users with a pending password reset to the change-password page', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresTeam: true }),
        buildSession({ user: buildUser({ passwordResetRequired: true }) }),
      );
      expect(result).toEqual({ name: 'change-password' });
    });

    it('lets the change-password page through for users with a pending reset', async () => {
      const result = await resolveRouteGuard(
        buildRoute(
          { requiresAuth: true },
          { name: 'change-password', fullPath: '/change-password' },
        ),
        buildSession({ user: buildUser({ passwordResetRequired: true }) }),
      );
      expect(result).toBe(true);
    });
  });

  describe('role and permission checks', () => {
    it('rejects non-team accounts on team routes', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresTeam: true }),
        buildSession(),
      );
      expect(result).toEqual({ name: 'forbidden' });
    });

    it('allows team accounts on team routes', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresTeam: true }),
        buildSession({ user: buildUser({ userType: 'TEAM' }) }),
      );
      expect(result).toBe(true);
    });

    it('rejects non-staff accounts on staff routes', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresStaff: true }),
        buildSession({ user: buildUser({ userType: 'TEAM' }) }),
      );
      expect(result).toEqual({ name: 'forbidden' });
    });

    it('rejects non-super-admins on super-admin routes', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresSuperAdmin: true }),
        buildSession(),
      );
      expect(result).toEqual({ name: 'forbidden' });
    });

    it('rejects accounts without the required permission', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresStaff: true, requiredPermission: 'AWARD_MANAGE' }),
        buildSession(),
      );
      expect(result).toEqual({ name: 'forbidden' });
    });

    it('allows accounts holding the required permission', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiresStaff: true, requiredPermission: 'AWARD_MANAGE' }),
        buildSession({
          user: buildUser({ permissions: ['AWARD_MANAGE' as PermissionCode] }),
        }),
      );
      expect(result).toBe(true);
    });

    it('allows super-admins regardless of the permission list', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ requiresAuth: true, requiredPermission: 'SCREEN_MANAGE' }),
        buildSession({ user: buildUser({ userType: 'SUPER_ADMIN' }) }),
      );
      expect(result).toBe(true);
    });
  });

  describe('guest-only routes', () => {
    it('sends authenticated staff users to their permission home', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ guestOnly: true }),
        buildSession({ user: buildUser({ userType: 'SUPER_ADMIN' }) }),
      );
      expect(result).toBe('/admin');
    });

    it('sends authenticated team users to the contest list', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ guestOnly: true }),
        buildSession({ user: buildUser({ userType: 'TEAM' }) }),
      );
      expect(result).toBe('/contests');
    });

    it('allows unauthenticated users on guest-only pages', async () => {
      const result = await resolveRouteGuard(
        buildRoute({ guestOnly: true }),
        buildSession({ user: null }),
      );
      expect(result).toBe(true);
    });
  });
});
