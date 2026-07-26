import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminContestListView from './AdminContestListView.vue';

const { listContests } = vi.hoisted(() => ({ listContests: vi.fn() }));
vi.mock('../api/admin-contests', () => ({
  adminContestApi: {
    listContests,
    createContest: vi.fn(),
    updateContest: vi.fn(),
  },
}));
vi.mock('../auth/session', () => ({
  useSession: () => ({ isSuperAdmin: { value: true } }),
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }));

describe('AdminContestListView', () => {
  beforeEach(() => {
    listContests.mockResolvedValue({
      content: [{
        id: 42,
        name: 'Rust Regional',
        status: 'DRAFT',
        visibility: 'PRIVATE',
        startAt: null,
        freezeAt: null,
        endAt: null,
        version: 0,
        createdAt: '2026-07-20T00:00:00Z',
        updatedAt: '2026-07-20T00:00:00Z',
        deletedAt: null,
      }],
      page: 0,
      size: 25,
      totalElements: 1,
      totalPages: 1,
    });
  });

  it('loads a paged contest list without exposing an unavailable clone action', async () => {
    const wrapper = mount(AdminContestListView);
    await flushPromises();

    expect(listContests).toHaveBeenCalledWith(0, 25);
    expect(wrapper.text()).toContain('Rust Regional');
    expect(wrapper.text()).not.toContain('克隆');
  });
});
