<template>
  <ElCard v-loading="loading" shadow="never" class="problem-editor-card publication-card">
    <ElAlert
      v-if="errorMessage"
      :title="errorMessage"
      type="error"
      show-icon
      :closable="false"
      class="publication-alert"
    />
    <template v-if="publication">
      <ElAlert
        :title="statusText"
        :type="publication.visibility === 'PUBLIC' ? 'success' : 'info'"
        :closable="false"
        show-icon
        class="publication-alert"
      />
      <ElForm label-position="top" class="publication-form">
        <ElFormItem :label="t('可见性')">
          <ElRadioGroup v-model="form.visibility">
            <ElRadioButton value="PRIVATE">{{ t('私有（仅比赛）') }}</ElRadioButton>
            <ElRadioButton value="PUBLIC">{{ t('公开（练习题库）') }}</ElRadioButton>
          </ElRadioGroup>
        </ElFormItem>
        <ElRow :gutter="12" class="admin-form-grid">
          <ElCol :xs="24" :sm="8">
            <ElFormItem :label="t('难度（0-10，选填）')">
              <ElInputNumber
                v-model="form.difficulty"
                :min="0"
                :max="10"
                :placeholder="t('未设置')"
                controls-position="right"
              />
            </ElFormItem>
          </ElCol>
          <ElCol :xs="24" :sm="16">
            <ElFormItem :label="t('标签（用逗号分隔，选填）')">
              <ElInput v-model="tagsText" :placeholder="t('例如 dp, greedy, graph')" />
            </ElFormItem>
          </ElCol>
        </ElRow>
        <ElAlert
          v-if="form.visibility === 'PUBLIC'"
          :title="t('发布后所有人可在练习题库中查看题面并提交。再次切回“私有”即下架。')"
          type="warning"
          :closable="false"
          show-icon
        />
        <ElButton type="primary" :loading="saving" class="publication-save" @click="save">
          {{ form.visibility === 'PUBLIC' ? t('发布到公开题库') : t('保存并保持私有') }}
        </ElButton>
      </ElForm>
    </template>
  </ElCard>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { adminProblemApi } from '../../api/admin-problems';
import type { ProblemPublication } from '../../api/admin-problems';
import { getErrorMessage } from '../../api/client';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{ problemId: number }>();
const emit = defineEmits<{ (event: 'updated', publication: ProblemPublication): void }>();
const { t } = useI18n();

const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const publication = ref<ProblemPublication | null>(null);
const form = ref<{ visibility: 'PUBLIC' | 'PRIVATE'; difficulty: number | null }>({
  visibility: 'PRIVATE',
  difficulty: null,
});
const tagsText = ref('');

const statusText = computed(() => {
  if (!publication.value) return '';
  return publication.value.visibility === 'PUBLIC'
    ? t('当前状态：已发布（{time}）', { time: formatDateTime(publication.value.publishedAt) })
    : t('当前状态：未发布');
});

const parsedTags = computed(() =>
  tagsText.value
    .split(/[,，]/)
    .map((tag) => tag.trim().toLowerCase())
    .filter((tag) => tag.length > 0),
);

function applyPublication(value: ProblemPublication) {
  publication.value = value;
  form.value = { visibility: value.visibility, difficulty: value.difficulty };
  tagsText.value = value.tags.join(', ');
}

async function loadPublication() {
  loading.value = true;
  errorMessage.value = '';
  try {
    applyPublication(await adminProblemApi.getPublication(props.problemId));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  errorMessage.value = '';
  try {
    const updated = await adminProblemApi.updatePublication(props.problemId, {
      visibility: form.value.visibility,
      difficulty: form.value.difficulty,
      tags: parsedTags.value,
    });
    applyPublication(updated);
    ElMessage.success(
      updated.visibility === 'PUBLIC' ? t('题目已发布到公开题库') : t('题目已保存为私有'),
    );
    emit('updated', updated);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    saving.value = false;
  }
}

watch(() => props.problemId, loadPublication);
onMounted(loadPublication);
</script>

<style scoped>
.publication-card {
  max-width: 860px;
}
.publication-alert {
  margin-bottom: 18px;
}
.publication-save {
  margin-top: 6px;
}
</style>
