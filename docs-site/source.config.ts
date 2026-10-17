import { remarkMdxMermaid } from 'fumadocs-core/mdx-plugins';
import { defineConfig, defineDocs } from 'fumadocs-mdx/config';

type ImageNode = { type: string; url?: string; children?: ImageNode[] };

// Included Markdown keeps absolute image URLs for GitHub readers. Resolve our
// own images from the candidate source so builds never depend on live DNS.
function remarkLocalImages() {
  return (tree: ImageNode) => {
    const visit = (node: ImageNode) => {
      if (node.type === 'image' && node.url?.startsWith('https://codifix.dev/images/')) {
        node.url = node.url.slice('https://codifix.dev'.length);
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

export const docs = defineDocs({
  dir: 'content/docs',
});

export default defineConfig({
  mdxOptions: {
    remarkPlugins: (defaults) => [remarkLocalImages, ...defaults, remarkMdxMermaid],
  },
});
