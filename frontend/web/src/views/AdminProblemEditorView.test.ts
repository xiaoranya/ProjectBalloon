import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminProblemEditorView from './AdminProblemEditorView.vue';

const {
  getProblem,
  updateProblem,
  listTestdataVersions,
  listAttachments,
  listStatements,
  deleteStatement,
  routeParams,
  routeQuery,
  routerPush,
} = vi.hoisted(() => ({
  getProblem: vi.fn(),
  updateProblem: vi.fn(),
  listTestdataVersions: vi.fn(),
  listAttachments: vi.fn(),
  listStatements: vi.fn(),
  deleteStatement: vi.fn(),
  routeParams: { problemId: '7' as string | undefined },
  routeQuery: {} as Record<string, string>,
  routerPush: vi.fn(),
}));
vi.mock('../api/admin-problems', () => ({
  adminProblemApi: {
    getProblem,
    updateProblem,
    createProblem: vi.fn(),
    upsertStatement: vi.fn(),
    uploadAttachment: vi.fn(),
    deleteAttachment: vi.fn(),
    downloadAttachment: vi.fn(),
    uploadTestdata: vi.fn(),
    downloadTestdata: vi.fn(),
    listTestdataVersions,
    downloadTestdataVersion: vi.fn(),
    activateTestdataVersion: vi.fn(),
    listAttachments,
    listStatements,
    deleteStatement,
  },
}));
vi.mock('vue-router', () => ({
  useRoute: () => ({ params: routeParams, query: routeQuery }),
  useRouter: () => ({ push: routerPush, replace: vi.fn() }),
}));
vi.mock('../components/CodeEditor.vue', () => ({
  default: {
    name: 'CodeEditor',
    props: ['modelValue', 'language', 'readonly', 'height', 'placeholder'],
    emits: ['update:modelValue'],
    template: '<textarea :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
  },
}));

const problem = {
  id: 7,
  slug: 'two-sum',
  title: 'Two Sum',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['cpp'],
  testdataVersion: 1,
  testdataSha256: 'abc',
  defaultLangCode: 'en',
  createdBy: 1,
  version: 3,
  createdAt: '2026-07-20T00:00:00Z',
  updatedAt: '2026-07-20T00:00:00Z',
};

describe('AdminProblemEditorView', () => {
  beforeEach(() => {
    routeParams.problemId = '7';
    delete routeQuery.contestId;
    routerPush.mockReset();
    getProblem.mockResolvedValue(problem);
    listTestdataVersions.mockResolvedValue([
      {
        problemId: 7,
        version: 1,
        caseCount: 1,
        bytes: 20,
        sha256: 'abc',
        uploadedByUserId: 1,
        active: true,
        createdAt: '2026-07-20T00:00:00Z',
      },
    ]);
    listAttachments.mockResolvedValue([]);
    listStatements.mockResolvedValue([
      {
        problemId: 7,
        langCode: 'en',
        body: '# Existing statement',
        renderedHtml: '<h1>Existing statement</h1>',
        updatedAt: '2026-07-20T00:00:00Z',
      },
    ]);
    updateProblem.mockResolvedValue({ ...problem, version: 4 });
  });

  it('returns scoped contest managers to their contest workbench', async () => {
    routeQuery.contestId = '42';
    const wrapper = mount(AdminProblemEditorView);
    await flushPromises();
    const back = wrapper.findAll('button').find((button) => button.text().includes('返回题库'))!;

    await back.trigger('click');

    expect(routerPush).toHaveBeenCalledWith('/admin/contests/42');
  });

  it('loads only the Rust Problem read model and communicates unsupported aggregate reads', async () => {
    const wrapper = mount(AdminProblemEditorView);
    await flushPromises();

    expect(getProblem).toHaveBeenCalledWith(7);
    expect(listTestdataVersions).toHaveBeenCalledWith(7);
    expect(listAttachments).toHaveBeenCalledWith(7);
    expect(listStatements).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('Two Sum');
    expect(wrapper.text()).toContain('expectedVersion');
    expect(wrapper.text()).toContain('当前测试数据版本');
    expect(wrapper.find('textarea').element.value).toBe('# Existing statement');
    expect(wrapper.text()).toContain('v1');
  });

  it('submits the loaded optimistic-concurrency version when saving metadata', async () => {
    const wrapper = mount(AdminProblemEditorView);
    await flushPromises();
    const saveButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存基本信息'));
    await saveButton?.trigger('click');
    await flushPromises();

    expect(updateProblem).toHaveBeenCalledWith(
      7,
      expect.objectContaining({
        expectedVersion: 3,
        slug: 'two-sum',
        title: 'Two Sum',
        languages: ['cpp'],
        defaultLangCode: 'en',
      }),
    );
  });
});
