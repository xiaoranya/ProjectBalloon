import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AwardsManageView from './AwardsManageView.vue';
import { awardsApi } from '../api/awards';
import { contestApi } from '../api/contest';

const replace = vi.fn();
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: { contestId: '7' } }),
  useRouter: () => ({ replace }),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/awards', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/awards')>();
  return {
    ...actual,
    awardsApi: {
      listCategories: vi.fn(),
      createCategory: vi.fn(),
      updateCategory: vi.fn(),
      deleteCategory: vi.fn(),
      completedRuns: vi.fn(),
      get: vi.fn(),
      generate: vi.fn(),
      candidates: vi.fn(),
      addRecipient: vi.fn(),
      removeRecipient: vi.fn(),
      freeze: vi.fn(),
      unfreeze: vi.fn(),
      csv: vi.fn(),
      certificates: vi.fn(),
    },
  };
});

const category = {
  id: 2,
  contestId: 7,
  code: 'GOLD',
  name: '金奖',
  displayOrder: 1,
  includeStar: false,
  groupName: null,
  participationType: 'OFFICIAL' as const,
  firstBlood: false,
  version: 1,
  ruleType: 'FIXED_COUNT' as const,
  ratio: null,
  fixedCount: 1,
  rankFrom: null,
  rankTo: null,
};
const set = {
  id: 3,
  contestId: 7,
  resolverRunId: 9,
  finalScoreboardSnapshotId: 12,
  status: 'DRAFT' as const,
  version: 4,
  generatedAt: '2026-07-20T08:00:00Z',
  frozenAt: null,
  conflicts: [],
  recipients: [
    {
      id: 11,
      categoryId: 2,
      categoryCode: 'GOLD',
      categoryName: '金奖',
      teamId: 8,
      teamName: 'Team Eight',
      school: 'School',
      rank: 1,
      solved: 8,
      penaltyMinutes: 600,
      participationType: 'OFFICIAL',
      groupName: null,
      isStar: false,
      isManual: false,
    },
  ],
};

describe('AwardsManageView', () => {
  beforeEach(() => {
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Contest 7' }],
    } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(awardsApi.listCategories).mockResolvedValue([category]);
    vi.mocked(awardsApi.completedRuns).mockResolvedValue([
      { id: 9, completedAt: '2026-07-20T07:00:00Z' },
    ]);
    vi.mocked(awardsApi.get).mockResolvedValue(set);
    vi.mocked(awardsApi.candidates).mockResolvedValue([
      {
        teamId: 8,
        teamName: 'Team Eight',
        school: 'School',
        rank: 1,
        participationType: 'OFFICIAL',
        groupName: null,
        isStar: false,
      },
    ]);
    vi.mocked(awardsApi.freeze).mockResolvedValue({
      ...set,
      status: 'FROZEN',
      version: 5,
      frozenAt: '2026-07-20T09:00:00Z',
    });
    vi.mocked(awardsApi.certificates).mockResolvedValue(new Blob(['certificate']));
  });

  it('loads categories, completed official runs, the award set, and candidates', async () => {
    const wrapper = mount(AwardsManageView);
    await flushPromises();
    expect(awardsApi.completedRuns).toHaveBeenCalledWith(7);
    expect(awardsApi.candidates).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('Team Eight');
    expect(wrapper.text()).toContain('金奖');
  });

  it('freezes using the displayed award-set version', async () => {
    const wrapper = mount(AwardsManageView);
    await flushPromises();
    const lock = wrapper.findAll('button').find((button) => button.text().includes('锁定名单'))!;
    await lock.trigger('click');
    await flushPromises();
    expect(awardsApi.freeze).toHaveBeenCalledWith(7, 4);
  });

  it('enables certificate export only after the award snapshot is frozen', async () => {
    const wrapper = mount(AwardsManageView);
    await flushPromises();
    expect(
      wrapper
        .findAll('button')
        .find((button) => button.text().includes('导出证书数据'))!
        .attributes('disabled'),
    ).toBeDefined();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('锁定名单'))!
      .trigger('click');
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('导出证书数据'))!
      .trigger('click');
    await flushPromises();
    expect(awardsApi.certificates).toHaveBeenCalledWith(7);
  });
});
