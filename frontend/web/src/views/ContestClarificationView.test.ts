import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ContestClarificationView from './ContestClarificationView.vue';

const mocks = vi.hoisted(() => ({
  listProblems: vi.fn(),
  listMine: vi.fn(),
  ask: vi.fn(),
  subscribe: vi.fn(),
  success: vi.fn(),
}));
vi.mock('../api/contest', () => ({ contestApi: { listProblems: mocks.listProblems } }));
vi.mock('../api/clarifications', () => ({
  clarificationApi: { listMine: mocks.listMine, ask: mocks.ask },
}));
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: mocks.subscribe }));
vi.mock('vue-router', () => ({ useRoute: () => ({ params: { contestId: '7' } }) }));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return { ...actual, ElMessage: { success: mocks.success } };
});

const clarification = {
  id: 9,
  contestId: 7,
  teamId: 3,
  teamName: 'Sensitive Team',
  scope: 'GENERAL',
  problemId: null,
  problemAlias: null,
  question: 'Is the input sorted?',
  status: 'ANSWERED',
  reply: 'No.',
  replyVisibility: 'PRIVATE',
  askedByUserId: 44,
  repliedByUserId: 55,
  repliedAt: '2026-07-20T08:03:00Z',
  convertedAnnouncementId: null,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:03:00Z',
  version: 1,
};

describe('ContestClarificationView', () => {
  beforeEach(() => {
    mocks.listProblems.mockResolvedValue([{ problemId: 2, alias: 'A', title: 'Balloons' }]);
    mocks.listMine.mockResolvedValue([clarification]);
    mocks.subscribe.mockReturnValue({ stop: vi.fn() });
  });
  afterEach(() => vi.clearAllMocks());

  it('reuses contestApi.listProblems and subscribes only to the team stream', async () => {
    const wrapper = mount(ContestClarificationView);
    await flushPromises();

    expect(mocks.listProblems).toHaveBeenCalledWith(7);
    expect(mocks.listMine).toHaveBeenCalledWith(7);
    expect(mocks.subscribe).toHaveBeenCalledWith(
      expect.objectContaining({
        contestId: 7,
        scope: 'TEAM',
        eventTypes: ['CLARIFICATION_UPDATED'],
      }),
    );
    expect(wrapper.text()).toContain('我的问题');
  });

  it('does not expose staff-only team and user identifiers', async () => {
    const wrapper = mount(ContestClarificationView);
    await flushPromises();

    expect(wrapper.text()).toContain('Is the input sorted?');
    expect(wrapper.text()).toContain('No.');
    expect(wrapper.text()).not.toContain('Sensitive Team');
    expect(wrapper.text()).not.toContain('44');
    expect(wrapper.text()).not.toContain('55');
  });

  it('submits the exact problem-scoped DTO', async () => {
    mocks.ask.mockResolvedValue({
      ...clarification,
      id: 10,
      scope: 'PROBLEM',
      problemId: 2,
      problemAlias: 'A',
    });
    const wrapper = mount(ContestClarificationView);
    await flushPromises();
    const radios = wrapper.findAll('input[type="radio"]');
    await radios[1].setValue(true);
    await flushPromises();
    const select = wrapper.findAllComponents({ name: 'ElSelect' })[0];
    await select.setValue(2);
    await wrapper.find('textarea').setValue('  Need a hint?  ');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(mocks.ask).toHaveBeenCalledWith(7, {
      scope: 'PROBLEM',
      problemId: 2,
      question: 'Need a hint?',
    });
    expect(mocks.success).toHaveBeenCalledWith('问题已提交');
  });
});
