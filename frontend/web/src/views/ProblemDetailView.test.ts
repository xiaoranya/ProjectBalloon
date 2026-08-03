import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProblemDetailView from './ProblemDetailView.vue';
import { contestApi } from '../api/contest';

const push = vi.fn();
const route = { params: { contestId: '7', problemId: '1' } };
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push }) }));
vi.mock('../api/contest', () => ({ contestApi: { listProblems: vi.fn(), submit: vi.fn() } }));
vi.mock('../components/CodeEditor.vue', () => ({
  default: {
    name: 'CodeEditor',
    props: ['modelValue', 'language', 'readonly', 'height'],
    emits: ['update:modelValue'],
    template:
      '<textarea :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
  },
}));

const problem = {
  problemId: 1,
  contestId: 7,
  slug: 'a-problem',
  alias: 'A',
  displayOrder: 1,
  title: 'A problem',
  color: '#2563eb',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 64,
  languages: ['cpp', 'output'],
  statement: { langCode: 'en', renderedHtml: '<p>Statement</p>', updatedAt: '' },
};

describe('ProblemDetailView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    push.mockReset();
    vi.mocked(contestApi.listProblems).mockResolvedValue([problem]);
    vi.mocked(contestApi.submit).mockResolvedValue({
      submissionId: 9,
      judgementId: 'j-9',
      status: 'PENDING',
      submittedAt: '',
    });
  });

  function mountView() {
    return mount(ProblemDetailView, {
      props: { contest: { id: 7, name: 'Finals', status: 'RUNNING' } as never },
      global: { stubs: { RouterLink: true, UploadFilled: true } },
    });
  }

  it('rejects source larger than 64 KiB before calling the submit API', async () => {
    const wrapper = mountView();
    await flushPromises();

    await wrapper.find('textarea').setValue('x'.repeat(65_537));
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.text()).toContain('提交内容不能超过 64 KiB');
    expect(contestApi.submit).not.toHaveBeenCalled();
  });

  it('submits the editor source as a Main.<ext> file and navigates to the submission detail', async () => {
    const wrapper = mountView();
    await flushPromises();

    await wrapper.find('textarea').setValue('int main() { return 0; }');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(contestApi.submit).toHaveBeenCalledTimes(1);
    const [contestId, problemId, language, file] = vi.mocked(contestApi.submit).mock.calls[0];
    expect(contestId).toBe(7);
    expect(problemId).toBe(1);
    expect(language).toBe('cpp');
    expect(file).toBeInstanceOf(File);
    expect(file.name).toBe('Main.cpp');
    await expect(file.text()).resolves.toBe('int main() { return 0; }');
    expect(push).toHaveBeenCalledWith('/contests/7/submissions/9');
  });

  it('submits an output ZIP through the upload control', async () => {
    const wrapper = mountView();
    await flushPromises();

    await wrapper.findComponent({ name: 'ElSelect' }).vm.$emit('update:modelValue', 'output');
    await flushPromises();
    const onChange = wrapper.findComponent({ name: 'ElUpload' }).props('onChange') as (
      file: unknown,
    ) => void;
    const file = { name: 'out.zip', size: 12 } as File;
    onChange({ raw: file });
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(contestApi.submit).toHaveBeenCalledWith(7, 1, 'output', file);
    expect(push).toHaveBeenCalledWith('/contests/7/submissions/9');
  });

  it('submits an uploaded source file directly in file mode', async () => {
    const wrapper = mountView();
    await flushPromises();

    await wrapper.findComponent({ name: 'ElSegmented' }).vm.$emit('update:modelValue', 'file');
    await flushPromises();
    const onChange = wrapper.findComponent({ name: 'ElUpload' }).props('onChange') as (
      file: unknown,
    ) => void;
    const file = { name: 'solution.cc', size: 23 } as File;
    onChange({ raw: file });
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(contestApi.submit).toHaveBeenCalledWith(7, 1, 'cpp', file);
    expect(push).toHaveBeenCalledWith('/contests/7/submissions/9');
  });

  it('rejects a source file whose extension does not match the language', async () => {
    const wrapper = mountView();
    await flushPromises();

    await wrapper.findComponent({ name: 'ElSegmented' }).vm.$emit('update:modelValue', 'file');
    await flushPromises();
    const onChange = wrapper.findComponent({ name: 'ElUpload' }).props('onChange') as (
      file: unknown,
    ) => void;
    onChange({ raw: { name: 'Main.py', size: 12 } as File });
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.text()).toContain('源码文件扩展名需匹配所选语言');
    expect(contestApi.submit).not.toHaveBeenCalled();
  });
});
