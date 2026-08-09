import { describe, expect, it } from 'vitest';
import { homeForUser } from './access';

describe('permission landing routes', () => {
  it('keeps contestant identities on their own applications', () => {
    expect(homeForUser({ userType: 'TEAM', permissions: [] })).toBe('/contests');
    expect(homeForUser({ userType: 'INDIVIDUAL', permissions: [] })).toBe('/practice');
  });

  it('lands super administrators on the admin application', () => {
    expect(homeForUser({ userType: 'SUPER_ADMIN', permissions: [] })).toBe('/admin');
  });

  it.each([
    ['CONTEST_MANAGE', '/admin'],
    ['CLARIFICATION_MANAGE', '/judge'],
    ['PRINTING_MANAGE', '/printer'],
    ['BALLOON_MANAGE', '/balloon'],
    ['RESOLVER_MANAGE', '/resolver'],
    ['AWARD_MANAGE', '/awards'],
    ['SCREEN_MANAGE', '/screen/manage'],
    ['LIVE_MANAGE', '/live/manage'],
  ] as const)('uses %s as a staff landing capability', (permission, route) => {
    expect(homeForUser({ userType: 'STAFF', permissions: [permission] })).toBe(route);
  });

  it('uses a deterministic priority for accounts with multiple permissions', () => {
    expect(
      homeForUser({
        userType: 'STAFF',
        permissions: ['PRINTING_MANAGE', 'CLARIFICATION_MANAGE'],
      }),
    ).toBe('/judge');
  });
});
