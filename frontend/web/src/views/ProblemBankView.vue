<template>
  <main class="bank-page">
    <header><div><h1>公开题库</h1><p>按标签与难度筛选已发布题目</p></div><RouterLink to="/training">训练计划</RouterLink></header>
    <div class="filters"><ElInput v-model="tag" clearable placeholder="标签" @change="applyFilters"/><ElSelect v-model="difficulty" clearable placeholder="难度" @change="applyFilters"><ElOption v-for="value in 11" :key="value-1" :label="value-1" :value="value-1"/></ElSelect><ElButton :loading="loading" @click="load">刷新</ElButton></div>
    <ElTable :data="page.content" v-loading="loading" row-key="id" @row-click="open">
      <ElTableColumn prop="slug" label="编号" width="150"/><ElTableColumn prop="title" label="题目" min-width="260"/>
      <ElTableColumn label="难度" width="90"><template #default="{row}">{{ row.difficulty ?? '未标注' }}</template></ElTableColumn>
      <ElTableColumn label="标签"><template #default="{row}"><ElTag v-for="item in row.tags" :key="item" size="small">{{ item }}</ElTag></template></ElTableColumn>
    </ElTable>
    <ElPagination
      v-if="page.totalPages > 1"
      v-model:current-page="currentPage"
      :page-size="page.size"
      :total="page.totalElements"
      layout="prev, pager, next, total"
      class="pagination-row"
      @current-change="load"
    />
    <ElDrawer v-model="drawer" :title="selected?.title" size="min(760px, 92vw)"><div v-if="selected" class="statement" v-html="selected.statement || '<p>暂无题面</p>'"/></ElDrawer>
  </main>
</template>
<script setup lang="ts">
import { onMounted, ref } from 'vue'; import { ElMessage } from 'element-plus'; import { trainingApi, type BankProblem } from '../api/training'; import { getErrorMessage } from '../api/client';
const page=ref({content:[] as BankProblem[],page:0,size:50,totalElements:0,totalPages:0}); const currentPage=ref(1); const tag=ref(''); const difficulty=ref<number>(); const loading=ref(false); const drawer=ref(false); const selected=ref<BankProblem>();
async function load(){loading.value=true;try{page.value=await trainingApi.problemBank(currentPage.value-1,page.value.size,tag.value||undefined,difficulty.value)}catch(e){ElMessage.error(getErrorMessage(e))}finally{loading.value=false}}
function applyFilters(){currentPage.value=1;void load()}
async function open(row:BankProblem){try{selected.value=await trainingApi.problem(row.slug);drawer.value=true}catch(e){ElMessage.error(getErrorMessage(e))}}
onMounted(load);
</script>
<style scoped>.bank-page{max-width:1120px;margin:0 auto;padding:28px 20px}header{display:flex;align-items:end;justify-content:space-between;margin-bottom:22px;border-bottom:1px solid #dcdfe6;padding-bottom:18px}h1{margin:0;font-size:28px}p{margin:6px 0 0;color:#606266}.filters{display:grid;grid-template-columns:minmax(180px,320px) 150px auto;gap:10px;margin-bottom:14px}.el-tag+.el-tag{margin-left:6px}.statement:deep(img){max-width:100%}@media(max-width:640px){.filters{grid-template-columns:1fr}header{align-items:start}}</style>
