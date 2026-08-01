import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import SubmissionListView from './SubmissionListView.vue';

const mocks = vi.hoisted(() => ({ listSubmissions: vi.fn(), push: vi.fn() }));
const route = { params: { contestId: '7' } };
vi.mock('../api/contest', () => ({ contestApi: { listSubmissions: mocks.listSubmissions } }));
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push: mocks.push }) }));

const page = {
  content: [
    { id: 9, contestId: 7, problemId: 1, problemAlias: 'A', teamId: 2, teamName: 'Blue Team', language: 'cpp', sourceSizeBytes: 12, status: 'JUDGING', submittedAt: '2026-08-01T09:00:00Z', judgedAt: null, activeJudgementId: null, verdict: null, totalTimeMs: null, peakMemoryKb: null, scoreMilli: null },
    { id: 8, contestId: 7, problemId: 1, problemAlias: 'A', teamId: 2, teamName: 'Blue Team', language: 'cpp', sourceSizeBytes: 12, status: 'ACCEPTED', submittedAt: '2026-08-01T08:00:00Z', judgedAt: '2026-08-01T08:01:00Z', activeJudgementId: 'j-8', verdict: 'ACCEPTED', totalTimeMs: 1, peakMemoryKb: 2, scoreMilli: 100_000 },
  ],
  page: 0,
  size: 30,
  totalElements: 2,
  totalPages: 1,
};

describe('SubmissionListView', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    mocks.listSubmissions.mockResolvedValue(page);
  });
  afterEach(() => vi.useRealTimers());

  it('loads submissions and schedules refresh while judging is unfinished', async () => {
    const wrapper = mount(SubmissionListView);
    await flushPromises();

    expect(mocks.listSubmissions).toHaveBeenCalledWith(7, 0);
    expect(wrapper.text()).toContain('AC++');
    expect(wrapper.text()).toContain('判题中');
    vi.advanceTimersByTime(4_000);
    await flushPromises();
    expect(mocks.listSubmissions).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });

  it('navigates to the clicked submission detail', async () => {
    const wrapper = mount(SubmissionListView);
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');

    expect(mocks.push).toHaveBeenCalledWith('/contests/7/submissions/9');
    wrapper.unmount();
  });
});
