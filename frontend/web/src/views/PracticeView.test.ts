import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PracticeView from './PracticeView.vue';

const mocks = vi.hoisted(() => ({
  problemBank: vi.fn(),
  progress: vi.fn(),
  favorites: vi.fn(),
  submissions: vi.fn(),
  submission: vi.fn(),
  favorite: vi.fn(),
  editorial: vi.fn(),
  submit: vi.fn(),
}));

vi.mock('../api/training', () => ({ trainingApi: mocks }));
vi.mock('vue-router', () => ({ useRoute: () => ({ query: {} }) }));

const problems = [
  { id: 1, slug: 'accepted', title: '已通过题', statement: '<p>Accepted</p>', difficulty: 1, tags: ['easy'], publishedAt: null },
  { id: 2, slug: 'pending', title: '待完成题', statement: '<p>Pending</p>', difficulty: null, tags: [], publishedAt: null },
];
const submissionPage = { content: [{ id: 9, problemId: 2, problemSlug: 'pending', problemTitle: '待完成题', trainingEnrollmentId: null, language: 'cpp', sourceSizeBytes: 12, status: 'PENDING', submittedAt: '', judgedAt: null, activeJudgementId: null, verdict: null, totalTimeMs: null, peakMemoryKb: null, score: null }], page: 0, size: 100, totalElements: 1, totalPages: 1 };

describe('PracticeView', () => {
  beforeEach(() => {
    mocks.problemBank.mockResolvedValue({ content: problems, page: 0, size: 100, totalElements: 2, totalPages: 1 });
    mocks.progress.mockResolvedValue([{ problemId: 1, attempts: 2, bestScore: 100, solved: true, lastSubmissionId: 8, solvedAt: '', updatedAt: '' }, { problemId: 2, attempts: 1, bestScore: 0, solved: false, lastSubmissionId: null, solvedAt: null, updatedAt: '' }]);
    mocks.favorites.mockResolvedValue([problems[1]]);
    mocks.submissions.mockResolvedValue(submissionPage);
    mocks.favorite.mockResolvedValue({ problemId: 2, favorite: false });
    mocks.editorial.mockResolvedValue({ problemId: 2, langCode: 'en', title: '题解', bodyHtml: '<p>Idea</p>', unlockPolicy: 'ALWAYS', unlocked: true, updatedAt: '' });
    mocks.submit.mockResolvedValue({ submissionId: 10, judgementId: 'j-10', status: 'PENDING', submittedAt: '' });
  });

  it('loads practice state, selects the requested problem, and toggles favorites', async () => {
    const wrapper = mount(PracticeView);
    await flushPromises();

    expect(mocks.problemBank).toHaveBeenCalledWith(0, 100);
    expect(wrapper.text()).toContain('待完成题');
    expect(wrapper.text()).toContain('已通过');

    await wrapper.findAll('aside button').find((button) => button.text().includes('pending'))!.trigger('click');
    await wrapper.find('button[title="收藏"]').trigger('click');
    await flushPromises();
    expect(mocks.favorite).toHaveBeenCalledWith(2, false);
  });

  it('opens an editorial and submits non-empty source for the selected problem', async () => {
    const wrapper = mount(PracticeView);
    await flushPromises();

    await wrapper.findAll('aside button').find((button) => button.text().includes('pending'))!.trigger('click');
    await wrapper.findAll('button').find((button) => button.text() === '题解')!.trigger('click');
    await flushPromises();
    expect(mocks.editorial).toHaveBeenCalledWith(2);

    await wrapper.find('textarea').setValue('int main() { return 0; }');
    await wrapper.find('.submit-toolbar button').trigger('click');
    await flushPromises();
    expect(mocks.submit).toHaveBeenCalledWith(2, 'cpp', 'int main() { return 0; }', undefined, undefined);
  });
});
