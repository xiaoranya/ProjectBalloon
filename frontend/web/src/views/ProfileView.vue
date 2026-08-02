<template>
  <el-container direction="vertical" class="profile-page">
    <el-header height="auto" class="page-head"><div><p class="eyebrow">Account</p><h1>账户设置</h1></div><RouterLink to="/practice">返回练习</RouterLink></el-header>
    <el-main class="page-body">
      <ElCard shadow="never" class="profile-card">
        <ElForm label-position="top" @submit.prevent="saveProfile">
          <ElFormItem label="用户名"><ElInput :model-value="session.state.user?.username" disabled /></ElFormItem>
          <ElFormItem label="显示名称"><ElInput v-model="displayName" maxlength="128" show-word-limit /></ElFormItem>
          <ElButton type="primary" :loading="saving" @click="saveProfile">保存显示名称</ElButton>
        </ElForm>
        <ElDivider />
        <div class="password-row"><div><h2>登录密码</h2><p>定期更新密码可以减少账户风险。</p></div><ElButton @click="router.push('/change-password')">修改密码</ElButton></div>
      </ElCard>
      <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" />
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { useSession } from '../auth/session';
import { getErrorMessage } from '../api/client';
const router=useRouter(),session=useSession(),displayName=ref(session.state.user?.displayName??''),saving=ref(false),errorMessage=ref('');
async function saveProfile(){if(!displayName.value.trim())return;saving.value=true;errorMessage.value='';try{await session.updateProfile(displayName.value);ElMessage.success('账户信息已保存')}catch(error){errorMessage.value=getErrorMessage(error)}finally{saving.value=false}}
</script>
<style scoped>.profile-page{max-width:760px;margin:0 auto}.page-head{display:flex;justify-content:space-between;align-items:end;border-bottom:1px solid #dcdfe6;padding:28px 20px 18px;margin-bottom:20px;height:auto}.page-body{padding:0 20px 28px}.profile-page h1,.profile-page h2,.profile-page p{margin:0}.page-head p:not(.eyebrow){color:#606266;margin-top:6px}.profile-card{max-width:560px}.password-row{display:flex;justify-content:space-between;align-items:center;gap:20px}.password-row p{color:#606266;margin-top:6px}@media(max-width:600px){.password-row{align-items:flex-start;flex-direction:column}}</style>
