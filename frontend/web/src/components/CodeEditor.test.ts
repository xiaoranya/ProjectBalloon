import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

const createEditor = vi.hoisted(() => vi.fn());

vi.mock('monaco-editor/editor/editor.api', () => ({
  editor: {
    create: createEditor,
    setModelLanguage: vi.fn(),
  },
}));
vi.mock('monaco-editor/languages/definitions/cpp/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/java/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/python/register', () => ({}));
vi.mock('monaco-editor/languages/definitions/markdown/register', () => ({}));
vi.mock('monaco-editor/language/json/monaco.contribution.js', () => ({}));
vi.mock('monaco-editor/editor/contrib/suggest/browser/suggestController.js', () => ({}));
vi.mock('monaco-editor/editor/editor.worker.js?worker', () => ({ default: class {} }));
vi.mock('monaco-editor/language/json/json.worker.js?worker', () => ({ default: class {} }));
vi.mock('./monacoCompletion', () => ({ registerCompletionProviders: vi.fn() }));

import CodeEditor from './CodeEditor.vue';

const maliciousSource = '<script>alert("xss")</script>\nint main() { return 0; }';

function stubEditor() {
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

describe('CodeEditor', () => {
  beforeEach(() => {
    createEditor.mockReset();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('hands the bound v-model content to monaco as plain text, never as HTML', () => {
    const editor = stubEditor();
    const wrapper = mount(CodeEditor, { props: { modelValue: maliciousSource } });

    expect(createEditor).toHaveBeenCalledTimes(1);
    const options = createEditor.mock.calls[0][1] as { value: string };
    expect(options.value).toBe(maliciousSource);

    const host = wrapper.find('.code-editor').element;
    expect(host.children).toHaveLength(0);
    expect(host.innerHTML).toBe('');
    expect(wrapper.find('script').exists()).toBe(false);
    expect(editor.value).toBe('');
  });

  it('pushes external v-model updates into the editor as text', async () => {
    stubEditor();
    const wrapper = mount(CodeEditor, { props: { modelValue: 'int a;' } });
    const editor = createEditor.mock.results[0].value as ReturnType<typeof stubEditor>;

    await wrapper.setProps({ modelValue: maliciousSource });
    await nextTick();

    expect(editor.value).toBe(maliciousSource);
  });

  it('emits every content change straight back to v-model without a debounce window', async () => {
    const editor = stubEditor();
    const wrapper = mount(CodeEditor, { props: { modelValue: '' } });

    editor.value = 'int x;';
    editor.triggerContentChange();
    await nextTick();
    expect(wrapper.emitted('update:modelValue')).toEqual([['int x;']]);

    // A submit fired immediately after another keystroke must not miss it.
    editor.value = maliciousSource;
    editor.triggerContentChange();
    await nextTick();
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([maliciousSource]);
  });

  it('does not echo programmatic v-model updates back to the parent', async () => {
    stubEditor();
    const wrapper = mount(CodeEditor, { props: { modelValue: 'int a;' } });
    const editor = createEditor.mock.results[0].value as ReturnType<typeof stubEditor>;

    await wrapper.setProps({ modelValue: maliciousSource });
    await nextTick();

    expect(editor.value).toBe(maliciousSource);
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});
