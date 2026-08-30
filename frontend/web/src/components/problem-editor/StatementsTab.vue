<template>
  <ElCard shadow="never" class="problem-editor-card">
    <template #header>
      <ElSpace wrap :size="14" class="problem-card-heading">
        <div>
          <strong>{{ t('题面编辑') }}</strong
          ><small>{{ t('Rust 后端按语言代码 upsert Markdown 题面。') }}</small>
        </div>
        <ElButton :icon="Plus" @click="addStatementDraft">{{ t('添加语言') }}</ElButton>
      </ElSpace>
    </template>
    <ElEmpty v-if="statementDrafts.length === 0" :description="t('尚未添加题面语言')" />
    <div
      v-for="(statement, index) in statementDrafts"
      :key="statement.key"
      class="statement-editor"
    >
      <ElSpace wrap :size="14" class="statement-editor-heading">
        <ElInput
          v-model="statement.langCode"
          maxlength="5"
          :placeholder="t('en 或 zh-CN')"
          :disabled="statement.savedBody !== null"
        />
        <ElTag v-if="statement.savedBody === statement.body" type="success">{{
          t('已保存')
        }}</ElTag>
        <ElButton
          v-if="statement.savedBody !== null"
          link
          type="danger"
          @click="deleteStatement(statement)"
          >{{ t('删除题面') }}</ElButton
        >
        <ElButton
          v-if="statement.savedBody === null"
          link
          type="danger"
          @click="statementDrafts.splice(index, 1)"
          >{{ t('移除草稿') }}</ElButton
        >
      </ElSpace>
      <CodeEditor
        v-model="statement.body"
        language="markdown"
        height="360px"
        :placeholder="t('Markdown 题面正文')"
      />
      <ElRow justify="end" class="problem-editor-actions">
        <ElButton type="primary" :loading="statement.saving" @click="saveStatement(statement)">{{
          t('保存此语言题面')
        }}</ElButton>
      </ElRow>
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Plus } from '@element-plus/icons-vue';
import { adminProblemApi } from '../../api/admin-problems';
import { getErrorMessage } from '../../api/client';
import type { ProblemResponse, ProblemStatementResponse } from '../../api/types';
import CodeEditor from '../CodeEditor.vue';
import { useI18n } from '../../i18n';

interface StatementDraft {
  key: number;
  langCode: string;
  body: string;
  savedBody: string | null;
  saving: boolean;
}

const props = defineProps<{
  problem: ProblemResponse | null;
  statements: ProblemStatementResponse[];
}>();
const emit = defineEmits<{
  'problem-refreshed': [value: ProblemResponse | null];
  'error-message': [message: string];
}>();
const { t } = useI18n();

const statementDrafts = ref<StatementDraft[]>([]);
let statementKey = 0;
const langCodePattern = /^[a-z]{2}(?:-[A-Z]{2})?$/;

watch(
  () => props.statements,
  (value) => {
    statementDrafts.value = value.map((statement) => ({
      key: ++statementKey,
      langCode: statement.langCode,
      body: statement.body,
      savedBody: statement.body,
      saving: false,
    }));
  },
  { immediate: true },
);

function addStatementDraft() {
  const preferred = props.problem?.defaultLangCode ?? 'en';
  const existingCodes = new Set(statementDrafts.value.map((statement) => statement.langCode));
  const langCode = existingCodes.has(preferred) ? '' : preferred;
  statementDrafts.value.push({
    key: ++statementKey,
    langCode,
    body: '',
    savedBody: null,
    saving: false,
  });
}

async function saveStatement(statement: StatementDraft) {
  if (!props.problem || !langCodePattern.test(statement.langCode)) {
    ElMessage.error(t('题面语言代码应为 en 或 zh-CN 格式'));
    return;
  }
  if (!statement.body.trim()) {
    ElMessage.error(t('请输入题面正文'));
    return;
  }
  if (
    statementDrafts.value.some(
      (item) => item.key !== statement.key && item.langCode === statement.langCode,
    )
  ) {
    ElMessage.error(t('同一语言只能保留一个题面编辑器'));
    return;
  }
  statement.saving = true;
  try {
    const refreshed = await adminProblemApi.upsertStatement(
      props.problem.id,
      statement.langCode,
      statement.body,
    );
    emit('problem-refreshed', refreshed.problem);
    statement.savedBody = statement.body;
    ElMessage.success(t('{lang} 题面已保存', { lang: statement.langCode }));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    statement.saving = false;
  }
}

async function deleteStatement(statement: StatementDraft) {
  if (!props.problem || statement.savedBody === null) return;
  try {
    await ElMessageBox.confirm(
      t('确认删除 {lang} 题面？', { lang: statement.langCode }),
      t('删除题面'),
      {
        type: 'warning',
      },
    );
    await adminProblemApi.deleteStatement(props.problem.id, statement.langCode);
    statementDrafts.value = statementDrafts.value.filter((item) => item.key !== statement.key);
    ElMessage.success(t('{lang} 题面已删除', { lang: statement.langCode }));
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}
</script>

<style scoped>
.problem-editor-card {
  margin-bottom: 20px;
}
.problem-card-heading,
.statement-editor-heading {
  width: 100%;
  justify-content: space-between;
}
.problem-card-heading strong,
.problem-card-heading small {
  display: block;
}
.problem-card-heading small {
  margin-top: 4px;
  color: var(--muted);
}
.statement-editor-heading :deep(.el-input) {
  width: 180px;
}
.statement-editor {
  display: grid;
  gap: 14px;
  padding: 20px 0;
  border-top: 1px solid var(--border);
}
.statement-editor:first-of-type {
  border-top: 0;
}
@media (max-width: 680px) {
  .problem-card-heading,
  .statement-editor-heading {
    align-items: stretch;
    flex-direction: column;
  }
  .statement-editor-heading :deep(.el-input) {
    width: 100%;
  }
}
</style>
