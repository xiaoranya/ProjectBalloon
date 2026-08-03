import { describe, expect, it } from 'vitest';
import { routes } from './routes';

describe('clarification routing', () => {
  it('allows authenticated individual users to access public training', () => {
    const training = routes.find((route) => route.path === '/training');
    expect(training?.meta).toMatchObject({ requiresAuth: true });
    expect(training?.meta?.requiresTeam).not.toBe(true);
  });

  it('adds a team-only contest clarification route', () => {
    const contestant = routes.find((route) => route.path === '/contests/:contestId');
    expect(contestant?.meta).toMatchObject({ requiresAuth: true, requiresTeam: true });
    expect(
      contestant?.children?.find((child) => child.name === 'contest-clarifications'),
    ).toMatchObject({
      path: 'clarifications',
    });
  });

  it('retains clarification and adds printing to the team contest route', () => {
    const contestant = routes.find((route) => route.path === '/contests/:contestId');
    expect(
      contestant?.children?.find((child) => child.name === 'contest-clarifications'),
    ).toMatchObject({ path: 'clarifications' });
    expect(contestant?.children?.find((child) => child.name === 'contest-printing')).toMatchObject({
      path: 'printing',
    });
  });

  it('adds the judge workspace only behind the JUDGE role guard', () => {
    const judge = routes.find((route) => route.path === '/judge');
    expect(judge?.meta).toMatchObject({
      requiresAuth: true,
      requiresStaff: true,
      requiresJudge: true,
    });
    expect(judge?.children?.find((child) => child.name === 'judge-home')).toBeDefined();
  });

  it('adds the printer workspace only behind the PRINTER role guard', () => {
    const printer = routes.find((route) => route.path === '/printer');
    expect(printer?.meta).toMatchObject({
      requiresAuth: true,
      requiresStaff: true,
      requiresPrinter: true,
    });
    expect(printer?.children?.find((child) => child.name === 'printer-home')).toBeDefined();
  });

  it('adds the balloon workspace behind the BALLOON_STAFF role guard', () => {
    const balloon = routes.find((route) => route.path === '/balloon');
    expect(balloon?.meta).toMatchObject({
      requiresAuth: true,
      requiresStaff: true,
      requiresBalloonStaff: true,
    });
    expect(balloon?.children?.find((child) => child.name === 'balloon-home')).toBeDefined();
  });

  it('guards Resolver controls while keeping the official display public', () => {
    const resolver = routes.find((route) => route.path === '/resolver');
    expect(resolver?.meta).toMatchObject({
      requiresAuth: true,
      requiresStaff: true,
      requiresResolverOperator: true,
    });
    expect(resolver?.children?.find((child) => child.name === 'resolver-home')).toBeDefined();
    expect(
      routes.find((route) => route.name === 'resolver-display')?.meta?.requiresAuth,
    ).toBeUndefined();
  });

  it('guards the awards workspace for award operators', () => {
    const awards = routes.find((route) => route.path === '/awards');
    expect(awards?.meta).toMatchObject({
      requiresAuth: true,
      requiresStaff: true,
      requiresAwardOperator: true,
    });
    expect(awards?.children?.find((child) => child.name === 'awards-home')).toBeDefined();
    expect(awards?.children?.find((child) => child.name === 'awards-presentation')).toBeDefined();
    expect(awards?.children?.find((child) => child.name === 'awards-host-script')).toBeDefined();
    expect(
      routes.find((route) => route.name === 'awards-display')?.meta?.requiresAuth,
    ).toBeUndefined();
  });

  it('guards screen controls while keeping the registered screen client public', () => {
    expect(routes.find((route) => route.name === 'screen-manage')?.meta).toMatchObject({
      requiresAuth: true,
      requiresScreenOperator: true,
    });
    expect(
      routes.find((route) => route.name === 'screen-client')?.meta?.requiresAuth,
    ).toBeUndefined();
  });
});

describe('admin bulk rejudge routing', () => {
  it('uses a contest-scoped route guarded for SUPER_ADMIN and CONTEST_ADMIN managers', () => {
    const admin = routes.find((route) => route.path === '/admin');
    const route = admin?.children?.find((child) => child.name === 'admin-contest-rejudge-tasks');

    expect(route).toMatchObject({
      path: 'contests/:contestId/rejudge-tasks',
      meta: { requiresAdmin: true },
    });
    expect(route?.meta?.requiresSuperAdmin).not.toBe(true);
  });

  it('allows both administrator types to open an existing scoped problem editor', () => {
    const admin = routes.find((route) => route.path === '/admin');
    const editor = admin?.children?.find((child) => child.name === 'admin-problem-editor');
    const create = admin?.children?.find((child) => child.name === 'admin-problem-new');

    expect(editor?.meta).toMatchObject({ requiresAdmin: true });
    expect(editor?.meta?.requiresSuperAdmin).not.toBe(true);
    expect(create?.meta).toMatchObject({ requiresSuperAdmin: true });
  });
});
