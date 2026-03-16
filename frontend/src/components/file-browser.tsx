import { useCallback, useState } from "react";
import { ChevronRightIcon, FolderIcon, FolderOpenIcon, Loader2Icon, MusicIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useBrowsePath } from "@/lib/api/mutations";
import { cn } from "@/lib/utils";
import type { components } from "@/lib/api/types.gen";

type BrowseEntry = components["schemas"]["BrowseEntry"];

// ── Tree node state ────────────────────────────────────────────

interface TreeNode {
  entry: BrowseEntry;
  children: TreeNode[] | null; // null = not yet loaded
  isOpen: boolean;
  isLoading: boolean;
}

// ── Helpers ────────────────────────────────────────────────────

/** Return the last segment of a path, or "/" for the root. */
function basename(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  if (trimmed === "") return "/";
  const lastSlash = trimmed.lastIndexOf("/");
  return lastSlash === -1 ? trimmed : trimmed.slice(lastSlash + 1);
}

// ── Component ──────────────────────────────────────────────────

export function FileBrowser({ onSelect }: { onSelect: (path: string) => void }) {
  const [rootNode, setRootNode] = useState<TreeNode | null>(null);
  const [pathInput, setPathInput] = useState("/");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [isLoadingRoot, setIsLoadingRoot] = useState(false);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const browsePath = useBrowsePath();

  // Load children for a given path and return tree nodes
  const loadEntries = useCallback(
    async (path: string): Promise<TreeNode[]> => {
      const result = await browsePath.mutateAsync({
        body: { path },
      });

      if (!result) return [];

      return result
        .filter((e: BrowseEntry) => e.is_dir || e.is_audio)
        .sort((a: BrowseEntry, b: BrowseEntry) => {
          // Directories first, then alphabetical
          if (a.is_dir && !b.is_dir) return -1;
          if (!a.is_dir && b.is_dir) return 1;
          return a.name.localeCompare(b.name);
        })
        .map(
          (entry: BrowseEntry): TreeNode => ({
            entry,
            children: entry.is_dir ? null : [],
            isOpen: false,
            isLoading: false,
          }),
        );
    },
    [browsePath],
  );

  // Navigate to a path (load its contents and build root tree node)
  const navigateToPath = useCallback(
    async (path: string) => {
      setError(null);
      setIsLoadingRoot(true);
      setPathInput(path);
      setSelectedPath(path);
      try {
        const children = await loadEntries(path);
        setRootNode({
          entry: {
            name: basename(path),
            path,
            is_dir: true,
            is_audio: false,
          },
          children,
          isOpen: true,
          isLoading: false,
        });
        setHasLoaded(true);
      } catch {
        setError(`Failed to browse "${path}". Check the path exists on the server.`);
        setRootNode(null);
      } finally {
        setIsLoadingRoot(false);
      }
    },
    [loadEntries],
  );

  // Handle path input submission
  const handlePathSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = pathInput.trim();
    if (trimmed) {
      void navigateToPath(trimmed);
    }
  };

  // Toggle a directory node open/closed, lazily loading children
  const toggleNode = useCallback(
    async (targetPath: string) => {
      const updateNodes = async (node: TreeNode): Promise<TreeNode> => {
        if (node.entry.path === targetPath) {
          // This is the target node — toggle it
          if (node.isOpen) {
            return { ...node, isOpen: false };
          }

          // Need to open — load children if not loaded yet
          if (node.children === null) {
            try {
              const children = await loadEntries(node.entry.path);
              return { ...node, children, isOpen: true, isLoading: false };
            } catch {
              return { ...node, isLoading: false, children: [] };
            }
          }

          return { ...node, isOpen: true };
        }

        // Recurse into children to find the target
        if (node.children && node.children.length > 0) {
          const updatedChildren = await Promise.all(
            node.children.map((child) => updateNodes(child)),
          );
          return { ...node, children: updatedChildren };
        }

        return node;
      };

      if (!rootNode) return;
      const updated = await updateNodes(rootNode);
      setRootNode(updated);
    },
    [rootNode, loadEntries],
  );

  // Build breadcrumb segments from the browsed root path
  const rootPath = rootNode?.entry.path ?? "/";
  const breadcrumbs = rootPath.split("/").filter(Boolean);

  return (
    <div className="space-y-4">
      {/* Path input */}
      <form onSubmit={handlePathSubmit} className="flex gap-2">
        <Input
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          placeholder="Enter server path..."
          className="flex-1 font-mono text-sm"
        />
        <Button type="submit" size="sm" variant="outline" disabled={isLoadingRoot}>
          {isLoadingRoot ? "Loading..." : "Browse"}
        </Button>
      </form>

      {/* Breadcrumb trail */}
      <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
        <button
          type="button"
          className="transition-colors hover:text-foreground"
          onClick={() => void navigateToPath("/")}
        >
          /
        </button>
        {breadcrumbs.map((segment, i) => {
          const path = "/" + breadcrumbs.slice(0, i + 1).join("/");
          return (
            <span key={path} className="flex items-center gap-1">
              <ChevronRightIcon className="size-3" />
              <button
                type="button"
                className="transition-colors hover:text-foreground"
                onClick={() => void navigateToPath(path)}
              >
                {segment}
              </button>
            </span>
          );
        })}
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Tree view */}
      {isLoadingRoot ? (
        <div className="flex items-center justify-center py-16">
          <Loader2Icon className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : !hasLoaded ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <FolderIcon className="size-10 text-muted-foreground/40" />
          <p className="mt-4 text-sm text-muted-foreground">
            Enter a path and click Browse to explore the server filesystem.
          </p>
        </div>
      ) : !rootNode && !error ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-16">
          <FolderOpenIcon className="size-10 text-muted-foreground/40" />
          <p className="mt-4 text-sm text-muted-foreground">This directory is empty.</p>
        </div>
      ) : rootNode ? (
        <div className="rounded-xl border bg-card shadow-sm">
          <div className="max-h-96 overflow-y-auto p-2">
            <TreeNodeItem
              node={rootNode}
              depth={0}
              onToggle={toggleNode}
              selectedPath={selectedPath}
              onSelectPath={setSelectedPath}
            />
          </div>
        </div>
      ) : null}

      {/* Selected folder bar */}
      {hasLoaded && !error && (
        <div className="flex items-center justify-between rounded-xl border bg-muted/30 px-4 py-3">
          <div className="min-w-0">
            <p className="text-sm font-medium">Selected path</p>
            {selectedPath ? (
              <p className="truncate font-mono text-xs text-muted-foreground">{selectedPath}</p>
            ) : (
              <p className="text-xs text-muted-foreground italic">Click a folder to select it</p>
            )}
          </div>
          <Button
            size="sm"
            disabled={!selectedPath}
            onClick={() => {
              if (selectedPath) onSelect(selectedPath);
            }}
          >
            Select this folder
          </Button>
        </div>
      )}
    </div>
  );
}

// ── Tree node item ─────────────────────────────────────────────

function TreeNodeItem({
  node,
  depth,
  onToggle,
  selectedPath,
  onSelectPath,
}: {
  node: TreeNode;
  depth: number;
  onToggle: (targetPath: string) => void;
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
}) {
  const isSelected = node.entry.is_dir && selectedPath === node.entry.path;

  const handleChevronClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (node.entry.is_dir) {
      onToggle(node.entry.path);
    }
  };

  const handleRowClick = () => {
    if (node.entry.is_dir) {
      onSelectPath(node.entry.path);
    }
  };

  return (
    <div>
      <button
        type="button"
        className={cn(
          "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
          node.entry.is_dir ? "hover:bg-muted/60" : "cursor-default text-muted-foreground",
          isSelected && "bg-accent text-accent-foreground",
        )}
        style={{ paddingLeft: `${depth * 20 + 8}px` }}
        onClick={handleRowClick}
      >
        {/* Expand chevron */}
        {node.entry.is_dir ? (
          <span
            role="button"
            tabIndex={-1}
            className="flex shrink-0 items-center justify-center rounded p-0.5 transition-colors hover:bg-muted"
            onClick={handleChevronClick}
            onKeyDown={() => {}}
          >
            <ChevronRightIcon
              className={cn(
                "size-3.5 text-muted-foreground transition-transform",
                node.isOpen && "rotate-90",
              )}
            />
          </span>
        ) : (
          <span className="size-3.5 shrink-0" />
        )}

        {/* Icon */}
        {node.isLoading ? (
          <Loader2Icon className="size-4 shrink-0 animate-spin text-muted-foreground" />
        ) : node.entry.is_dir ? (
          node.isOpen ? (
            <FolderOpenIcon className="size-4 shrink-0 text-amber-500" />
          ) : (
            <FolderIcon className="size-4 shrink-0 text-amber-500" />
          )
        ) : (
          <MusicIcon className="size-4 shrink-0 text-muted-foreground" />
        )}

        {/* Name */}
        <span className="truncate">{node.entry.name}</span>
      </button>

      {/* Children */}
      {node.isOpen &&
        node.children &&
        node.children.length > 0 &&
        node.children.map((child) => (
          <TreeNodeItem
            key={child.entry.path}
            node={child}
            depth={depth + 1}
            onToggle={onToggle}
            selectedPath={selectedPath}
            onSelectPath={onSelectPath}
          />
        ))}
    </div>
  );
}
