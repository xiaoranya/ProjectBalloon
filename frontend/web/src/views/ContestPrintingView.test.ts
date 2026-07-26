import { flushPromises, mount } from '@vue/test-utils';
import { reactive } from 'vue';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ContestPrintingView from './ContestPrintingView.vue';
import { printingApi } from '../api/printing';
import { subscribeContestEvents } from '../realtime/contest-events';

const useRoute = vi.hoisted(() => vi.fn());
vi.mock('vue-router', () => ({ useRoute }));
vi.mock('../api/printing', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/printing')>();
  return { ...actual, printingApi: { create: vi.fn(), listMine: vi.fn(), pdf: vi.fn() } };
});
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })) }));

const request = {
  id: 9, contestId: 7, teamId: 2, teamName: 'Team', seatNo: 'A01', contentHash: 'abc', pageCount: 1,
  status: 'QUEUED' as const, printerId: null, cupsJobId: null, requestedByUserId: 3, operatorUserId: null,
  completedAt: null, failedReason: null, createdAt: '2026-07-20T08:00:00Z', updatedAt: '2026-07-20T08:00:00Z', version: 0,
};

describe('ContestPrintingView', () => {
  beforeEach(() => {
    useRoute.mockReturnValue(reactive({ params: { contestId: '7' } }));
    vi.mocked(printingApi.listMine).mockResolvedValue([request]);
    vi.mocked(printingApi.create).mockResolvedValue(request);
    vi.mocked(printingApi.pdf).mockResolvedValue(new Blob(['pdf'], { type: 'application/pdf' }));
  });

  it('loads team history and subscribes to print updates', async () => {
    const wrapper = mount(ContestPrintingView);
    await flushPromises();
    expect(printingApi.listMine).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('打印请求 #9');
    expect(subscribeContestEvents).toHaveBeenCalledWith(expect.objectContaining({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['PRINT_REQUEST_UPDATED'],
    }));
  });

  it('normalizes content before creating and clears the editor', async () => {
    const wrapper = mount(ContestPrintingView);
    await flushPromises();
    const textarea = wrapper.find('textarea');
    await textarea.setValue('  hello\r\nworld  ');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(printingApi.create).toHaveBeenCalledWith(7, '  hello\nworld  ');
    expect((textarea.element as HTMLTextAreaElement).value).toBe('');
  });

  it('blocks content that exceeds five estimated pages', async () => {
    const wrapper = mount(ContestPrintingView);
    await flushPromises();
    await wrapper.find('textarea').setValue(Array.from({ length: 251 }, () => 'x').join('\n'));
    expect(wrapper.text()).toContain('预计 6 / 5 页');
    expect(wrapper.find('button[type="submit"]').attributes('disabled')).toBeDefined();
  });

  it('does not offer a PDF download while a legacy request is still rendering', async () => {
    vi.mocked(printingApi.listMine).mockResolvedValue([{ ...request, status: 'REQUESTED' }]);
    const wrapper = mount(ContestPrintingView);
    await flushPromises();
    expect(wrapper.text()).toContain('PDF 正在生成');
    expect(wrapper.text()).not.toContain('下载 PDF');
  });

  it('reloads data and reconnects realtime when a reused route changes contest', async () => {
    const route = reactive({ params: { contestId: '7' } });
    useRoute.mockReturnValue(route);
    const stop = vi.fn();
    vi.mocked(subscribeContestEvents).mockReturnValue({ stop });
    const wrapper = mount(ContestPrintingView);
    await flushPromises();

    route.params.contestId = '8';
    await flushPromises();

    expect(stop).toHaveBeenCalled();
    expect(printingApi.listMine).toHaveBeenLastCalledWith(8);
    expect(subscribeContestEvents).toHaveBeenLastCalledWith(expect.objectContaining({ contestId: 8 }));
    wrapper.unmount();
  });

  it('downloads an owned request PDF', async () => {
    const createObjectURL = vi.fn(() => 'blob:team-print');
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('URL', { ...URL, createObjectURL, revokeObjectURL });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined);
    const wrapper = mount(ContestPrintingView, { attachTo: document.body });
    await flushPromises();
    const download = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent?.includes('下载 PDF')) as HTMLButtonElement;
    download.click();
    await flushPromises();
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(printingApi.pdf).toHaveBeenCalledWith(9);
    expect(createObjectURL).toHaveBeenCalled();
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:team-print');
    click.mockRestore();
    wrapper.unmount();
  });
});
