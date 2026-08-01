import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ContestListView from './ContestListView.vue';

const mocks = vi.hoisted(() => ({
  listContests: vi.fn(),
  push: vi.fn(),
  logout: vi.fn(),
}));

vi.mock('../api/contest', () => ({ contestApi: { listContests: mocks.listContests } }));
vi.mock('../auth/session', () => ({
  useSession: () => ({ state: { user: { displayName: 'Alice' } }, logout: mocks.logout }),
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ push: mocks.push }) }));

const contest = {
  id: 7,
  name: 'Spring Finals',
  status: 'RUNNING' as const,
  visibility: 'PUBLIC' as const,
  startAt: '2026-08-01T08:00:00Z',
  freezeAt: null,
  endAt: '2026-08-01T12:00:00Z',
  version: 1,
  createdAt: '',
  updatedAt: '',
  deletedAt: null,
};

describe('ContestListView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listContests.mockResolvedValue({ content: [contest], page: 0, size: 50, totalElements: 1, totalPages: 1 });
    mocks.logout.mockResolvedValue(undefined);
  });

  it('loads accessible contests and enters the selected contest', async () => {
    const wrapper = mount(ContestListView);
    await flushPromises();

    expect(mocks.listContests).toHaveBeenCalledWith(0, 50);
    expect(wrapper.text()).toContain('Alice');
    expect(wrapper.text()).toContain('Spring Finals');
    await wrapper.findAll('button').find((button) => button.text().includes('进入比赛'))!.trigger('click');
    expect(mocks.push).toHaveBeenCalledWith('/contests/7/problems');
  });

  it('logs out before navigating to login', async () => {
    const wrapper = mount(ContestListView);
    await flushPromises();
    await wrapper.findAll('button').find((button) => button.text().includes('退出登录'))!.trigger('click');
    await flushPromises();

    expect(mocks.logout).toHaveBeenCalledOnce();
    expect(mocks.push).toHaveBeenCalledWith('/login');
  });
});
