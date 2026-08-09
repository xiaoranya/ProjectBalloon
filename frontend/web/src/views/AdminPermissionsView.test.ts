import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminPermissionsView from './AdminPermissionsView.vue';

const { listContestManagementScopes, listContests, updateContestManagementScope } = vi.hoisted(
  () => ({
    listContestManagementScopes: vi.fn(),
    listContests: vi.fn(),
    updateContestManagementScope: vi.fn(),
  }),
);

vi.mock('../api/admin', () => ({
  adminApi: { listContestManagementScopes, listContests, updateContestManagementScope },
}));

describe('AdminPermissionsView', () => {
  beforeEach(() => {
    listContestManagementScopes.mockResolvedValue([
      {
        userId: 7,
        username: 'contest-manager',
        displayName: 'Contest Administrator',
        enabled: true,
        contestIds: [2],
      },
    ]);
    listContests.mockResolvedValue({
      content: [
        { id: 2, name: 'Regional', status: 'DRAFT' },
        { id: 4, name: 'Final', status: 'RUNNING' },
      ],
    });
    updateContestManagementScope.mockResolvedValue({
      userId: 7,
      username: 'contest-manager',
      displayName: 'Contest Administrator',
      enabled: true,
      contestIds: [2, 4],
    });
  });

  it('renders administrator assignments and saves changed scope', async () => {
    const wrapper = mount(AdminPermissionsView);
    await flushPromises();

    expect(wrapper.text()).toContain('Contest Administrator');
    expect(wrapper.text()).toContain('Regional');
    expect(wrapper.text()).toContain('Final');

    const checkboxes = wrapper.findAll('input[type="checkbox"]');
    await checkboxes[1].setValue(true);
    const saveButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存授权'));
    await saveButton?.trigger('click');
    await flushPromises();

    expect(updateContestManagementScope).toHaveBeenCalledWith(7, [2, 4]);
  });
});
