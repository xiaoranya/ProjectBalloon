import { flushPromises, mount, type DOMWrapper } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AttachmentsTab from './AttachmentsTab.vue';
import BasicInfoTab from './BasicInfoTab.vue';
import PublicationPanel from './PublicationPanel.vue';
import StatementsTab from './StatementsTab.vue';
import TestdataTab from './TestdataTab.vue';
import { adminProblemApi, type ProblemPublication } from '../../api/admin-problems';
import { ApiError } from '../../api/client';
import type {
  ProblemAttachmentResponse,
  ProblemResponse,
  ProblemStatementResponse,
  ProblemTestdataVersionResponse,
} from '../../api/types';

const createEditor = vi.hoisted(() => vi.fn());
vi.mock('monaco-editor/editor/editor.api', () => ({
  editor: { create: createEditor, setModelLanguage: vi.fn() },
}));
vi.mock('monaco-editor/languages/definitions/cpp/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/java/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/python/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/markdown/register', () => ({}));
vi.mock('monaco-editor/language/json/monaco.contribution.js', () => ({}));
vi.mock('monaco-editor/editor/contrib/suggest/browser/suggestController.js', () => ({}));
vi.mock('monaco-editor/editor/editor.worker.js?worker', () => ({ default: class {} }));
vi.mock('monaco-editor/language/json/json.worker.js?worker', () => ({ default: class {} }));
vi.mock('../monacoCompletion', () => ({ registerCompletionProviders: vi.fn() }));

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: elementMocks.success, error: elementMocks.error },
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});
const routerMocks = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock('vue-router', () => ({ useRouter: () => routerMocks }));
vi.mock('../../api/admin-problems', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/admin-problems')>();
  return {
    ...actual,
    adminProblemApi: {
      ...actual.adminProblemApi,
      createProblem: vi.fn(),
      updateProblem: vi.fn(),
      getProblem: vi.fn(),
      uploadInteractor: vi.fn(),
      getPublication: vi.fn(),
      updatePublication: vi.fn(),
      upsertStatement: vi.fn(),
      deleteStatement: vi.fn(),
      uploadAttachment: vi.fn(),
      deleteAttachment: vi.fn(),
      downloadAttachment: vi.fn(),
      uploadTestdata: vi.fn(),
      listTestdataVersions: vi.fn(),
      downloadTestdata: vi.fn(),
      downloadTestdataVersion: vi.fn(),
      activateTestdataVersion: vi.fn(),
    },
  };
});

const problem: ProblemResponse = {
  id: 12,
  slug: 'two-sum',
  title: 'Two Sum',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['c', 'cpp'],
  testdataVersion: 3,
  testdataSha256: 'a'.repeat(64),
  defaultLangCode: 'en',
  createdBy: 1,
  version: 3,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:00:00Z',
  judgeMode: 'STANDARD',
  interactorObjectKey: null,
  interactorSha256: null,
};
const statement: ProblemStatementResponse = {
  problemId: 12,
  langCode: 'en',
  body: '# Hello',
  renderedHtml: '<h1>Hello</h1>',
  updatedAt: '2026-07-20T08:00:00Z',
};
const attachment: ProblemAttachmentResponse = {
  id: 21,
  problemId: 12,
  kind: 'SAMPLE',
  originalFilename: 'sample.zip',
  contentType: 'application/zip',
  bytes: 1536,
  sha256: 'b'.repeat(64),
  createdAt: '2026-07-20T08:00:00Z',
};
const testdataVersion: ProblemTestdataVersionResponse = {
  problemId: 12,
  version: 3,
  caseCount: 25,
  bytes: 2048,
  sha256: 'a'.repeat(64),
  createdAt: '2026-07-20T08:00:00Z',
  uploadedByUserId: 1,
  active: true,
};
const oldTestdataVersion: ProblemTestdataVersionResponse = {
  ...testdataVersion,
  version: 1,
  caseCount: null,
  bytes: null,
  sha256: 'c'.repeat(64),
  active: false,
};
const publication: ProblemPublication = {
  visibility: 'PRIVATE',
  difficulty: 2,
  tags: ['dp', 'Greedy'],
  publishedAt: null,
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

function setFiles(input: DOMWrapper<Element>, file: File) {
  Object.defineProperty(input.element, 'files', { value: [file], configurable: true });
}

function stubMonacoEditor() {
  let contentListener: (() => void) | null = null;
  const editor = {
    value: '',
    onDidChangeModelContent: (listener: () => void) => {
      contentListener = listener;
      return { dispose: () => undefined };
    },
    getValue: () => editor.value,
    setValue: (value: string) => {
      editor.value = value;
    },
    getModel: () => null,
    updateOptions: () => undefined,
    dispose: () => undefined,
    triggerContentChange: () => contentListener?.(),
  };
  createEditor.mockReturnValue(editor);
  return editor;
}

describe('problem-editor panels', () => {
  beforeEach(() => {
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    elementMocks.confirm.mockReset();
    routerMocks.replace.mockReset();
    createEditor.mockReset();
    stubMonacoEditor();
    vi.stubGlobal(
      'URL',
      Object.assign(URL, {
        createObjectURL: vi.fn(() => 'blob:mock'),
        revokeObjectURL: vi.fn(),
      }),
    );
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
  });

  it('BasicInfoTab prefills the form and creates new problems', async () => {
    vi.mocked(adminProblemApi.createProblem).mockResolvedValue({ ...problem, id: 13, version: 1 });
    const wrapper = mount(BasicInfoTab, { props: { problem: null, isNew: true } });
    await flushPromises();
    const inputs = wrapper.findAll('input.el-input__inner');
    await inputs[0].setValue('two-sum');
    await inputs[1].setValue('Two Sum');
    const vm = wrapper.vm as unknown as { save: () => Promise<void> };
    await vm.save();
    await flushPromises();
    expect(adminProblemApi.createProblem).toHaveBeenCalledWith(
      expect.objectContaining({ slug: 'two-sum', title: 'Two Sum', defaultLangCode: 'en' }),
    );
    expect(routerMocks.replace).toHaveBeenCalledWith('/admin/problems/13');
    expect(wrapper.emitted('problem-refreshed')![0][0]).toEqual({ ...problem, id: 13, version: 1 });
    expect(elementMocks.success).toHaveBeenCalled();
    wrapper.unmount();

    const blocked = mount(BasicInfoTab, { props: { problem: null, isNew: true } });
    await flushPromises();
    await (blocked.vm as unknown as { save: () => Promise<void> }).save();
    await flushPromises();
    expect(vi.mocked(adminProblemApi.createProblem).mock.calls.length).toBe(1);
    blocked.unmount();
  });

  it('BasicInfoTab updates with expectedVersion and recovers from version conflicts', async () => {
    vi.mocked(adminProblemApi.updateProblem).mockResolvedValue({ ...problem, version: 4 });
    const wrapper = mount(BasicInfoTab, { props: { problem, isNew: false } });
    await flushPromises();
    expect(wrapper.text()).toContain('当前并发版本为 3');
    await (wrapper.vm as unknown as { save: () => Promise<void> }).save();
    await flushPromises();
    expect(adminProblemApi.updateProblem).toHaveBeenCalledWith(
      12,
      expect.objectContaining({ slug: 'two-sum', expectedVersion: 3 }),
    );
    expect(elementMocks.success).toHaveBeenCalled();
    wrapper.unmount();

    vi.mocked(adminProblemApi.updateProblem).mockRejectedValueOnce(
      new ApiError(409, 'PROBLEM_VERSION_STALE', 'problem version stale'),
    );
    vi.mocked(adminProblemApi.getProblem).mockResolvedValue({ ...problem, version: 5 });
    const conflicted = mount(BasicInfoTab, { props: { problem, isNew: false } });
    await flushPromises();
    await (conflicted.vm as unknown as { save: () => Promise<void> }).save();
    await flushPromises();
    expect(adminProblemApi.getProblem).toHaveBeenCalledWith(12);
    const messages = conflicted.emitted('error-message')!;
    expect(messages).toHaveLength(2);
    expect(String(messages[1][0])).toContain('题目已被其他管理员修改');
    conflicted.unmount();
  });

  it('BasicInfoTab switches judge modes and exposes interactor fields', async () => {
    const wrapper = mount(BasicInfoTab, { props: { problem, isNew: false } });
    await flushPromises();
    const select = wrapper.findComponent({ name: 'ElSelect' });
    (select.vm as unknown as { $emit: (event: string, value: string) => void }).$emit(
      'update:modelValue',
      'INTERACTIVE',
    );
    await flushPromises();
    expect(wrapper.text()).toContain('Interactor 对象键');
    (select.vm as unknown as { $emit: (event: string, value: string) => void }).$emit(
      'update:modelValue',
      'OUTPUT_ONLY',
    );
    (select.vm as unknown as { $emit: (event: string, value: string) => void }).$emit(
      'change',
      'OUTPUT_ONLY',
    );
    await flushPromises();
    vi.mocked(adminProblemApi.updateProblem).mockResolvedValue(problem);
    await (wrapper.vm as unknown as { save: () => Promise<void> }).save();
    await flushPromises();
    expect(adminProblemApi.updateProblem).toHaveBeenCalledWith(
      12,
      expect.objectContaining({ judgeMode: 'OUTPUT_ONLY', languages: ['output'] }),
    );
    wrapper.unmount();
  });

  it('PublicationPanel loads, publishes, and reports failures', async () => {
    const updated: ProblemPublication = {
      visibility: 'PUBLIC',
      difficulty: 2,
      tags: ['dp', 'greedy'],
      publishedAt: '2026-07-22T01:00:00Z',
    };
    vi.mocked(adminProblemApi.getPublication).mockResolvedValue(publication);
    vi.mocked(adminProblemApi.updatePublication).mockResolvedValue(updated);
    const wrapper = mount(PublicationPanel, { props: { problemId: 12 } });
    await flushPromises();
    expect(wrapper.text()).toContain('当前状态：未发布');
    expect(
      (wrapper.find('input[placeholder="例如 dp, greedy, graph"]').element as HTMLInputElement)
        .value,
    ).toBe('dp, Greedy');
    await wrapper.find('input[placeholder="例如 dp, greedy, graph"]').setValue(' dp, Greedy ,');
    await button(wrapper, '保存并保持私有').trigger('click');
    await flushPromises();
    expect(adminProblemApi.updatePublication).toHaveBeenCalledWith(12, {
      visibility: 'PRIVATE',
      difficulty: 2,
      tags: ['dp', 'greedy'],
    });
    expect(wrapper.emitted('updated')![0][0]).toEqual(updated);
    expect(wrapper.text()).toContain('当前状态：已发布');
    expect(wrapper.text()).toContain('发布到公开题库');
    wrapper.unmount();

    vi.mocked(adminProblemApi.getPublication).mockRejectedValueOnce(
      new ApiError(404, 'PROBLEM_NOT_FOUND', 'problem not found'),
    );
    const failed = mount(PublicationPanel, { props: { problemId: 12 } });
    await flushPromises();
    expect(failed.text()).toContain('题目不存在或不可访问');
    failed.unmount();
  });

  it('StatementsTab saves, rejects invalid drafts, and deletes after confirmation', async () => {
    const refreshedProblem = { ...problem, version: 4 };
    vi.mocked(adminProblemApi.upsertStatement).mockResolvedValue({
      result: statement,
      problem: refreshedProblem,
      refreshFailed: false as const,
    });
    vi.mocked(adminProblemApi.deleteStatement).mockResolvedValue(undefined);
    const wrapper = mount(StatementsTab, {
      props: { problem, statements: [statement] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('已保存');
    expect(wrapper.find('input[disabled]').exists()).toBe(true);
    await button(wrapper, '保存此语言题面').trigger('click');
    await flushPromises();
    expect(adminProblemApi.upsertStatement).toHaveBeenCalledWith(12, 'en', '# Hello');
    expect(wrapper.emitted('problem-refreshed')![0][0]).toEqual(refreshedProblem);

    await button(wrapper, '添加语言').trigger('click');
    const saveButtons = wrapper
      .findAll('button')
      .filter((row) => row.text().includes('保存此语言题面'));
    await saveButtons[saveButtons.length - 1].trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(vi.mocked(adminProblemApi.upsertStatement).mock.calls.length).toBe(1);

    elementMocks.confirm.mockResolvedValue(undefined);
    await button(wrapper, '删除题面').trigger('click');
    await flushPromises();
    expect(adminProblemApi.deleteStatement).toHaveBeenCalledWith(12, 'en');
    expect(wrapper.text()).not.toContain('已保存');
    wrapper.unmount();

    elementMocks.confirm.mockRejectedValue('cancel');
    const guarded = mount(StatementsTab, { props: { problem, statements: [statement] } });
    await flushPromises();
    await button(guarded, '删除题面').trigger('click');
    await flushPromises();
    expect(vi.mocked(adminProblemApi.deleteStatement).mock.calls.length).toBe(1);
    guarded.unmount();
  });

  it('StatementsTab blocks duplicate languages', async () => {
    const wrapper = mount(StatementsTab, {
      props: { problem, statements: [statement, { ...statement, problemId: 12 }] },
    });
    await flushPromises();
    const upsertCalls = vi.mocked(adminProblemApi.upsertStatement).mock.calls.length;
    await button(wrapper, '保存此语言题面').trigger('click');
    await flushPromises();
    expect(vi.mocked(adminProblemApi.upsertStatement).mock.calls.length).toBe(upsertCalls);
    expect(elementMocks.error).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('TestdataTab lists versions, downloads, uploads, and activates', async () => {
    vi.mocked(adminProblemApi.downloadTestdata).mockResolvedValue(new Blob([new Uint8Array([1])]));
    vi.mocked(adminProblemApi.listTestdataVersions).mockResolvedValue([
      oldTestdataVersion,
      { ...testdataVersion, version: 4 },
    ]);
    vi.mocked(adminProblemApi.activateTestdataVersion).mockResolvedValue({
      result: oldTestdataVersion,
      problem,
      refreshFailed: false as const,
    });
    elementMocks.confirm.mockResolvedValue(undefined);
    const wrapper = mount(TestdataTab, {
      props: { problem, initialTestdataVersions: [testdataVersion, oldTestdataVersion] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('v3');
    expect(wrapper.text()).toContain('25');
    const activateButtons = wrapper.findAll('button').filter((row) => row.text().includes('激活'));
    expect(activateButtons).toHaveLength(2);
    expect(activateButtons[0].attributes('disabled')).toBeDefined();
    await button(wrapper, '下载当前 ZIP').trigger('click');
    await flushPromises();
    expect(adminProblemApi.downloadTestdata).toHaveBeenCalledWith(12, expect.anything());

    await activateButtons[1].trigger('click');
    await flushPromises();
    expect(adminProblemApi.activateTestdataVersion).toHaveBeenCalledWith(12, 1, 3);
    expect(wrapper.emitted('problem-refreshed')![0][0]).toEqual(problem);
    expect(adminProblemApi.listTestdataVersions).toHaveBeenCalledWith(12);
    wrapper.unmount();

    const file = new File([new Uint8Array([1, 2, 3])], 'data.zip', { type: 'application/zip' });
    const uploader = mount(TestdataTab, {
      props: { problem, initialTestdataVersions: [testdataVersion] },
    });
    await flushPromises();
    vi.mocked(adminProblemApi.uploadTestdata).mockResolvedValue({
      result: { ...testdataVersion, version: 4 },
      problem,
      refreshFailed: false as const,
    });
    setFiles(uploader.find('input[type="file"]'), file);
    await uploader.find('input[type="file"]').trigger('change');
    await flushPromises();
    expect(uploader.text()).toContain('data.zip');
    await button(uploader, '上传新版本').trigger('click');
    await flushPromises();
    expect(adminProblemApi.uploadTestdata).toHaveBeenCalledWith(12, file);
    expect(elementMocks.success).toHaveBeenCalled();
    uploader.unmount();

    const rejected = mount(TestdataTab, {
      props: { problem, initialTestdataVersions: [testdataVersion] },
    });
    await flushPromises();
    setFiles(rejected.find('input[type="file"]'), new File([new Uint8Array([1])], 'data.txt'));
    await rejected.find('input[type="file"]').trigger('change');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(button(rejected, '上传新版本').attributes('disabled')).toBeDefined();
    rejected.unmount();
  });

  it('AttachmentsTab uploads, lists, downloads, and removes attachments', async () => {
    const refreshedProblem = { ...problem, version: 4 };
    vi.mocked(adminProblemApi.uploadAttachment).mockResolvedValue({
      result: attachment,
      problem: refreshedProblem,
      refreshFailed: false as const,
    });
    vi.mocked(adminProblemApi.deleteAttachment).mockResolvedValue({
      result: undefined,
      problem: refreshedProblem,
      refreshFailed: false as const,
    });
    vi.mocked(adminProblemApi.downloadAttachment).mockResolvedValue(
      new Blob([new Uint8Array([1])]),
    );
    elementMocks.confirm.mockResolvedValue(undefined);
    const wrapper = mount(AttachmentsTab, {
      props: { problem, initialAttachments: [attachment] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('sample.zip');
    expect(wrapper.text()).toContain('SAMPLE');
    await button(wrapper, '下载').trigger('click');
    await flushPromises();
    expect(adminProblemApi.downloadAttachment).toHaveBeenCalledWith(12, 21);

    const file = new File([new Uint8Array([1])], 'notes.pdf', { type: 'application/pdf' });
    setFiles(wrapper.find('input[type="file"]'), file);
    await wrapper.find('input[type="file"]').trigger('change');
    await flushPromises();
    expect(wrapper.text()).toContain('notes.pdf');
    await button(wrapper, '上传附件').trigger('click');
    await flushPromises();
    expect(adminProblemApi.uploadAttachment).toHaveBeenCalledWith(12, 'SAMPLE', file);
    expect(wrapper.emitted('problem-refreshed')![0][0]).toEqual(refreshedProblem);

    await button(wrapper, '删除').trigger('click');
    await flushPromises();
    expect(adminProblemApi.deleteAttachment).toHaveBeenCalledWith(12, 21);
    expect(wrapper.text()).not.toContain('sample.zip');
    wrapper.unmount();
  });
});
