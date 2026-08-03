import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import TrainingView from './TrainingView.vue';

const mocks = vi.hoisted(() => ({
  sets: vi.fn(),
  set: vi.fn(),
  enroll: vi.fn(),
}));

vi.mock('../api/training', () => ({ trainingApi: mocks }));

const sets = [
  {
    id: 7,
    slug: 'graphs',
    title: '图论基础',
    description: '基础图算法',
    visibility: 'PUBLIC',
    itemCount: 2,
  },
  {
    id: 8,
    slug: 'dp',
    title: '动态规划',
    description: '状态转移',
    visibility: 'PUBLIC',
    itemCount: 1,
  },
] as const;

const details = {
  7: {
    setInfo: sets[0],
    items: [
      {
        problemId: 1,
        slug: 'bfs',
        title: 'BFS',
        position: 1,
        required: true,
        difficulty: 2,
        tags: ['graph'],
      },
      {
        problemId: 2,
        slug: 'dfs',
        title: 'DFS',
        position: 2,
        required: false,
        difficulty: null,
        tags: [],
      },
    ],
  },
  8: {
    setInfo: sets[1],
    items: [
      {
        problemId: 3,
        slug: 'knapsack',
        title: 'Knapsack',
        position: 1,
        required: true,
        difficulty: 4,
        tags: ['dp'],
      },
    ],
  },
};

describe('TrainingView', () => {
  beforeEach(() => {
    mocks.sets.mockResolvedValue([...sets]);
    mocks.set.mockImplementation((id: number) => Promise.resolve(details[id as 7 | 8]));
    mocks.enroll.mockResolvedValue({ id: 10, setId: 7, teamId: null, status: 'ACTIVE' });
  });

  it('loads the first public set, renders required items, and switches sets', async () => {
    const wrapper = mount(TrainingView);
    await flushPromises();

    expect(mocks.sets).toHaveBeenCalledOnce();
    expect(mocks.set).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('图论基础');
    expect(wrapper.text()).toContain('BFS');
    expect(wrapper.text()).toContain('必做');

    await wrapper.findAll('.el-menu-item')[1].trigger('click');
    await flushPromises();
    expect(mocks.set).toHaveBeenLastCalledWith(8);
    expect(wrapper.text()).toContain('Knapsack');
  });

  it('enrolls in the currently selected training set', async () => {
    const wrapper = mount(TrainingView);
    await flushPromises();

    const enrollButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('加入训练'));
    expect(enrollButton).toBeDefined();
    await enrollButton!.trigger('click');
    await flushPromises();

    expect(mocks.enroll).toHaveBeenCalledWith(7);
    wrapper.unmount();
  });
});
