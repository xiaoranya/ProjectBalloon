import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RegisterView from './RegisterView.vue';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  register: vi.fn(),
  replace: vi.fn(),
  state: { loading: false, user: null },
}));
vi.mock('../auth/session', () => ({
  useSession: () => ({ state: mocks.state, register: mocks.register }),
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ replace: mocks.replace }) }));

describe('RegisterView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    mocks.register.mockResolvedValue(undefined);
  });

  it('links back to the login page', () => {
    const wrapper = mount(RegisterView, { global: { stubs: { RouterLink: true } } });
    expect(wrapper.text()).toContain('注册练习账号');
    expect(wrapper.text()).toContain('注册并开始练习');
    expect(wrapper.find('router-link-stub').attributes('to')).toBe('/login');
  });

  it('registers with trimmed values and routes to practice', async () => {
    const wrapper = mount(RegisterView, { global: { stubs: { RouterLink: true } } });
    await wrapper.find('input[autocomplete="username"]').setValue('  alice  ');
    await wrapper.find('input[maxlength="128"]').setValue('  Alice  ');
    await wrapper.find('input[type="password"]').setValue('password123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(mocks.register).toHaveBeenCalledWith('alice', 'password123', 'Alice');
    expect(mocks.replace).toHaveBeenCalledWith('/practice');
  });

  it('shows business errors next to the form', async () => {
    mocks.register.mockRejectedValueOnce(new Error('用户名已被占用'));
    const wrapper = mount(RegisterView, { global: { stubs: { RouterLink: true } } });
    await wrapper.find('input[autocomplete="username"]').setValue('alice');
    await wrapper.find('input[type="password"]').setValue('password123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.text()).toContain('用户名已被占用');
    expect(mocks.replace).not.toHaveBeenCalled();
  });
});
