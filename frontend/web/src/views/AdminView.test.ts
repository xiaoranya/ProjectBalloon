import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminView from './AdminView.vue';

const { getHealth, listAuditLogs } = vi.hoisted(() => ({
  getHealth: vi.fn(),
  listAuditLogs: vi.fn(),
}));

vi.mock('../api/admin', () => ({
  adminApi: { getHealth, listAuditLogs },
}));

vi.mock('../auth/session', () => ({
  useSession: () => ({ isSuperAdmin: { value: true } }),
}));

describe('AdminView', () => {
  beforeEach(() => {
    getHealth.mockResolvedValue({
      status: 'down',
      service: 'xcpc-platform',
      time: '2026-07-20T06:00:00Z',
      realtimeOutbox: { pending: 3, failed: 1, redisConnected: false },
      judgeDispatch: {
        pending: 2,
        failed: 0,
        workers: { online: 4, stale: 1, capacity: 16, activeTasks: 6 },
        rabbitmq: { status: 'up', queuedTasks: 2, queuedResults: 1, deadTasks: 0 },
      },
      objectStorage: { status: 'up' },
      objectCleanup: { pending: 2, failed: 1, missingReferences: 3 },
    });
    listAuditLogs.mockResolvedValue({
      content: [
        {
          id: 1,
          actorUserId: 88,
          action: 'STAFF_ACCOUNT_UPDATED',
          targetType: 'user',
          targetId: '9',
          requestIp: '10.0.0.8',
          result: 'success',
          createdAt: '2026-07-20T05:00:00Z',
        },
      ],
      page: 0,
      size: 25,
      totalElements: 1,
      totalPages: 1,
    });
  });

  it('renders the Rust readiness shape and audit rows', async () => {
    const wrapper = mount(AdminView);
    await flushPromises();

    expect(wrapper.text()).toContain('存在异常');
    expect(wrapper.text()).toContain('实时事件');
    expect(wrapper.text()).toContain('未连接');
    expect(wrapper.text()).toContain('判题调度与 Worker');
    expect(wrapper.text()).toContain('RabbitMQ');
    expect(wrapper.text()).toContain('对象存储');
    expect(wrapper.text()).toContain('对象存储一致性');
    expect(wrapper.text()).toContain('缺失引用');
    expect(wrapper.text()).toContain('3');
    expect(wrapper.text()).toContain('CUPS 打印');
    expect(wrapper.text()).toContain('未配置');
    expect(wrapper.text()).toContain('STAFF_ACCOUNT_UPDATED');
    expect(wrapper.text()).toContain('10.0.0.8');
    expect(listAuditLogs).toHaveBeenCalledWith(expect.objectContaining({ page: 0, size: 25 }));
  });
});
