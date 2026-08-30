import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LiveManageView from './LiveManageView.vue';
import { contestApi } from '../api/contest';
import { presentationApi } from '../api/presentation';
import { setLocale } from '../i18n';

vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/presentation', () => ({
  presentationApi: {
    templates: vi.fn(),
    config: vi.fn(),
    tokens: vi.fn(),
    update: vi.fn(),
    createToken: vi.fn(),
    revokeToken: vi.fn(),
    createTemplate: vi.fn(),
    updateTemplate: vi.fn(),
  },
}));

const liveConfig = {
  contestId: 7,
  mode: 'LIVE' as const,
  enabled: true,
  title: 'Finals',
  subtitle: null,
  accentColor: '#22c55e',
  rowLimit: 12,
  showAnnouncements: true,
  announcementIntervalSeconds: 10,
  template: 'DEFAULT' as const,
  customTemplateId: null,
  updatedAt: null,
} as Awaited<ReturnType<typeof presentationApi.config>>;

describe('LiveManageView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Finals' }],
    } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(presentationApi.templates).mockResolvedValue(
      [] as Awaited<ReturnType<typeof presentationApi.templates>>,
    );
    vi.mocked(presentationApi.config).mockResolvedValue(liveConfig);
    vi.mocked(presentationApi.tokens).mockResolvedValue(
      [] as Awaited<ReturnType<typeof presentationApi.tokens>>,
    );
    vi.mocked(presentationApi.update).mockResolvedValue(liveConfig);
  });

  it('loads the first contest live config and its broadcast tokens', async () => {
    const wrapper = mount(LiveManageView);
    await flushPromises();

    expect(wrapper.text()).toContain('直播展示控制台');
    expect(presentationApi.config).toHaveBeenCalledWith(7, 'LIVE');
    expect(presentationApi.tokens).toHaveBeenCalledWith(7);
  });

  it('saves the live configuration', async () => {
    const wrapper = mount(LiveManageView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存'))!
      .trigger('click');
    await flushPromises();

    expect(presentationApi.update).toHaveBeenCalledWith(
      7,
      'LIVE',
      expect.objectContaining({ enabled: true, rowLimit: 12, template: 'DEFAULT' }),
    );
  });

  it('surfaces config load failures as an alert', async () => {
    vi.mocked(presentationApi.config).mockRejectedValueOnce(new Error('会话已过期'));
    const wrapper = mount(LiveManageView);
    await flushPromises();

    expect(wrapper.text()).toContain('会话已过期');
  });
});
