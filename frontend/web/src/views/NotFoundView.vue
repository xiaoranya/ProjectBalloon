<template>
  <main class="center-page">
    <ElResult icon="warning" title="页面不存在" sub-title="请检查地址，或返回比赛列表">
      <template #extra>
        <ElButton type="primary" @click="$router.push('/contests')">返回比赛列表</ElButton>
      </template>
    </ElResult>
  </main>
</template>
