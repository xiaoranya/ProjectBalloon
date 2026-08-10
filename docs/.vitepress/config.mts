import { defineConfig } from 'vitepress'

// English default-locale sidebar. The zh-CN mirror uses the same structure
// with `/zh-CN/` prefixed links.
const enSidebar = [
  {
    text: 'Getting Started',
    items: [
      { text: 'ProjectBalloon Documentation', link: '/' },
      { text: 'Quickstart', link: '/ops/quickstart' },
    ],
  },
  {
    text: 'User Guide',
    items: [
      { text: 'Contestant Guide', link: '/user/contestant/' },
      { text: 'Daily Practice Guide', link: '/user/contestant/practice' },
      { text: 'Administrator Guide', link: '/user/admin/' },
      { text: 'On-Site Operations Guide', link: '/user/onsite/' },
    ],
  },
  {
    text: 'Operations',
    items: [
      { text: 'Installation', link: '/ops/install' },
      { text: 'Operations', link: '/ops/ops' },
      { text: 'Competition Mode', link: '/ops/competition-mode' },
      { text: 'Configuration Reference', link: '/ops/configuration' },
      { text: 'Troubleshooting', link: '/ops/troubleshooting' },
      { text: 'Backup and Restore', link: '/ops/backup-restore' },
      { text: 'Disaster Recovery', link: '/ops/disaster-recovery' },
      { text: 'Pressure Test', link: '/ops/pressure-test' },
    ],
  },
]

const zhSidebar = [
  {
    text: '快速开始',
    items: [
      { text: 'ProjectBalloon 文档', link: '/zh-CN/' },
      { text: '快速开始', link: '/zh-CN/ops/quickstart' },
    ],
  },
  {
    text: '用户指南',
    items: [
      { text: '选手指南', link: '/zh-CN/user/contestant/' },
      { text: '日常练习指南', link: '/zh-CN/user/contestant/practice' },
      { text: '管理员指南', link: '/zh-CN/user/admin/' },
      { text: '现场运营指南', link: '/zh-CN/user/onsite/' },
    ],
  },
  {
    text: '运维',
    items: [
      { text: '安装', link: '/zh-CN/ops/install' },
      { text: '运维', link: '/zh-CN/ops/ops' },
      { text: '比赛模式', link: '/zh-CN/ops/competition-mode' },
      { text: '配置参考', link: '/zh-CN/ops/configuration' },
      { text: '故障排查', link: '/zh-CN/ops/troubleshooting' },
      { text: '备份与恢复', link: '/zh-CN/ops/backup-restore' },
      { text: '灾难恢复', link: '/zh-CN/ops/disaster-recovery' },
      { text: '压测', link: '/zh-CN/ops/pressure-test' },
    ],
  },
]

export default defineConfig({
  title: 'ProjectBalloon Documentation',
  description:
    'The manual for ProjectBalloon: installing the platform, running an official contest, and using the web interface as a contestant, administrator, or on-site staff member.',
  base: '/ProjectBalloon/',
  lastUpdated: true,
  cleanUrls: false,
  rewrites: {
    'README.md': 'index.md',
    'user/README.md': 'user/index.md',
    'user/contestant/README.md': 'user/contestant/index.md',
    'user/admin/README.md': 'user/admin/index.md',
    'user/onsite/README.md': 'user/onsite/index.md',
    'zh-CN/README.md': 'zh-CN/index.md',
    'zh-CN/user/README.md': 'zh-CN/user/index.md',
    'zh-CN/user/contestant/README.md': 'zh-CN/user/contestant/index.md',
    'zh-CN/user/admin/README.md': 'zh-CN/user/admin/index.md',
    'zh-CN/user/onsite/README.md': 'zh-CN/user/onsite/index.md',
  },
  locales: {
    root: {
      label: 'English',
      lang: 'en',
      themeConfig: {
        nav: [
          { text: 'Documentation', link: '/' },
          { text: 'User Guide', link: '/user/' },
          { text: 'Operations', link: '/ops/quickstart' },
        ],
        sidebar: enSidebar,
        docFooter: {
          prev: 'Previous page',
          next: 'Next page',
        },
        outline: {
          label: 'On this page',
        },
        lastUpdated: {
          text: 'Last updated',
        },
      },
    },
    'zh-CN': {
      label: '简体中文',
      lang: 'zh-CN',
      title: 'ProjectBalloon 文档',
      themeConfig: {
        nav: [
          { text: '文档', link: '/zh-CN/' },
          { text: '用户指南', link: '/zh-CN/user/' },
          { text: '运维', link: '/zh-CN/ops/quickstart' },
        ],
        sidebar: zhSidebar,
        docFooter: {
          prev: '上一页',
          next: '下一页',
        },
        outline: {
          label: '本页目录',
        },
        lastUpdated: {
          text: '最后更新',
        },
      },
    },
  },
})
