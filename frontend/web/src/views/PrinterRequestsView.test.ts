import { flushPromises, mount } from '@vue/test-utils';
import { ElMessage } from 'element-plus';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PrinterRequestsView from './PrinterRequestsView.vue';
import { adminApi } from '../api/admin';
import { contestApi } from '../api/contest';
import { printingApi } from '../api/printing';
import { subscribeContestEvents } from '../realtime/contest-events';

const replace = vi.fn();
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: { contestId: '8' } }),
  useRouter: () => ({ replace }),
}));
vi.mock('../api/admin', () => ({ adminApi: { getHealth: vi.fn() } }));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/printing', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/printing')>();
  return { ...actual, printingApi: { listAll: vi.fn(), retry: vi.fn(), cancel: vi.fn(), reject: vi.fn(), pdf: vi.fn() } };
});
vi.mock('../realtime/contest-events', () => ({ subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })) }));

const request = {
  id: 9, contestId: 8, teamId: 2, teamName: 'Team', seatNo: 'A01', contentHash: 'abc', pageCount: 1,
  status: 'FAILED' as const, printerId: null, cupsJobId: null, requestedByUserId: 3, operatorUserId: null,
  completedAt: null, failedReason: 'CUPS down', createdAt: '2026-07-20T08:00:00Z', updatedAt: '2026-07-20T08:00:00Z', version: 0,
};

function mountView() { return mount(PrinterRequestsView, { attachTo: document.body }); }

describe('PrinterRequestsView', () => {
  beforeEach(() => {
    replace.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({ content: [
      { id: 7, name: 'Contest 7' }, { id: 8, name: 'Contest 8' },
    ] } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(adminApi.getHealth).mockResolvedValue({ status: 'up', service: 'api', time: '', cups: { status: 'down' } });
    vi.mocked(printingApi.listAll).mockResolvedValue([request]);
    vi.mocked(printingApi.retry).mockResolvedValue({ ...request, status: 'QUEUED' });
  });

  it('keeps a query-selected contest, loads all statuses, and uses staff realtime', async () => {
    const wrapper = mountView();
    await flushPromises();
    expect(printingApi.listAll).toHaveBeenCalledWith(8, undefined);
    expect(wrapper.text()).toContain('CUPS 连接不可用');
    expect(subscribeContestEvents).toHaveBeenCalledWith(expect.objectContaining({
      contestId: 8, scope: 'STAFF', eventTypes: ['PRINT_REQUEST_UPDATED'],
    }));
    wrapper.unmount();
  });

  it('shows the explicit unknown state when CUPS health is omitted', async () => {
    vi.mocked(adminApi.getHealth).mockResolvedValue({ status: 'up', service: 'api', time: '' });
    const wrapper = mountView();
    await flushPromises();
    expect(wrapper.text()).toContain('CUPS 未配置或连接状态未知');
    wrapper.unmount();
  });

  it('still loads the queue when the optional health probe fails', async () => {
    vi.mocked(adminApi.getHealth).mockRejectedValue(new Error('probe unavailable'));
    const wrapper = mountView();
    await flushPromises();
    expect(printingApi.listAll).toHaveBeenCalledWith(8, undefined);
    expect(wrapper.text()).toContain('Team');
    expect(wrapper.text()).toContain('CUPS 未配置或连接状态未知');
    wrapper.unmount();
  });

  it('shows an explicit queue error instead of presenting a failed initial load as empty', async () => {
    vi.mocked(printingApi.listAll).mockRejectedValueOnce(new Error('queue unavailable'));
    const wrapper = mountView();
    await flushPromises();
    expect(wrapper.text()).toContain('queue unavailable');
    expect(wrapper.text()).toContain('打印队列加载失败，请重试');
    expect(wrapper.text()).not.toContain('当前筛选下没有打印请求');
    wrapper.unmount();
  });

  it('retries only by request id from the detail drawer', async () => {
    const wrapper = mountView();
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');
    await flushPromises();
    const retry = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent?.includes('重试')) as HTMLButtonElement;
    retry.click();
    await flushPromises();
    expect(printingApi.retry).toHaveBeenCalledWith(9);
    wrapper.unmount();
  });

  it('warns when a successful mutation cannot refresh the queue', async () => {
    const warning = vi.spyOn(ElMessage, 'warning').mockImplementation(() => ({ close: vi.fn() } as never));
    vi.mocked(printingApi.listAll)
      .mockResolvedValueOnce([request])
      .mockRejectedValueOnce(new Error('refresh unavailable'));
    const wrapper = mountView();
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');
    await flushPromises();
    const retry = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent?.includes('重试')) as HTMLButtonElement;
    retry.click();
    await flushPromises();
    expect(warning).toHaveBeenCalledWith('打印请求已重试，但刷新队列失败，请手动重试');
    expect(wrapper.text()).toContain('refresh unavailable');
    warning.mockRestore();
    wrapper.unmount();
  });

  it('hides PDF download until the request has finished rendering', async () => {
    vi.mocked(printingApi.listAll).mockResolvedValue([{ ...request, status: 'REQUESTED' }]);
    const wrapper = mountView();
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');
    await flushPromises();
    expect(document.body.textContent).toContain('PDF 正在生成');
    expect(Array.from(document.body.querySelectorAll('button')).some((button) => button.textContent?.includes('下载 PDF'))).toBe(false);
    wrapper.unmount();
  });

  it('downloads the PDF blob and revokes its object URL', async () => {
    const createObjectURL = vi.fn(() => 'blob:test');
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('URL', { ...URL, createObjectURL, revokeObjectURL });
    vi.mocked(printingApi.pdf).mockResolvedValue(new Blob(['pdf'], { type: 'application/pdf' }));
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined);
    const wrapper = mountView();
    await flushPromises();
    await wrapper.find('tbody tr').trigger('click');
    await flushPromises();
    const download = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent?.includes('下载 PDF')) as HTMLButtonElement;
    download.click();
    await flushPromises();
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(printingApi.pdf).toHaveBeenCalledWith(9);
    expect(createObjectURL).toHaveBeenCalled();
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:test');
    click.mockRestore();
    wrapper.unmount();
  });
});
