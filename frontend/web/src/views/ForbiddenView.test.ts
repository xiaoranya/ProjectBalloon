import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ForbiddenView from './ForbiddenView.vue';
import type { PermissionCode } from '../api/types';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  push: vi.fn(),
  user: null as { userType: 'TEAM' | 'SUPER_ADMIN'; permissions: PermissionCode[] } | null,
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ push: mocks.push }) }));
vi.mock('../auth/session', () => ({ useSession: () => ({ state: { user: mocks.user } }) }));

describe('ForbiddenView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    mocks.user = { userType: 'TEAM', permissions: [] };
  });

  it('explains the denial and routes teams back to the contest list', async () => {
    const wrapper = mount(ForbiddenView);
    expect(wrapper.text()).toContain('没有访问权限');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('返回可用入口'))!
      .trigger('click');
    await flushPromises();
    expect(mocks.push).toHaveBeenCalledWith('/contests');
  });

  it('routes staff back to their home workspace', async () => {
    mocks.user = { userType: 'SUPER_ADMIN', permissions: [] };
    const wrapper = mount(ForbiddenView);
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('返回可用入口'))!
      .trigger('click');
    await flushPromises();
    expect(mocks.push).toHaveBeenCalledWith('/admin');
  });
});
