import { remarkMdxMermaid } from 'fumadocs-core/mdx-plugins';
import { defineConfig, defineDocs } from 'fumadocs-mdx/config';

type ReferenceNode = { type: string; url?: string; children?: ReferenceNode[] };

// Canonical Markdown uses absolute URLs for GitHub. Resolve our own references
// locally so previews stay on the preview and images need no production DNS.
function remarkLocalReferences() {
  return (tree: ReferenceNode) => {
    const visit = (node: ReferenceNode) => {
      if (
        (node.type === 'image' || node.type === 'link') &&
        node.url &&
        /^https:\/\/codifix\.dev\/(?:docs(?:[/?#]|$)|images\/|demo(?:[/?#]|$))/.test(node.url)
      ) {
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
    remarkPlugins: (defaults) => [remarkLocalReferences, ...defaults, remarkMdxMermaid],
  },
});
