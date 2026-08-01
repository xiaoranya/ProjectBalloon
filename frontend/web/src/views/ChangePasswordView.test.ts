import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ChangePasswordView from './ChangePasswordView.vue';

const mocks = vi.hoisted(() => ({ changePassword: vi.fn(), replace: vi.fn() }));
vi.mock('../auth/session', () => ({
  useSession: () => ({
    state: { loading: false, user: { userType: 'INDIVIDUAL', passwordResetRequired: true } },
    changePassword: mocks.changePassword,
  }),
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ replace: mocks.replace }) }));

describe('ChangePasswordView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.changePassword.mockResolvedValue({ userType: 'INDIVIDUAL' });
  });

  it('submits the password change and returns to the user home route', async () => {
    const wrapper = mount(ChangePasswordView);
    const inputs = wrapper.findAll('input');
    await inputs[0].setValue('old-password');
    await inputs[1].setValue('new-password');
    await inputs[2].setValue('new-password');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(mocks.changePassword).toHaveBeenCalledWith('old-password', 'new-password');
    expect(mocks.replace).toHaveBeenCalledWith('/practice');
    expect(wrapper.text()).toContain('首次登录，请修改密码');
  });
});
