import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import JudgeClarificationView from './JudgeClarificationView.vue';

const mocks = vi.hoisted(() => ({
  listContests: vi.fn(),
  listAll: vi.fn(),
  get: vi.fn(),
  reply: vi.fn(),
  close: vi.fn(),
  convert: vi.fn(),
  subscribe: vi.fn(),
  replace: vi.fn(),
  confirm: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: mocks.listContests } }));
vi.mock('../api/clarifications', () => ({
  clarificationApi: {
    listAll: mocks.listAll,
    get: mocks.get,
    reply: mocks.reply,
    close: mocks.close,
    convert: mocks.convert,
  },
}));
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: mocks.subscribe }));
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ replace: mocks.replace }),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: mocks.success, error: mocks.error },
    ElMessageBox: { confirm: mocks.confirm },
  };
});

const pending = {
  id: 9,
  contestId: 7,
  teamId: 3,
  teamName: 'Team Three',
  scope: 'PROBLEM',
  problemId: 2,
  problemAlias: 'A',
  question: 'Is the input sorted?',
  status: 'PENDING',
  reply: null,
  replyVisibility: null,
  askedByUserId: 44,
  repliedByUserId: null,
  repliedAt: null,
  convertedAnnouncementId: null,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:00:00Z',
  version: 0,
};
const answered = {
  ...pending,
  status: 'ANSWERED',
  reply: 'No.',
  replyVisibility: 'PUBLIC',
  repliedByUserId: 55,
  repliedAt: '2026-07-20T08:03:00Z',
  version: 1,
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

describe('JudgeClarificationView', () => {
  beforeEach(() => {
    mocks.listContests.mockResolvedValue({ content: [{ id: 7, name: 'Regional' }] });
    mocks.listAll.mockResolvedValue([pending]);
    mocks.get.mockResolvedValue(pending);
    mocks.subscribe.mockReturnValue({ stop: vi.fn() });
    mocks.confirm.mockResolvedValue(undefined);
  });
  afterEach(() => vi.clearAllMocks());

  it('loads pending by default and subscribes to the staff event stream', async () => {
    const wrapper = mount(JudgeClarificationView);
    await flushPromises();

    expect(mocks.listAll).toHaveBeenCalledWith(7, 'PENDING');
    expect(mocks.subscribe).toHaveBeenCalledWith(
      expect.objectContaining({
        contestId: 7,
        scope: 'STAFF',
        eventTypes: ['CLARIFICATION_UPDATED'],
      }),
    );
    expect(wrapper.text()).toContain('Team Three');
  });

  it('shows sensitive team and actor identifiers only in the judge detail', async () => {
    const wrapper = mount(JudgeClarificationView, { attachTo: document.body });
    await flushPromises();
    await button(wrapper, '查看详情').trigger('click');
    await flushPromises();

    expect(document.body.textContent).toContain('队伍 #3');
    expect(document.body.textContent).toContain('用户 #44');
  });

  it('replies with the exact visibility DTO', async () => {
    mocks.reply.mockResolvedValue(answered);
    mocks.listAll.mockResolvedValueOnce([pending]).mockResolvedValueOnce([]);
    const wrapper = mount(JudgeClarificationView, { attachTo: document.body });
    await flushPromises();
    await button(wrapper, '查看详情').trigger('click');
    await flushPromises();
    const textarea = document.body.querySelector(
      '.clarification-reply-form textarea',
    ) as HTMLTextAreaElement;
    textarea.value = ' Public answer ';
    textarea.dispatchEvent(new Event('input'));
    await flushPromises();
    const publicRadio = Array.from(
      document.body.querySelectorAll('.clarification-reply-form label'),
    ).find((label) => label.textContent?.includes('公开回复')) as HTMLLabelElement;
    publicRadio.click();
    await flushPromises();
    await button(wrapper, '提交回复').trigger('click');
    await flushPromises();

    expect(mocks.reply).toHaveBeenCalledWith(9, { reply: 'Public answer', visibility: 'PUBLIC' });
  });

  it('converts only an answered PUBLIC reply using nullable defaults', async () => {
    mocks.listAll.mockResolvedValue([answered]);
    mocks.get.mockResolvedValue(answered);
    mocks.convert.mockResolvedValue({ id: 12 });
    const wrapper = mount(JudgeClarificationView, { attachTo: document.body });
    await flushPromises();
    await button(wrapper, '查看详情').trigger('click');
    await flushPromises();
    await button(wrapper, '转为公告').trigger('click');
    await flushPromises();
    await button(wrapper, '发布公告').trigger('click');
    await flushPromises();

    expect(mocks.convert).toHaveBeenCalledWith(9, { title: null, body: null });
  });
});
