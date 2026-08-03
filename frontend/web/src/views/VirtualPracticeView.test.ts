import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import VirtualPracticeView from './VirtualPracticeView.vue';

const mocks = vi.hoisted(() => ({
  virtualSessions: vi.fn(),
  problemBank: vi.fn(),
  virtualSession: vi.fn(),
  archiveVirtualSession: vi.fn(),
  createVirtualSession: vi.fn(),
  confirm: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../api/training', () => ({ trainingApi: mocks }));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessageBox: { confirm: mocks.confirm },
    ElMessage: { success: mocks.success, error: mocks.error },
  };
});

const session = {
  id: 11,
  title: '个人虚拟赛',
  startAt: '2026-08-01T08:00:00Z',
  endAt: '2026-08-01T12:00:00Z',
  serverTime: '2026-08-01T09:00:00Z',
  status: 'RUNNING' as const,
  totalProblems: 1,
  solvedProblems: 0,
};
const detail = {
  session,
  problems: [
    { problemId: 4, slug: 'graph', title: 'Graph', position: 1, solved: false, attempts: 0 },
  ],
};

describe('VirtualPracticeView', () => {
  beforeEach(() => {
    mocks.virtualSessions.mockResolvedValue([session]);
    mocks.problemBank.mockResolvedValue({
      content: [
        {
          id: 4,
          slug: 'graph',
          title: 'Graph',
          statement: null,
          difficulty: 2,
          tags: [],
          publishedAt: null,
        },
      ],
    });
    mocks.virtualSession.mockResolvedValue(detail);
    mocks.archiveVirtualSession.mockResolvedValue({ ...session, status: 'ARCHIVED' });
    mocks.confirm.mockResolvedValue(true);
  });

  it('loads the selected session and archives it only after confirmation', async () => {
    const wrapper = mount(VirtualPracticeView);
    await flushPromises();

    expect(mocks.virtualSessions).toHaveBeenCalledOnce();
    expect(mocks.virtualSession).toHaveBeenCalledWith(11);
    expect(wrapper.text()).toContain('个人虚拟赛');
    expect(wrapper.text()).toContain('Graph');

    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('归档'))!
      .trigger('click');
    await flushPromises();
    expect(mocks.confirm).toHaveBeenCalled();
    expect(mocks.archiveVirtualSession).toHaveBeenCalledWith(11);
  });
});
