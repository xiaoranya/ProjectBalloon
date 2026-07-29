<template>
  <section class="admin-page">
    <header class="admin-page-header"><div><p class="eyebrow">Daily Practice</p><h1>日常练习</h1><p>管理练习资源、判题配额与源码保留周期。</p></div><ElButton type="primary" :loading="savingSettings" @click="saveSettings">保存设置</ElButton></header>
    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />
    <div class="settings-grid">
      <ElCard shadow="never" class="admin-card"><template #header><strong>平台配额</strong></template><ElForm label-position="top"><ElFormItem label="每日提交上限"><ElInputNumber v-model="settings.dailySubmissionLimit" :min="1" :max="10000" /></ElFormItem><ElFormItem label="并发判题上限"><ElInputNumber v-model="settings.concurrentJudgingLimit" :min="1" :max="20" /></ElFormItem><ElFormItem label="源码保留天数"><ElInputNumber v-model="settings.sourceRetentionDays" :min="1" :max="3650" /></ElFormItem></ElForm></ElCard>
      <ElCard shadow="never" class="admin-card"><template #header><strong>题解管理</strong></template><ElForm label-position="top"><ElFormItem label="公开题目"><ElSelect v-model="editorialProblemId" filterable clearable placeholder="选择题目" @change="loadEditorial"><ElOption v-for="problem in problems" :key="problem.id" :label="`${problem.slug} · ${problem.title}`" :value="problem.id" /></ElSelect></ElFormItem><ElFormItem label="语言"><ElInput v-model="editorialLang" maxlength="8" @change="loadEditorial" /></ElFormItem><template v-if="editorialProblemId"><ElFormItem label="标题"><ElInput v-model="editorial.title" maxlength="255" /></ElFormItem><ElFormItem label="解锁条件"><ElSelect v-model="editorial.unlockPolicy"><ElOption label="始终可见" value="ALWAYS" /><ElOption label="提交后解锁" value="AFTER_ATTEMPT" /><ElOption label="通过后解锁" value="AFTER_ACCEPTED" /></ElSelect></ElFormItem><ElFormItem label="Markdown 内容"><ElInput v-model="editorial.body" type="textarea" :rows="14" /></ElFormItem><ElCheckbox v-model="editorial.published">发布题解</ElCheckbox><div class="editorial-actions"><ElButton type="primary" :loading="savingEditorial" @click="saveEditorial">保存题解</ElButton></div></template><ElEmpty v-else description="选择题目开始维护题解" /></ElForm></ElCard>
    </div>
  </section>
</template>
<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { ElMessage } from 'element-plus';
import { getErrorMessage } from '../api/client';
import { trainingApi, type BankProblem } from '../api/training';
const problems=ref<BankProblem[]>([]),editorialProblemId=ref<number>(),editorialLang=ref('en'),savingSettings=ref(false),savingEditorial=ref(false),errorMessage=ref('');const settings=reactive({dailySubmissionLimit:200,concurrentJudgingLimit:3,sourceRetentionDays:365});const editorial=reactive({title:'',body:'',unlockPolicy:'AFTER_ATTEMPT',published:false});
async function loadEditorial(){if(!editorialProblemId.value)return;try{const value=await trainingApi.adminEditorial(editorialProblemId.value,editorialLang.value);editorial.title=value.title;editorial.body=value.bodyMarkdown??'';editorial.unlockPolicy=value.unlockPolicy;editorial.published=value.unlocked}catch(error){if((error as {status?:number}).status===404){editorial.title='';editorial.body='';editorial.unlockPolicy='AFTER_ATTEMPT';editorial.published=false}else errorMessage.value=getErrorMessage(error)}}
async function saveSettings(){savingSettings.value=true;try{await trainingApi.updatePracticeSettings(settings);ElMessage.success('练习设置已保存')}catch(error){errorMessage.value=getErrorMessage(error)}finally{savingSettings.value=false}}
async function saveEditorial(){if(!editorialProblemId.value)return;savingEditorial.value=true;try{await trainingApi.saveEditorial(editorialProblemId.value,editorialLang.value,editorial);ElMessage.success('题解已保存')}catch(error){errorMessage.value=getErrorMessage(error)}finally{savingEditorial.value=false}}
onMounted(async()=>{try{const [value,bank]=await Promise.all([trainingApi.practiceSettings(),trainingApi.problemBank(0,100)]);Object.assign(settings,value);problems.value=bank.content}catch(error){errorMessage.value=getErrorMessage(error)}});
</script>
<style scoped>.settings-grid{display:grid;grid-template-columns:minmax(280px,380px) minmax(0,1fr);gap:20px}.admin-card{min-width:0}.settings-grid .el-select,.settings-grid .el-input{width:100%}.editorial-actions{margin-top:18px}@media(max-width:800px){.settings-grid{grid-template-columns:1fr}}</style>
