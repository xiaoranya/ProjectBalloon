import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminAnnouncementsView from './AdminAnnouncementsView.vue';

const mocks = vi.hoisted(() => ({ list: vi.fn(), push: vi.fn() }));
vi.mock('../api/announcements', () => ({ announcementApi: {
  list: mocks.list,
  create: vi.fn(),
  update: vi.fn(),
  schedule: vi.fn(),
  cancel: vi.fn(),
  pin: vi.fn(),
  withdraw: vi.fn(),
} }));
vi.mock('vue-router', () => ({
  useRoute: () => ({ params: { contestId: '7' } }),
  useRouter: () => ({ push: mocks.push }),
}));

describe('AdminAnnouncementsView', () => {
  beforeEach(() => {
    mocks.list.mockResolvedValue([
      {
        id: 9, contestId: 7, title: '赛前提醒', body: '十分钟后封榜', pinned: true,
        status: 'SCHEDULED', createdByUserId: 1, publishedAt: null,
        scheduledAt: '2026-07-22T10:00:00Z', withdrawnAt: null, withdrawnByUserId: null,
        sourceClarificationId: null, cancelledAt: null, cancelledByUserId: null,
        createdAt: '2026-07-22T08:00:00Z', updatedAt: '2026-07-22T08:00:00Z', version: 0,
      },
    ]);
  });

  it('loads complete manager history and exposes scheduled actions', async () => {
    const wrapper = mount(AdminAnnouncementsView);
    await flushPromises();

    expect(mocks.list).toHaveBeenCalledWith(7, true);
    expect(wrapper.text()).toContain('赛前提醒');
    expect(wrapper.text()).toContain('待发布');
    expect(wrapper.text()).toContain('编辑计划');
    expect(wrapper.text()).toContain('取消计划');
  });
});
