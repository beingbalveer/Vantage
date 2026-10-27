export interface TreeNode {
  id: string; // unique path id e.g. "dir:src/lib" or "file:src/lib/api.ts"
  name: string; // display name e.g. "components" or "src/lib" (when compacted)
  fullPath: string; // real full path in repository e.g. "src/lib/components"
  isDir: boolean;
  children: TreeNode[];
  depth: number;
}

export interface VisibleNode {
  node: TreeNode;
  depth: number;
  isOpen: boolean;
}

interface BuildTreeOptions {
  compactSingleChildDirs?: boolean;
}

interface InternalDir {
  name: string;
  fullPath: string;
  subdirs: Map<string, InternalDir>;
  files: string[];
}

/**
 * Builds a hierarchical tree from a flat list of relative file paths.
 */
export function buildFileTree(
  filePaths: string[],
  options: BuildTreeOptions = { compactSingleChildDirs: true }
): TreeNode[] {
  const root: InternalDir = {
    name: "",
    fullPath: "",
    subdirs: new Map(),
    files: [],
  };

  for (const filePath of filePaths) {
    if (!filePath || filePath.trim() === "") continue;
    const parts = filePath.split("/").filter(Boolean);
    if (parts.length === 0) continue;

    let current = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      let next = current.subdirs.get(part);
      if (!next) {
        const fullPath = current.fullPath ? `${current.fullPath}/${part}` : part;
        next = {
          name: part,
          fullPath,
          subdirs: new Map(),
          files: [],
        };
        current.subdirs.set(part, next);
      }
      current = next;
    }

    const fileName = parts[parts.length - 1];
    current.files.push(fileName);
  }

  function convertDir(
    dir: InternalDir,
    currentDepth: number
  ): TreeNode[] {
    const nodes: TreeNode[] = [];

    // Convert subdirs
    for (const [_, sub] of dir.subdirs) {
      let displayName = sub.name;
      let targetDir = sub;

      // Single-child compaction (VS Code style: src/lib/components)
      if (options.compactSingleChildDirs) {
        while (targetDir.files.length === 0 && targetDir.subdirs.size === 1) {
          const onlyChild = targetDir.subdirs.values().next().value!;
          displayName = `${displayName}/${onlyChild.name}`;
          targetDir = onlyChild;
        }
      }

      const children = convertDir(targetDir, currentDepth + 1);

      nodes.push({
        id: `dir:${targetDir.fullPath}`,
        name: displayName,
        fullPath: targetDir.fullPath,
        isDir: true,
        children,
        depth: currentDepth,
      });
    }

    // Sort folders alphabetically
    nodes.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

    // Convert files
    const fileNodes: TreeNode[] = [];
    for (const fileName of dir.files) {
      const fullPath = dir.fullPath ? `${dir.fullPath}/${fileName}` : fileName;
      fileNodes.push({
        id: `file:${fullPath}`,
        name: fileName,
        fullPath,
        isDir: false,
        children: [],
        depth: currentDepth,
      });
    }

    // Sort files alphabetically
    fileNodes.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

    return [...nodes, ...fileNodes];
  }

  return convertDir(root, 0);
}


/**
 * Flattens tree into visible row nodes according to expanded directories.
 */
export function flattenVisibleTree(
  nodes: TreeNode[],
  expandedDirs: Set<string>,
  depth = 0
): VisibleNode[] {
  const result: VisibleNode[] = [];

  for (const node of nodes) {
    const isOpen = node.isDir ? expandedDirs.has(node.id) : false;
    result.push({
      node,
      depth,
      isOpen,
    });

    if (node.isDir && isOpen && node.children.length > 0) {
      result.push(...flattenVisibleTree(node.children, expandedDirs, depth + 1));
    }
  }

  return result;
}

/**
 * Collect all folder IDs from tree.
 */
export function collectAllFolderIds(nodes: TreeNode[]): Set<string> {
  const ids = new Set<string>();
  function walk(items: TreeNode[]) {
    for (const item of items) {
      if (item.isDir) {
        ids.add(item.id);
        walk(item.children);
      }
    }
  }
  walk(nodes);
  return ids;
}

/**
 * Given a file path, finds all ancestor directory IDs needed to reveal it.
 */
export function getAncestorFolderIds(filePath: string): Set<string> {
  const ids = new Set<string>();
  const parts = filePath.split("/").filter(Boolean);
  let current = "";
  for (let i = 0; i < parts.length - 1; i++) {
    current = current ? `${current}/${parts[i]}` : parts[i];
    ids.add(`dir:${current}`);
  }
  return ids;
}
