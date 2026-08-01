import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProblemDetailView from './ProblemDetailView.vue';
import { contestApi } from '../api/contest';

const push = vi.fn();
const route = { params: { contestId: '7', problemId: '1' } };
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push }) }));
vi.mock('../api/contest', () => ({ contestApi: { listProblems: vi.fn(), submit: vi.fn() } }));

const problem = {
  problemId: 1,
  contestId: 7,
  slug: 'a-problem',
  alias: 'A',
  displayOrder: 1,
  title: 'A problem',
  color: '#2563eb',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 64,
  languages: ['cpp', 'output'],
  statement: { langCode: 'en', renderedHtml: '<p>Statement</p>', updatedAt: '' },
};

describe('ProblemDetailView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    push.mockReset();
    vi.mocked(contestApi.listProblems).mockResolvedValue([problem]);
    vi.mocked(contestApi.submit).mockResolvedValue({ submissionId: 9, judgementId: 'j-9', status: 'PENDING', submittedAt: '' });
  });

  function mountView() {
    return mount(ProblemDetailView, {
      props: { contest: { id: 7, name: 'Finals', status: 'RUNNING' } as never },
      global: { stubs: { RouterLink: true, UploadFilled: true } },
    });
  }

  it('rejects source larger than 64 KiB before calling the submit API', async () => {
    const wrapper = mountView();
    await flushPromises();

    const onChange = wrapper.findComponent({ name: 'ElUpload' }).props('onChange') as (file: unknown) => void;
    onChange({ raw: { name: 'Main.cpp', size: 65_537 } });
    await flushPromises();
    expect(wrapper.text()).toContain('提交文件不能超过 64 KiB');
    expect(contestApi.submit).not.toHaveBeenCalled();
  });

  it('submits the selected file and navigates to the submission detail', async () => {
    const wrapper = mountView();
    await flushPromises();
    const file = { name: 'Main.cpp', size: 12 } as File;
    const onChange = wrapper.findComponent({ name: 'ElUpload' }).props('onChange') as (file: unknown) => void;
    onChange({ raw: file });
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(contestApi.submit).toHaveBeenCalledWith(7, 1, 'cpp', file);
    expect(push).toHaveBeenCalledWith('/contests/7/submissions/9');
  });
});
