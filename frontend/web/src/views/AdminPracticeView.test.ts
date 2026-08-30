import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminPracticeView from './AdminPracticeView.vue';
import { trainingApi } from '../api/training';
import { setLocale } from '../i18n';

vi.mock('../api/training', () => ({
  trainingApi: {
    practiceSettings: vi.fn(),
    problemBank: vi.fn(),
    adminEditorial: vi.fn(),
    saveEditorial: vi.fn(),
    updatePracticeSettings: vi.fn(),
  },
}));

const settings = {
  dailySubmissionLimit: 150,
  concurrentJudgingLimit: 4,
  sourceRetentionDays: 90,
} as Awaited<ReturnType<typeof trainingApi.practiceSettings>>;

describe('AdminPracticeView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    vi.mocked(trainingApi.practiceSettings).mockResolvedValue(settings);
    vi.mocked(trainingApi.problemBank).mockResolvedValue({
      content: [{ id: 5, slug: 'a-plus-b', title: 'A+B' }],
    } as Awaited<ReturnType<typeof trainingApi.problemBank>>);
    vi.mocked(trainingApi.updatePracticeSettings).mockResolvedValue(undefined);
  });

  function mountView() {
    return mount(AdminPracticeView, { global: { stubs: { CodeEditor: true } } });
  }

  it('loads practice settings and the problem bank on mount', async () => {
    const wrapper = mountView();
    await flushPromises();

    expect(trainingApi.practiceSettings).toHaveBeenCalledOnce();
    expect(trainingApi.problemBank).toHaveBeenCalledWith(0, 100);
    expect(wrapper.text()).toContain('平台配额');
    expect(wrapper.text()).toContain('题解管理');
  });

  it('saves the platform quotas', async () => {
    const wrapper = mountView();
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存设置'))!
      .trigger('click');
    await flushPromises();

    expect(trainingApi.updatePracticeSettings).toHaveBeenCalledWith(settings);
  });

  it('surfaces load failures as an alert', async () => {
    vi.mocked(trainingApi.practiceSettings).mockRejectedValueOnce(new Error('会话已过期'));
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.text()).toContain('会话已过期');
  });
});
