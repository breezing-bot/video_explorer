import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import { getBackupTree, getScanStatus, listScanRoots, startScan } from "./services/tauriApi";
import {
  BackupTreeNode,
  ScanCompletedEvent,
  ScanErrorEvent,
  ScanProgressEvent,
  ScanRootOption,
  ScanStartedEvent,
  ScanStatusDto,
} from "./types";

const DEFAULT_STATUS: ScanStatusDto = {
  isRunning: false,
  rootPath: null,
  totalCandidates: 0,
  scannedFiles: 0,
  hashedFiles: 0,
  errorCount: 0,
  lastError: null,
  startedAt: null,
  finishedAt: null,
};

function ratioLabel(ratio: number): string {
  return `${Math.round(ratio * 100)}%`;
}

function directoryColor(ratio: number): string {
  if (ratio < 0.3) {
    return "border-red-200 bg-red-50 text-red-700";
  }
  if (ratio < 0.8) {
    return "border-amber-200 bg-amber-50 text-amber-700";
  }
  return "border-emerald-200 bg-emerald-50 text-emerald-700";
}

function videoColor(backupCount: number): string {
  if (backupCount <= 1) {
    return "border-red-200 bg-red-50 text-red-700";
  }
  return "border-emerald-200 bg-emerald-50 text-emerald-700";
}

function filterTree(nodes: BackupTreeNode[], keyword: string): BackupTreeNode[] {
  const value = keyword.trim().toLowerCase();
  if (!value) {
    return nodes;
  }

  const walk = (node: BackupTreeNode): BackupTreeNode | null => {
    const childHits = node.children
      .map((child) => walk(child))
      .filter((child): child is BackupTreeNode => child !== null);

    const selfHit =
      node.name.toLowerCase().includes(value) || node.fullPath.toLowerCase().includes(value);

    if (selfHit || childHits.length > 0) {
      return { ...node, children: childHits };
    }

    return null;
  };

  return nodes
    .map((node) => walk(node))
    .filter((node): node is BackupTreeNode => node !== null);
}

type TreeItemProps = {
  node: BackupTreeNode;
  depth: number;
};

function TreeItem({ node, depth }: TreeItemProps) {
  const [expanded, setExpanded] = useState(depth < 2);
  const hasChildren = node.children.length > 0;
  const paddingLeft = 8 + depth * 16;

  const badgeClass =
    node.nodeType === "video" ? videoColor(node.backupCount) : directoryColor(node.backupRatio);

  return (
    <div>
      <div className="flex items-center gap-2 border-b border-slate-100 px-2 py-1.5" style={{ paddingLeft }}>
        {node.nodeType === "directory" ? (
          <button
            type="button"
            className="h-6 w-6 rounded border border-slate-200 text-xs text-slate-700"
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? "-" : "+"}
          </button>
        ) : (
          <span className="inline-block h-6 w-6" />
        )}

        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-slate-900">{node.name}</div>
          <div className="truncate font-mono text-[10px] text-slate-500">{node.fullPath}</div>
        </div>

        <span className={`rounded-full border px-2 py-0.5 text-xs font-semibold ${badgeClass}`}>
          {node.nodeType === "video"
            ? node.backupCount <= 1
              ? "single"
              : `x${node.backupCount}`
            : `${ratioLabel(node.backupRatio)} (${node.backedUpVideoCount}/${node.videoCount})`}
        </span>
      </div>

      {expanded && hasChildren && (
        <div>
          {node.children.map((child) => (
            <TreeItem key={child.key} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

function App() {
  const [rootPath, setRootPath] = useState("");
  const [keyword, setKeyword] = useState("");
  const [status, setStatus] = useState<ScanStatusDto>(DEFAULT_STATUS);
  const [message, setMessage] = useState("Ready");
  const [roots, setRoots] = useState<ScanRootOption[]>([]);
  const [selectedRootIds, setSelectedRootIds] = useState<number[]>([]);
  const [treeData, setTreeData] = useState<BackupTreeNode[]>([]);
  const [isLoadingRoots, setIsLoadingRoots] = useState(false);
  const [isLoadingTree, setIsLoadingTree] = useState(false);
  const selectedRootIdsRef = useRef<number[]>(selectedRootIds);

  const progressPercent = useMemo(() => {
    if (status.totalCandidates === 0) {
      return 0;
    }
    return Math.round((status.scannedFiles / status.totalCandidates) * 100);
  }, [status.scannedFiles, status.totalCandidates]);

  const visibleTree = useMemo(() => filterTree(treeData, keyword), [treeData, keyword]);

  useEffect(() => {
    selectedRootIdsRef.current = selectedRootIds;
  }, [selectedRootIds]);

  useEffect(() => {
    void loadStatus();
    void loadRoots();
  }, []);

  useEffect(() => {
    if (!selectedRootIds.length) {
      setTreeData([]);
      return;
    }
    void loadTree(selectedRootIds);
  }, [selectedRootIds]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    const setupListeners = async () => {
      unlisteners.push(
        await listen<ScanStartedEvent>("scan:started", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: true,
            rootPath: event.payload.rootPath,
            totalCandidates: event.payload.totalCandidates,
            scannedFiles: 0,
            hashedFiles: 0,
            errorCount: 0,
            lastError: null,
            startedAt: new Date().toISOString(),
            finishedAt: null,
          }));
          setMessage(`Scanning started: ${event.payload.rootPath}`);
        }),
      );

      unlisteners.push(
        await listen<ScanProgressEvent>("scan:progress", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: true,
            scannedFiles: event.payload.scannedFiles,
            totalCandidates: event.payload.totalCandidates,
            hashedFiles: event.payload.hashedFiles,
            errorCount: event.payload.errorCount,
          }));
          setMessage(`Scanning: ${event.payload.path}`);
        }),
      );

      unlisteners.push(
        await listen<ScanCompletedEvent>("scan:completed", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: false,
            scannedFiles: event.payload.scannedFiles,
            hashedFiles: event.payload.hashedFiles,
            errorCount: event.payload.errorCount,
            finishedAt: event.payload.finishedAt,
          }));
          setMessage(
            `Scan finished. Scanned ${event.payload.scannedFiles}, hashed ${event.payload.hashedFiles}, removed ${event.payload.removedPaths}`,
          );

          void loadRoots().then(() => {
            if (selectedRootIdsRef.current.length) {
              void loadTree(selectedRootIdsRef.current);
            }
          });
        }),
      );

      unlisteners.push(
        await listen<ScanErrorEvent>("scan:error", (event) => {
          setStatus((current) => ({
            ...current,
            errorCount: current.errorCount + 1,
            lastError: event.payload.message,
          }));
          setMessage(`Error: ${event.payload.message}`);
        }),
      );
    };

    void setupListeners();

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const chooseFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select video folder",
    });

    if (typeof selected === "string") {
      setRootPath(selected);
    }
  };

  const triggerScan = async () => {
    if (!rootPath.trim()) {
      setMessage("Please select a folder first.");
      return;
    }

    try {
      await startScan(rootPath);
      setMessage("Scan task queued.");
    } catch (error) {
      setMessage(String(error));
    }
  };

  const toggleRoot = (rootId: number) => {
    setSelectedRootIds((current) => {
      if (current.includes(rootId)) {
        return current.filter((value) => value !== rootId);
      }
      return [...current, rootId].sort((left, right) => left - right);
    });
  };

  async function loadStatus() {
    try {
      const data = await getScanStatus();
      setStatus(data);
      if (data.rootPath) {
        setRootPath(data.rootPath);
      }
    } catch (error) {
      setMessage(`Status fetch failed: ${String(error)}`);
    }
  }

  async function loadRoots() {
    setIsLoadingRoots(true);
    try {
      const data = await listScanRoots();
      setRoots(data);
      setSelectedRootIds((current) => current.filter((item) => data.some((root) => root.id === item)));
    } catch (error) {
      setMessage(`Failed to load scan roots: ${String(error)}`);
    } finally {
      setIsLoadingRoots(false);
    }
  }

  async function loadTree(rootIds: number[]) {
    setIsLoadingTree(true);
    try {
      const data = await getBackupTree({ rootIds });
      setTreeData(data);
    } catch (error) {
      setMessage(`Failed to load tree: ${String(error)}`);
    } finally {
      setIsLoadingTree(false);
    }
  }

  return (
    <main className="mx-auto grid max-w-7xl gap-4 pb-4 text-slate-900 md:gap-5 md:pb-6">
      <section className="rounded-2xl bg-gradient-to-r from-slate-950/90 via-slate-900/90 to-teal-900/85 px-5 py-4 text-white shadow-[0_14px_30px_rgba(8,24,48,0.32)] md:px-6 md:py-5">
        <h1 className="text-2xl font-semibold tracking-tight md:text-3xl">Video Backup Explorer</h1>
        <p className="mt-1.5 text-sm text-slate-200 md:text-base">
          Build index by scanning folders, then select multiple historical roots to inspect backup
          coverage as a colored directory tree.
        </p>
      </section>

      <section className="panel p-4 md:p-5">
        <div className="mb-3 flex items-center justify-between gap-3">
          <h2 className="text-lg font-semibold text-slate-900">Scanner</h2>
          <span
            className={
              status.isRunning
                ? "rounded-full bg-teal-100 px-2.5 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-teal-700"
                : "rounded-full bg-orange-100 px-2.5 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-orange-700"
            }
          >
            {status.isRunning ? "Running" : "Idle"}
          </span>
        </div>

        <div className="grid gap-2.5 md:grid-cols-[1fr_auto_auto]">
          <input
            type="text"
            className="input-base"
            value={rootPath}
            onChange={(event) => setRootPath(event.currentTarget.value)}
            placeholder="Select or type a folder path"
          />
          <button type="button" className="btn-soft" onClick={chooseFolder}>
            Choose Folder
          </button>
          <button type="button" className="btn-primary" onClick={triggerScan} disabled={status.isRunning}>
            Start Scan
          </button>
        </div>

        <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-slate-200">
          <div
            className="h-full rounded-full bg-gradient-to-r from-amber-600 to-teal-500 transition-[width] duration-200"
            style={{ width: `${progressPercent}%` }}
          />
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2.5 md:grid-cols-4">
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Total</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.totalCandidates}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Scanned</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.scannedFiles}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Hashed</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.hashedFiles}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Errors</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.errorCount}</strong>
          </div>
        </div>

        <p className="mt-3 text-sm text-slate-600">{message}</p>
      </section>

      <section className="panel p-4 md:p-5">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg font-semibold text-slate-900">Historical Scan Roots</h2>
          <button type="button" className="btn-soft" onClick={() => void loadRoots()} disabled={isLoadingRoots}>
            {isLoadingRoots ? "Loading..." : "Refresh Roots"}
          </button>
        </div>

        <div className="grid gap-2 md:grid-cols-2">
          {roots.map((root) => (
            <label
              key={root.id}
              className="flex cursor-pointer items-start gap-3 rounded-lg border border-slate-200 bg-white px-3 py-2"
            >
              <input
                type="checkbox"
                className="mt-1 h-4 w-4 rounded border-slate-300 accent-teal-600"
                checked={selectedRootIds.includes(root.id)}
                onChange={() => toggleRoot(root.id)}
              />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-semibold text-slate-900">{root.canonicalPath}</p>
                <p className="text-xs text-slate-600">
                  {root.status} · {root.backedUpVideos}/{root.totalVideos} · {ratioLabel(root.backupRatio)}
                </p>
              </div>
            </label>
          ))}
        </div>

        {!roots.length && <p className="text-sm text-slate-500">No historical roots yet. Run scan first.</p>}
      </section>

      <section className="panel p-4 md:p-5">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg font-semibold text-slate-900">Backup Tree</h2>
          <div className="flex items-center gap-2">
            <input
              type="text"
              className="input-base min-w-[240px]"
              placeholder="Filter by directory or video path"
              value={keyword}
              onChange={(event) => setKeyword(event.currentTarget.value)}
            />
            <button
              type="button"
              className="btn-soft"
              onClick={() => void loadTree(selectedRootIds)}
              disabled={!selectedRootIds.length || isLoadingTree}
            >
              {isLoadingTree ? "Loading..." : "Refresh Tree"}
            </button>
          </div>
        </div>

        <div className="mb-2 text-xs text-slate-600">
          Selected roots: {selectedRootIds.length} · Displayed roots: {visibleTree.length}
        </div>

        <div className="max-h-[620px] overflow-auto rounded-xl border border-slate-200 bg-white">
          {visibleTree.map((root) => (
            <TreeItem key={root.key} node={root} depth={0} />
          ))}

          {!visibleTree.length && (
            <p className="px-3 py-4 text-sm text-slate-500">
              {selectedRootIds.length ? "No matching nodes for current filter." : "Select historical roots to render tree."}
            </p>
          )}
        </div>
      </section>
    </main>
  );
}

export default App;
