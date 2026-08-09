<template>
  <ElDialog v-model="cancelVisible" :title="t('取消气球任务')" width="min(540px, 92vw)">
    <ElForm label-position="top">
      <ElFormItem :label="t('取消原因')" :error="cancelError || undefined">
        <ElInput v-model="cancelReason" type="textarea" :rows="4" maxlength="255" show-word-limit />
      </ElFormItem>
    </ElForm>
    <template #footer>
      <ElButton @click="cancelVisible = false">{{ t('返回') }}</ElButton>
      <ElButton
        type="danger"
        :loading="action === 'cancel'"
        :disabled="!!cancelError"
        @click="emit('cancel')"
        >{{ t('确认取消') }}</ElButton
      >
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from '../../i18n';

defineProps<{
  action: string;
}>();
const emit = defineEmits<{
  cancel: [];
}>();
const { t } = useI18n();
const cancelVisible = defineModel<boolean>('cancelVisible');
const cancelReason = defineModel<string>('cancelReason');
const cancelError = computed(() => ((cancelReason.value ?? '').trim() ? '' : t('请输入取消原因')));
</script>
