import { RouterLinkStub, flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminLayout from './AdminLayout.vue';
import AwardsLayout from './AwardsLayout.vue';
import BalloonLayout from './BalloonLayout.vue';
import ContestantLayout from './ContestantLayout.vue';
import JudgeLayout from './JudgeLayout.vue';
import PrinterLayout from './PrinterLayout.vue';
import ResolverLayout from './ResolverLayout.vue';
import { contestApi } from '../api/contest';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  logout: vi.fn(),
  push: vi.fn(),
  replace: vi.fn(),
}));

interface SessionUser {
  displayName: string;
  userType: string;
}
let sessionUser: SessionUser | null = null;
let deploymentMode = 'standard';
let canManageContests = true;
let isSuperAdmin = true;
vi.mock('../auth/session', () => ({
  useSession: () => ({
    state: { user: sessionUser, deployment: { mode: deploymentMode } },
    canManageContests: { value: canManageContests },
    isSuperAdmin: { value: isSuperAdmin },
    logout: mocks.logout,
  }),
}));
const elementMocks = vi.hoisted(() => ({ success: vi.fn() }));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return { ...actual, ElMessage: { success: elementMocks.success } };
});
vi.mock('../api/contest', () => ({ contestApi: { getContest: vi.fn() } }));
let route = { params: { contestId: '7' }, path: '/admin' };
vi.mock('vue-router', () => ({
  useRoute: () => route,
  useRouter: () => ({ push: mocks.push, replace: mocks.replace }),
}));

const contest = {
  id: 7,
  name: 'Spring Finals',
  status: 'RUNNING' as const,
  visibility: 'PUBLIC' as const,
  startAt: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
  freezeAt: null,
  endAt: new Date(Date.now() + 3 * 60 * 60 * 1000).toISOString(),
  version: 1,
  createdAt: '',
  updatedAt: '',
  deletedAt: null,
};

const layoutStubs = { RouterLink: RouterLinkStub, RouterView: true };

describe('layouts', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    route = { params: { contestId: '7' }, path: '/admin' };
    mocks.logout.mockReset();
    mocks.push.mockReset();
    mocks.replace.mockReset();
    elementMocks.success.mockReset();
    sessionUser = { displayName: 'Alice', userType: 'SUPER_ADMIN' };
    deploymentMode = 'standard';
    canManageContests = true;
    isSuperAdmin = true;
  });

  it('AdminLayout renders the full navigation for super admins and logs out', async () => {
    const wrapper = mount(AdminLayout, { global: { stubs: layoutStubs } });
    await flushPromises();
    expect(wrapper.text()).toContain('赛事管理控制台');
    expect(wrapper.text()).toContain('健康与审计');
    expect(wrapper.text()).toContain('比赛管理');
    expect(wrapper.text()).toContain('队伍批量导入');
    expect(wrapper.text()).not.toContain('终端绑定');
    expect(wrapper.text()).toContain('题库管理');
    expect(wrapper.text()).toContain('日常练习');
    expect(wrapper.text()).toContain('工作人员账号');
    expect(wrapper.text()).toContain('赛事管理范围');
    expect(wrapper.text()).toContain('Alice');
    expect(wrapper.text()).toContain('超级管理员');
    await wrapper.find('button[aria-label="退出登录"]').trigger('click');
    await flushPromises();
    expect(mocks.logout).toHaveBeenCalled();
    expect(mocks.replace).toHaveBeenCalledWith('/admin/login');
    wrapper.unmount();
  });

  it('AdminLayout hides restricted navigation for staff in competition mode', async () => {
    isSuperAdmin = false;
    canManageContests = false;
    sessionUser = { displayName: 'Bob', userType: 'STAFF' };
    deploymentMode = 'competition';
    const wrapper = mount(AdminLayout, { global: { stubs: layoutStubs } });
    await flushPromises();
    expect(wrapper.text()).toContain('健康与审计');
    expect(wrapper.text()).not.toContain('比赛管理');
    expect(wrapper.text()).not.toContain('题库管理');
    expect(wrapper.text()).toContain('工作人员');
    wrapper.unmount();
  });

  it('ContestantLayout loads the contest and renders the countdown navigation', async () => {
    vi.mocked(contestApi.getContest).mockResolvedValue(contest);
    const wrapper = mount(ContestantLayout, { global: { stubs: layoutStubs } });
    await flushPromises();
    expect(contestApi.getContest).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('Spring Finals');
    expect(wrapper.text()).toContain('分钟后结束');
    const links = wrapper.findAllComponents(RouterLinkStub);
    expect(links.map((link) => link.props('to'))).toEqual([
      '/contests/7/problems',
      '/contests/7/submissions',
      '/contests/7/clarifications',
      '/contests/7/printing',
      '/contests/7/scoreboard',
    ]);
    wrapper.unmount();
  });

  it('ContestantLayout shows load errors and handles the user menu commands', async () => {
    vi.mocked(contestApi.getContest).mockRejectedValue(new Error('contest service unavailable'));
    const wrapper = mount(ContestantLayout, { global: { stubs: layoutStubs } });
    await flushPromises();
    expect(wrapper.text()).toContain('contest service unavailable');
    const dropdown = wrapper.findComponent({ name: 'ElDropdown' });
    (dropdown.vm as unknown as { $emit: (event: string, command: string) => void }).$emit(
      'command',
      'contests',
    );
    await flushPromises();
    expect(mocks.push).toHaveBeenCalledWith('/contests');
    (dropdown.vm as unknown as { $emit: (event: string, command: string) => void }).$emit(
      'command',
      'logout',
    );
    await flushPromises();
    expect(mocks.logout).toHaveBeenCalled();
    expect(elementMocks.success).toHaveBeenCalled();
    expect(mocks.push).toHaveBeenCalledWith('/login');
    wrapper.unmount();
  });

  it('simple role layouts render their brand and log out', async () => {
    const cases = [
      { component: AwardsLayout, brand: '奖项管理' },
      { component: BalloonLayout, brand: '气球工作台' },
      { component: JudgeLayout, brand: '裁判工作台' },
      { component: PrinterLayout, brand: '打印工作台' },
      { component: ResolverLayout, brand: 'Resolver 控制台' },
    ];
    for (const item of cases) {
      const wrapper = mount(item.component, { global: { stubs: layoutStubs } });
      await flushPromises();
      expect(wrapper.text()).toContain(item.brand);
      expect(wrapper.text()).toContain('Alice');
      await wrapper
        .findAll('button')
        .find((row) => row.text().includes('退出登录'))!
        .trigger('click');
      await flushPromises();
      expect(mocks.logout).toHaveBeenCalled();
      expect(mocks.replace).toHaveBeenCalledWith('/admin/login');
      wrapper.unmount();
    }
  });
});
