import { createMDX } from 'fumadocs-mdx/next';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));

/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  async redirects() {
    return ['memory-layer.dev', 'www.memory-layer.dev'].map((host) => ({
      source: '/:path*',
      has: [{ type: 'host', value: host }],
      destination: 'https://codifix.dev/:path*',
      permanent: true,
    }));
  },
  turbopack: {
    // The canonical user guides live next to docs-site, and Fumadocs includes
    // them while compiling the thin route wrappers.
    root: repositoryRoot,
  },
};

const withMDX = createMDX();

export default withMDX(config);
