import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LoginView from './LoginView.vue';

const mocks = vi.hoisted(() => ({ login: vi.fn(), logout: vi.fn(), replace: vi.fn() }));
const route = { query: {} };
vi.mock('../auth/session', () => ({
  useSession: () => ({ state: { loading: false }, login: mocks.login, logout: mocks.logout }),
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
  });

  async function submit(
    wrapper: ReturnType<typeof mount>,
    username = 'alice',
    password = 'password',
  ) {
    const inputs = wrapper.findAll('input');
    await inputs[0].setValue(username);
    await inputs[1].setValue(password);
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
