import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LoginView from './LoginView.vue';

const mocks = vi.hoisted(() => ({
  login: vi.fn(),
  workstationLogin: vi.fn(),
  logout: vi.fn(),
  replace: vi.fn(),
  state: { loading: false, deployment: { mode: 'standard', activeContest: null } },
}));
const route = { query: {} };
vi.mock('../auth/session', () => ({
  useSession: () => ({
    state: mocks.state,
    login: mocks.login,
    workstationLogin: mocks.workstationLogin,
    logout: mocks.logout,
  }),
}));
vi.mock('vue-router', () => ({
  useRoute: () => route,
  useRouter: () => ({ replace: mocks.replace }),
}));

const individual = {
  id: 4,
  username: 'alice',
  displayName: 'Alice',
  userType: 'INDIVIDUAL' as const,
  roles: [],
  passwordResetRequired: false,
};

describe('LoginView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    route.query = {};
    mocks.login.mockResolvedValue(individual);
    mocks.logout.mockResolvedValue(undefined);
    mocks.state.deployment.mode = 'standard';
  });

  it('offers pairing and account login in competition mode', async () => {
    mocks.state.deployment.mode = 'competition';
    mocks.workstationLogin.mockResolvedValue({
      ...individual,
      userType: 'TEAM',
      competition: { contestId: 23, contestName: 'Final', workstationId: 5, seatNo: 'A01' },
    });
    const wrapper = mount(LoginView, { global: { stubs: { RouterLink: true } } });

    expect(wrapper.text()).toContain('配对码');
    expect(wrapper.text()).toContain('账号密码');
    await wrapper.find('input[placeholder="请输入本机配对码"]').setValue('AB-CD23');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(mocks.workstationLogin).toHaveBeenCalledWith('AB-CD23');
    expect(mocks.replace).toHaveBeenCalledWith('/contests/23');
    expect(wrapper.text()).not.toContain('注册个人练习账号');
  });

  async function submit(
    wrapper: ReturnType<typeof mount>,
    username = 'alice',
    password = 'password',
  ) {
    await wrapper.find('input[placeholder="请输入用户名"]').setValue(username);
    await wrapper.find('input[placeholder="请输入密码"]').setValue(password);
    await wrapper.find('form').trigger('submit');
    await flushPromises();
  }

  it('routes individual users to practice after login', async () => {
    const wrapper = mount(LoginView, { global: { stubs: { RouterLink: true } } });
    await submit(wrapper);

    expect(mocks.login).toHaveBeenCalledWith('alice', 'password');
    expect(mocks.replace).toHaveBeenCalledWith('/practice');
  });

  it('requires password reset before allowing normal navigation', async () => {
    mocks.login.mockResolvedValue({ ...individual, passwordResetRequired: true });
    const wrapper = mount(LoginView, { global: { stubs: { RouterLink: true } } });
    await submit(wrapper);

    expect(mocks.replace).toHaveBeenCalledWith('/change-password');
  });

  it('rejects staff accounts at the contestant login and logs them out', async () => {
    mocks.login.mockResolvedValue({ ...individual, userType: 'SUPER_ADMIN' });
    const wrapper = mount(LoginView, { global: { stubs: { RouterLink: true } } });
    await submit(wrapper);

    expect(mocks.logout).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain('该账号不是参赛队账号');
    expect(mocks.replace).not.toHaveBeenCalled();
  });
});
