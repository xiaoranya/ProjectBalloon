import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminPermissionsView from './AdminPermissionsView.vue';

const { listContestAdminScopes, listContests, updateContestAdminScope } = vi.hoisted(() => ({
  listContestAdminScopes: vi.fn(),
  listContests: vi.fn(),
  updateContestAdminScope: vi.fn(),
}));

vi.mock('../api/admin', () => ({
  adminApi: { listContestAdminScopes, listContests, updateContestAdminScope },
}));

describe('AdminPermissionsView', () => {
  beforeEach(() => {
    listContestAdminScopes.mockResolvedValue([
      {
        userId: 7,
        username: 'contest-admin',
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
    updateContestAdminScope.mockResolvedValue({
      userId: 7,
      username: 'contest-admin',
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

    expect(updateContestAdminScope).toHaveBeenCalledWith(7, [2, 4]);
  });
});
