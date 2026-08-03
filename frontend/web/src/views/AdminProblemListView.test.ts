import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminProblemListView from './AdminProblemListView.vue';

const { listProblems, push } = vi.hoisted(() => ({ listProblems: vi.fn(), push: vi.fn() }));
vi.mock('../api/admin-problems', () => ({
  adminProblemApi: { listProblems, deleteProblem: vi.fn() },
}));
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }));

describe('AdminProblemListView', () => {
  beforeEach(() => {
    listProblems.mockResolvedValue({
      content: [
        {
          id: 7,
          slug: 'two-sum',
          title: 'Two Sum',
          timeLimitMs: 1000,
          memoryLimitMb: 256,
          outputLimitKb: 65536,
          languages: ['cpp', 'python'],
          testdataVersion: 2,
          testdataSha256: 'abc',
          defaultLangCode: 'en',
          createdBy: 1,
          version: 3,
          createdAt: '2026-07-20T00:00:00Z',
          updatedAt: '2026-07-20T00:00:00Z',
        },
      ],
      page: 0,
      size: 50,
      totalElements: 1,
      totalPages: 1,
    });
  });

  it('loads the bounded paged list and exposes the separate create route', async () => {
    const wrapper = mount(AdminProblemListView);
    await flushPromises();

    expect(listProblems).toHaveBeenCalledWith(0, 50);
    expect(wrapper.text()).toContain('Two Sum');
    expect(wrapper.text()).toContain('v2');
    await wrapper.get('button').trigger('click');
    expect(push).toHaveBeenCalledWith('/admin/problems/new');
  });
});
