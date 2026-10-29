import {existsSync, readFileSync} from 'fs';
import path from 'path';
import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

// Docs versions (Q48). versions.json lists the permanent X.Y.0 snapshots. At
// deploy time scripts/docs_versions.sh also writes rolling-versions.json (never
// committed) naming the two rolling versions (latest patch, latest
// pre-release) and the default; without it (PR builds, local dev) the default
// is the newest stable entry of versions.json.
const readJson = (file: string): any =>
  existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : null;

const versionList: string[] = readJson(path.join(__dirname, 'versions.json')) ?? [];
const rolling: {patch: string | null; prerelease: string | null; default: string} | null =
  readJson(path.join(__dirname, 'rolling-versions.json'));

// X.Y.Z or X.Y.Z-pre.N -> comparable [major, minor, patch]; a pre-release sorts
// below its release, which is irrelevant here because only stable entries are compared.
const semver = (v: string): number[] => v.split('-')[0].split('.').map(Number);
const compareSemver = (a: string, b: string): number => {
  const [x, y] = [semver(a), semver(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
};
const newestStable = versionList
  .filter((v) => !v.includes('-'))
  .sort(compareSemver)
  .pop();

const defaultVersion = rolling?.default ?? newestStable;

const versionLabel = (v: string): string =>
  v === rolling?.prerelease
    ? `${v} (pre-release)`
    : v === rolling?.patch
      ? `${v} (latest patch)`
      : v;

const docsVersions = Object.fromEntries(
  versionList.map((v) => [
    v,
    v === rolling?.prerelease
      ? {label: versionLabel(v), banner: 'unreleased' as const}
      : {label: versionLabel(v)},
  ]),
);

const config: Config = {
  title: 'SSG Tether Capture',
  tagline: 'A Bevy-powered space tether capture simulation',
  favicon: 'img/favicon.ico',

  // Future flags, see https://docusaurus.io/docs/api/docusaurus-config#future
  future: {
    v4: true, // Improve compatibility with the upcoming Docusaurus v4
  },

  // Set the production url of your site here
  url: 'https://conjunction-crew.github.io/',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/ssg-tether-capture/',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'conjunction-crew', // Usually your GitHub org/user name.
  projectName: 'ssg-tether-capture', // Usually your repo name.

  onBrokenLinks: 'throw',

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          // The default version shown to users: the newest stable docs version
          lastVersion: defaultVersion,
          // Expose the working (unreleased) docs under `next` so you can preview them
          versions: {
            current: {
              label: 'Unreleased',
              path: 'next',
              banner: 'unreleased',
              badge: true,
            },
            ...docsVersions,
          },
          sidebarPath: './sidebars.ts',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/social-card.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Tether Capture',
      logo: {
        alt: 'Tether Capture Logo',
        src: 'img/tether-capture-logo.svg',
      },
        items: [
        {
          type: 'docSidebar',
          sidebarId: 'mainSidebar',
          position: 'left',
          label: 'Docs',
        },
        // Show the version dropdown in the navbar so users can switch to 'Unreleased' (`next`)
        {
          type: 'docsVersionDropdown',
          position: 'right',
        },
        {
          href: 'https://github.com/conjunction-crew/ssg-tether-capture',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Introduction',
              to: '/docs/getting-started/introduction',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/conjunction-crew/ssg-tether-capture',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Conjunction Crew. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
