import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminStaffAccountsView from './AdminStaffAccountsView.vue';

const { listStaffAccounts, createStaffAccount, updateStaffAccount, resetStaffPassword } =
  vi.hoisted(() => ({
    listStaffAccounts: vi.fn(),
    createStaffAccount: vi.fn(),
    updateStaffAccount: vi.fn(),
    resetStaffPassword: vi.fn(),
  }));

vi.mock('../api/admin', () => ({
  adminApi: {
    listStaffAccounts,
    createStaffAccount,
    updateStaffAccount,
    resetStaffPassword,
  },
}));

describe('AdminStaffAccountsView', () => {
  beforeEach(() => {
    listStaffAccounts.mockResolvedValue({
      content: [
        {
          id: 9,
          username: 'judge-01',
          displayName: 'Judge One',
          userType: 'STAFF',
          permissions: ['CLARIFICATION_MANAGE'],
          enabled: true,
          passwordResetRequired: true,
          lastLoginAt: null,
          createdAt: '2026-07-17T00:00:00Z',
          updatedAt: '2026-07-17T00:00:00Z',
        },
      ],
    });
  });

  it('renders staff identity, permissions, and password state', async () => {
    const wrapper = mount(AdminStaffAccountsView);
    await flushPromises();

    expect(wrapper.text()).toContain('Judge One');
    expect(wrapper.text()).toContain('@judge-01');
    expect(wrapper.text()).toContain('答疑处理');
    expect(wrapper.text()).toContain('等待首次改密');
    expect(listStaffAccounts).toHaveBeenCalledOnce();
  });
});
