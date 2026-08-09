<template>
  <el-container direction="vertical" class="training-page">
    <el-header height="auto" class="page-head">
      <div>
        <h1>{{ t('训练计划') }}</h1>
      </div>
      <RouterLink to="/problem-bank">
        <ElButton>{{ t('公开题库') }}</ElButton>
      </RouterLink>
    </el-header>
    <el-main class="page-body">
      <div class="layout">
        <aside class="set-nav">
          <ElMenu :default-active="activeId" class="set-menu" @select="onMenuSelect">
            <ElMenuItem v-for="item in sets" :key="item.id" :index="String(item.id)">
              <div class="set-item">
                <strong>{{ item.title }}</strong>
                <span>{{ t('{count} 题', { count: item.itemCount }) }}</span>
              </div>
            </ElMenuItem>
          </ElMenu>
        </aside>
        <section v-loading="loading">
          <template v-if="selected">
            <div class="set-head">
              <div>
                <h2>{{ selected.setInfo.title }}</h2>
                <p>{{ selected.setInfo.description }}</p>
              </div>
              <ElButton type="primary" @click="enroll">{{ t('加入训练') }}</ElButton>
            </div>
            <ElTable :data="selected.items" row-key="problemId">
              <ElTableColumn prop="position" label="#" width="64" />
              <ElTableColumn prop="title" :label="t('题目')" />
              <ElTableColumn :label="t('要求')" width="90">
                <template #default="{ row }">
                  <ElTag v-if="row.required" type="danger" size="small">{{ t('必做') }}</ElTag>
                  <span v-else>{{ t('选做') }}</span>
                </template>
              </ElTableColumn>
              <ElTableColumn :label="t('难度')" width="80">
                <template #default="{ row }">{{ row.difficulty ?? '-' }}</template>
              </ElTableColumn>
            </ElTable>
          </template>
          <ElEmpty v-else :description="t('暂无公开训练集')" />
        </section>
      </div>
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { ElMessage } from 'element-plus';
import { trainingApi, type TrainingSet, type TrainingSetDetail } from '../api/training';
import { getErrorMessage } from '../api/client';
import { useI18n } from '../i18n';
const { t } = useI18n();
const sets = ref<TrainingSet[]>([]);
const selected = ref<TrainingSetDetail>();
const loading = ref(false);
const activeId = ref('');
function onMenuSelect(index: string) {
  void select(Number(index));
}
async function select(id: number) {
  activeId.value = String(id);
  loading.value = true;
  try {
    selected.value = await trainingApi.set(id);
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  } finally {
    loading.value = false;
  }
}
async function load() {
  try {
    sets.value = await trainingApi.sets();
    if (sets.value[0]) await select(sets.value[0].id);
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function enroll() {
  if (!selected.value) return;
  try {
    await trainingApi.enroll(selected.value.setInfo.id);
    ElMessage.success(t('已加入训练计划'));
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
onMounted(load);
</script>
<style scoped>
.training-page {
  max-width: 1180px;
  margin: 0 auto;
}
.page-head {
  display: flex;
  align-items: end;
  justify-content: space-between;
  border-bottom: 1px solid #dcdfe6;
  padding: 28px 20px 18px;
  height: auto;
}
.page-body {
  padding: 0 20px 28px;
}
.page-head h1 {
  margin: 0;
  font-size: 28px;
}
.page-head p {
  margin: 6px 0 0;
  color: #606266;
}
.layout {
  display: grid;
  grid-template-columns: 260px 1fr;
  gap: 24px;
}
.set-nav {
  border-right: 1px solid #dcdfe6;
  padding-right: 14px;
}
.set-menu {
  border-right: none;
}
.set-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
}
.set-item span {
  color: #909399;
}
.set-head {
  display: flex;
  justify-content: space-between;
  align-items: end;
  border-bottom: 1px solid #dcdfe6;
  padding-bottom: 18px;
  margin-bottom: 20px;
}
.set-head h2 {
  margin: 0;
}
.set-head p {
  margin: 6px 0 0;
  color: #606266;
}
@media (max-width: 720px) {
  .layout {
    grid-template-columns: 1fr;
  }
  .set-nav {
    border-right: 0;
    border-bottom: 1px solid #dcdfe6;
    padding: 0 0 12px;
  }
}
</style>
