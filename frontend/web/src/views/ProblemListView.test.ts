import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProblemListView from './ProblemListView.vue';

const mocks = vi.hoisted(() => ({ listProblems: vi.fn(), push: vi.fn() }));
const route = { params: { contestId: '7' } };
vi.mock('../api/contest', () => ({ contestApi: { listProblems: mocks.listProblems } }));
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push: mocks.push }) }));

const problem = {
  problemId: 3,
  contestId: 7,
  slug: 'balloons',
  alias: 'A',
  displayOrder: 1,
  title: 'Balloons',
  color: '#2563eb',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 64,
  languages: ['cpp'],
  statement: null,
};

describe('ProblemListView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listProblems.mockResolvedValue([problem]);
  });

  it('loads contest-scoped problems and opens the selected problem by keyboard', async () => {
    const wrapper = mount(ProblemListView);
    await flushPromises();

    expect(mocks.listProblems).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('Balloons');
    await wrapper.find('.problem-card').trigger('keyup.enter');
    expect(mocks.push).toHaveBeenCalledWith('/contests/7/problems/3');
  });
});
