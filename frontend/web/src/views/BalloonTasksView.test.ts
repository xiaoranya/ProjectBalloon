import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import BalloonTasksView from './BalloonTasksView.vue';
import { balloonApi } from '../api/balloons';
import { contestApi } from '../api/contest';
import { subscribeContestEvents } from '../realtime/contest-events';

const replace = vi.fn();
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: { contestId: '7' } }),
  useRouter: () => ({ replace }),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/balloons', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/balloons')>();
  return { ...actual, balloonApi: { list: vi.fn(), stats: vi.fn(), claim: vi.fn(), deliver: vi.fn(), cancel: vi.fn(), reopen: vi.fn(), note: vi.fn() } };
});
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })) }));

const task = {
  id: 9, contestId: 7, teamId: 2, problemId: 3, submissionId: 4, color: '#ff0000', isFirstBlood: true,
  status: 'PENDING' as const, seatNo: 'A01', teamName: 'Team Red', problemAlias: 'A', note: null,
  claimedByUserId: null, claimedAt: null, deliveredAt: null, cancelledAt: null, cancelledReason: null,
  createdAt: '2026-07-20T08:00:00Z', updatedAt: '2026-07-20T08:00:00Z', version: 0, reopenedCount: 0,
};
const stats = { total: 1, pending: 1, claimed: 0, delivered: 0, cancelled: 0, firstBlood: 1 };

describe('BalloonTasksView', () => {
  beforeEach(() => {
    replace.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({ content: [{ id: 7, name: 'Contest 7' }] } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(balloonApi.list).mockResolvedValue([task]);
    vi.mocked(balloonApi.stats).mockResolvedValue(stats);
    vi.mocked(balloonApi.claim).mockResolvedValue({ ...task, status: 'CLAIMED', claimedByUserId: 6, version: 1 });
  });

  it('loads the selected contest and subscribes to balloon events', async () => {
    const wrapper = mount(BalloonTasksView);
    await flushPromises();
    expect(balloonApi.list).toHaveBeenCalledWith(7, undefined);
    expect(wrapper.text()).toContain('Team Red');
    expect(wrapper.text()).toContain('First Blood');
    expect(subscribeContestEvents).toHaveBeenCalledWith(expect.objectContaining({ contestId: 7, scope: 'STAFF', eventTypes: ['BALLOON_TASK_UPDATED'] }));
    wrapper.unmount();
  });

  it('claims with the displayed optimistic-lock version', async () => {
    const wrapper = mount(BalloonTasksView, { attachTo: document.body });
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');
    await flushPromises();
    const claim = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent?.includes('领取任务')) as HTMLButtonElement;
    claim.click();
    await flushPromises();
    expect(balloonApi.claim).toHaveBeenCalledWith(9, 0);
    wrapper.unmount();
  });
});
