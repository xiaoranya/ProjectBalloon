import { mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import NotFoundView from './NotFoundView.vue';
import { setLocale } from '../i18n';

const push = vi.fn();

describe('NotFoundView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
  });

  it('explains the miss and routes back to the contest list', async () => {
    const wrapper = mount(NotFoundView, { global: { mocks: { $router: { push } } } });
    expect(wrapper.text()).toContain('页面不存在');
    await wrapper.findAll('button')[0].trigger('click');
    expect(push).toHaveBeenCalledWith('/contests');
  });
});
