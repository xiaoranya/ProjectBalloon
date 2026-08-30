import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminLoginView from './AdminLoginView.vue';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  login: vi.fn(),
  logout: vi.fn(),
  replace: vi.fn(),
  query: {} as Record<string, unknown>,
  state: { loading: false },
}));
vi.mock('../auth/session', () => ({
  useSession: () => ({
    state: mocks.state,
    login: mocks.login,
    logout: mocks.logout,
  }),
}));
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: mocks.query }),
  useRouter: () => ({ replace: mocks.replace }),
}));

const staff = {
  id: 2,
  username: 'chief',
  displayName: 'Chief',
  userType: 'SUPER_ADMIN' as const,
  permissions: [],
  passwordResetRequired: false,
};

describe('AdminLoginView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    mocks.query = {};
    mocks.login.mockResolvedValue(staff);
  });

  async function submit(
    wrapper: ReturnType<typeof mount>,
    username = 'chief',
    password = 'password',
  ) {
    await wrapper.find('input[autocomplete="username"]').setValue(username);
    await wrapper.find('input[autocomplete="current-password"]').setValue(password);
    await wrapper.find('form').trigger('submit');
    await flushPromises();
  }

  it('renders the staff login entry', () => {
    const wrapper = mount(AdminLoginView);
    expect(wrapper.text()).toContain('工作人员登录');
    expect(wrapper.text()).toContain('进入工作台');
  });

  it('routes staff to their home workspace', async () => {
    const wrapper = mount(AdminLoginView);
    await submit(wrapper);

    expect(mocks.login).toHaveBeenCalledWith('chief', 'password');
    expect(mocks.logout).not.toHaveBeenCalled();
    expect(mocks.replace).toHaveBeenCalledWith('/admin');
  });

  it('rejects team accounts and logs them out', async () => {
    mocks.login.mockResolvedValue({ ...staff, userType: 'TEAM' });
    const wrapper = mount(AdminLoginView);
    await submit(wrapper);

    expect(mocks.logout).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain('该账号是参赛队账号');
    expect(mocks.replace).not.toHaveBeenCalled();
  });

  it('forces a password reset before navigation', async () => {
    mocks.login.mockResolvedValue({ ...staff, passwordResetRequired: true });
    const wrapper = mount(AdminLoginView);
    await submit(wrapper);

    expect(mocks.replace).toHaveBeenCalledWith('/change-password');
  });

  it('honours safe redirect targets after login', async () => {
    mocks.query = { redirect: '/admin/problems' };
    const wrapper = mount(AdminLoginView);
    await submit(wrapper);

    expect(mocks.replace).toHaveBeenCalledWith('/admin/problems');
  });

  it('blocks empty submissions through form validation', async () => {
    const wrapper = mount(AdminLoginView);
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(mocks.login).not.toHaveBeenCalled();
  });
});
