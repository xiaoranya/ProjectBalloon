import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SubmissionDetailView from './SubmissionDetailView.vue';

const mocks = vi.hoisted(() => ({ getSubmission: vi.fn(), push: vi.fn() }));
const route = { params: { contestId: '7', submissionId: '9' } };
vi.mock('../api/contest', () => ({ contestApi: { getSubmission: mocks.getSubmission } }));
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push: mocks.push }) }));
vi.mock('../components/CodeEditor.vue', () => ({
  default: {
    name: 'CodeEditor',
    props: ['modelValue', 'language', 'readonly', 'height'],
    emits: ['update:modelValue'],
    template:
      '<textarea :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
  },
}));

const submission = {
  id: 9,
  contestId: 7,
  problemId: 1,
  problemAlias: 'A',
  teamId: 2,
  teamName: 'Blue Team',
  language: 'cpp',
  sourceSizeBytes: 22,
  status: 'ACCEPTED',
  submittedAt: '2026-08-01T09:00:00Z',
  judgedAt: '2026-08-01T09:01:00Z',
  activeJudgementId: 'j-9',
  verdict: 'ACCEPTED',
  totalTimeMs: 12,
  peakMemoryKb: 2048,
  scoreMilli: null,
  source: 'int main() { return 0; }',
  sourceSha256: null,
  judgements: [
    {
      id: 'j-9',
      verdict: 'ACCEPTED',
      totalTimeMs: 12,
      peakMemoryKb: 2048,
      compileLog: 'compiled',
      workerId: 'worker-1',
      startedAt: '2026-08-01T09:00:10Z',
      completedAt: '2026-08-01T09:00:12Z',
      createdAt: '2026-08-01T09:00:10Z',
      version: 1,
      superseded: false,
      active: true,
      scoreMilli: null,
      runs: [
        {
          testIndex: 1,
          verdict: 'ACCEPTED',
          timeMs: 12,
          memoryKb: 2048,
          exitCode: 0,
          stderrTail: null,
        },
      ],
      subtaskScores: [],
    },
  ],
};

describe('SubmissionDetailView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getSubmission.mockResolvedValue(submission);
  });

  it('loads and renders the active judgement, test run, and source', async () => {
    const wrapper = mount(SubmissionDetailView);
    await flushPromises();

    expect(mocks.getSubmission).toHaveBeenCalledWith(7, 9);
    expect(wrapper.text()).toContain('答案正确');
    expect(wrapper.text()).toContain('compiled');
    const sourceEditor = wrapper.find('textarea').element as HTMLTextAreaElement;
    expect(sourceEditor.value).toContain('int main() { return 0; }');
    expect(wrapper.text()).toContain('12 ms');
    wrapper.unmount();
  });

  it('uses the contest-scoped back route', async () => {
    const wrapper = mount(SubmissionDetailView);
    await flushPromises();
    await wrapper.find('button').trigger('click');

    expect(mocks.push).toHaveBeenCalledWith('/contests/7/submissions');
    wrapper.unmount();
  });
});
