// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Served from GitHub Pages at https://chauhanswapnil.github.io/agentcam/.
// Links inside pages are relative (`../install/`), because Starlight does
// not add `base` to links written in Markdown.
export default defineConfig({
  site: 'https://chauhanswapnil.github.io',
  base: '/agentcam',
  trailingSlash: 'always',
  // Screenshots live in docs/screenshots (the README uses them too).
  vite: { server: { fs: { allow: ['..'] } } },
  integrations: [
    starlight({
      title: 'agentcam',
      description:
        'A dashcam for your coding agents. Replay, search and audit Claude Code and Codex sessions, locally.',
      logo: { src: './src/assets/logo.svg', replacesTitle: false },
      favicon: '/favicon.svg',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/chauhanswapnil/agentcam' },
      ],
      editLink: {
        baseUrl: 'https://github.com/chauhanswapnil/agentcam/edit/main/site/',
      },
      lastUpdated: true,
      customCss: [
        '@fontsource-variable/inter',
        '@fontsource-variable/jetbrains-mono',
        './src/styles/theme.css',
      ],
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'What agentcam is', slug: 'start/what-it-is' },
            { label: 'Install', slug: 'start/install' },
            { label: 'First run', slug: 'start/first-run' },
            { label: 'Try it with demo data', slug: 'start/demo-data' },
          ],
        },
        {
          label: 'Using agentcam',
          items: [
            { label: 'Overview and sessions', slug: 'use/overview' },
            { label: 'Replay', slug: 'use/replay' },
            { label: 'Live and alerts', slug: 'use/live' },
            { label: 'Search, prompts and thinking', slug: 'use/search' },
            { label: 'What lasted and who wrote it', slug: 'use/lasted' },
            { label: 'Compare and instructions', slug: 'use/compare' },
            { label: 'Profile and fixes', slug: 'use/profile' },
            { label: 'Cost and usage limits', slug: 'use/cost' },
            { label: 'Audit', slug: 'use/audit' },
            { label: 'Setup, skills and MCP', slug: 'use/setup' },
            { label: 'Export and share', slug: 'use/export' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Audit rules', slug: 'guides/audit-rules' },
            { label: 'Block next time (guard)', slug: 'guides/guard' },
            { label: 'agentcam as an MCP server', slug: 'guides/mcp' },
            { label: 'Evidence and SIEM', slug: 'guides/evidence-and-siem' },
            { label: 'Running a team server', slug: 'guides/team-server' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Commands', slug: 'reference/commands' },
            { label: 'Files and folders', slug: 'reference/files' },
            { label: 'Supported agents', slug: 'reference/agents' },
            { label: 'Privacy', slug: 'reference/privacy' },
            { label: 'FAQ', slug: 'reference/faq' },
            { label: 'Contributing', slug: 'reference/contributing' },
          ],
        },
      ],
    }),
  ],
});
