import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProblemBankView from './ProblemBankView.vue';

const mocks = vi.hoisted(() => ({
  problemBank: vi.fn(),
  problem: vi.fn(),
}));

vi.mock('../api/training', () => ({ trainingApi: mocks }));

const problem = {
  id: 4,
  slug: 'shortest-path',
  title: '最短路',
  statement: '<p>Find a path.</p>',
  difficulty: null,
  tags: ['graph'],
  publishedAt: null,
};

describe('ProblemBankView', () => {
  beforeEach(() => {
    mocks.problemBank.mockResolvedValue({
      content: [problem],
      page: 0,
      size: 50,
      totalElements: 1,
      totalPages: 1,
    });
    mocks.problem.mockResolvedValue(problem);
  });

  it('loads the public bank with the first page and displays nullable fields safely', async () => {
    const wrapper = mount(ProblemBankView);
    await flushPromises();

    expect(mocks.problemBank).toHaveBeenCalledWith(0, 50, undefined, undefined);
    expect(wrapper.text()).toContain('shortest-path');
    expect(wrapper.text()).toContain('未标注');
  });

  it('opens the selected problem statement from a table row', async () => {
    const wrapper = mount(ProblemBankView);
    await flushPromises();

    wrapper.findComponent({ name: 'ElTable' }).vm.$emit('row-click', problem);
    await flushPromises();

    expect(mocks.problem).toHaveBeenCalledWith('shortest-path');
    expect(wrapper.text()).toContain('Find a path.');
  });
});
