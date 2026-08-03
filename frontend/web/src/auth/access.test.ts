import { describe, expect, it } from 'vitest';
import { homeForUserType, staffHomeByUserType } from './access';

const staffTypes = [
  'SUPER_ADMIN',
  'CONTEST_ADMIN',
  'JUDGE',
  'PRINTER',
  'BALLOON_STAFF',
  'AWARD_OPERATOR',
  'RESOLVER_OPERATOR',
  'SCREEN_OPERATOR',
  'LIVE_OPERATOR',
] as const;

describe('role landing routes', () => {
  it('keeps teams on the contestant route', () => {
    expect(homeForUserType('TEAM')).toBe('/contests');
  });

  it('lands judges on the implemented clarification desk', () => {
    expect(homeForUserType('JUDGE')).toBe('/judge');
    expect(staffHomeByUserType.JUDGE).toBe('/judge');
  });

  it('lands printers on the implemented printing desk', () => {
    expect(homeForUserType('PRINTER')).toBe('/printer');
    expect(staffHomeByUserType.PRINTER).toBe('/printer');
  });

  it('lands balloon staff on the delivery desk', () => {
    expect(homeForUserType('BALLOON_STAFF')).toBe('/balloon');
    expect(staffHomeByUserType.BALLOON_STAFF).toBe('/balloon');
  });

  it('lands resolver operators on the Resolver console', () => {
    expect(homeForUserType('RESOLVER_OPERATOR')).toBe('/resolver');
    expect(staffHomeByUserType.RESOLVER_OPERATOR).toBe('/resolver');
  });

  it('lands award operators on the awards workspace', () => {
    expect(homeForUserType('AWARD_OPERATOR')).toBe('/awards');
    expect(staffHomeByUserType.AWARD_OPERATOR).toBe('/awards');
  });

  it('lands screen operators on the screen control workspace', () => {
    expect(homeForUserType('SCREEN_OPERATOR')).toBe('/screen/manage');
    expect(staffHomeByUserType.SCREEN_OPERATOR).toBe('/screen/manage');
  });

  it('lands live operators on the broadcast control workspace', () => {
    expect(homeForUserType('LIVE_OPERATOR')).toBe('/live/manage');
    expect(staffHomeByUserType.LIVE_OPERATOR).toBe('/live/manage');
  });

  it.each(
    staffTypes.filter(
      (userType) =>
        ![
          'JUDGE',
          'PRINTER',
          'BALLOON_STAFF',
          'RESOLVER_OPERATOR',
          'AWARD_OPERATOR',
          'SCREEN_OPERATOR',
          'LIVE_OPERATOR',
        ].includes(userType),
    ),
  )('keeps %s on the implemented admin page', (userType) => {
    expect(homeForUserType(userType)).toBe('/admin');
    expect(staffHomeByUserType[userType]).toBe('/admin');
  });
});
