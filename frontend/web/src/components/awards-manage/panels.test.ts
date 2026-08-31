import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AwardCategoriesPanel from './AwardCategoriesPanel.vue';
import AwardMetricsRow from './AwardMetricsRow.vue';
import AwardRecipientsPanel from './AwardRecipientsPanel.vue';
import AwardsCommandBar from './AwardsCommandBar.vue';
import {
  awardsApi,
  type AwardCategory,
  type AwardCandidate,
  type AwardRecipient,
  type AwardSet,
} from '../../api/awards';
import { ApiError } from '../../api/client';

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: {
      success: elementMocks.success,
      error: elementMocks.error,
      warning: elementMocks.warning,
    },
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});
vi.mock('../../api/awards', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/awards')>();
  return {
    ...actual,
    awardsApi: {
      ...actual.awardsApi,
      createCategory: vi.fn(),
      updateCategory: vi.fn(),
      deleteCategory: vi.fn(),
      addRecipient: vi.fn(),
      removeRecipient: vi.fn(),
    },
  };
});

const category: AwardCategory = {
  id: 1,
  contestId: 7,
  code: 'GOLD',
  name: '金奖',
  displayOrder: 1,
  includeStar: true,
  groupName: null,
  participationType: 'OFFICIAL',
  firstBlood: false,
  version: 4,
  ruleType: 'FIXED_COUNT',
  ratio: null,
  fixedCount: 3,
  rankFrom: null,
  rankTo: null,
};
const ratioCategory: AwardCategory = {
  ...category,
  id: 2,
  code: 'SILVER',
  name: '银奖',
  displayOrder: 2,
  includeStar: false,
  ruleType: 'RATIO',
  fixedCount: null,
  ratio: 0.1,
};
const recipient: AwardRecipient = {
  id: 30,
  categoryId: 1,
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
};
const draftSet: AwardSet = {
  id: 6,
  contestId: 7,
  resolverRunId: 5,
  finalScoreboardSnapshotId: 12,
  status: 'DRAFT',
  version: 2,
  generatedAt: '2026-07-22T01:00:00Z',
  frozenAt: null,
  recipients: [recipient],
  conflicts: [],
};
const candidate: AwardCandidate = {
  teamId: 9,
  teamName: 'Team Nine',
  school: null,
  rank: 2,
  participationType: 'OFFICIAL',
  groupName: null,
  isStar: false,
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

describe('awards-manage panels', () => {
  beforeEach(() => {
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    elementMocks.warning.mockReset();
    elementMocks.confirm.mockReset();
  });

  it('AwardMetricsRow counts unique teams and reflects the list status', () => {
    const wrapper = mount(AwardMetricsRow, {
      props: {
        categories: [category, ratioCategory],
        awardSet: {
          ...draftSet,
          recipients: [recipient, { ...recipient, id: 31, teamId: 9 }, { ...recipient, id: 32 }],
        },
      },
    });
    expect(wrapper.text()).toContain('奖项类别');
    expect(wrapper.text()).toContain('2');
    expect(wrapper.text()).toContain('获奖队伍');
    expect(wrapper.text()).toContain('草稿');
    wrapper.unmount();

    const locked = mount(AwardMetricsRow, {
      props: { categories: [category], awardSet: { ...draftSet, status: 'FROZEN' } },
    });
    expect(locked.text()).toContain('已锁定');
    locked.unmount();

    const empty = mount(AwardMetricsRow, { props: { categories: [], awardSet: null } });
    expect(empty.text()).toContain('未生成');
    empty.unmount();
  });

  it('AwardsCommandBar gates the buttons by list status and emits commands', async () => {
    const onUpdate = vi.fn();
    const wrapper = mount(AwardsCommandBar, {
      props: {
        categories: [category],
        awardSet: draftSet,
        completedRuns: [{ id: 5, completedAt: '2026-07-22T01:00:00Z' }],
        mutating: false,
        exporting: false,
        resolverRunId: 5,
        'onUpdate:resolverRunId': onUpdate,
      },
    });
    expect(wrapper.text()).toContain('名单操作');
    await button(wrapper, '生成名单').trigger('click');
    expect(wrapper.emitted('generate')).toHaveLength(1);
    await button(wrapper, '锁定名单').trigger('click');
    expect(wrapper.emitted('freeze')).toHaveLength(1);
    expect(button(wrapper, '导出证书数据').attributes('disabled')).toBeDefined();
    wrapper.unmount();

    const frozen = mount(AwardsCommandBar, {
      props: {
        categories: [category],
        awardSet: { ...draftSet, status: 'FROZEN' },
        completedRuns: [],
        mutating: false,
        exporting: false,
        resolverRunId: null,
      },
    });
    expect(frozen.text()).toContain('解除锁定');
    expect(button(frozen, '生成名单').attributes('disabled')).toBeDefined();
    await button(frozen, '解除锁定').trigger('click');
    expect(frozen.emitted('unfreeze')).toHaveLength(1);
    await button(frozen, '导出证书数据').trigger('click');
    expect(frozen.emitted('export-certificates')).toHaveLength(1);
    frozen.unmount();
  });

  it('AwardCategoriesPanel renders rule labels, recipient counts, and freezes editing', () => {
    const wrapper = mount(AwardCategoriesPanel, {
      props: {
        contestId: 7,
        categories: [category, ratioCategory],
        awardSet: draftSet,
      },
    });
    expect(wrapper.text()).toContain('前 3 支符合条件的队伍');
    expect(wrapper.text()).toContain('前 10%');
    expect(wrapper.text()).toContain('1 支队伍');
    expect(wrapper.text()).toContain('含打星队');
    wrapper.unmount();

    const frozen = mount(AwardCategoriesPanel, {
      props: {
        contestId: 7,
        categories: [category],
        awardSet: { ...draftSet, status: 'FROZEN' },
      },
    });
    expect(button(frozen, '新增类别').attributes('disabled')).toBeDefined();
    frozen.unmount();
  });

  it('AwardCategoriesPanel creates a category from the dialog payload', async () => {
    const saved: AwardCategory = { ...category, id: 3, code: 'BRONZE', name: '铜奖', version: 1 };
    vi.mocked(awardsApi.createCategory).mockResolvedValue(saved);
    const wrapper = mount(AwardCategoriesPanel, {
      props: { contestId: 7, categories: [category], awardSet: draftSet },
    });
    await button(wrapper, '新增类别').trigger('click');
    const dialog = wrapper.find('.award-categories-card + .el-dialog, .el-dialog');
    await dialog.findAll('input')[0].setValue(' bronze ');
    await dialog.findAll('input')[1].setValue(' 铜奖 ');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(awardsApi.createCategory).toHaveBeenCalledWith(
      7,
      expect.objectContaining({
        code: 'BRONZE',
        name: '铜奖',
        rule: expect.objectContaining({ ruleType: 'FIXED_COUNT', fixedCount: 1 }),
      }),
    );
    expect(wrapper.emitted('update:categories')![0][0]).toEqual([category, saved]);
    expect(elementMocks.success).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('AwardCategoriesPanel warns without code or name and edits existing categories', async () => {
    const wrapper = mount(AwardCategoriesPanel, {
      props: { contestId: 7, categories: [category], awardSet: draftSet },
    });
    const createCalls = vi.mocked(awardsApi.createCategory).mock.calls.length;
    await button(wrapper, '新增类别').trigger('click');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(elementMocks.warning).toHaveBeenCalled();
    expect(vi.mocked(awardsApi.createCategory).mock.calls.length).toBe(createCalls);

    vi.mocked(awardsApi.updateCategory).mockResolvedValue({ ...category, name: '冠军' });
    await button(wrapper, '编辑').trigger('click');
    const dialog = wrapper.find('.el-dialog');
    await dialog.findAll('input')[1].setValue('冠军');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(awardsApi.updateCategory).toHaveBeenCalledWith(
      1,
      4,
      expect.objectContaining({ code: 'GOLD', name: '冠军' }),
    );
    wrapper.unmount();
  });

  it('AwardCategoriesPanel deletes a category after confirmation', async () => {
    elementMocks.confirm.mockResolvedValue(undefined);
    vi.mocked(awardsApi.deleteCategory).mockResolvedValue(undefined);
    const wrapper = mount(AwardCategoriesPanel, {
      props: { contestId: 7, categories: [category], awardSet: draftSet },
    });
    await button(wrapper, '删除').trigger('click');
    await flushPromises();
    expect(awardsApi.deleteCategory).toHaveBeenCalledWith(1, 4);
    expect(wrapper.emitted('refresh')).toHaveLength(1);
    expect(elementMocks.success).toHaveBeenCalled();
    wrapper.unmount();

    elementMocks.confirm.mockRejectedValue('cancel');
    const guarded = mount(AwardCategoriesPanel, {
      props: { contestId: 7, categories: [category], awardSet: draftSet },
    });
    await button(guarded, '删除').trigger('click');
    await flushPromises();
    expect(vi.mocked(awardsApi.deleteCategory).mock.calls.length).toBe(1);
    guarded.unmount();
  });

  it('AwardRecipientsPanel lists recipients and removes manual entries', async () => {
    const next: AwardSet = { ...draftSet, version: 3, recipients: [] };
    vi.mocked(awardsApi.removeRecipient).mockResolvedValue(next);
    const wrapper = mount(AwardRecipientsPanel, {
      props: {
        contestId: 7,
        awardSet: {
          ...draftSet,
          recipients: [
            recipient,
            {
              ...recipient,
              id: 31,
              isManual: true,
              school: null,
              teamId: 9,
              teamName: 'Team Nine',
            },
          ],
        },
        categories: [category],
        candidates: [candidate],
        mutating: false,
      },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('2 条记录');
    expect(wrapper.text()).toContain('手工');
    expect(wrapper.text()).toContain('School · Team #8');
    expect(wrapper.text()).toContain('— · Team #9');
    const remove = wrapper.findAll('button').filter((row) => row.text().includes('移除'));
    expect(remove).toHaveLength(1);
    await remove[0].trigger('click');
    await flushPromises();
    expect(awardsApi.removeRecipient).toHaveBeenCalledWith(31, 2);
    expect(wrapper.emitted('update:award-set')![0][0]).toEqual(next);
    wrapper.unmount();
  });

  it('AwardRecipientsPanel adds a manual recipient from the dialog', async () => {
    const next: AwardSet = { ...draftSet, version: 3 };
    vi.mocked(awardsApi.addRecipient).mockResolvedValue(next);
    const wrapper = mount(AwardRecipientsPanel, {
      props: {
        contestId: 7,
        awardSet: draftSet,
        categories: [category],
        candidates: [candidate],
        mutating: false,
      },
    });
    await button(wrapper, '手工添加').trigger('click');
    const confirm = button(wrapper, '确认添加');
    expect(confirm.attributes('disabled')).toBeDefined();
    const selects = wrapper.findAllComponents({ name: 'ElSelect' });
    await (selects[1].vm as unknown as { $emit: (event: string, value: number) => void }).$emit(
      'update:modelValue',
      9,
    );
    await button(wrapper, '确认添加').trigger('click');
    await flushPromises();
    expect(awardsApi.addRecipient).toHaveBeenCalledWith(7, 1, 9, 2);
    expect(wrapper.emitted('update:award-set')![0][0]).toEqual(next);
    wrapper.unmount();
  });

  it('AwardRecipientsPanel reports add and remove failures', async () => {
    vi.mocked(awardsApi.removeRecipient).mockRejectedValue(
      new ApiError(409, 'AWARDS_FROZEN', 'awards frozen'),
    );
    const wrapper = mount(AwardRecipientsPanel, {
      props: {
        contestId: 7,
        awardSet: { ...draftSet, recipients: [{ ...recipient, isManual: true }] },
        categories: [category],
        candidates: [],
        mutating: false,
      },
    });
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((row) => row.text().includes('移除'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    wrapper.unmount();

    vi.mocked(awardsApi.addRecipient).mockRejectedValue(
      new ApiError(409, 'AWARDS_VERSION_CONFLICT', 'awards version conflict'),
    );
    const second = mount(AwardRecipientsPanel, {
      props: {
        contestId: 7,
        awardSet: draftSet,
        categories: [category],
        candidates: [candidate],
        mutating: false,
      },
    });
    await button(second, '手工添加').trigger('click');
    const selects = second.findAllComponents({ name: 'ElSelect' });
    await (selects[1].vm as unknown as { $emit: (event: string, value: number) => void }).$emit(
      'update:modelValue',
      9,
    );
    await button(second, '确认添加').trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalledTimes(2);
    second.unmount();
  });
});
