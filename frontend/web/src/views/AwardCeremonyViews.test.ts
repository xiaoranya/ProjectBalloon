import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AwardDisplayView from './AwardDisplayView.vue';
import AwardHostScriptView from './AwardHostScriptView.vue';
import AwardPresentationControlView from './AwardPresentationControlView.vue';
import { awardsApi, type AwardHostScript, type AwardPresentation } from '../api/awards';
import { contestApi } from '../api/contest';
import { subscribeContestEvents } from '../realtime/contest-events';

const replace = vi.fn();
vi.mock('vue-router', () => ({ useRoute: () => ({ query: { contestId: '7' } }), useRouter: () => ({ replace }) }));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/awards', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/awards')>();
  return { ...actual, awardsApi: { ...actual.awardsApi, presentation: vi.fn(), updatePresentation: vi.fn(), hostScript: vi.fn(), saveHostScript: vi.fn() } };
});
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })) }));

const presentation: AwardPresentation = {
  contestId: 7, contestName: 'Regional', contestStatus: 'ENDED', serverTime: '2026-07-22T01:00:00Z', status: 'WAITING', currentCategoryId: 1, autoRotate: false, intervalSeconds: 15, stateUpdatedAt: '2026-07-22T01:00:00Z',
  categories: [{ id: 1, code: 'GOLD', name: '金奖', displayOrder: 1, groupName: null, firstBlood: false, recipients: [{ id: 3, problemId: null, problemAlias: null, teamId: 8, teamName: 'Team Eight', school: 'School', seatNo: 'A08', groupName: null, participationType: 'OFFICIAL', star: false, rank: 1, solved: 8, penaltyMinutes: 600 }] }],
};
const hostScript: AwardHostScript = { contestId: 7, contestName: 'Regional', serverTime: presentation.serverTime, presentationStatus: 'PRESENTING', currentCategoryId: 1, nextCategoryId: null, autoRotate: false, intervalSeconds: 15, stateUpdatedAt: presentation.stateUpdatedAt, version: 2, updatedAt: presentation.serverTime, openingText: '欢迎', closingText: '结束', sections: [{ categoryId: 1, code: 'GOLD', name: '金奖', firstBlood: false, current: true, cueText: '请上台', recipients: presentation.categories[0].recipients }] };

describe('award ceremony views', () => {
  beforeEach(() => {
    replace.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({ content: [{ id: 7, name: 'Regional' }] } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(awardsApi.presentation).mockResolvedValue(presentation);
    vi.mocked(awardsApi.updatePresentation).mockResolvedValue({ ...presentation, status: 'PRESENTING' });
    vi.mocked(awardsApi.hostScript).mockResolvedValue(hostScript);
    vi.mocked(awardsApi.saveHostScript).mockResolvedValue({ ...hostScript, version: 3 });
  });

  it('controls the public presentation and subscribes to award invalidations', async () => {
    const wrapper = mount(AwardPresentationControlView); await flushPromises();
    expect(awardsApi.presentation).toHaveBeenCalledWith(7);
    expect(subscribeContestEvents).toHaveBeenCalledWith(expect.objectContaining({ contestId: 7, scope: 'PUBLIC', eventTypes: ['AWARDS_UPDATED'] }));
    await wrapper.findAll('button').find((button) => button.text().includes('开始/继续'))!.trigger('click'); await flushPromises();
    expect(awardsApi.updatePresentation).toHaveBeenCalledWith(7, expect.objectContaining({ status: 'PRESENTING', currentCategoryId: 1 }));
    wrapper.unmount();
  });

  it('renders the frozen public recipient list without authentication', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({ ...presentation, status: 'PRESENTING' });
    const wrapper = mount(AwardDisplayView); await flushPromises();
    expect(wrapper.text()).toContain('Team Eight');
    expect(wrapper.text()).toContain('Regional');
    wrapper.unmount();
  });

  it('loads the host cue sheet and saves with its displayed version', async () => {
    const wrapper = mount(AwardHostScriptView); await flushPromises();
    expect(wrapper.text()).toContain('请上台');
    const textarea = wrapper.find('textarea'); await textarea.setValue('新的开场');
    await wrapper.findAll('button').find((button) => button.text().includes('保存脚本'))!.trigger('click'); await flushPromises();
    expect(awardsApi.saveHostScript).toHaveBeenCalledWith(7, expect.objectContaining({ openingText: '新的开场', expectedVersion: 2 }));
    wrapper.unmount();
  });
});
