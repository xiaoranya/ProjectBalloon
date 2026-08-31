import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, ref } from 'vue';
import BalloonCancelDialog from './BalloonCancelDialog.vue';
import BalloonDetailDrawer from './BalloonDetailDrawer.vue';
import BalloonStatsRow from './BalloonStatsRow.vue';
import BalloonTasksTable from './BalloonTasksTable.vue';
import BalloonToolbar from './BalloonToolbar.vue';
import type { BalloonStats, BalloonTask } from '../../api/balloons';

vi.mock('../../utils/format', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../utils/format')>()),
}));

const pending: BalloonTask = {
  id: 11,
  contestId: 7,
  teamId: 8,
  problemId: 3,
  submissionId: 44,
  color: '#ef4444',
  isFirstBlood: true,
  status: 'PENDING',
  seatNo: 'A1',
  teamName: 'Team Eight',
  problemAlias: 'A',
  note: null,
  claimedByUserId: null,
  claimedAt: null,
  deliveredAt: null,
  cancelledAt: null,
  cancelledReason: null,
  createdAt: '2026-07-22T01:00:00Z',
  updatedAt: '2026-07-22T01:00:00Z',
  version: 1,
  reopenedCount: 0,
};
const delivered: BalloonTask = {
  ...pending,
  id: 12,
  status: 'DELIVERED',
  isFirstBlood: false,
  claimedByUserId: 7,
  claimedAt: '2026-07-22T01:05:00Z',
  deliveredAt: '2026-07-22T01:15:00Z',
};
const cancelled: BalloonTask = {
  ...pending,
  id: 13,
  status: 'CANCELLED',
  cancelledAt: '2026-07-22T01:20:00Z',
  cancelledReason: '重复生成',
  reopenedCount: 1,
};

const stats: BalloonStats = {
  total: 3,
  pending: 1,
  claimed: 0,
  delivered: 1,
  cancelled: 1,
  firstBlood: 1,
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

describe('balloon-tasks panels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('BalloonStatsRow renders every counter with zeros for missing stats', () => {
    const wrapper = mount(BalloonStatsRow, { props: { stats } });
    expect(wrapper.text()).toContain('全部任务');
    expect(wrapper.text()).toContain('First Blood');
    expect(wrapper.findAll('strong').map((strong) => strong.text())).toEqual([
      '3',
      '1',
      '0',
      '1',
      '1',
    ]);
    wrapper.unmount();

    const empty = mount(BalloonStatsRow, { props: { stats: null } });
    expect(empty.findAll('strong').map((strong) => strong.text())).toEqual([
      '0',
      '0',
      '0',
      '0',
      '0',
    ]);
    empty.unmount();
  });

  it('BalloonTasksTable renders rows and reports the failure empty text', async () => {
    const wrapper = mount(BalloonTasksTable, {
      props: { tasks: [pending, delivered], loading: false, loaded: true },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Team Eight');
    expect(wrapper.text()).toContain('座位 A1 · 任务 #11');
    expect(wrapper.text()).toContain('First Blood');
    expect(wrapper.text()).toContain('普通任务');
    expect(wrapper.text()).toContain('已送达');
    expect(wrapper.text()).toContain('工作人员 #7');
    expect(wrapper.text()).toContain('尚未领取');
    await wrapper.findAll('.el-table__row')[0].trigger('click');
    expect(wrapper.emitted('open-detail')![0][0]).toEqual(pending);
    wrapper.unmount();

    const failed = mount(BalloonTasksTable, {
      props: { tasks: [], loading: false, loaded: false },
    });
    await flushPromises();
    expect(failed.text()).toContain('气球任务加载失败，请重试');
    failed.unmount();

    const empty = mount(BalloonTasksTable, { props: { tasks: [], loading: false, loaded: true } });
    await flushPromises();
    expect(empty.text()).toContain('当前筛选下暂无气球任务');
    empty.unmount();
  });

  it('BalloonToolbar gates actions by contest and propagates keyword updates', async () => {
    const onUpdateKeyword = vi.fn();
    const wrapper = mount(BalloonToolbar, {
      props: {
        contests: [
          {
            id: 7,
            name: 'Finals',
            status: 'RUNNING' as const,
            visibility: 'PUBLIC' as const,
            startAt: null,
            freezeAt: null,
            endAt: null,
            version: 1,
            createdAt: '',
            updatedAt: '',
            deletedAt: null,
          },
        ],
        problemOptions: ['A'],
        loading: false,
        action: '',
        selectedContestId: 7,
        statusFilter: 'ALL',
        problemFilter: '',
        keyword: '',
        'onUpdate:keyword': onUpdateKeyword,
      },
    });
    expect(button(wrapper, '智能领取').attributes('disabled')).toBeUndefined();
    await button(wrapper, '智能领取').trigger('click');
    expect(wrapper.emitted('dispatch')).toHaveLength(1);
    await button(wrapper, '刷新').trigger('click');
    expect(wrapper.emitted('refresh')).toHaveLength(1);
    await wrapper.find('input.el-input__inner').setValue('Eight');
    expect(onUpdateKeyword).toHaveBeenCalledWith('Eight');
    wrapper.unmount();

    const noContest = mount(BalloonToolbar, {
      props: {
        contests: [],
        problemOptions: [],
        loading: false,
        action: 'dispatch',
        selectedContestId: null,
        statusFilter: 'ALL',
        problemFilter: '',
        keyword: '',
      },
    });
    expect(button(noContest, '智能领取').attributes('disabled')).toBeDefined();
    noContest.unmount();
  });

  it('BalloonDetailDrawer shows pending actions and emits task commands', async () => {
    const onUpdateNote = vi.fn();
    const wrapper = mount(BalloonDetailDrawer, {
      props: {
        action: '',
        canDeliver: true,
        detailVisible: true,
        note: '',
        selected: pending,
        'onUpdate:note': onUpdateNote,
      },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('待领取');
    expect(wrapper.text()).toContain('First Blood');
    expect(wrapper.text()).toContain('#44');
    await button(wrapper, '领取任务').trigger('click');
    expect(wrapper.emitted('claim')).toHaveLength(1);
    await button(wrapper, '标记已送达').trigger('click');
    expect(wrapper.emitted('deliver')).toHaveLength(1);
    await button(wrapper, '取消任务').trigger('click');
    expect(wrapper.emitted('open-cancel')).toHaveLength(1);
    await wrapper.find('textarea').setValue('东门入口');
    expect(onUpdateNote).toHaveBeenCalledWith('东门入口');
    await button(wrapper, '保存备注').trigger('click');
    expect(wrapper.emitted('save-note')).toHaveLength(1);
    wrapper.unmount();

    const claimed = mount(BalloonDetailDrawer, {
      props: {
        action: '',
        canDeliver: false,
        detailVisible: true,
        note: '',
        selected: { ...pending, status: 'CLAIMED' },
      },
    });
    await flushPromises();
    const texts = claimed.findAll('button').map((row) => row.text());
    expect(texts.some((text) => text.includes('领取任务'))).toBe(false);
    expect(texts.some((text) => text.includes('标记已送达'))).toBe(false);
    expect(texts.some((text) => text.includes('取消任务'))).toBe(true);
    claimed.unmount();

    const reopened = mount(BalloonDetailDrawer, {
      props: { action: '', canDeliver: false, detailVisible: true, note: '', selected: cancelled },
    });
    await flushPromises();
    expect(reopened.text()).toContain('已取消');
    expect(reopened.text()).toContain('重复生成');
    expect(reopened.text()).toContain('1 次');
    await button(reopened, '重新打开').trigger('click');
    expect(reopened.emitted('reopen')).toHaveLength(1);
    reopened.unmount();
  });

  it('BalloonCancelDialog requires a reason before confirming', async () => {
    const Harness = defineComponent({
      setup(_, { expose }) {
        const reason = ref('');
        expose({ reason });
        return () =>
          h(BalloonCancelDialog, {
            action: '',
            cancelVisible: true,
            cancelReason: reason.value,
            'onUpdate:cancelReason': (value: string | undefined) => {
              reason.value = value ?? '';
            },
          });
      },
    });
    const wrapper = mount(Harness);
    await flushPromises();
    expect(button(wrapper, '确认取消').attributes('disabled')).toBeDefined();
    await wrapper.find('textarea').setValue('座位无人');
    await flushPromises();
    expect(button(wrapper, '确认取消').attributes('disabled')).toBeUndefined();
    await wrapper.find('textarea').setValue('');
    await flushPromises();
    expect(button(wrapper, '确认取消').attributes('disabled')).toBeDefined();
    await wrapper.find('textarea').setValue('座位无人');
    await flushPromises();
    const dialog = wrapper.findComponent(BalloonCancelDialog);
    await button(wrapper, '确认取消').trigger('click');
    expect(dialog.emitted('cancel')).toHaveLength(1);
    await button(wrapper, '返回').trigger('click');
    expect(dialog.emitted('update:cancelVisible')![0][0]).toBe(false);
    wrapper.unmount();
  });
});
