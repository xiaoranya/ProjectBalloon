import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProfileView from './ProfileView.vue';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  updateProfile: vi.fn(),
  push: vi.fn(),
  state: {
    loading: false,
    user: {
      username: 'alice',
      displayName: 'Alice',
      userType: 'INDIVIDUAL',
      permissions: [],
      passwordResetRequired: false,
    },
  },
}));
vi.mock('../auth/session', () => ({
  useSession: () => ({ state: mocks.state, updateProfile: mocks.updateProfile }),
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ push: mocks.push }) }));

describe('ProfileView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    mocks.updateProfile.mockResolvedValue(undefined);
    mocks.state.user = { ...mocks.state.user, displayName: 'Alice' };
  });

  function mountProfile() {
    return mount(ProfileView, { global: { stubs: { RouterLink: true } } });
  }

  it('shows the frozen username and the editable display name', () => {
    const wrapper = mountProfile();
    const username = wrapper.find('input[disabled]').element as HTMLInputElement;
    expect(username.value).toBe('alice');
    const displayName = wrapper.find('input[maxlength="128"]').element as HTMLInputElement;
    expect(displayName.value).toBe('Alice');
  });

  it('refuses to save a blank display name', async () => {
    const wrapper = mountProfile();
    await wrapper.find('input[maxlength="128"]').setValue('   ');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存显示名称'))!
      .trigger('click');
    await flushPromises();

    expect(mocks.updateProfile).not.toHaveBeenCalled();
  });

  it('saves a new display name', async () => {
    const wrapper = mountProfile();
    await wrapper.find('input[maxlength="128"]').setValue('Alice Chen');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存显示名称'))!
      .trigger('click');
    await flushPromises();

    expect(mocks.updateProfile).toHaveBeenCalledWith('Alice Chen');
    expect(wrapper.text()).not.toContain('保存失败');
  });

  it('surfaces profile errors as an alert', async () => {
    mocks.updateProfile.mockRejectedValueOnce(new Error('会话已过期'));
    const wrapper = mountProfile();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存显示名称'))!
      .trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('会话已过期');
  });
});
