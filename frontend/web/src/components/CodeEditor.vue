<template>
  <div ref="host" class="code-editor" :style="{ height }" />
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import * as monaco from 'monaco-editor/editor/editor.api';
import 'monaco-editor/languages/definitions/cpp/register';
import 'monaco-editor/languages/definitions/java/register';
import 'monaco-editor/languages/definitions/python/register';
import 'monaco-editor/languages/definitions/markdown/register';
import 'monaco-editor/editor/contrib/suggest/browser/suggestController.js';
import editorWorker from 'monaco-editor/editor/editor.worker.js?worker';
import { registerCompletionProviders } from './monacoCompletion';

self.MonacoEnvironment = {
  getWorker: () => new editorWorker(),
};

const props = withDefaults(
  defineProps<{
    modelValue: string;
    language?: string;
    readonly?: boolean;
    height?: string;
    placeholder?: string;
  }>(),
  { language: 'cpp', readonly: false, height: '360px', placeholder: '' },
);
const emit = defineEmits<{ (event: 'update:modelValue', value: string): void }>();

const host = ref<HTMLElement | null>(null);
let editor: monaco.editor.IStandaloneCodeEditor | null = null;
let syncTimer: number | undefined;

function languageId(language: string): string {
  if (language === 'java') return 'java';
  if (language === 'python') return 'python';
  if (language === 'markdown') return 'markdown';
  return 'cpp';
}

onMounted(() => {
  if (!host.value) return;
  registerCompletionProviders();
  editor = monaco.editor.create(host.value, {
    value: props.modelValue,
    language: languageId(props.language),
    readOnly: props.readonly,
    automaticLayout: true,
    minimap: { enabled: false },
    fontSize: 13,
    lineHeight: 20,
    tabSize: 4,
    insertSpaces: true,
    scrollBeyondLastLine: false,
    renderLineHighlight: 'line',
    roundedSelection: false,
    folding: true,
    glyphMargin: false,
    wordWrap: 'off',
    placeholder: props.placeholder || undefined,
    theme: 'vs',
  });
  editor.onDidChangeModelContent(() => {
    if (syncTimer) window.clearTimeout(syncTimer);
    syncTimer = window.setTimeout(() => {
      emit('update:modelValue', editor?.getValue() ?? '');
    }, 200);
  });
});

watch(
  () => props.modelValue,
  (value) => {
    if (editor && editor.getValue() !== value) editor.setValue(value);
  },
);

watch(
  () => props.language,
  (language) => {
    const model = editor?.getModel();
    if (model) monaco.editor.setModelLanguage(model, languageId(language));
  },
);

watch(
  () => props.readonly,
  (readonly) => editor?.updateOptions({ readOnly: readonly }),
);

onBeforeUnmount(() => {
  if (syncTimer) window.clearTimeout(syncTimer);
  editor?.dispose();
  editor = null;
});

defineExpose({ getEditor: () => editor });
</script>

<style scoped>
.code-editor {
  width: 100%;
  border: 1px solid var(--border);
}
</style>
